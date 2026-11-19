//! Análisis guardados en disco: se abren al iniciar la app y se comparan con el siguiente análisis.
//!
//! Viven en `%LOCALAPPDATA%\com.appwindowstreesize.app\scans`: un `.awts` por disco o carpeta con el
//! último análisis, y un `.changes.json` con lo que cambió respecto al análisis anterior.

use crate::AppState;
use scanner::Tree;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Cambios de menos de 1 MB no aparecen en la comparación.
const MIN_CHANGE: u64 = 1 << 20;

fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("scans"))
}

/// Nombre de archivo estable para una raíz (FNV-1a de la ruta en minúsculas).
fn key(root: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in root.to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

fn tree_file(app: &AppHandle, root: &str) -> Option<PathBuf> {
    dir(app).map(|d| d.join(format!("{}.awts", key(root))))
}

fn changes_file(app: &AppHandle, root: &str) -> Option<PathBuf> {
    dir(app).map(|d| d.join(format!("{}.changes.json", key(root))))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Changes<'a> {
    root: &'a str,
    /// Fecha del análisis anterior.
    from: i64,
    /// Fecha del análisis nuevo.
    to: i64,
    tree: scanner::Change,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    root: String,
    scanned_at: i64,
}

/// Al terminar un análisis completo: lo compara con el guardado anterior y lo guarda.
pub fn after_scan(app: AppHandle, root: String, tree: Arc<RwLock<Tree>>) {
    std::thread::spawn(move || {
        let (Some(file), Some(changes)) = (tree_file(&app, &root), changes_file(&app, &root)) else { return };
        if let Ok(old) = scanner::load(&file) {
            let t = tree.read().unwrap_or_else(|e| e.into_inner());
            if old.scanned_at() > 0 && old.scanned_at() < t.scanned_at() {
                let c = Changes { root: &root, from: old.scanned_at(), to: t.scanned_at(), tree: scanner::diff(&old, &t, MIN_CHANGE) };
                drop(old);
                if let Ok(json) = serde_json::to_vec(&c) {
                    let _ = std::fs::write(&changes, json);
                }
            }
        }
        write(&tree, &file);
        let _ = app.emit("scan://changes", root);
    });
}

fn write(tree: &RwLock<Tree>, file: &std::path::Path) {
    let t = tree.read().unwrap_or_else(|e| e.into_inner());
    if t.is_live() || t.scanned_at() == 0 {
        return;
    }
    if let Err(e) = scanner::save(&t, file) {
        eprintln!("No se pudo guardar el análisis en {}: {e}", file.display());
    }
}

fn generations() -> &'static Mutex<HashMap<String, u64>> {
    static G: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    G.get_or_init(Default::default)
}

/// Guarda el árbol unos segundos después del último cambio (papelera, refrescar carpeta),
/// así varios cambios seguidos se guardan una sola vez.
pub fn schedule(app: &AppHandle, root: &str, tree: Arc<RwLock<Tree>>) {
    let Some(file) = tree_file(app, root) else { return };
    let gen = {
        let mut g = generations().lock().unwrap();
        let v = g.entry(root.to_string()).or_insert(0);
        *v += 1;
        *v
    };
    let root = root.to_string();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(4));
        if generations().lock().unwrap().get(&root) == Some(&gen) {
            write(&tree, &file);
        }
    });
}

/// Lo que cambió desde el análisis anterior, tal como se guardó (JSON).
pub fn changes(app: &AppHandle, root: &str) -> Option<serde_json::Value> {
    let bytes = std::fs::read(changes_file(app, root)?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Al abrir la app: carga los análisis guardados, del más reciente al más antiguo.
/// Cada uno avisa a la interfaz con `scan://loaded` en cuanto está listo.
pub fn load_all(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(dir) = dir(&app) else { return };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<(i64, String, PathBuf)> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "awts"))
            .filter_map(|p| scanner::read_header(&p).ok().map(|h| (h.scanned_at, h.root.to_string_lossy().into_owned(), p)))
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, root, path) in files {
            let state = app.state::<AppState>();
            if state.trees.lock().unwrap().contains_key(&root) {
                continue;
            }
            let Ok(tree) = scanner::load(&path) else { continue };
            let scanned_at = tree.scanned_at();
            // Si mientras tanto se empezó a analizar este disco, gana el análisis nuevo.
            let inserted = {
                let mut trees = state.trees.lock().unwrap();
                if trees.contains_key(&root) {
                    false
                } else {
                    trees.insert(root.clone(), Arc::new(RwLock::new(tree)));
                    true
                }
            };
            if inserted {
                let _ = app.emit("scan://loaded", Loaded { root, scanned_at });
            }
        }
    });
}

/// Fecha del análisis que hay en memoria para cada raíz.
pub fn scanned_at(state: &AppState, root: &str) -> Option<i64> {
    let tree = state.trees.lock().unwrap().get(root).cloned()?;
    let t = tree.read().unwrap_or_else(|e| e.into_inner());
    (t.scanned_at() > 0).then(|| t.scanned_at())
}
