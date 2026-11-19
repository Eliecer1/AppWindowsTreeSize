//! Lectura directa de la tabla maestra de archivos (MFT) de un volumen NTFS.
//!
//! En lugar de abrir carpeta por carpeta, se lee la tabla completa del disco de una sola pasada:
//! cada archivo es un registro de 1 KB con su nombre, su carpeta, su tamaño y sus fechas. Es lo que
//! hace WizTree y suele ser de 10 a 20 veces más rápido. Necesita permisos de administrador para
//! abrir el volumen (`\\.\C:`); sin ellos, el escaneo vuelve al recorrido normal.
//!
//! El análisis del formato no depende de Windows, así que se prueba en cualquier sistema con una
//! imagen NTFS pequeña.

use crate::{Context, FileStat, NewFile, Progress, Tree};
use std::io;
use std::path::Path;
use std::sync::atomic::Ordering;

/// Algo de lo que se pueden leer bytes en una posición: el volumen o un archivo de imagen.
pub trait ReadAt {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
}

impl ReadAt for std::fs::File {
    #[cfg(unix)]
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(self, buf, offset)
    }
    #[cfg(windows)]
    fn read_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = self.seek_read(buf, offset)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            buf = &mut buf[n..];
            offset += n as u64;
        }
        Ok(())
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

const ROOT_RECORD: usize = 5;
const REF_MASK: u64 = 0xFFFF_FFFF_FFFF;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

const NS_DOS: u8 = 2;

const R_IN_USE: u8 = 1;
const R_DIR: u8 = 2;

/// Geometría del volumen, leída del sector de arranque.
#[derive(Debug, Clone, Copy)]
struct Boot {
    cluster: u64,
    record: usize,
    mft_offset: u64,
}

fn read_boot(r: &impl ReadAt) -> io::Result<Boot> {
    let mut b = [0u8; 512];
    r.read_at(&mut b, 0)?;
    if &b[3..11] != b"NTFS    " {
        return Err(bad("El volumen no es NTFS"));
    }
    let sector = u16_at(&b, 0x0B) as u64;
    let spc = b[0x0D];
    let spc = if spc > 0x80 { 1u64 << (256 - spc as u32) } else { spc as u64 };
    let cluster = sector * spc;
    let cpr = b[0x40] as i8;
    let record = if cpr > 0 { cpr as u64 * cluster } else { 1u64 << (-(cpr as i32)) } as usize;
    if cluster == 0 || !(256..=65536).contains(&record) || !record.is_multiple_of(512) {
        return Err(bad("Geometría NTFS no válida"));
    }
    Ok(Boot { cluster, record, mft_offset: u64_at(&b, 0x30) * cluster })
}

/// Aplica la "secuencia de actualización": NTFS guarda los dos últimos bytes de cada bloque de
/// 512 bytes aparte, para detectar escrituras incompletas.
fn fixup(rec: &mut [u8]) -> bool {
    if rec.len() < 48 || &rec[0..4] != b"FILE" {
        return false;
    }
    let off = u16_at(rec, 4) as usize;
    let count = u16_at(rec, 6) as usize;
    if count == 0 || off + count * 2 > rec.len() || (count - 1) * 512 > rec.len() {
        return false;
    }
    let usn = [rec[off], rec[off + 1]];
    for i in 1..count {
        let pos = i * 512 - 2;
        if rec[pos..pos + 2] != usn {
            return false;
        }
        rec[pos] = rec[off + i * 2];
        rec[pos + 1] = rec[off + i * 2 + 1];
    }
    true
}

/// Tramos (posición en el disco, longitud) de un atributo no residente. `None` en la posición
/// es un tramo disperso, que no ocupa espacio.
fn runs(b: &[u8]) -> Vec<(Option<u64>, u64)> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while i < b.len() && b[i] != 0 {
        let h = b[i];
        let (ls, os) = ((h & 0x0F) as usize, (h >> 4) as usize);
        i += 1;
        if ls == 0 || ls > 8 || os > 8 || i + ls + os > b.len() {
            break;
        }
        let mut len = 0u64;
        for k in 0..ls {
            len |= (b[i + k] as u64) << (8 * k);
        }
        i += ls;
        if os == 0 {
            out.push((None, len));
        } else {
            let mut off = 0i64;
            for k in 0..os {
                off |= (b[i + k] as i64) << (8 * k);
            }
            // Extensión de signo.
            let shift = 64 - 8 * os as u32;
            off = (off << shift) >> shift;
            lcn += off;
            out.push((Some(lcn as u64), len));
        }
        i += os;
    }
    out
}

/// Lo que se sabe de un registro de la MFT.
#[derive(Default)]
struct Records {
    flags: Vec<u8>,
    seq: Vec<u16>,
    logical: Vec<u64>,
    allocated: Vec<u64>,
    modified: Vec<i64>,
    attrs: Vec<u32>,
    /// Nombres: (registro, carpeta, posición del nombre, longitud).
    names: Vec<(u32, u64, u32, u16)>,
    text: String,
}

impl Records {
    fn with_len(n: usize) -> Records {
        Records {
            flags: vec![0; n],
            seq: vec![0; n],
            logical: vec![0; n],
            allocated: vec![0; n],
            modified: vec![0; n],
            attrs: vec![0; n],
            names: Vec::with_capacity(n),
            text: String::with_capacity(n * 16),
        }
    }
}

/// Analiza un registro ya corregido y suma lo que aporta a `recs`.
fn parse_record(rec: &[u8], index: usize, cluster: u64, recs: &mut Records) {
    let flags = u16_at(rec, 0x16);
    if flags & 1 == 0 {
        return;
    }
    let base = (u64_at(rec, 0x20) & REF_MASK) as usize;
    let target = if base != 0 { base } else { index };
    if target >= recs.flags.len() {
        return;
    }
    if base == 0 {
        recs.flags[target] |= R_IN_USE | if flags & 2 != 0 { R_DIR } else { 0 };
        recs.seq[target] = u16_at(rec, 0x10);
    }
    let used = (u32_at(rec, 0x18) as usize).min(rec.len());
    let mut off = u16_at(rec, 0x14) as usize;
    while off + 16 <= used {
        let ty = u32_at(rec, off);
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, off + 4) as usize;
        if len < 16 || off + len > used {
            break;
        }
        let a = &rec[off..off + len];
        let non_resident = a[8] != 0;
        let named = a[9] != 0;
        let resident_value = || -> Option<&[u8]> {
            let vl = u32_at(a, 0x10) as usize;
            let vo = u16_at(a, 0x14) as usize;
            a.get(vo..vo + vl)
        };
        match ty {
            ATTR_STANDARD_INFORMATION if !non_resident => {
                if let Some(v) = resident_value().filter(|v| v.len() >= 0x24) {
                    recs.modified[target] = filetime(u64_at(v, 8) as i64);
                    recs.attrs[target] = u32_at(v, 0x20);
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                if let Some(v) = resident_value().filter(|v| v.len() >= 0x42) {
                    let nlen = v[0x40] as usize;
                    let ns = v[0x41];
                    if ns != NS_DOS && v.len() >= 0x42 + nlen * 2 {
                        let units: Vec<u16> = (0..nlen).map(|k| u16_at(v, 0x42 + k * 2)).collect();
                        let name = String::from_utf16_lossy(&units);
                        let pos = recs.text.len() as u32;
                        recs.text.push_str(&name);
                        recs.names.push((target as u32, u64_at(v, 0), pos, name.len() as u16));
                    }
                }
            }
            ATTR_DATA => {
                if non_resident {
                    if a.len() >= 0x40 {
                        let start_vcn = u64_at(a, 0x10);
                        if !named && start_vcn == 0 {
                            recs.logical[target] = u64_at(a, 0x30);
                        }
                        let ro = u16_at(a, 0x20) as usize;
                        if ro < a.len() {
                            let clusters: u64 = runs(&a[ro..]).iter().filter(|r| r.0.is_some()).map(|r| r.1).sum();
                            recs.allocated[target] += clusters * cluster;
                        }
                    }
                } else if !named {
                    // Datos residentes: viven dentro del propio registro y no ocupan clústeres.
                    recs.logical[target] = u32_at(a, 0x10) as u64;
                }
            }
            _ => {}
        }
        off += len;
    }
}

/// FILETIME (cientos de nanosegundos desde 1601) a segundos Unix.
fn filetime(ft: i64) -> i64 {
    if ft <= 0 {
        0
    } else {
        ft / 10_000_000 - 11_644_473_600
    }
}

/// Lee todos los registros de la MFT.
fn read_records(r: &impl ReadAt, progress: &Progress) -> io::Result<Records> {
    let boot = read_boot(r)?;
    let mut rec0 = vec![0u8; boot.record];
    r.read_at(&mut rec0, boot.mft_offset)?;
    if !fixup(&mut rec0) {
        return Err(bad("No se pudo leer el registro de la MFT"));
    }
    // El registro 0 es la propia MFT: sus tramos dicen dónde está el resto de la tabla.
    let mut extents = Vec::new();
    let mut mft_size = 0u64;
    {
        let used = (u32_at(&rec0, 0x18) as usize).min(rec0.len());
        let mut off = u16_at(&rec0, 0x14) as usize;
        while off + 16 <= used {
            let ty = u32_at(&rec0, off);
            let len = u32_at(&rec0, off + 4) as usize;
            if ty == ATTR_END || len < 16 || off + len > used {
                break;
            }
            let a = &rec0[off..off + len];
            if ty == ATTR_DATA && a[8] != 0 && a[9] == 0 && a.len() >= 0x40 {
                mft_size = u64_at(a, 0x30);
                extents = runs(&a[u16_at(a, 0x20) as usize..]);
            }
            off += len;
        }
    }
    if extents.is_empty() || mft_size == 0 {
        return Err(bad("No se encontró la ubicación de la MFT"));
    }
    let total = (mft_size / boot.record as u64) as usize;
    let mut recs = Records::with_len(total);
    let chunk_records = (4 << 20) / boot.record;
    let mut buf = vec![0u8; chunk_records * boot.record];
    let mut index = 0usize;
    'outer: for (lcn, len) in extents {
        let Some(lcn) = lcn else {
            index += (len * boot.cluster / boot.record as u64) as usize;
            continue;
        };
        let start = lcn * boot.cluster;
        let bytes = len * boot.cluster;
        let mut done = 0u64;
        while done < bytes {
            if index >= total {
                break 'outer;
            }
            if progress.is_cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelado"));
            }
            let n = ((bytes - done) as usize).min(buf.len()).min((total - index) * boot.record);
            let n = n - n % boot.record;
            if n == 0 {
                break;
            }
            r.read_at(&mut buf[..n], start + done)?;
            for rec in buf[..n].chunks_exact_mut(boot.record) {
                if fixup(rec) {
                    parse_record(rec, index, boot.cluster, &mut recs);
                }
                index += 1;
            }
            progress.files.store(index as u64, Ordering::Relaxed);
            done += n as u64;
        }
    }
    Ok(recs)
}

/// Arma el árbol a partir de los registros, desde la carpeta raíz del volumen.
fn build(root: &Path, recs: Records, progress: &Progress) -> Tree {
    let n = recs.flags.len();
    // Cuántos nombres tiene cada archivo, para detectar enlaces duros.
    let mut name_count = vec![0u8; n];
    // Hijos de cada carpeta, en formato compacto (inicio de cada carpeta en `kids`).
    let mut entries: Vec<(u32, u32, u32, u16)> = Vec::with_capacity(recs.names.len());
    for &(rec, parent_ref, pos, len) in &recs.names {
        let parent = (parent_ref & REF_MASK) as usize;
        let pseq = (parent_ref >> 48) as u16;
        let r = rec as usize;
        if r == parent || parent >= n || recs.flags[r] & R_IN_USE == 0 || recs.flags[parent] & (R_IN_USE | R_DIR) != (R_IN_USE | R_DIR) {
            continue;
        }
        // Una carpeta borrada cuyo número se reutilizó no es la carpeta de este archivo.
        if pseq != 0 && recs.seq[parent] != 0 && pseq != recs.seq[parent] {
            continue;
        }
        name_count[r] = name_count[r].saturating_add(1);
        entries.push((parent as u32, rec, pos, len));
    }
    entries.sort_unstable_by_key(|e| e.0);
    let mut start = vec![0u32; n + 1];
    for e in &entries {
        start[e.0 as usize + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }

    let mut tree = Tree::new(root);
    let mut visited = vec![false; n];
    let mut queue = std::collections::VecDeque::from([(ROOT_RECORD, Tree::ROOT, Context::default())]);
    visited[ROOT_RECORD] = true;
    let mut dirs_seen = 0u64;
    let mut bytes = 0u64;
    while let Some((rec, id, ctx)) = queue.pop_front() {
        dirs_seen += 1;
        let kids = &entries[start[rec] as usize..start[rec + 1] as usize];
        let mut files = Vec::new();
        let mut subdirs = Vec::new();
        for &(_, child, pos, len) in kids {
            let c = child as usize;
            let name = &recs.text[pos as usize..pos as usize + len as usize];
            if recs.flags[c] & R_DIR != 0 {
                if !visited[c] {
                    visited[c] = true;
                    subdirs.push((c, name));
                }
                continue;
            }
            let attrs = recs.attrs[c];
            let cloud = attrs & (0x0040_0000 | 0x0004_0000 | 0x0000_1000) != 0;
            let logical = recs.logical[c];
            let allocated = recs.allocated[c];
            files.push(NewFile {
                name: name.to_string(),
                stat: FileStat { logical, allocated, cloud },
                modified: recs.modified[c],
                category: ctx.categorize(&crate::extension(name)),
                file_id: (name_count[c] > 1).then_some(c as u64),
            });
        }
        bytes += tree.add_files(id, files);
        let ids = tree.add_dirs(id, subdirs.iter().map(|s| s.1));
        for (cid, (c, name)) in ids.into_iter().zip(subdirs) {
            queue.push_back((c, cid, ctx.enter(name)));
        }
        progress.dirs.store(dirs_seen, Ordering::Relaxed);
        progress.bytes.store(bytes, Ordering::Relaxed);
    }
    tree.finish(Tree::ROOT);
    tree
}

/// Escanea un volumen NTFS completo leyendo su MFT desde `reader`.
pub fn scan_volume(root: &Path, reader: &impl ReadAt, progress: &Progress) -> io::Result<Tree> {
    let recs = read_records(reader, progress)?;
    Ok(build(root, recs, progress))
}

/// Ruta del volumen (`\\.\C:`) si `root` es la raíz de una unidad (`C:\`).
pub fn volume_device(root: &Path) -> Option<String> {
    let s = root.to_string_lossy();
    let b = s.as_bytes();
    let is_root = (b.len() == 2 || (b.len() == 3 && (b[2] == b'\\' || b[2] == b'/'))) && b[0].is_ascii_alphabetic() && b[1] == b':';
    is_root.then(|| format!(r"\\.\{}:", b[0] as char))
}

/// Intenta escanear `root` leyendo la MFT. Solo funciona en la raíz de una unidad NTFS y con
/// permisos de administrador; si no, devuelve un error y se usa el recorrido normal.
pub fn try_scan(root: &Path, progress: &Progress) -> io::Result<Tree> {
    let dev = volume_device(root).ok_or_else(|| bad("No es la raíz de una unidad"))?;
    if !cfg!(windows) {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "Solo en Windows"));
    }
    let file = std::fs::File::open(&dev)?;
    scan_volume(root, &file, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    struct Img(Vec<u8>);
    impl ReadAt for Img {
        fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
            let o = offset as usize;
            let src = self.0.get(o..o + buf.len()).ok_or(io::ErrorKind::UnexpectedEof)?;
            buf.copy_from_slice(src);
            Ok(())
        }
    }

    fn image() -> Img {
        let gz = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ntfs16.img.gz")).unwrap();
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&gz[..]).read_to_end(&mut out).unwrap();
        Img(out)
    }

    #[test]
    fn reads_ntfs_image() {
        // Imagen de 16 MB creada con mkntfs y ntfs-3g:
        //   Videos/a.mp4 (300 000 bytes), Videos/sub/b.bin (5 000), Descargas/pequeno.txt ("hola\n"),
        //   Descargas/enlace.mp4 (enlace duro a Videos/a.mp4), Descargas/disperso.bin (50 MB disperso),
        //   "Carpeta ñandú" con f1.txt … f300.txt, raiz.bin (2 000 000 bytes).
        let progress = Progress::default();
        let t = scan_volume(Path::new("X:\\"), &image(), &progress).unwrap();
        let find = |p: &str| t.find(Path::new(p)).unwrap_or_else(|| panic!("falta {p}"));
        let info = |p: &str| t.info(find(p)).unwrap();

        let a = info("Videos/a.mp4");
        assert_eq!(a.logical, 300_000);
        assert_eq!(a.size, 300_000u64.div_ceil(4096) * 4096);
        assert_eq!(info("Videos/sub/b.bin").logical, 5_000);
        // Los archivos pequeños viven dentro de la MFT y no ocupan clústeres.
        let small = info("Descargas/pequeno.txt");
        assert_eq!((small.logical, small.size), (5, 0));
        let sparse = info("Descargas/disperso.bin");
        assert_eq!(sparse.logical, 50 << 20);
        assert!(sparse.size < 1 << 20);
        // El enlace duro cuenta una sola vez.
        let link = info("Descargas/enlace.mp4");
        assert!(a.hardlink != link.hardlink);
        assert_eq!(a.size + link.size, 300_000u64.div_ceil(4096) * 4096);
        assert_eq!(info("Carpeta ñandú").files, 300);
        assert_eq!(info("raiz.bin").logical, 2_000_000);
        // Los archivos del sistema de NTFS también ocupan espacio y aparecen en la raíz.
        assert!(info("$MFT").size > 0);
        assert!(t.info(Tree::ROOT).unwrap().size > 2_300_000);
        assert!(!t.is_live());
        assert!(t.scanned_at() > 0);
    }

    #[test]
    fn rejects_non_ntfs() {
        let img = Img(vec![0u8; 4096]);
        assert!(scan_volume(Path::new("X:\\"), &img, &Progress::default()).is_err());
    }

    #[test]
    fn decodes_runs() {
        // 0x21: 1 byte de longitud, 2 de posición. Segundo tramo relativo y negativo, tercero disperso.
        let r = runs(&[0x21, 0x10, 0x00, 0x01, 0x21, 0x08, 0x00, 0xFF, 0x01, 0x05, 0x00]);
        assert_eq!(r, vec![(Some(0x100), 0x10), (Some(0x100 - 0x100), 8), (None, 5)]);
    }

    /// En Windows con permisos de administrador (como en GitHub Actions) lee la MFT de verdad.
    #[cfg(windows)]
    #[test]
    fn reads_real_volume_when_admin() {
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let root = format!("{}\\", &windir[..2]);
        let t = match try_scan(Path::new(&root), &Progress::default()) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return,
            Err(e) => panic!("no se pudo leer la MFT de {root}: {e}"),
        };
        let explorer = Path::new(&windir).join("explorer.exe");
        let rel = explorer.strip_prefix(&root).unwrap();
        let id = t.find(rel).expect("explorer.exe en el árbol");
        assert_eq!(t.info(id).unwrap().logical, std::fs::metadata(&explorer).unwrap().len());
        assert!(t.info(Tree::ROOT).unwrap().files > 50_000);
    }

    #[test]
    fn drive_roots() {
        assert_eq!(volume_device(Path::new("C:\\")).as_deref(), Some(r"\\.\C:"));
        assert_eq!(volume_device(Path::new("d:")).as_deref(), Some(r"\\.\d:"));
        assert!(volume_device(Path::new("C:\\Users")).is_none());
    }
}
