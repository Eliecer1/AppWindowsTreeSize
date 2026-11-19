mod cleanup;

use scanner::{CategorySize, NodeId, NodeInfo, Progress, Tree};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Árboles escaneados, uno por unidad o carpeta raíz, y el escaneo en curso.
#[derive(Default)]
struct AppState {
    trees: Mutex<HashMap<String, Arc<RwLock<Tree>>>>,
    running: Mutex<Option<Arc<Progress>>>,
}

impl AppState {
    fn tree(&self, root: &str) -> Result<Arc<RwLock<Tree>>, String> {
        self.trees
            .lock()
            .unwrap()
            .get(root)
            .cloned()
            .ok_or_else(|| format!("Todavía no se ha analizado {root}"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DriveInfo {
    root: String,
    name: String,
    total: u64,
    available: u64,
    kind: String,
    removable: bool,
    file_system: String,
    scanned: bool,
}

#[tauri::command]
fn list_drives(state: State<AppState>) -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let trees = state.trees.lock().unwrap();
    let mut out: Vec<DriveInfo> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let root = d.mount_point().to_string_lossy().into_owned();
            DriveInfo {
                scanned: trees.contains_key(&root),
                name: d.name().to_string_lossy().into_owned(),
                total: d.total_space(),
                available: d.available_space(),
                kind: match d.kind() {
                    sysinfo::DiskKind::SSD => "SSD".into(),
                    sysinfo::DiskKind::HDD => "HDD".into(),
                    _ => "Disco".into(),
                },
                removable: d.is_removable(),
                file_system: d.file_system().to_string_lossy().into_owned(),
                root,
            }
        })
        .collect();
    out.sort_by(|a, b| a.root.cmp(&b.root));
    out.dedup_by(|a, b| a.root == b.root);
    out
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    root: String,
    files: u64,
    dirs: u64,
    bytes: u64,
    elapsed_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanDone {
    root: String,
    files: u64,
    dirs: u64,
    bytes: u64,
    errors: u64,
    elapsed_ms: u64,
    cancelled: bool,
}

/// Inicia el escaneo en segundo plano. El avance llega como eventos `scan://progress`
/// y el final como `scan://done`.
#[tauri::command]
fn start_scan(root: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    if !Path::new(&root).is_dir() {
        return Err(format!("No se encuentra la carpeta {root}"));
    }
    let progress = Arc::new(Progress::default());
    {
        let mut running = state.running.lock().unwrap();
        if running.is_some() {
            return Err("Ya hay un análisis en curso".into());
        }
        *running = Some(progress.clone());
    }
    // El árbol queda disponible desde ya y se va llenando: se puede explorar mientras se analiza.
    let tree = Arc::new(RwLock::new(Tree::new(Path::new(&root))));
    state.trees.lock().unwrap().insert(root.clone(), tree.clone());

    std::thread::spawn(move || {
        let started = Instant::now();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let ticker = {
            let (app, progress, done, root) = (app.clone(), progress.clone(), done.clone(), root.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    let _ = app.emit(
                        "scan://progress",
                        ScanProgress {
                            root: root.clone(),
                            files: progress.files.load(Ordering::Relaxed),
                            dirs: progress.dirs.load(Ordering::Relaxed),
                            bytes: progress.bytes.load(Ordering::Relaxed),
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        },
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
        };

        scanner::scan_into(&tree, &progress);
        done.store(true, Ordering::Relaxed);
        let _ = ticker.join();

        let state = app.state::<AppState>();
        *state.running.lock().unwrap() = None;
        let _ = app.emit(
            "scan://done",
            ScanDone {
                root,
                files: progress.files.load(Ordering::Relaxed),
                dirs: progress.dirs.load(Ordering::Relaxed),
                bytes: progress.bytes.load(Ordering::Relaxed),
                errors: progress.errors.load(Ordering::Relaxed),
                elapsed_ms: started.elapsed().as_millis() as u64,
                cancelled: progress.is_cancelled(),
            },
        );
    });
    Ok(())
}

#[tauri::command]
fn cancel_scan(state: State<AppState>) {
    if let Some(p) = state.running.lock().unwrap().as_ref() {
        p.cancel();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderView {
    node: NodeInfo,
    ancestors: Vec<NodeInfo>,
    children: Vec<NodeInfo>,
    total_children: usize,
    categories: Vec<CategorySize>,
}

/// Ejecuta una consulta al árbol fuera del hilo de la ventana, para que la interfaz nunca se congele
/// mientras se recorre un árbol de millones de archivos.
async fn query<T: Send + 'static>(
    state: &AppState,
    root: &str,
    f: impl FnOnce(&Tree) -> T + Send + 'static,
) -> Result<T, String> {
    let tree = state.tree(root)?;
    tauri::async_runtime::spawn_blocking(move || f(&tree.read().unwrap_or_else(|e| e.into_inner())))
        .await
        .map_err(|e| e.to_string())
}

/// Todo lo que la vista Explorar necesita de una carpeta en una sola llamada.
#[tauri::command]
async fn get_folder(root: String, id: NodeId, limit: usize, state: State<'_, AppState>) -> Result<FolderView, String> {
    query(&state, &root, move |t| {
        let node = t.info(id).ok_or("Elemento no encontrado")?;
        Ok(FolderView {
            total_children: t.kids(id).len(),
            children: t.children(id, 0, limit),
            ancestors: t.ancestors(id),
            categories: t.categories(id),
            node,
        })
    })
    .await?
}

/// Vuelve a leer del disco solo una carpeta. Devuelve `None` si la carpeta ya no existe.
#[tauri::command]
async fn rescan_folder(root: String, id: NodeId, state: State<'_, AppState>) -> Result<Option<NodeId>, String> {
    let tree = state.tree(&root)?;
    tauri::async_runtime::spawn_blocking(move || scanner::rescan(&tree, id, &Progress::default()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_children(root: String, id: NodeId, offset: usize, limit: usize, state: State<'_, AppState>) -> Result<Vec<NodeInfo>, String> {
    query(&state, &root, move |t| t.children(id, offset, limit)).await
}

#[tauri::command]
async fn largest_files(root: String, id: NodeId, limit: usize, state: State<'_, AppState>) -> Result<Vec<NodeInfo>, String> {
    query(&state, &root, move |t| t.largest_files(id, limit)).await
}

#[tauri::command]
async fn search(root: String, query_text: String, limit: usize, state: State<'_, AppState>) -> Result<Vec<NodeInfo>, String> {
    query(&state, &root, move |t| t.search(&query_text, limit)).await
}

#[tauri::command]
async fn cleanup_suggestions(root: String, state: State<'_, AppState>) -> Result<Vec<cleanup::Suggestion>, String> {
    query(&state, &root, cleanup::suggestions).await
}

/// Abre el Explorador de Windows en la carpeta, o con el archivo seleccionado.
#[tauri::command]
fn open_in_explorer(path: String, is_dir: bool) -> Result<(), String> {
    let path = PathBuf::from(path);
    #[cfg(target_os = "windows")]
    let result = if is_dir {
        std::process::Command::new("explorer.exe").arg(&path).spawn()
    } else {
        // explorer.exe necesita "/select," y la ruta en el mismo argumento.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
    };
    #[cfg(target_os = "macos")]
    let result = if is_dir {
        std::process::Command::new("open").arg(&path).spawn()
    } else {
        std::process::Command::new("open").arg("-R").arg(&path).spawn()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(if is_dir { path.as_path() } else { path.parent().unwrap_or(&path) })
        .spawn();
    result.map(|_| ()).map_err(|e| format!("No se pudo abrir el Explorador: {e}"))
}

/// Envía elementos a la papelera de reciclaje y los quita del árbol. Devuelve los bytes liberados.
/// El árbol solo se bloquea un instante al principio y al final: mientras Windows mueve los archivos,
/// la interfaz se puede seguir usando.
#[tauri::command]
async fn move_to_trash(root: String, ids: Vec<NodeId>, state: State<'_, AppState>) -> Result<u64, String> {
    let tree = state.tree(&root)?;
    tauri::async_runtime::spawn_blocking(move || {
        let targets: Vec<(NodeId, PathBuf)> = {
            let t = tree.read().unwrap_or_else(|e| e.into_inner());
            ids.into_iter().filter(|id| t.attached(*id) && *id != Tree::ROOT).map(|id| (id, t.path_of(id))).collect()
        };
        let mut done = Vec::new();
        let mut failed = Vec::new();
        for (id, path) in targets {
            match trash::delete(&path) {
                Ok(()) => done.push(id),
                Err(e) => failed.push(format!("{}: {e}", path.display())),
            }
        }
        let freed: u64 = {
            let mut t = tree.write().unwrap_or_else(|e| e.into_inner());
            done.iter().filter_map(|id| t.remove(*id)).sum()
        };
        if failed.is_empty() || !done.is_empty() {
            Ok(freed)
        } else {
            Err(format!("No se pudo enviar a la papelera:\n{}", failed.join("\n")))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_drives,
            start_scan,
            cancel_scan,
            get_folder,
            get_children,
            rescan_folder,
            largest_files,
            search,
            cleanup_suggestions,
            open_in_explorer,
            move_to_trash,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar AppWindowsTreeSize");
}
