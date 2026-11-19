//! Herramientas sobre un árbol ya analizado: filtrar archivos, buscar duplicados y exportar informes.

use crate::{Category, NodeId, NodeInfo, Progress, Tree, F_CLOUD, F_DIR, F_LINK, NO_PARENT};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::Ordering;

/// Filtros de la vista Archivos.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FileFilter {
    /// Tamaño mínimo en disco.
    pub min_size: u64,
    /// Solo archivos sin modificar desde hace al menos estos días (0 = sin filtro).
    pub older_than_days: u32,
    pub category: Option<Category>,
    /// Extensión sin punto, en minúsculas.
    pub ext: Option<String>,
}

/// Una carpeta con sus hijos a varios niveles, para dibujar el mapa de bloques anidado y los anillos.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubNode {
    pub id: NodeId,
    pub name: String,
    pub size: u64,
    pub category: Category,
    pub is_dir: bool,
    pub children: Vec<SubNode>,
}

impl Tree {
    /// `id` y sus hijos hasta `depth` niveles: como mucho `max_children` por carpeta, y nada que
    /// ocupe menos de una milésima del total (no se vería).
    pub fn subtree(&self, id: NodeId, depth: usize, max_children: usize) -> Option<SubNode> {
        let total = self.size(id).max(1);
        self.attached(id).then(|| self.sub(id, depth, max_children, total / 1000))
    }

    fn sub(&self, id: NodeId, depth: usize, max: usize, min: u64) -> SubNode {
        let n = &self.nodes[id as usize];
        let children = if depth == 0 || n.flags & F_DIR == 0 {
            Vec::new()
        } else {
            let mut kids: Vec<NodeId> = self.kids(id).to_vec();
            if self.live {
                kids.sort_by(|a, b| self.order(*a, *b));
            }
            kids.into_iter().take(max).filter(|k| self.size(*k) > min).map(|k| self.sub(k, depth - 1, max, min)).collect()
        };
        SubNode {
            id,
            name: if id == Tree::ROOT { self.root_path.to_string_lossy().into_owned() } else { self.name(id).to_string() },
            size: n.size,
            category: n.category,
            is_dir: n.flags & F_DIR != 0,
            children,
        }
    }

    /// Los archivos más grandes bajo `id` que cumplen el filtro.
    pub fn find_files(&self, id: NodeId, f: &FileFilter, now: i64, limit: usize) -> Vec<NodeInfo> {
        let cutoff = if f.older_than_days > 0 { now - f.older_than_days as i64 * 86_400 } else { i64::MAX };
        let ext = f.ext.as_deref().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).filter(|e| !e.is_empty());
        let mut hits: Vec<(u64, NodeId)> = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = &self.nodes[cur as usize];
            if n.flags & F_DIR != 0 {
                stack.extend(self.kids(cur));
                continue;
            }
            if n.size < f.min_size || n.modified > cutoff || f.category.is_some_and(|c| c != n.category) {
                continue;
            }
            if let Some(e) = &ext {
                if crate::extension(self.name(cur)) != *e {
                    continue;
                }
            }
            hits.push((n.size, cur));
        }
        hits.sort_unstable_by_key(|h| std::cmp::Reverse(h.0));
        hits.into_iter().take(limit).filter_map(|(_, i)| self.info(i)).collect()
    }

    /// Archivos que se pueden leer para comparar su contenido: ni enlaces duros ya contados
    /// ni marcadores de la nube (leerlos los descargaría).
    fn dup_candidates(&self, id: NodeId, min_size: u64) -> HashMap<u64, Vec<NodeId>> {
        let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = &self.nodes[cur as usize];
            if n.flags & F_DIR != 0 {
                stack.extend(self.kids(cur));
            } else if n.flags & (F_LINK | F_CLOUD) == 0 && n.logical >= min_size.max(1) && n.parent != NO_PARENT {
                by_size.entry(n.logical).or_default().push(cur);
            }
        }
        by_size.retain(|_, v| v.len() > 1);
        by_size
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    /// Tamaño de cada copia.
    pub size: u64,
    /// Espacio que se recupera dejando una sola copia.
    pub wasted: u64,
    pub files: Vec<NodeInfo>,
}

/// Lee el principio, el medio y el final de un archivo (64 KB de cada parte).
fn quick_hash(path: &Path, len: u64) -> io::Result<u64> {
    const PART: u64 = 64 * 1024;
    let mut f = File::open(path)?;
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0u8; PART as usize];
    for off in [0, len / 2, len.saturating_sub(PART)] {
        f.seek(SeekFrom::Start(off))?;
        let n = f.read(&mut buf)?;
        h.update(&buf[..n]);
    }
    Ok(h.digest())
}

fn full_hash(path: &Path, progress: &Progress) -> io::Result<u128> {
    let mut f = File::open(path)?;
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        progress.bytes.fetch_add(n as u64, Ordering::Relaxed);
    }
    Ok(h.digest128())
}

/// Busca archivos con exactamente el mismo contenido bajo `id`: primero por tamaño, luego por una
/// huella rápida de tres trozos y al final por la huella del archivo completo. El árbol no se
/// bloquea mientras se leen los archivos.
pub fn duplicates(tree: &std::sync::RwLock<Tree>, id: NodeId, min_size: u64, progress: &Progress) -> Vec<DupGroup> {
    let groups: Vec<(u64, Vec<(NodeId, std::path::PathBuf)>)> = {
        let t = tree.read().unwrap_or_else(|e| e.into_inner());
        t.dup_candidates(id, min_size).into_iter().map(|(s, ids)| (s, ids.into_iter().map(|i| (i, t.path_of(i))).collect())).collect()
    };
    progress.files.store(groups.iter().map(|g| g.1.len() as u64).sum(), Ordering::Relaxed);
    let found: Vec<(u64, Vec<NodeId>)> = groups
        .into_par_iter()
        .flat_map_iter(|(size, files)| {
            let mut out = Vec::new();
            if progress.is_cancelled() {
                return out;
            }
            let mut quick: HashMap<u64, Vec<(NodeId, &std::path::Path)>> = HashMap::new();
            for (i, p) in &files {
                if let Ok(h) = quick_hash(p, size) {
                    quick.entry(h).or_default().push((*i, p));
                }
                progress.dirs.fetch_add(1, Ordering::Relaxed);
            }
            for same in quick.into_values().filter(|v| v.len() > 1) {
                let mut full: HashMap<u128, Vec<NodeId>> = HashMap::new();
                for (i, p) in same {
                    if let Ok(h) = full_hash(p, progress) {
                        full.entry(h).or_default().push(i);
                    }
                }
                out.extend(full.into_values().filter(|v| v.len() > 1).map(|v| (size, v)));
            }
            out
        })
        .collect();
    let t = tree.read().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<DupGroup> = found
        .into_iter()
        .map(|(_, ids)| {
            let files: Vec<NodeInfo> = ids.into_iter().filter(|i| t.attached(*i)).filter_map(|i| t.info(i)).collect();
            let size = files.first().map_or(0, |f| f.size);
            DupGroup { size, wasted: size * files.len().saturating_sub(1) as u64, files }
        })
        .filter(|g| g.files.len() > 1)
        .collect();
    out.sort_unstable_by_key(|g| std::cmp::Reverse(g.wasted));
    out.truncate(500);
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn iso_date(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    // Fecha civil a partir de días desde 1970 (algoritmo de Howard Hinnant).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02}")
}

/// Exporta a CSV las carpetas y archivos bajo `id` hasta `depth` niveles, de mayor a menor.
pub fn export_csv(tree: &Tree, id: NodeId, depth: usize, path: &Path) -> io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    // Excel en español separa con punto y coma; la marca BOM hace que reconozca los acentos.
    w.write_all("\u{feff}Ruta;Tipo;Tamaño en disco (bytes);Tamaño (bytes);Archivos;Carpetas;Modificado;Categoría\n".as_bytes())?;
    let mut stack = vec![(id, 0usize)];
    while let Some((cur, d)) = stack.pop() {
        let Some(i) = tree.info(cur) else { continue };
        writeln!(
            w,
            "{};{};{};{};{};{};{};{}",
            csv_field(&i.path),
            if i.is_dir { "Carpeta" } else { "Archivo" },
            i.size,
            i.logical,
            i.files,
            i.dirs,
            iso_date(i.modified),
            i.category.label()
        )?;
        if d < depth {
            // Al revés, para que la pila los saque de mayor a menor.
            stack.extend(tree.kids(cur).iter().rev().map(|c| (*c, d + 1)));
        }
    }
    w.flush()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let s = if i == 0 || v >= 100.0 { format!("{v:.0}") } else if v >= 10.0 { format!("{v:.1}") } else { format!("{v:.2}") };
    format!("{} {}", s.replace('.', ","), U[i])
}

/// Exporta un informe HTML autónomo: resumen, tipos de archivo, carpetas y archivos más grandes.
pub fn export_html(tree: &Tree, id: NodeId, path: &Path) -> io::Result<()> {
    let Some(root) = tree.info(id) else { return Err(io::ErrorKind::NotFound.into()) };
    let mut h = String::new();
    let row = |name: &str, path: &str, size: u64, total: u64| {
        let pct = if total > 0 { size as f64 * 100.0 / total as f64 } else { 0.0 };
        format!(
            "<tr><td title=\"{}\">{}</td><td class=n>{}</td><td class=b><i style=\"width:{:.1}%\"></i></td></tr>",
            html_escape(path),
            html_escape(name),
            human(size),
            pct
        )
    };
    h.push_str("<!doctype html><html lang=es><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\">");
    h.push_str(&format!("<title>Informe de espacio · {}</title>", html_escape(&root.path)));
    h.push_str("<style>body{font:14px/1.5 'Segoe UI',system-ui,sans-serif;max-width:960px;margin:32px auto;padding:0 16px;color:#18202c;background:#f6f8fb}h1{font-size:22px;margin:0}h2{font-size:16px;margin:28px 0 8px}.muted{color:#5d6a7e}.cards{display:flex;gap:12px;flex-wrap:wrap;margin-top:16px}.card{background:#fff;border:1px solid #dfe4ec;border-radius:10px;padding:12px 16px}.card b{display:block;font-size:20px}table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #dfe4ec;border-radius:10px;overflow:hidden}td{padding:6px 10px;border-top:1px solid #eef1f6;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:520px}.n{text-align:right;font-family:Consolas,monospace}.b{width:30%}.b i{display:block;height:6px;border-radius:3px;background:#2f6fea}@media(prefers-color-scheme:dark){body{background:#0c0f15;color:#e8edf5}.card,table{background:#171c27;border-color:#272f3e}td{border-color:#1d2330}.muted{color:#9aa6b8}}</style>");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    h.push_str(&format!(
        "<h1>Informe de espacio en disco</h1><p class=muted>{} · generado el {} con AppWindowsTreeSize</p>",
        html_escape(&root.path),
        iso_date(now)
    ));
    h.push_str(&format!(
        "<div class=cards><div class=card><span class=muted>En disco</span><b>{}</b></div><div class=card><span class=muted>Archivos</span><b>{}</b></div><div class=card><span class=muted>Carpetas</span><b>{}</b></div>{}</div>",
        human(root.size),
        root.files,
        root.dirs,
        if root.cloud > 0 { format!("<div class=card><span class=muted>Solo en la nube</span><b>{}</b></div>", human(root.cloud)) } else { String::new() }
    ));
    h.push_str("<h2>Por tipo de archivo</h2><table>");
    for c in tree.categories(id) {
        h.push_str(&row(c.category.label(), "", c.size, root.size));
    }
    h.push_str("</table><h2>Carpetas y archivos más grandes</h2><table>");
    for k in tree.children(id, 0, 50) {
        h.push_str(&row(&k.name, &k.path, k.size, root.size));
    }
    h.push_str("</table><h2>Los 100 archivos más grandes</h2><table>");
    for f in tree.largest_files(id, 100) {
        h.push_str(&row(&f.name, &f.path, f.size, root.size));
    }
    h.push_str("</table></html>");
    std::fs::write(path, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use std::sync::RwLock;

    fn write(path: &Path, content: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn finds_duplicates_by_content() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let a = vec![7u8; 200_000];
        let mut b = a.clone();
        b[150_000] = 8; // Mismo tamaño, contenido distinto en un punto que la huella rápida no lee.
        write(&r.join("x/copia1.bin"), &a);
        write(&r.join("y/copia2.bin"), &a);
        write(&r.join("z/copia3.bin"), &a);
        write(&r.join("x/distinto.bin"), &b);
        write(&r.join("pequeno1.txt"), b"hola");
        write(&r.join("pequeno2.txt"), b"hola");
        let t = RwLock::new(scan(r, &Progress::default()));
        let g = duplicates(&t, Tree::ROOT, 1000, &Progress::default());
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].files.len(), 3);
        assert!(g[0].files.iter().all(|f| f.name.starts_with("copia")));
        assert_eq!(g[0].wasted, g[0].size * 2);
        // Sin tamaño mínimo también aparecen los pequeños.
        assert_eq!(duplicates(&t, Tree::ROOT, 0, &Progress::default()).len(), 2);
    }

    #[test]
    fn filters_files() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.iso"), &vec![0u8; 50_000]);
        write(&d.path().join("b.txt"), &vec![0u8; 60_000]);
        write(&d.path().join("c.iso"), &vec![0u8; 100]);
        let t = scan(d.path(), &Progress::default());
        let now = 2_000_000_000;
        let by_ext = t.find_files(Tree::ROOT, &FileFilter { ext: Some(".ISO".into()), ..Default::default() }, now, 10);
        assert_eq!(by_ext.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["a.iso", "c.iso"]);
        let big = t.find_files(Tree::ROOT, &FileFilter { min_size: 10_000, ..Default::default() }, now, 10);
        assert_eq!(big[0].name, "b.txt");
        assert_eq!(big.len(), 2);
        // Recién creados: ninguno lleva un año sin tocarse.
        let real_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        assert!(t.find_files(Tree::ROOT, &FileFilter { older_than_days: 365, ..Default::default() }, real_now, 10).is_empty());
        let docs = t.find_files(Tree::ROOT, &FileFilter { category: Some(Category::Docs), ..Default::default() }, now, 10);
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn subtree_levels() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a/b/c/f.bin"), &vec![0u8; 50_000]);
        write(&d.path().join("a/g.bin"), &vec![0u8; 20_000]);
        let t = scan(d.path(), &Progress::default());
        let s = t.subtree(Tree::ROOT, 2, 10).unwrap();
        assert_eq!(s.children[0].name, "a");
        assert_eq!(s.children[0].children.len(), 2);
        assert!(s.children[0].children.iter().all(|c| c.children.is_empty()));
    }

    #[test]
    fn exports_reports() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("datos/informe; final.txt"), b"abc");
        let t = scan(d.path(), &Progress::default());
        let csv = d.path().join("out.csv");
        export_csv(&t, Tree::ROOT, 5, &csv).unwrap();
        let s = std::fs::read_to_string(&csv).unwrap();
        assert!(s.contains("\"") && s.contains("informe; final.txt"));
        assert_eq!(s.lines().count(), 4); // Cabecera, raíz, carpeta y archivo.
        let html = d.path().join("out.html");
        export_html(&t, Tree::ROOT, &html).unwrap();
        assert!(std::fs::read_to_string(&html).unwrap().contains("informe; final.txt"));
        assert_eq!(iso_date(0), "");
        assert_eq!(iso_date(1_700_000_000), "2023-11-14");
    }
}
