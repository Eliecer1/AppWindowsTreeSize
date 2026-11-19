//! Guardar y abrir análisis en disco, para ver los resultados al instante al abrir la app.
//!
//! Formato binario propio comprimido con LZ4. Solo se guarda lo que sigue en el árbol (no lo que se
//! borró o se reemplazó al refrescar), en orden por niveles, así los hijos se reconstruyen a partir
//! del padre de cada elemento y conservan su orden de mayor a menor.

use crate::{Category, Dir, Node, NodeId, Tree, F_DIR, NO_DIR, NO_PARENT};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"AWTS";
const VERSION: u32 = 1;

/// Lo que se puede saber de un análisis guardado sin abrirlo entero.
#[derive(Debug, Clone)]
pub struct Header {
    pub root: PathBuf,
    pub scanned_at: i64,
    pub nodes: u64,
}

/// Guarda el árbol en `path`. Escribe primero en un archivo temporal, así un corte a mitad
/// no deja un análisis dañado.
pub fn save(tree: &Tree, path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let file = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
        let mut w = lz4_flex::frame::FrameEncoder::new(file);
        write_tree(tree, &mut w)?;
        w.finish().map_err(io::Error::other)?.flush()?;
    }
    std::fs::rename(&tmp, path)
}

fn write_tree(tree: &Tree, w: &mut impl Write) -> io::Result<()> {
    // Orden por niveles de lo que sigue en el árbol, y el id nuevo de cada elemento.
    let mut order: Vec<NodeId> = Vec::with_capacity(tree.attached_len());
    let mut new_id = vec![NO_PARENT; tree.nodes.len()];
    let mut queue = VecDeque::from([Tree::ROOT]);
    while let Some(id) = queue.pop_front() {
        new_id[id as usize] = order.len() as NodeId;
        order.push(id);
        queue.extend(tree.kids(id));
    }

    w.write_all(MAGIC)?;
    put_u32(w, VERSION)?;
    put_i64(w, tree.scanned_at)?;
    put_str(w, &tree.root_path.to_string_lossy())?;
    put_u64(w, order.len() as u64)?;
    for &id in &order {
        let n = &tree.nodes[id as usize];
        let name = tree.name(id).as_bytes();
        put_u16(w, name.len() as u16)?;
        w.write_all(name)?;
        w.write_all(&[n.flags, n.category.index() as u8])?;
        put_u32(w, if n.parent == NO_PARENT { NO_PARENT } else { new_id[n.parent as usize] })?;
        put_u64(w, n.size)?;
        put_u64(w, n.logical)?;
        put_i64(w, n.modified)?;
        if n.dir != NO_DIR {
            let d = &tree.dirs[n.dir as usize];
            for v in [d.cloud, d.cloud_files, d.files, d.dirs] {
                put_u64(w, v)?;
            }
            for v in d.cats {
                put_u64(w, v)?;
            }
        }
    }
    let links: Vec<(u64, NodeId)> =
        tree.links.iter().filter(|(_, p)| tree.attached(**p)).map(|(k, p)| (*k, new_id[*p as usize])).collect();
    put_u64(w, links.len() as u64)?;
    for (k, p) in links {
        put_u64(w, k)?;
        put_u32(w, p)?;
    }
    Ok(())
}

fn open(path: &Path) -> io::Result<lz4_flex::frame::FrameDecoder<BufReader<File>>> {
    Ok(lz4_flex::frame::FrameDecoder::new(BufReader::with_capacity(1 << 20, File::open(path)?)))
}

fn read_head(r: &mut impl Read) -> io::Result<Header> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC || get_u32(r)? != VERSION {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "No es un análisis guardado de esta versión"));
    }
    Ok(Header { scanned_at: get_i64(r)?, root: PathBuf::from(get_str(r)?), nodes: get_u64(r)? })
}

/// Lee solo la cabecera de un análisis guardado.
pub fn read_header(path: &Path) -> io::Result<Header> {
    read_head(&mut open(path)?)
}

/// Abre un análisis guardado.
pub fn load(path: &Path) -> io::Result<Tree> {
    let mut r = open(path)?;
    let h = read_head(&mut r)?;
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "Análisis guardado dañado");
    let count = usize::try_from(h.nodes).map_err(|_| bad())?;
    if count == 0 || count > u32::MAX as usize - 1 {
        return Err(bad());
    }
    let mut t = Tree {
        root_path: h.root,
        nodes: Vec::with_capacity(count),
        dirs: Vec::new(),
        names: String::new(),
        live: false,
        seen: HashMap::new(),
        links: HashMap::new(),
        scanned_at: h.scanned_at,
    };
    let mut name = Vec::with_capacity(256);
    for i in 0..count {
        let len = get_u16(&mut r)? as usize;
        name.resize(len, 0);
        r.read_exact(&mut name)?;
        let mut fc = [0u8; 2];
        r.read_exact(&mut fc)?;
        let parent = get_u32(&mut r)?;
        if (i == 0) != (parent == NO_PARENT) || (parent != NO_PARENT && parent as usize >= i) {
            return Err(bad());
        }
        let category = *Category::ALL.get(fc[1] as usize).ok_or_else(bad)?;
        let off = t.names.len() as u32;
        t.names.push_str(std::str::from_utf8(&name).map_err(|_| bad())?);
        let mut n = Node {
            name: off,
            name_len: len as u16,
            flags: fc[0],
            category,
            parent,
            dir: NO_DIR,
            size: get_u64(&mut r)?,
            logical: get_u64(&mut r)?,
            modified: get_i64(&mut r)?,
        };
        if n.flags & F_DIR != 0 {
            let mut d = Dir {
                cloud: get_u64(&mut r)?,
                cloud_files: get_u64(&mut r)?,
                files: get_u64(&mut r)?,
                dirs: get_u64(&mut r)?,
                ..Dir::default()
            };
            for c in d.cats.iter_mut() {
                *c = get_u64(&mut r)?;
            }
            n.dir = t.dirs.len() as u32;
            t.dirs.push(d);
        }
        if parent != NO_PARENT {
            let pd = t.nodes[parent as usize].dir;
            if pd == NO_DIR {
                return Err(bad());
            }
            t.dirs[pd as usize].children.push(i as NodeId);
        }
        t.nodes.push(n);
    }
    if t.nodes[0].dir == NO_DIR {
        return Err(bad());
    }
    let links = get_u64(&mut r)?;
    for _ in 0..links {
        let k = get_u64(&mut r)?;
        let p = get_u32(&mut r)?;
        if (p as usize) < count {
            t.links.insert(k, p);
        }
    }
    for d in t.dirs.iter_mut() {
        d.children.shrink_to_fit();
    }
    Ok(t)
}

fn put_u16(w: &mut impl Write, v: u16) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn put_u32(w: &mut impl Write, v: u32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn put_u64(w: &mut impl Write, v: u64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn put_i64(w: &mut impl Write, v: i64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn put_str(w: &mut impl Write, s: &str) -> io::Result<()> {
    put_u32(w, s.len() as u32)?;
    w.write_all(s.as_bytes())
}
fn get<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}
fn get_u16(r: &mut impl Read) -> io::Result<u16> {
    get(r).map(u16::from_le_bytes)
}
fn get_u32(r: &mut impl Read) -> io::Result<u32> {
    get(r).map(u32::from_le_bytes)
}
fn get_u64(r: &mut impl Read) -> io::Result<u64> {
    get(r).map(u64::from_le_bytes)
}
fn get_i64(r: &mut impl Read) -> io::Result<i64> {
    get(r).map(i64::from_le_bytes)
}
fn get_str(r: &mut impl Read) -> io::Result<String> {
    let len = get_u32(r)? as usize;
    if len > 1 << 16 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Ruta demasiado larga"));
    }
    let mut b = vec![0u8; len];
    r.read_exact(&mut b)?;
    String::from_utf8(b).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Ruta no válida"))
}
