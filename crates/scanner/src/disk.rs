//! Lectura de carpetas y cuánto ocupa de verdad cada archivo en el disco.
//!
//! El tamaño que muestra el Explorador (tamaño lógico) no siempre es lo que ocupa:
//! - Los archivos de OneDrive, iCloud o Dropbox "disponibles solo en línea" son marcadores
//!   de posición: aparentan pesar varios GB pero en el disco ocupan casi nada.
//! - Los archivos comprimidos por NTFS o dispersos (sparse) ocupan menos que su tamaño.
//! - Todo archivo ocupa clústeres completos (normalmente de 4 KB).
//! - Un mismo archivo puede tener varios nombres (enlaces duros, muy comunes en `C:\Windows\WinSxS`).
//!   Para no contarlo dos veces, cada entrada trae el identificador del archivo en el volumen.

use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStat {
    /// Tamaño que ve el usuario en el Explorador.
    pub logical: u64,
    /// Espacio que ocupa realmente en el disco.
    pub allocated: u64,
    /// Es un marcador de posición de un servicio en la nube que no está completamente descargado.
    pub cloud: bool,
}

/// Una entrada leída de una carpeta, antes de agregarla al árbol.
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub name: String,
    pub is_dir: bool,
    pub logical: u64,
    /// Atributos de Windows (0 en otros sistemas).
    pub attrs: u32,
    /// Espacio ocupado cuando el sistema ya lo da al listar la carpeta.
    pub allocated: Option<u64>,
    /// Última modificación en segundos Unix.
    pub modified: i64,
    /// Identificador del archivo en el volumen, cuando puede tener otros nombres (enlaces duros).
    pub file_id: Option<u64>,
}

/// Información del volumen necesaria para calcular el espacio ocupado.
#[derive(Clone, Copy, Debug)]
pub struct Volume {
    pub cluster: u64,
    /// El sistema de archivos admite enlaces duros con identificadores fiables (NTFS).
    pub hardlinks: bool,
}

impl Volume {
    pub fn of(root: &Path) -> Volume {
        Volume {
            cluster: imp::cluster_size(root).filter(|c| *c > 0).unwrap_or(4096),
            hardlinks: imp::supports_hardlinks(root),
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn round(&self, bytes: u64) -> u64 {
        bytes.div_ceil(self.cluster) * self.cluster
    }
}

/// Lee el contenido de una carpeta sin seguir enlaces simbólicos ni uniones (junctions),
/// así no se cuentan dos veces carpetas como "Documents and Settings".
pub fn list(path: &Path, vol: &Volume) -> io::Result<Vec<RawEntry>> {
    imp::list(path, vol)
}

pub fn stat(path: &Path, raw: &RawEntry, vol: &Volume) -> FileStat {
    imp::stat(path, raw, vol)
}

#[cfg(windows)]
mod imp {
    use super::{FileStat, RawEntry, Volume};
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetCompressedFileSizeW,
        GetDiskFreeSpaceW, GetFileInformationByHandleEx, GetVolumeInformationW, GetVolumePathNameW,
        FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
        FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        INVALID_FILE_SIZE, OPEN_EXISTING,
    };

    /// Ruta en UTF-16 con el prefijo `\\?\`, para que funcionen rutas de más de 260 caracteres.
    pub fn wide(p: &Path) -> Vec<u16> {
        let s = p.as_os_str();
        let raw: Vec<u16> = s.encode_wide().collect();
        let starts = |pre: &str| raw.len() >= pre.len() && raw[..pre.len()].iter().copied().eq(pre.encode_utf16());
        let mut out: Vec<u16> = if starts(r"\\?\") || starts(r"\\.\") || !p.is_absolute() {
            raw
        } else if starts(r"\\") {
            r"\\?\UNC\".encode_utf16().chain(raw[2..].iter().copied()).collect()
        } else {
            r"\\?\".encode_utf16().chain(raw).collect()
        };
        out.push(0);
        out
    }

    fn plain_wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn cluster_size(root: &Path) -> Option<u64> {
        let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
        let ok = unsafe { GetDiskFreeSpaceW(volume_root(root).as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
        (ok != 0).then(|| spc as u64 * bps as u64)
    }

    /// `C:\` para cualquier ruta dentro de C:.
    fn volume_root(p: &Path) -> Vec<u16> {
        let mut buf = vec![0u16; 1024];
        let ok = unsafe { GetVolumePathNameW(plain_wide(p).as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if ok == 0 {
            return plain_wide(p);
        }
        buf
    }

    pub fn supports_hardlinks(root: &Path) -> bool {
        let mut fs = [0u16; 64];
        let ok = unsafe {
            GetVolumeInformationW(
                volume_root(root).as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        if ok == 0 {
            return false;
        }
        let len = fs.iter().position(|c| *c == 0).unwrap_or(fs.len());
        String::from_utf16_lossy(&fs[..len]).eq_ignore_ascii_case("NTFS")
    }

    /// FILETIME (cientos de nanosegundos desde 1601) a segundos Unix.
    fn unix_secs(ft: i64) -> i64 {
        if ft <= 0 {
            0
        } else {
            ft / 10_000_000 - 11_644_473_600
        }
    }

    /// Lista una carpeta de una vez con `FileIdBothDirectoryInfo`: nombres, tamaños, fechas,
    /// atributos e identificador de cada archivo, sin abrir los archivos uno por uno.
    pub fn list(path: &Path, vol: &Volume) -> io::Result<Vec<RawEntry>> {
        let h = unsafe {
            CreateFileW(
                wide(path).as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        struct Guard(windows_sys::Win32::Foundation::HANDLE);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { CloseHandle(self.0) };
            }
        }
        let _g = Guard(h);

        let mut out = Vec::new();
        // Alineado a 8 bytes, como exige la estructura.
        let mut buf = vec![0u64; 8 * 1024];
        let mut class = FileIdBothDirectoryRestartInfo;
        loop {
            let ok = unsafe {
                GetFileInformationByHandleEx(h, class, buf.as_mut_ptr() as *mut c_void, (buf.len() * 8) as u32)
            };
            class = FileIdBothDirectoryInfo;
            if ok == 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    break;
                }
                return Err(e);
            }
            let base = buf.as_ptr() as *const u8;
            let mut off = 0usize;
            loop {
                // SAFETY: Windows llena el búfer con estructuras encadenadas por NextEntryOffset.
                let info = unsafe { &*(base.add(off) as *const FILE_ID_BOTH_DIR_INFO) };
                let name_ptr = unsafe { (info as *const FILE_ID_BOTH_DIR_INFO as *const u8).add(std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)) } as *const u16;
                let name16 = unsafe { std::slice::from_raw_parts(name_ptr, info.FileNameLength as usize / 2) };
                let skip = name16 == [b'.' as u16] || name16 == [b'.' as u16, b'.' as u16];
                let attrs = info.FileAttributes;
                // Con un punto de reanálisis, EaSize guarda su etiqueta. Los enlaces simbólicos y las uniones
                // son "sustitutos de nombre": se saltan. Los marcadores de la nube no lo son y se cuentan.
                let surrogate = attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && info.EaSize & 0x2000_0000 != 0;
                if !skip && !surrogate {
                    let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
                    out.push(RawEntry {
                        name: String::from_utf16_lossy(name16),
                        is_dir,
                        logical: if is_dir { 0 } else { info.EndOfFile.max(0) as u64 },
                        attrs,
                        allocated: None,
                        modified: unix_secs(info.LastWriteTime),
                        file_id: (vol.hardlinks && !is_dir && info.FileId != 0).then_some(info.FileId as u64),
                    });
                }
                if info.NextEntryOffset == 0 {
                    break;
                }
                off += info.NextEntryOffset as usize;
            }
        }
        Ok(out)
    }

    pub fn stat(path: &Path, raw: &RawEntry, vol: &Volume) -> FileStat {
        let logical = raw.logical;
        let attrs = raw.attrs;
        let cloud = attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE) != 0;
        let special = cloud || attrs & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0;
        let allocated = if special {
            // Solo consulta metadatos: no descarga el archivo de la nube.
            let mut high = 0u32;
            let low = unsafe { GetCompressedFileSizeW(wide(path).as_ptr(), &mut high) };
            if low == INVALID_FILE_SIZE && std::io::Error::last_os_error().raw_os_error().unwrap_or(0) != 0 {
                if cloud { 0 } else { vol.round(logical) }
            } else {
                vol.round(((high as u64) << 32) | low as u64)
            }
        } else {
            vol.round(logical)
        };
        FileStat { logical, allocated: allocated.min(vol.round(logical)), cloud }
    }
}

#[cfg(unix)]
mod imp {
    use super::{FileStat, RawEntry, Volume};
    use std::io;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub fn cluster_size(_root: &Path) -> Option<u64> {
        None
    }

    pub fn supports_hardlinks(_root: &Path) -> bool {
        true
    }

    pub fn list(path: &Path, vol: &Volume) -> io::Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)?.flatten() {
            // `DirEntry::metadata` no sigue enlaces simbólicos.
            let Ok(meta) = entry.metadata() else { continue };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            let is_dir = ft.is_dir();
            out.push(RawEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir,
                logical: if is_dir { 0 } else { meta.len() },
                attrs: 0,
                // En Linux y macOS los bloques asignados ya reflejan archivos dispersos y comprimidos.
                allocated: Some(meta.blocks() * 512),
                modified: meta.mtime(),
                file_id: (vol.hardlinks && !is_dir && meta.nlink() > 1).then_some(meta.ino() ^ (meta.dev() << 48)),
            });
        }
        Ok(out)
    }

    pub fn stat(_path: &Path, raw: &RawEntry, _vol: &Volume) -> FileStat {
        FileStat { logical: raw.logical, allocated: raw.allocated.unwrap_or(raw.logical), cloud: false }
    }
}
