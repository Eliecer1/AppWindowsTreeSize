//! Cuánto ocupa de verdad un archivo en el disco y si está solo en la nube.
//!
//! El tamaño que muestra el Explorador (tamaño lógico) no siempre es lo que ocupa:
//! - Los archivos de OneDrive, iCloud o Dropbox "disponibles solo en línea" son marcadores
//!   de posición: aparentan pesar varios GB pero en el disco ocupan casi nada.
//! - Los archivos comprimidos por NTFS o dispersos (sparse) ocupan menos que su tamaño.
//! - Todo archivo ocupa clústeres completos (normalmente de 4 KB).

use std::fs::Metadata;
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

/// Información del volumen necesaria para calcular el espacio ocupado.
#[derive(Clone, Copy, Debug)]
pub struct Volume {
    pub cluster: u64,
}

impl Volume {
    pub fn of(root: &Path) -> Volume {
        Volume { cluster: imp::cluster_size(root).filter(|c| *c > 0).unwrap_or(4096) }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn round(&self, bytes: u64) -> u64 {
        bytes.div_ceil(self.cluster) * self.cluster
    }
}

pub fn stat(path: &Path, meta: &Metadata, vol: &Volume) -> FileStat {
    imp::stat(path, meta, vol)
}

#[cfg(windows)]
mod imp {
    use super::{FileStat, Volume};
    use std::fs::Metadata;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{
        GetCompressedFileSizeW, GetDiskFreeSpaceW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_OFFLINE,
        FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_SPARSE_FILE,
        INVALID_FILE_SIZE,
    };

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn cluster_size(root: &Path) -> Option<u64> {
        let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
        let ok = unsafe { GetDiskFreeSpaceW(wide(root).as_ptr(), &mut spc, &mut bps, &mut free, &mut total) };
        (ok != 0).then(|| spc as u64 * bps as u64)
    }

    pub fn stat(path: &Path, meta: &Metadata, vol: &Volume) -> FileStat {
        let logical = meta.len();
        let attrs = meta.file_attributes();
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
    use super::{FileStat, Volume};
    use std::fs::Metadata;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub fn cluster_size(_root: &Path) -> Option<u64> {
        None
    }

    pub fn stat(_path: &Path, meta: &Metadata, _vol: &Volume) -> FileStat {
        // En Linux y macOS los bloques asignados ya reflejan archivos dispersos y comprimidos.
        FileStat { logical: meta.len(), allocated: meta.blocks() * 512, cloud: false }
    }
}
