//! Motor de escaneo de AppWindowsTreeSize.
//!
//! Recorre una carpeta o unidad en paralelo, suma el tamaño de cada carpeta y guarda el
//! resultado en un árbol plano (`Tree`) que la interfaz consulta por partes: nunca se envía
//! el árbol completo, solo los hijos de la carpeta que se está viendo.

mod category;
mod disk;

pub use category::{extension, Category, Context};
pub use disk::{FileStat, Volume};

use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

pub type NodeId = u32;
const NO_PARENT: NodeId = NodeId::MAX;

/// Contadores que el escaneo actualiza mientras avanza, para mostrar el progreso en vivo.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
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

/// Un archivo o carpeta dentro del árbol.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: Box<str>,
    pub parent: NodeId,
    pub is_dir: bool,
    /// Espacio que ocupa realmente en el disco. Es el tamaño con el que se ordena y se dibuja todo.
    pub size: u64,
    /// Tamaño que muestra el Explorador de Windows.
    pub logical: u64,
    /// Bytes que están solo en la nube (OneDrive y similares) y no ocupan espacio local.
    pub cloud: u64,
    /// Archivos contenidos que son marcadores de posición de la nube.
    pub cloud_files: u64,
    /// Archivos contenidos (1 para un archivo).
    pub files: u64,
    /// Subcarpetas contenidas a cualquier profundidad.
    pub dirs: u64,
    /// Última modificación en segundos Unix (para carpetas, la más reciente de su contenido).
    pub modified: i64,
    pub category: Category,
    /// Bytes por categoría, solo en carpetas.
    pub by_category: Option<Box<[u64; 9]>>,
    /// Hijos ordenados de mayor a menor tamaño.
    pub children: Vec<NodeId>,
}

/// Resultado de un escaneo.
pub struct Tree {
    root_path: PathBuf,
    entries: Vec<Entry>,
}

/// Carpeta intermedia que se construye en paralelo antes de aplanarla.
struct Tmp {
    name: String,
    is_dir: bool,
    size: u64,
    logical: u64,
    cloud: u64,
    cloud_files: u64,
    modified: i64,
    category: Category,
    children: Vec<Tmp>,
}

fn modified_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn walk(path: &Path, name: String, ctx: Context, vol: &Volume, progress: &Progress) -> Tmp {
    progress.dirs.fetch_add(1, Ordering::Relaxed);
    let mut files = Vec::new();
    let mut subdirs = Vec::new();

    if !progress.is_cancelled() {
        match fs::read_dir(path) {
            Ok(rd) => {
                for entry in rd.flatten() {
                    // `DirEntry::metadata` no sigue enlaces simbólicos ni uniones (junctions),
                    // así que no contamos dos veces carpetas como "Documents and Settings".
                    let Ok(meta) = entry.metadata() else {
                        progress.errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    };
                    let ft = meta.file_type();
                    if ft.is_symlink() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if ft.is_dir() {
                        subdirs.push((entry.path(), name));
                    } else {
                        let st = disk::stat(&entry.path(), &meta, vol);
                        progress.files.fetch_add(1, Ordering::Relaxed);
                        progress.bytes.fetch_add(st.allocated, Ordering::Relaxed);
                        let ext = extension(&name);
                        files.push(Tmp {
                            category: ctx.categorize(&ext),
                            name,
                            is_dir: false,
                            size: st.allocated,
                            logical: st.logical,
                            cloud: if st.cloud { st.logical.saturating_sub(st.allocated) } else { 0 },
                            cloud_files: st.cloud as u64,
                            modified: modified_secs(&meta),
                            children: Vec::new(),
                        });
                    }
                }
            }
            Err(_) => {
                progress.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    let mut children: Vec<Tmp> = subdirs
        .into_par_iter()
        .map(|(p, n)| {
            let child_ctx = ctx.enter(&n);
            walk(&p, n, child_ctx, vol, progress)
        })
        .collect();
    children.append(&mut files);

    let size = children.iter().map(|c| c.size).sum();
    let logical = children.iter().map(|c| c.logical).sum();
    let cloud = children.iter().map(|c| c.cloud).sum();
    let cloud_files = children.iter().map(|c| c.cloud_files).sum();
    let modified = children.iter().map(|c| c.modified).max().unwrap_or(0);
    Tmp { name, is_dir: true, size, logical, cloud, cloud_files, modified, category: Category::Other, children }
}

/// Escanea `root` y devuelve el árbol completo. Si se cancela, devuelve lo leído hasta ese momento.
pub fn scan(root: &Path, progress: &Progress) -> Tree {
    let root_name = root.to_string_lossy().into_owned();
    let vol = Volume::of(root);
    let tmp = walk(root, root_name, Context::default(), &vol, progress);
    let mut entries = Vec::with_capacity(
        (progress.files.load(Ordering::Relaxed) + progress.dirs.load(Ordering::Relaxed)) as usize,
    );
    flatten(tmp, NO_PARENT, &mut entries);
    Tree { root_path: root.to_path_buf(), entries }
}

/// Copia el árbol temporal a un vector plano y calcula los totales de cada carpeta.
fn flatten(tmp: Tmp, parent: NodeId, out: &mut Vec<Entry>) -> NodeId {
    let id = out.len() as NodeId;
    out.push(Entry {
        name: tmp.name.into_boxed_str(),
        parent,
        is_dir: tmp.is_dir,
        size: tmp.size,
        logical: tmp.logical,
        cloud: tmp.cloud,
        cloud_files: tmp.cloud_files,
        files: if tmp.is_dir { 0 } else { 1 },
        dirs: 0,
        modified: tmp.modified,
        category: tmp.category,
        by_category: None,
        children: Vec::new(),
    });
    if !tmp.is_dir {
        return id;
    }

    let mut children = tmp.children;
    children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    let mut ids = Vec::with_capacity(children.len());
    let mut files = 0;
    let mut dirs = 0;
    let mut by_cat = [0u64; 9];
    for child in children {
        let cid = flatten(child, id, out);
        let c = &out[cid as usize];
        files += c.files;
        if c.is_dir {
            dirs += 1 + c.dirs;
            if let Some(b) = &c.by_category {
                for (acc, v) in by_cat.iter_mut().zip(b.iter()) {
                    *acc += v;
                }
            }
        } else {
            by_cat[c.category.index()] += c.size;
        }
        ids.push(cid);
    }
    let dominant = Category::ALL
        .into_iter()
        .max_by_key(|c| by_cat[c.index()])
        .filter(|c| by_cat[c.index()] > 0)
        .unwrap_or(Category::Other);

    let e = &mut out[id as usize];
    e.files = files;
    e.dirs = dirs;
    e.category = dominant;
    e.by_category = Some(Box::new(by_cat));
    e.children = ids;
    id
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

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, id: NodeId) -> Option<&Entry> {
        self.entries.get(id as usize)
    }

    /// Ruta completa en disco de un nodo.
    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != Self::ROOT && cur != NO_PARENT {
            let e = &self.entries[cur as usize];
            parts.push(&*e.name);
            cur = e.parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.into_iter().rev() {
            p.push(part);
        }
        p
    }

    pub fn info(&self, id: NodeId) -> Option<NodeInfo> {
        let e = self.entry(id)?;
        Some(NodeInfo {
            id,
            parent: (e.parent != NO_PARENT).then_some(e.parent),
            name: if id == Self::ROOT { self.root_path.to_string_lossy().into_owned() } else { e.name.to_string() },
            path: self.path_of(id).to_string_lossy().into_owned(),
            is_dir: e.is_dir,
            size: e.size,
            logical: e.logical,
            cloud: e.cloud,
            cloud_files: e.cloud_files,
            cloud_only: !e.is_dir && e.cloud_files == 1,
            files: e.files,
            dirs: e.dirs,
            modified: e.modified,
            ext: if e.is_dir { String::new() } else { extension(&e.name) },
            category: e.category,
            has_children: e.children.iter().any(|c| self.entries[*c as usize].is_dir),
        })
    }

    /// Hijos de una carpeta, de mayor a menor, paginados.
    pub fn children(&self, id: NodeId, offset: usize, limit: usize) -> Vec<NodeInfo> {
        self.entry(id)
            .map(|e| e.children.iter().skip(offset).take(limit).filter_map(|c| self.info(*c)).collect())
            .unwrap_or_default()
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
            cur = self.entries[cur as usize].parent;
        }
        out.reverse();
        out
    }

    pub fn categories(&self, id: NodeId) -> Vec<CategorySize> {
        let Some(e) = self.entry(id) else { return Vec::new() };
        let mut v: Vec<CategorySize> = match &e.by_category {
            Some(b) => Category::ALL
                .into_iter()
                .map(|c| CategorySize { category: c, size: b[c.index()] })
                .filter(|c| c.size > 0)
                .collect(),
            None => vec![CategorySize { category: e.category, size: e.size }],
        };
        v.sort_by(|a, b| b.size.cmp(&a.size));
        v
    }

    /// Los `n` archivos más grandes bajo `id`.
    pub fn largest_files(&self, id: NodeId, n: usize) -> Vec<NodeInfo> {
        let mut stack = vec![id];
        let mut files: Vec<(u64, NodeId)> = Vec::new();
        while let Some(cur) = stack.pop() {
            let e = &self.entries[cur as usize];
            if e.is_dir {
                stack.extend(&e.children);
            } else {
                files.push((e.size, cur));
            }
        }
        files.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        files.into_iter().take(n).filter_map(|(_, i)| self.info(i)).collect()
    }

    /// Busca por nombre (sin distinguir mayúsculas). Si la consulta empieza por un punto, busca por extensión.
    pub fn search(&self, query: &str, limit: usize) -> Vec<NodeInfo> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let by_ext = q.strip_prefix('.').map(str::to_string);
        let mut hits: Vec<(u64, NodeId)> = self
            .entries
            .par_iter()
            .enumerate()
            .skip(1)
            .filter(|(_, e)| e.parent != NO_PARENT)
            .filter(|(_, e)| match &by_ext {
                Some(ext) => !e.is_dir && extension(&e.name) == *ext,
                None => e.name.to_lowercase().contains(&q),
            })
            .map(|(i, e)| (e.size, i as NodeId))
            .collect();
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().take(limit).filter_map(|(_, i)| self.info(i)).collect()
    }

    /// Busca el nodo de una ruta relativa a la raíz, comparando sin distinguir mayúsculas como hace Windows.
    pub fn find(&self, relative: &Path) -> Option<NodeId> {
        let mut cur = Self::ROOT;
        for comp in relative.components() {
            let name = comp.as_os_str().to_string_lossy();
            cur = *self.entries[cur as usize]
                .children
                .iter()
                .find(|c| self.entries[**c as usize].name.eq_ignore_ascii_case(&name))?;
        }
        Some(cur)
    }

    /// Quita un nodo del árbol tras borrarlo del disco y descuenta su tamaño de las carpetas superiores.
    pub fn remove(&mut self, id: NodeId) -> Option<u64> {
        if id == Self::ROOT {
            return None;
        }
        let e = self.entry(id)?;
        if e.parent == NO_PARENT {
            return None;
        }
        let (parent, size, totals) = (e.parent, e.size, Totals::of(e));
        self.entries[parent as usize].children.retain(|c| *c != id);
        self.apply(parent, &totals, false);
        self.detach(id);
        Some(size)
    }

    /// Vuelve a leer del disco solo la carpeta `id` y actualiza los totales de las carpetas superiores.
    /// La carpeta conserva su id. Devuelve `None` si ya no existe; en ese caso se quita del árbol.
    pub fn rescan(&mut self, id: NodeId, progress: &Progress) -> Option<NodeId> {
        let e = self.entry(id)?;
        if !e.is_dir || (id != Self::ROOT && e.parent == NO_PARENT) {
            return None;
        }
        if id == Self::ROOT {
            *self = scan(&self.root_path.clone(), progress);
            return Some(Self::ROOT);
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
            cur = self.entries[cur as usize].parent;
        }
        let ctx = chain.iter().rev().fold(Context::default(), |c, n| c.enter(&self.entries[*n as usize].name));
        let name = self.entries[id as usize].name.to_string();
        let vol = Volume::of(&self.root_path);
        let mut sub = Vec::new();
        flatten(walk(&path, name, ctx, &vol, progress), NO_PARENT, &mut sub);

        // Quita lo viejo de los totales y del árbol.
        let parent = self.entries[id as usize].parent;
        let old = Totals::of(&self.entries[id as usize]);
        self.apply(parent, &old, false);
        for c in std::mem::take(&mut self.entries[id as usize].children) {
            self.detach(c);
        }

        // Copia lo nuevo: la carpeta vuelve a su id y su contenido va al final del vector.
        let base = self.entries.len() as NodeId;
        let map = |i: NodeId| if i == 0 { id } else { base + i - 1 };
        let mut sub = sub.into_iter();
        let mut top = sub.next()?;
        top.parent = parent;
        top.children.iter_mut().for_each(|c| *c = map(*c));
        for mut e in sub {
            e.parent = map(e.parent);
            e.children.iter_mut().for_each(|c| *c = map(*c));
            self.entries.push(e);
        }
        let new = Totals::of(&top);
        self.entries[id as usize] = top;
        self.apply(parent, &new, true);
        Some(id)
    }

    /// Suma o resta los totales de un nodo en `start` y en todas sus carpetas superiores, y reordena.
    fn apply(&mut self, start: NodeId, t: &Totals, add: bool) {
        let f = |a: &mut u64, v: u64| *a = if add { *a + v } else { a.saturating_sub(v) };
        let mut cur = start;
        while cur != NO_PARENT {
            let a = &mut self.entries[cur as usize];
            f(&mut a.size, t.size);
            f(&mut a.logical, t.logical);
            f(&mut a.cloud, t.cloud);
            f(&mut a.cloud_files, t.cloud_files);
            f(&mut a.files, t.files);
            f(&mut a.dirs, t.dirs);
            if add {
                a.modified = a.modified.max(t.modified);
            }
            if let Some(b) = a.by_category.as_deref_mut() {
                for (acc, v) in b.iter_mut().zip(t.cats.iter()) {
                    f(acc, *v);
                }
                a.category = Category::ALL
                    .into_iter()
                    .max_by_key(|c| b[c.index()])
                    .filter(|c| b[c.index()] > 0)
                    .unwrap_or(Category::Other);
            }
            let mut kids = std::mem::take(&mut self.entries[cur as usize].children);
            kids.sort_by(|x, y| self.entries[*y as usize].size.cmp(&self.entries[*x as usize].size));
            let a = &mut self.entries[cur as usize];
            a.children = kids;
            cur = a.parent;
        }
    }

    /// Marca un nodo y todo su contenido como fuera del árbol, para que la búsqueda no los encuentre.
    fn detach(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let e = &mut self.entries[cur as usize];
            e.parent = NO_PARENT;
            stack.extend(std::mem::take(&mut e.children));
        }
    }
}

/// Lo que un nodo aporta a los totales de sus carpetas superiores.
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
    fn of(e: &Entry) -> Self {
        let cats = match &e.by_category {
            Some(b) => **b,
            None => {
                let mut b = [0u64; 9];
                b[e.category.index()] = e.size;
                b
            }
        };
        Totals {
            size: e.size,
            logical: e.logical,
            cloud: e.cloud,
            cloud_files: e.cloud_files,
            files: e.files,
            dirs: if e.is_dir { 1 + e.dirs } else { 0 },
            modified: e.modified,
            cats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![0u8; bytes]).unwrap();
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

        assert_eq!(t.rescan(videos, &Progress::default()), Some(videos));
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
        t.rescan(local, &Progress::default()).unwrap();
        let png = t.find(Path::new("AppData/Local/Temp/b.png")).unwrap();
        assert_eq!(t.info(png).unwrap().category, Category::Cache);

        let before = t.info(Tree::ROOT).unwrap();
        let dl = t.find(Path::new("Descargas")).unwrap();
        let dl_size = t.info(dl).unwrap().size;
        fs::remove_dir_all(d.path().join("Descargas")).unwrap();
        assert_eq!(t.rescan(dl, &Progress::default()), None);
        assert_eq!(t.info(Tree::ROOT).unwrap().size, before.size - dl_size);
        assert!(t.search("win11", 10).is_empty());
    }
}
