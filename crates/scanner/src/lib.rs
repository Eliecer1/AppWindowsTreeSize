//! Motor de escaneo de AppWindowsTreeSize.
//!
//! Recorre una carpeta o unidad en paralelo, suma el tamaño de cada carpeta y guarda el
//! resultado en un árbol plano (`Tree`) que la interfaz consulta por partes: nunca se envía
//! el árbol completo, solo los hijos de la carpeta que se está viendo.
//!
//! El árbol está pensado para millones de archivos: cada elemento ocupa 40 bytes, los nombres
//! viven todos juntos en un solo bloque de texto y los datos que solo tienen sentido en las
//! carpetas (hijos, totales por tipo) se guardan aparte, solo para ellas.

mod category;
mod diff;
mod disk;
pub mod mft;
mod persist;
mod tools;

pub use category::{extension, Category, Context};
pub use diff::{diff, Change};
pub use disk::{FileStat, RawEntry, Volume};
pub use persist::{load, read_header, save, Header};
pub use tools::{duplicates, export_csv, export_html, DupGroup, FileFilter, SubNode};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

pub type NodeId = u32;
const NO_PARENT: NodeId = NodeId::MAX;
const NO_DIR: u32 = u32::MAX;

const F_DIR: u8 = 1;
/// Archivo de la nube que no está descargado.
const F_CLOUD: u8 = 2;
/// Otro nombre (enlace duro) de un archivo que ya se contó en otra parte: no suma espacio.
const F_LINK: u8 = 4;

/// Qué archivos se miran: todos, solo los guardados en este PC o solo los que están en la nube.
///
/// Con `Local` se esconde lo que está solo en la nube y no ocupa nada en el disco. Con `Cloud` solo
/// aparece lo que está en la nube, y el tamaño de cada elemento pasa a ser lo que tiene en la nube
/// sin descargar, para ver qué ocuparía espacio si se bajara.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Storage {
    #[default]
    All,
    Local,
    Cloud,
}

/// Contadores que el escaneo actualiza mientras avanza, para mostrar el progreso en vivo.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    /// El escaneo usó la lectura directa de la MFT.
    pub fast: AtomicBool,
    cancel: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Un archivo o carpeta dentro del árbol (40 bytes).
#[derive(Debug, Clone)]
struct Node {
    /// Posición del nombre dentro de `Tree::names`.
    name: u32,
    name_len: u16,
    flags: u8,
    category: Category,
    parent: NodeId,
    /// Índice en `Tree::dirs` si es una carpeta.
    dir: u32,
    /// Espacio que ocupa realmente en el disco. Es el tamaño con el que se ordena y se dibuja todo.
    size: u64,
    /// Tamaño que muestra el Explorador de Windows.
    logical: u64,
    /// Última modificación en segundos Unix (para carpetas, la más reciente de su contenido).
    modified: i64,
}

/// Lo que solo tienen las carpetas.
#[derive(Debug, Clone, Default)]
struct Dir {
    /// Bytes que están solo en la nube (OneDrive y similares) y no ocupan espacio local.
    cloud: u64,
    /// Archivos contenidos que son marcadores de posición de la nube.
    cloud_files: u64,
    /// Archivos contenidos a cualquier profundidad.
    files: u64,
    /// Subcarpetas contenidas a cualquier profundidad.
    dirs: u64,
    /// Bytes por categoría.
    cats: [u64; 9],
    /// Hijos ordenados de mayor a menor tamaño.
    children: Vec<NodeId>,
}

/// Resultado de un escaneo. Se va llenando mientras el escaneo avanza, así que se puede consultar
/// (detrás de un `RwLock`) antes de que termine.
pub struct Tree {
    root_path: PathBuf,
    nodes: Vec<Node>,
    dirs: Vec<Dir>,
    names: String,
    /// Hay un escaneo escribiendo en el árbol: los hijos todavía no están ordenados.
    live: bool,
    /// Identificadores de archivo vistos durante el escaneo en curso, para detectar enlaces duros.
    seen: HashMap<u64, NodeId>,
    /// Archivos con más de un nombre: identificador → elemento donde se contó su espacio.
    links: HashMap<u64, NodeId>,
    /// Cuándo terminó el último análisis completo, en segundos Unix (0 si todavía no terminó).
    scanned_at: i64,
}

/// Lo que se agrega al árbol de un archivo leído del disco.
#[derive(Debug, Clone)]
pub struct NewFile {
    pub name: String,
    pub stat: FileStat,
    pub modified: i64,
    pub category: Category,
    pub file_id: Option<u64>,
}

/// Escanea `root` y devuelve el árbol completo. Si se cancela, devuelve lo leído hasta ese momento.
pub fn scan(root: &Path, progress: &Progress) -> Tree {
    let tree = RwLock::new(Tree::new(root));
    scan_into(&tree, progress);
    tree.into_inner().unwrap_or_else(|e| e.into_inner())
}

/// Escanea la raíz de `tree` escribiendo en él a medida que avanza, para que se pueda explorar
/// mientras tanto. Cada carpeta se agrega en cuanto se lee, y su tamaño se suma enseguida a las de arriba.
pub fn scan_into(tree: &RwLock<Tree>, progress: &Progress) {
    let root = write(tree).root_path.clone();
    // En la raíz de una unidad NTFS, con permisos de administrador, se lee la MFT de una vez.
    match mft::try_scan(&root, progress) {
        Ok(t) => {
            progress.files.store(t.dirs[0].files, Ordering::Relaxed);
            progress.dirs.store(t.dirs[0].dirs, Ordering::Relaxed);
            progress.fast.store(true, Ordering::Relaxed);
            *write(tree) = t;
            return;
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            write(tree).finish(Tree::ROOT);
            return;
        }
        Err(_) => {
            progress.files.store(0, Ordering::Relaxed);
            progress.dirs.store(0, Ordering::Relaxed);
            progress.bytes.store(0, Ordering::Relaxed);
        }
    }
    let vol = Volume::of(&root);
    walk(tree, Tree::ROOT, &root, Context::default(), &vol, progress);
    write(tree).finish(Tree::ROOT);
}

/// Vuelve a leer del disco solo la carpeta `id` y actualiza los totales de las carpetas superiores.
/// Mientras tanto el árbol se puede seguir consultando. La carpeta conserva su id.
/// Devuelve `None` si ya no existe; en ese caso se quita del árbol.
pub fn rescan(tree: &RwLock<Tree>, id: NodeId, progress: &Progress) -> Option<NodeId> {
    let (path, ctx) = write(tree).begin_rescan(id)?;
    let vol = Volume::of(&path);
    walk(tree, id, &path, ctx, &vol, progress);
    write(tree).finish(id);
    Some(id)
}

fn write(tree: &RwLock<Tree>) -> std::sync::RwLockWriteGuard<'_, Tree> {
    tree.write().unwrap_or_else(|e| e.into_inner())
}

fn walk(tree: &RwLock<Tree>, id: NodeId, path: &Path, ctx: Context, vol: &Volume, progress: &Progress) {
    progress.dirs.fetch_add(1, Ordering::Relaxed);
    if progress.is_cancelled() {
        return;
    }
    let raw = match disk::list(path, vol) {
        Ok(r) => r,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    for r in raw {
        if r.is_dir {
            subdirs.push(r.name);
            continue;
        }
        let stat = disk::stat(&path.join(&r.name), &r, vol);
        files.push(NewFile { category: ctx.categorize(&extension(&r.name)), modified: r.modified, file_id: r.file_id, stat, name: r.name });
    }
    progress.files.fetch_add(files.len() as u64, Ordering::Relaxed);

    // Un solo bloqueo por carpeta: se agregan sus archivos y subcarpetas y se suman a las de arriba.
    let next: Vec<(NodeId, PathBuf, Context)> = {
        let mut t = write(tree);
        // La carpeta se borró o se volvió a escanear mientras tanto: lo leído ya no le corresponde.
        if !t.attached(id) {
            return;
        }
        let counted = t.add_files(id, files);
        progress.bytes.fetch_add(counted, Ordering::Relaxed);
        let ids = t.add_dirs(id, subdirs.iter().map(String::as_str));
        ids.into_iter().zip(subdirs).map(|(cid, name)| (cid, path.join(&name), ctx.enter(&name))).collect()
    };
    next.into_par_iter().for_each(|(cid, p, c)| walk(tree, cid, &p, c, vol, progress));
}

/// Vista de un nodo lista para enviar a la interfaz.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub id: NodeId,
    /// Carpeta que lo contiene; `None` en la raíz.
    pub parent: Option<NodeId>,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// Espacio real en disco.
    pub size: u64,
    /// Tamaño que muestra el Explorador.
    pub logical: u64,
    /// Bytes que están solo en la nube.
    pub cloud: u64,
    pub cloud_files: u64,
    /// Archivo que no está descargado: está en la nube y no ocupa espacio local.
    pub cloud_only: bool,
    /// Otro nombre de un archivo cuyo espacio ya se contó en otra carpeta (enlace duro).
    pub hardlink: bool,
    pub files: u64,
    pub dirs: u64,
    pub modified: i64,
    pub ext: String,
    pub category: Category,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySize {
    pub category: Category,
    pub size: u64,
}

impl Tree {
    pub const ROOT: NodeId = 0;

    /// Árbol vacío listo para que `scan_into` lo llene.
    pub fn new(root: &Path) -> Tree {
        let mut t = Tree {
            root_path: root.to_path_buf(),
            nodes: Vec::new(),
            dirs: Vec::new(),
            names: String::new(),
            live: true,
            seen: HashMap::new(),
            links: HashMap::new(),
            scanned_at: 0,
        };
        t.add_dir(NO_PARENT, &root.to_string_lossy());
        t
    }

    /// Todavía hay un escaneo escribiendo en el árbol.
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Cuándo terminó el último análisis completo, en segundos Unix (0 si todavía no terminó).
    pub fn scanned_at(&self) -> i64 {
        self.scanned_at
    }

    /// Elementos que siguen en el árbol (sin contar lo que se borró o se reemplazó al refrescar).
    pub fn attached_len(&self) -> usize {
        let d = &self.dirs[0];
        (1 + d.files + d.dirs) as usize
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Bytes de memoria que ocupa el árbol, aproximadamente.
    pub fn memory(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.dirs.capacity() * std::mem::size_of::<Dir>()
            + self.dirs.iter().map(|d| d.children.capacity() * 4).sum::<usize>()
            + self.names.capacity()
    }

    /// El elemento existe y sigue en el árbol (no se borró ni se reemplazó al refrescar).
    pub fn attached(&self, id: NodeId) -> bool {
        id == Self::ROOT || self.nodes.get(id as usize).is_some_and(|n| n.parent != NO_PARENT)
    }

    pub fn name(&self, id: NodeId) -> &str {
        let n = &self.nodes[id as usize];
        &self.names[n.name as usize..n.name as usize + n.name_len as usize]
    }

    pub fn is_dir(&self, id: NodeId) -> bool {
        self.nodes.get(id as usize).is_some_and(|n| n.flags & F_DIR != 0)
    }

    pub fn size(&self, id: NodeId) -> u64 {
        self.nodes.get(id as usize).map_or(0, |n| n.size)
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(id as usize).map(|n| n.parent).filter(|p| *p != NO_PARENT)
    }

    /// Hijos directos de una carpeta (vacío para archivos).
    pub fn kids(&self, id: NodeId) -> &[NodeId] {
        match self.nodes.get(id as usize) {
            Some(n) if n.dir != NO_DIR => &self.dirs[n.dir as usize].children,
            _ => &[],
        }
    }

    fn dir_mut(&mut self, id: NodeId) -> &mut Dir {
        let d = self.nodes[id as usize].dir;
        &mut self.dirs[d as usize]
    }

    fn push_name(&mut self, name: &str) -> (u32, u16) {
        // Los nombres de Windows tienen como máximo 255 caracteres UTF-16 (765 bytes en UTF-8).
        let mut end = name.len().min(u16::MAX as usize);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let off = self.names.len() as u32;
        self.names.push_str(&name[..end]);
        (off, end as u16)
    }

    /// Agrega una carpeta vacía dentro de `parent` y devuelve su id.
    pub fn add_dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        let id = self.push_dir(parent, name);
        if parent != NO_PARENT {
            self.apply(parent, &Totals { dirs: 1, ..Totals::default() }, true, false);
        }
        id
    }

    /// Agrega varias carpetas vacías dentro de `parent` con una sola actualización de los totales.
    pub fn add_dirs<'a>(&mut self, parent: NodeId, names: impl IntoIterator<Item = &'a str>) -> Vec<NodeId> {
        let ids: Vec<NodeId> = names.into_iter().map(|n| self.push_dir(parent, n)).collect();
        if !ids.is_empty() {
            self.apply(parent, &Totals { dirs: ids.len() as u64, ..Totals::default() }, true, false);
        }
        ids
    }

    fn push_dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        let id = self.nodes.len() as NodeId;
        let (off, len) = self.push_name(name);
        self.dirs.push(Dir::default());
        self.nodes.push(Node {
            name: off,
            name_len: len,
            flags: F_DIR,
            category: Category::Other,
            parent,
            dir: (self.dirs.len() - 1) as u32,
            size: 0,
            logical: 0,
            modified: 0,
        });
        if parent != NO_PARENT {
            self.dir_mut(parent).children.push(id);
        }
        id
    }

    /// Agrega archivos a la carpeta `parent` y suma su tamaño a ella y a las de arriba.
    /// Los archivos que ya se vieron con otro nombre (enlaces duros) no suman espacio.
    /// Devuelve los bytes en disco que se sumaron.
    pub fn add_files(&mut self, parent: NodeId, files: Vec<NewFile>) -> u64 {
        let mut added = Totals::default();
        for f in files {
            let id = self.nodes.len() as NodeId;
            let mut flags = 0;
            if let Some(fid) = f.file_id {
                match self.seen.get(&fid) {
                    Some(first) => {
                        self.links.insert(fid, *first);
                        flags |= F_LINK;
                    }
                    None => {
                        self.seen.insert(fid, id);
                    }
                }
            }
            let (size, logical) = if flags & F_LINK != 0 { (0, f.stat.logical) } else { (f.stat.allocated, f.stat.logical) };
            if f.stat.cloud && flags & F_LINK == 0 {
                flags |= F_CLOUD;
            }
            let (off, len) = self.push_name(&f.name);
            let node = Node { name: off, name_len: len, flags, category: f.category, parent, dir: NO_DIR, size, logical, modified: f.modified };
            added.add(&node);
            self.nodes.push(node);
            self.dir_mut(parent).children.push(id);
        }
        let bytes = added.size;
        self.apply(parent, &added, true, false);
        bytes
    }

    /// Ruta completa en disco de un nodo.
    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != Self::ROOT && cur != NO_PARENT {
            parts.push(self.name(cur));
            cur = self.nodes[cur as usize].parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.into_iter().rev() {
            p.push(part);
        }
        p
    }

    pub fn info(&self, id: NodeId) -> Option<NodeInfo> {
        let n = self.nodes.get(id as usize)?;
        let is_dir = n.flags & F_DIR != 0;
        let (cloud, cloud_files, files, dirs) = if is_dir {
            let d = &self.dirs[n.dir as usize];
            (d.cloud, d.cloud_files, d.files, d.dirs)
        } else if n.flags & F_CLOUD != 0 {
            (n.logical.saturating_sub(n.size), 1, 1, 0)
        } else {
            (0, 0, 1, 0)
        };
        Some(NodeInfo {
            id,
            parent: (n.parent != NO_PARENT).then_some(n.parent),
            name: if id == Self::ROOT { self.root_path.to_string_lossy().into_owned() } else { self.name(id).to_string() },
            path: self.path_of(id).to_string_lossy().into_owned(),
            is_dir,
            size: n.size,
            logical: n.logical,
            cloud,
            cloud_files,
            cloud_only: n.flags & F_CLOUD != 0,
            hardlink: n.flags & F_LINK != 0,
            files,
            dirs,
            modified: n.modified,
            ext: if is_dir { String::new() } else { extension(self.name(id)) },
            category: n.category,
            has_children: self.kids(id).iter().any(|c| self.is_dir(*c)),
        })
    }

    /// Hijos de una carpeta, de mayor a menor, paginados.
    pub fn children(&self, id: NodeId, offset: usize, limit: usize) -> Vec<NodeInfo> {
        let kids = self.kids(id);
        if !self.live {
            return kids.iter().skip(offset).take(limit).filter_map(|c| self.info(*c)).collect();
        }
        // Durante el escaneo los tamaños cambian a cada momento: se ordena al consultar.
        let mut kids = kids.to_vec();
        kids.sort_by(|a, b| self.order(*a, *b));
        kids.into_iter().skip(offset).take(limit).filter_map(|c| self.info(c)).collect()
    }

    /// Cadena de ancestros desde la raíz hasta `id`, incluidos ambos.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeInfo> {
        let mut out = Vec::new();
        let mut cur = id;
        while cur != NO_PARENT {
            match self.info(cur) {
                Some(i) => out.push(i),
                None => break,
            }
            cur = self.nodes[cur as usize].parent;
        }
        out.reverse();
        out
    }

    pub fn categories(&self, id: NodeId) -> Vec<CategorySize> {
        let Some(n) = self.nodes.get(id as usize) else { return Vec::new() };
        let mut v: Vec<CategorySize> = if n.dir != NO_DIR {
            let b = &self.dirs[n.dir as usize].cats;
            Category::ALL
                .into_iter()
                .map(|c| CategorySize { category: c, size: b[c.index()] })
                .filter(|c| c.size > 0)
                .collect()
        } else {
            vec![CategorySize { category: n.category, size: n.size }]
        };
        v.sort_by(|a, b| b.size.cmp(&a.size));
        v
    }

    /// Los `n` archivos más grandes bajo `id`. Usa un montículo de tamaño `n`, así no reserva
    /// memoria para millones de archivos.
    pub fn largest_files(&self, id: NodeId, n: usize) -> Vec<NodeInfo> {
        if n == 0 {
            return Vec::new();
        }
        let mut heap: BinaryHeap<std::cmp::Reverse<(u64, NodeId)>> = BinaryHeap::with_capacity(n + 1);
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if self.is_dir(cur) {
                stack.extend(self.kids(cur));
                continue;
            }
            let s = self.size(cur);
            if heap.len() < n {
                heap.push(std::cmp::Reverse((s, cur)));
            } else if heap.peek().is_some_and(|m| m.0 .0 < s) {
                heap.pop();
                heap.push(std::cmp::Reverse((s, cur)));
            }
        }
        let mut v: Vec<(u64, NodeId)> = heap.into_iter().map(|r| r.0).collect();
        v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        v.into_iter().filter_map(|(_, i)| self.info(i)).collect()
    }

    /// Busca por nombre (sin distinguir mayúsculas). Si la consulta empieza por un punto, busca por extensión.
    pub fn search(&self, query: &str, limit: usize) -> Vec<NodeInfo> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let by_ext = q.strip_prefix('.').map(str::to_string);
        let mut hits: Vec<(u64, NodeId)> = (1..self.nodes.len() as NodeId)
            .into_par_iter()
            .filter(|i| self.nodes[*i as usize].parent != NO_PARENT)
            .filter(|i| match &by_ext {
                Some(ext) => !self.is_dir(*i) && extension(self.name(*i)) == *ext,
                None => self.name(*i).to_lowercase().contains(&q),
            })
            .map(|i| (self.size(i), i))
            .collect();
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().take(limit).filter_map(|(_, i)| self.info(i)).collect()
    }

    /// Busca el nodo de una ruta relativa a la raíz, comparando sin distinguir mayúsculas como hace Windows.
    pub fn find(&self, relative: &Path) -> Option<NodeId> {
        let mut cur = Self::ROOT;
        for comp in relative.components() {
            let name = comp.as_os_str().to_string_lossy();
            cur = *self.kids(cur).iter().find(|c| self.name(**c).eq_ignore_ascii_case(&name))?;
        }
        Some(cur)
    }

    /// Quita un nodo del árbol tras borrarlo del disco y descuenta su tamaño de las carpetas superiores.
    pub fn remove(&mut self, id: NodeId) -> Option<u64> {
        if id == Self::ROOT || !self.attached(id) || id as usize >= self.nodes.len() {
            return None;
        }
        let parent = self.nodes[id as usize].parent;
        let size = self.nodes[id as usize].size;
        let totals = self.totals_of(id);
        self.dir_mut(parent).children.retain(|c| *c != id);
        let resort = !self.live;
        self.apply(parent, &totals, false, resort);
        self.detach(id);
        Some(size)
    }

    /// Prepara el árbol para volver a leer la carpeta `id`: descuenta lo que tenía y la deja vacía.
    /// Devuelve la ruta y el contexto con que hay que recorrerla, o `None` si ya no existe.
    fn begin_rescan(&mut self, id: NodeId) -> Option<(PathBuf, Context)> {
        if !self.is_dir(id) || !self.attached(id) {
            return None;
        }
        if id == Self::ROOT {
            *self = Tree::new(&self.root_path.clone());
            return Some((self.root_path.clone(), Context::default()));
        }
        let path = self.path_of(id);
        if !path.is_dir() {
            self.remove(id);
            return None;
        }
        // Las carpetas de caché y de código se reconocen por sus ancestros, igual que en el escaneo completo.
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != Self::ROOT {
            chain.push(cur);
            cur = self.nodes[cur as usize].parent;
        }
        let ctx = chain.iter().rev().fold(Context::default(), |c, n| c.enter(self.name(*n)));

        let parent = self.nodes[id as usize].parent;
        let mut old = self.totals_of(id);
        old.dirs -= 1; // La carpeta en sí sigue existiendo.
        self.apply(parent, &old, false, false);
        for c in std::mem::take(&mut self.dir_mut(id).children) {
            self.detach(c);
        }
        let n = &mut self.nodes[id as usize];
        (n.size, n.logical, n.modified, n.category) = (0, 0, 0, Category::Other);
        *self.dir_mut(id) = Dir::default();

        // Los enlaces duros cuyo espacio se contó fuera de esta carpeta siguen contados allí.
        let outside: Vec<(u64, NodeId)> =
            self.links.iter().filter(|(_, p)| self.attached(**p) && !self.is_inside(**p, id)).map(|(k, v)| (*k, *v)).collect();
        self.links = outside.iter().copied().collect();
        self.seen = outside.into_iter().collect();
        self.live = true;
        Some((path, ctx))
    }

    /// `id` está dentro de la carpeta `ancestor` (a cualquier profundidad).
    fn is_inside(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        while id != NO_PARENT {
            if id == ancestor {
                return true;
            }
            id = self.nodes[id as usize].parent;
        }
        false
    }

    /// Ordena de mayor a menor los hijos de `id` (a cualquier profundidad) y de sus carpetas superiores.
    fn finish(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            self.sort_children(cur);
            stack.extend(self.kids(cur).iter().filter(|c| self.is_dir(**c)));
        }
        let mut cur = self.nodes[id as usize].parent;
        while cur != NO_PARENT {
            self.sort_children(cur);
            cur = self.nodes[cur as usize].parent;
        }
        self.seen = HashMap::new();
        self.live = false;
        if id == Self::ROOT {
            self.scanned_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
        }
    }

    fn sort_children(&mut self, id: NodeId) {
        if !self.is_dir(id) {
            return;
        }
        let mut kids = std::mem::take(&mut self.dir_mut(id).children);
        kids.sort_by(|x, y| self.order(*x, *y));
        kids.shrink_to_fit();
        self.dir_mut(id).children = kids;
    }

    /// Orden de los hijos: de mayor a menor; a igual tamaño, carpetas primero y luego por nombre.
    fn order(&self, a: NodeId, b: NodeId) -> std::cmp::Ordering {
        self.order_in(a, b, Storage::All)
    }

    /// Como `order`, pero con el tamaño que cuenta para el filtro `s`.
    fn order_in(&self, a: NodeId, b: NodeId, s: Storage) -> std::cmp::Ordering {
        let (na, nb) = (&self.nodes[a as usize], &self.nodes[b as usize]);
        self.amount(b, s)
            .cmp(&self.amount(a, s))
            .then((nb.flags & F_DIR).cmp(&(na.flags & F_DIR)))
            .then_with(|| self.name(a).cmp(self.name(b)))
    }

    /// Bytes de `id` que cuenta el filtro `s`: lo que ocupa en el disco o, con `Cloud`, lo que está
    /// solo en la nube.
    pub fn amount(&self, id: NodeId, s: Storage) -> u64 {
        let Some(n) = self.nodes.get(id as usize) else { return 0 };
        match s {
            Storage::Cloud if n.dir != NO_DIR => self.dirs[n.dir as usize].cloud,
            Storage::Cloud if n.flags & F_CLOUD != 0 => n.logical.saturating_sub(n.size),
            Storage::Cloud => 0,
            _ => n.size,
        }
    }

    /// `id` aparece con el filtro `s`.
    pub fn shows(&self, id: NodeId, s: Storage) -> bool {
        let Some(n) = self.nodes.get(id as usize) else { return false };
        let cloud = if n.dir != NO_DIR { self.dirs[n.dir as usize].cloud_files > 0 } else { n.flags & F_CLOUD != 0 };
        match s {
            Storage::All => true,
            // Fuera lo que está en la nube y no ocupa nada en este PC. Una carpeta vacía sigue apareciendo.
            Storage::Local => n.size > 0 || !cloud,
            Storage::Cloud => cloud,
        }
    }

    /// Como `info`, pero con el tamaño que cuenta para el filtro `s`.
    pub fn info_in(&self, id: NodeId, s: Storage) -> Option<NodeInfo> {
        let mut i = self.info(id)?;
        i.size = self.amount(id, s);
        Some(i)
    }

    /// Hijos de una carpeta que aparecen con el filtro `s`, de mayor a menor, paginados.
    pub fn children_in(&self, id: NodeId, s: Storage, offset: usize, limit: usize) -> Vec<NodeInfo> {
        if s == Storage::All {
            return self.children(id, offset, limit);
        }
        let mut kids: Vec<NodeId> = self.kids(id).iter().copied().filter(|c| self.shows(*c, s)).collect();
        // Los hijos ya están ordenados por espacio en disco, salvo mientras se escanea.
        if s == Storage::Cloud || self.live {
            kids.sort_by(|a, b| self.order_in(*a, *b, s));
        }
        kids.into_iter().skip(offset).take(limit).filter_map(|c| self.info_in(c, s)).collect()
    }

    /// Cuántos hijos de una carpeta aparecen con el filtro `s`.
    pub fn count_children(&self, id: NodeId, s: Storage) -> usize {
        let kids = self.kids(id);
        if s == Storage::All {
            kids.len()
        } else {
            kids.iter().filter(|c| self.shows(**c, s)).count()
        }
    }

    /// Como `ancestors`, con el tamaño que cuenta para el filtro `s`.
    pub fn ancestors_in(&self, id: NodeId, s: Storage) -> Vec<NodeInfo> {
        let mut v = self.ancestors(id);
        for i in &mut v {
            i.size = self.amount(i.id, s);
        }
        v
    }

    /// Como `categories`, con el tamaño que cuenta para el filtro `s`. Lo que está en la nube no se
    /// lleva por tipo durante el escaneo, así que se suma recorriendo la carpeta.
    pub fn categories_in(&self, id: NodeId, s: Storage) -> Vec<CategorySize> {
        if s != Storage::Cloud {
            return self.categories(id);
        }
        let mut cats = [0u64; 9];
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let Some(n) = self.nodes.get(cur as usize) else { continue };
            if n.dir != NO_DIR {
                if self.dirs[n.dir as usize].cloud_files > 0 {
                    stack.extend(self.kids(cur));
                }
            } else if n.flags & F_CLOUD != 0 {
                cats[n.category.index()] += n.logical.saturating_sub(n.size);
            }
        }
        let mut v: Vec<CategorySize> =
            Category::ALL.into_iter().map(|c| CategorySize { category: c, size: cats[c.index()] }).filter(|c| c.size > 0).collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.size));
        v
    }

    /// Suma o resta unos totales en `start` y en todas sus carpetas superiores.
    fn apply(&mut self, start: NodeId, t: &Totals, add: bool, resort: bool) {
        let f = |a: &mut u64, v: u64| *a = if add { *a + v } else { a.saturating_sub(v) };
        let mut cur = start;
        while cur != NO_PARENT {
            let n = &mut self.nodes[cur as usize];
            f(&mut n.size, t.size);
            f(&mut n.logical, t.logical);
            if add {
                n.modified = n.modified.max(t.modified);
            }
            let (next, dir) = (n.parent, n.dir);
            let d = &mut self.dirs[dir as usize];
            f(&mut d.cloud, t.cloud);
            f(&mut d.cloud_files, t.cloud_files);
            f(&mut d.files, t.files);
            f(&mut d.dirs, t.dirs);
            for (acc, v) in d.cats.iter_mut().zip(t.cats.iter()) {
                f(acc, *v);
            }
            let cats = d.cats;
            self.nodes[cur as usize].category =
                Category::ALL.into_iter().max_by_key(|c| cats[c.index()]).filter(|c| cats[c.index()] > 0).unwrap_or(Category::Other);
            if resort {
                self.sort_children(cur);
            }
            cur = next;
        }
    }

    /// Marca un nodo y todo su contenido como fuera del árbol, para que la búsqueda no los encuentre.
    fn detach(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = &mut self.nodes[cur as usize];
            n.parent = NO_PARENT;
            if n.dir != NO_DIR {
                let d = n.dir as usize;
                stack.extend(std::mem::take(&mut self.dirs[d].children));
            }
        }
    }

    /// Lo que un nodo aporta a los totales de sus carpetas superiores.
    fn totals_of(&self, id: NodeId) -> Totals {
        let n = &self.nodes[id as usize];
        if n.dir != NO_DIR {
            let d = &self.dirs[n.dir as usize];
            return Totals {
                size: n.size,
                logical: n.logical,
                cloud: d.cloud,
                cloud_files: d.cloud_files,
                files: d.files,
                dirs: 1 + d.dirs,
                modified: n.modified,
                cats: d.cats,
            };
        }
        let mut t = Totals::default();
        t.add(n);
        t
    }
}

/// Lo que un nodo aporta a los totales de sus carpetas superiores.
#[derive(Default)]
struct Totals {
    size: u64,
    logical: u64,
    cloud: u64,
    cloud_files: u64,
    files: u64,
    dirs: u64,
    modified: i64,
    cats: [u64; 9],
}

impl Totals {
    /// Suma un archivo.
    fn add(&mut self, f: &Node) {
        self.size += f.size;
        self.logical += f.logical;
        if f.flags & F_CLOUD != 0 {
            self.cloud += f.logical.saturating_sub(f.size);
            self.cloud_files += 1;
        }
        self.files += 1;
        self.modified = self.modified.max(f.modified);
        self.cats[f.category.index()] += f.size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![0u8; bytes]).unwrap();
    }

    fn re(t: &mut Tree, id: NodeId) -> Option<NodeId> {
        let lock = RwLock::new(std::mem::replace(t, Tree::new(Path::new(""))));
        let r = rescan(&lock, id, &Progress::default());
        *t = lock.into_inner().unwrap();
        r
    }

    fn sample() -> (tempfile::TempDir, Tree) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("Videos/vacaciones.mp4"), 50_000);
        write(&r.join("Videos/clip.MKV"), 30_000);
        write(&r.join("Descargas/win11.iso"), 40_000);
        write(&r.join("Descargas/notas.txt"), 10);
        write(&r.join("AppData/Local/Temp/a.png"), 700);
        write(&r.join("proyecto/node_modules/react/index.js"), 300);
        write(&r.join("raiz.bin"), 1);
        let progress = Progress::default();
        let tree = scan(r, &progress);
        assert_eq!(progress.files.load(Ordering::Relaxed), 7);
        (dir, tree)
    }

    #[test]
    fn sums_sizes_and_counts() {
        let (_d, t) = sample();
        let root = t.info(Tree::ROOT).unwrap();
        assert_eq!(root.logical, 50_000 + 30_000 + 40_000 + 10 + 700 + 300 + 1);
        // Cada archivo ocupa al menos un bloque completo en disco.
        assert!(root.size >= root.logical);
        assert_eq!(root.files, 7);
        assert_eq!(root.dirs, 8);
        let kids = t.children(Tree::ROOT, 0, 100);
        let names: Vec<_> = kids.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, ["Videos", "Descargas", "AppData", "proyecto", "raiz.bin"]);
        assert_eq!(kids[0].logical, 80_000);
        assert_eq!(kids[0].category, Category::Video);
    }

    #[test]
    fn categories_and_extensions() {
        let (_d, t) = sample();
        let temp = t.find(Path::new("AppData/Local/Temp/a.png")).unwrap();
        assert_eq!(t.info(temp).unwrap().category, Category::Cache);
        assert_eq!(t.info(temp).unwrap().ext, "png");
        let js = t.find(Path::new("proyecto/node_modules/react/index.js")).unwrap();
        assert_eq!(t.info(js).unwrap().category, Category::Code);
        let cats = t.categories(Tree::ROOT);
        assert_eq!(cats[0].category, Category::Video);
        assert_eq!(cats[0].size, t.info(t.find(Path::new("Videos")).unwrap()).unwrap().size);
    }

    #[test]
    fn find_is_case_insensitive_and_paths_roundtrip() {
        let (d, t) = sample();
        let id = t.find(Path::new("videos/CLIP.mkv")).unwrap();
        assert_eq!(t.path_of(id), d.path().join("Videos").join("clip.MKV"));
        let chain: Vec<_> = t.ancestors(id).into_iter().map(|n| n.size).collect();
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn largest_and_search() {
        let (_d, t) = sample();
        let top = t.largest_files(Tree::ROOT, 2);
        assert_eq!(top[0].name, "vacaciones.mp4");
        assert_eq!(top[1].name, "win11.iso");
        assert_eq!(t.search(".iso", 10).len(), 1);
        assert_eq!(t.search("VACA", 10)[0].name, "vacaciones.mp4");
    }

    #[test]
    fn sparse_files_count_disk_space_not_apparent_size() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("lleno.bin"), 1 << 20);
        // Un archivo disperso aparenta 64 MB pero casi no ocupa espacio en disco.
        let sparse_path = dir.path().join("disperso.bin");
        fs::File::create(&sparse_path).unwrap();
        // En Windows un archivo solo es disperso si se marca antes de agrandarlo.
        #[cfg(windows)]
        assert!(std::process::Command::new("fsutil").args(["sparse", "setflag"]).arg(&sparse_path).status().unwrap().success());
        fs::OpenOptions::new().write(true).open(&sparse_path).unwrap().set_len(64 << 20).unwrap();
        let t = scan(dir.path(), &Progress::default());
        let sparse = t.info(t.find(Path::new("disperso.bin")).unwrap()).unwrap();
        let full = t.info(t.find(Path::new("lleno.bin")).unwrap()).unwrap();
        assert_eq!(sparse.logical, 64 << 20);
        assert!(sparse.size < 1 << 20, "ocupa {}", sparse.size);
        assert!(full.size >= 1 << 20);
        assert!(!sparse.cloud_only);
        // Ordenado por espacio real: el archivo lleno va primero aunque aparente pesar menos.
        assert_eq!(t.children(Tree::ROOT, 0, 2)[0].name, "lleno.bin");
    }

    #[test]
    fn remove_updates_ancestors() {
        let (_d, mut t) = sample();
        let before = t.info(Tree::ROOT).unwrap();
        let id = t.find(Path::new("Descargas")).unwrap();
        let dl = t.info(id).unwrap();
        assert_eq!(t.remove(id), Some(dl.size));
        let root = t.info(Tree::ROOT).unwrap();
        assert_eq!(root.size, before.size - dl.size);
        assert_eq!(root.logical, before.logical - 40_010);
        assert_eq!(root.files, 5);
        assert!(t.find(Path::new("Descargas")).is_none());
        assert!(t.categories(Tree::ROOT).iter().all(|c| c.category != Category::Archive));
    }

    #[test]
    fn rescan_updates_only_that_folder_and_its_ancestors() {
        let (d, mut t) = sample();
        let before = t.info(Tree::ROOT).unwrap();
        let videos = t.find(Path::new("Videos")).unwrap();
        let old_videos = t.info(videos).unwrap();
        fs::remove_file(d.path().join("Videos/clip.MKV")).unwrap();
        write(&d.path().join("Videos/nuevos/fiesta.mp4"), 200_000);
        // Un cambio fuera de la carpeta no se ve hasta refrescarla a ella.
        write(&d.path().join("Descargas/otro.zip"), 5_000);

        assert_eq!(re(&mut t, videos), Some(videos));
        let v = t.info(videos).unwrap();
        assert_eq!(v.logical, 50_000 + 200_000);
        assert_eq!((v.files, v.dirs), (2, 1));
        let root = t.info(Tree::ROOT).unwrap();
        assert_eq!(root.logical, before.logical - 30_000 + 200_000);
        assert_eq!(root.size, before.size - old_videos.size + v.size);
        assert_eq!((root.files, root.dirs), (before.files + 1 - 1, before.dirs + 1));
        assert_eq!(t.find(Path::new("Videos/nuevos/fiesta.mp4")).map(|i| t.path_of(i)), Some(d.path().join("Videos/nuevos/fiesta.mp4")));
        assert!(t.find(Path::new("Descargas/otro.zip")).is_none());
        assert!(t.search("clip", 10).is_empty());
        assert_eq!(t.search("fiesta", 10).len(), 1);
        assert_eq!(t.children(Tree::ROOT, 0, 1)[0].id, videos);
        assert_eq!(t.children(videos, 0, 1)[0].name, "nuevos");
    }

    #[test]
    fn rescan_keeps_cache_context_and_handles_deleted_folders() {
        let (d, mut t) = sample();
        let local = t.find(Path::new("AppData/Local")).unwrap();
        write(&d.path().join("AppData/Local/Temp/b.png"), 900);
        re(&mut t, local).unwrap();
        let png = t.find(Path::new("AppData/Local/Temp/b.png")).unwrap();
        assert_eq!(t.info(png).unwrap().category, Category::Cache);

        let before = t.info(Tree::ROOT).unwrap();
        let dl = t.find(Path::new("Descargas")).unwrap();
        let dl_size = t.info(dl).unwrap().size;
        fs::remove_dir_all(d.path().join("Descargas")).unwrap();
        assert_eq!(re(&mut t, dl), None);
        assert_eq!(t.info(Tree::ROOT).unwrap().size, before.size - dl_size);
        assert!(t.search("win11", 10).is_empty());
    }

    #[test]
    fn can_browse_while_scanning() {
        let dir = tempfile::tempdir().unwrap();
        for a in 0..40 {
            for b in 0..10 {
                write(&dir.path().join(format!("c{a}/d{b}/f.bin")), 1000 + a * 10 + b);
            }
        }
        let tree = std::sync::Arc::new(RwLock::new(Tree::new(dir.path())));
        let worker = {
            let tree = tree.clone();
            std::thread::spawn(move || scan_into(&tree, &Progress::default()))
        };
        let mut last = 0;
        while !worker.is_finished() {
            let t = tree.read().unwrap();
            let root = t.info(Tree::ROOT).unwrap();
            assert!(root.logical >= last, "el total nunca baja mientras se escanea");
            last = root.logical;
            let kids = t.children(Tree::ROOT, 0, 100);
            assert!(kids.windows(2).all(|w| w[0].size >= w[1].size), "ordenados aunque el escaneo siga");
            if let Some(k) = kids.first() {
                t.children(k.id, 0, 100);
            }
        }
        worker.join().unwrap();
        let t = tree.read().unwrap();
        assert!(!t.is_live());
        let root = t.info(Tree::ROOT).unwrap();
        assert_eq!((root.files, root.dirs), (400, 440));
        assert_eq!(root.logical, (0..40).flat_map(|a| (0..10).map(move |b| 1000 + a * 10 + b)).sum::<u64>());
    }

    #[test]
    fn hardlinks_count_once() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("a/grande.bin"), 100_000);
        fs::create_dir_all(r.join("b")).unwrap();
        fs::hard_link(r.join("a/grande.bin"), r.join("b/otro-nombre.bin")).unwrap();
        write(&r.join("b/propio.bin"), 1_000);
        let mut t = scan(r, &Progress::default());
        let root = t.info(Tree::ROOT).unwrap();
        // El archivo de 100 KB tiene dos nombres pero ocupa espacio una sola vez.
        assert!(root.size >= 101_000 && root.size < 150_000, "ocupa {}", root.size);
        assert_eq!(root.files, 3);
        let a = t.find(Path::new("a/grande.bin")).unwrap();
        let b = t.find(Path::new("b/otro-nombre.bin")).unwrap();
        let (ia, ib) = (t.info(a).unwrap(), t.info(b).unwrap());
        assert_eq!(ia.hardlink as u8 + ib.hardlink as u8, 1, "solo uno de los dos nombres queda como enlace");
        assert_eq!(ia.size + ib.size, ia.size.max(ib.size));
        assert_eq!(ia.logical, ib.logical);

        // Al refrescar la carpeta con el nombre que no suma, sigue sin sumar.
        let (linked, counted) = if ib.hardlink { ("b", "a") } else { ("a", "b") };
        let lid = t.find(Path::new(linked)).unwrap();
        re(&mut t, lid).unwrap();
        assert_eq!(t.info(Tree::ROOT).unwrap().size, root.size);
        // Y al refrescar la que sí suma, el espacio sigue contado una sola vez.
        let cid = t.find(Path::new(counted)).unwrap();
        re(&mut t, cid).unwrap();
        let total = t.info(Tree::ROOT).unwrap().size;
        assert_eq!(total, root.size);
    }

    #[test]
    fn nodes_are_compact() {
        assert_eq!(std::mem::size_of::<Node>(), 40);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (d, mut t) = sample();
        // Lo borrado no se guarda.
        t.remove(t.find(Path::new("Descargas")).unwrap());
        let file = d.path().join("guardado.awts");
        save(&t, &file).unwrap();
        assert_eq!(read_header(&file).unwrap().root, d.path());
        let l = load(&file).unwrap();
        assert_eq!(l.len(), t.attached_len());
        assert_eq!(l.scanned_at(), t.scanned_at());
        assert!(l.scanned_at() > 0);
        let (a, b) = (t.info(Tree::ROOT).unwrap(), l.info(Tree::ROOT).unwrap());
        assert_eq!((a.size, a.logical, a.files, a.dirs), (b.size, b.logical, b.files, b.dirs));
        let names = |t: &Tree| t.children(Tree::ROOT, 0, 100).into_iter().map(|k| (k.name, k.size)).collect::<Vec<_>>();
        assert_eq!(names(&t), names(&l));
        assert_eq!(l.categories(Tree::ROOT).len(), t.categories(Tree::ROOT).len());
        let clip = l.find(Path::new("Videos/clip.MKV")).unwrap();
        assert_eq!(l.path_of(clip), d.path().join("Videos").join("clip.MKV"));
        assert!(l.find(Path::new("Descargas")).is_none());
        // Un archivo dañado se rechaza sin romper nada.
        fs::write(&file, b"no es un analisis").unwrap();
        assert!(load(&file).is_err());
    }

    #[test]
    fn diff_shows_what_grew_and_what_was_deleted() {
        let (d, before) = sample();
        write(&d.path().join("Videos/nuevo.mp4"), 500_000);
        fs::remove_dir_all(d.path().join("Descargas")).unwrap();
        let after = scan(d.path(), &Progress::default());
        let c = diff(&before, &after, 1);
        assert!(c.delta() > 400_000);
        assert_eq!(c.children[0].name, "Videos");
        assert!(c.children[0].children.iter().any(|k| k.name == "nuevo.mp4" && !k.existed));
        let dl = c.children.iter().find(|k| k.name == "Descargas").unwrap();
        assert!(dl.existed && !dl.exists && dl.new == 0);
        // Lo que no cambió no aparece.
        assert!(c.children.iter().all(|k| k.name != "AppData"));
    }
    #[test]
    fn storage_filter_splits_cloud_and_local() {
        let mut t = Tree::new(Path::new("C:\\"));
        let nf = |name: &str, logical: u64, allocated: u64, cloud: bool| NewFile {
            name: name.into(),
            stat: FileStat { logical, allocated, cloud },
            modified: 1,
            category: Category::Docs,
            file_id: None,
        };
        let od = t.add_dir(Tree::ROOT, "OneDrive");
        let fotos = t.add_dir(od, "Fotos");
        t.add_files(fotos, vec![nf("a.jpg", 9_000, 0, true), nf("b.jpg", 5_000, 0, true)]);
        t.add_files(od, vec![nf("local.docx", 3_000, 3_072, false), nf("medio.mp4", 10_000, 4_096, true)]);
        let juegos = t.add_dir(Tree::ROOT, "Juegos");
        t.add_files(juegos, vec![nf("big.pak", 20_000, 20_480, false)]);
        t.finish(Tree::ROOT);

        // Todo: igual que siempre.
        assert_eq!(t.count_children(od, Storage::All), 3);
        // En el disco: la carpeta que está entera en la nube desaparece; el archivo a medio bajar se queda.
        let local: Vec<_> = t.children_in(od, Storage::Local, 0, 10).into_iter().map(|k| k.name).collect();
        assert_eq!(local, ["medio.mp4", "local.docx"]);
        assert_eq!(t.count_children(od, Storage::Local), 2);
        // En la nube: solo lo que no está descargado, ordenado y medido por lo que hay en la nube.
        let cloud: Vec<_> = t.children_in(od, Storage::Cloud, 0, 10).into_iter().map(|k| (k.name, k.size)).collect();
        assert_eq!(cloud, [("Fotos".to_string(), 14_000), ("medio.mp4".to_string(), 10_000 - 4_096)]);
        let root: Vec<_> = t.children_in(Tree::ROOT, Storage::Cloud, 0, 10).into_iter().map(|k| k.name).collect();
        assert_eq!(root, ["OneDrive"]);
        assert_eq!(t.info_in(Tree::ROOT, Storage::Cloud).unwrap().size, 14_000 + 10_000 - 4_096);
        assert_eq!(t.ancestors_in(fotos, Storage::Cloud)[0].size, 14_000 + 10_000 - 4_096);
        assert_eq!(t.categories_in(Tree::ROOT, Storage::Cloud)[0].size, 14_000 + 10_000 - 4_096);
        let sub = t.subtree(Tree::ROOT, 2, 10, Storage::Local).unwrap();
        assert_eq!(sub.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Juegos", "OneDrive"]);
        assert!(sub.children[1].children.iter().all(|c| c.name != "Fotos"));

        let files = |s| t.find_files(Tree::ROOT, &FileFilter { storage: s, ..Default::default() }, 0, 10).into_iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(files(Storage::Cloud), ["a.jpg", "medio.mp4", "b.jpg"]);
        assert_eq!(files(Storage::Local), ["big.pak", "medio.mp4", "local.docx"]);
    }

}
