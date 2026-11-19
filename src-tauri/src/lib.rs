mod cleanup;
mod saved;

use scanner::{CategorySize, NodeId, NodeInfo, Progress, Storage, Tree};
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
    /// Búsqueda de duplicados en curso.
    dups: Mutex<Option<Arc<Progress>>>,
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
    /// Fecha del análisis que hay abierto (puede venir de un análisis guardado).
    scanned_at: Option<i64>,
    /// Es una carpeta analizada, no una unidad.
    folder: bool,
}

#[tauri::command]
fn list_drives(state: State<AppState>) -> Vec<DriveInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let scanned: HashMap<String, Option<i64>> = {
        let roots: Vec<String> = state.trees.lock().unwrap().keys().cloned().collect();
        roots.into_iter().map(|r| (r.clone(), saved::scanned_at(&state, &r))).collect()
    };
    let mut out: Vec<DriveInfo> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let root = d.mount_point().to_string_lossy().into_owned();
            DriveInfo {
                scanned: scanned.contains_key(&root),
                scanned_at: scanned.get(&root).copied().flatten(),
                folder: false,
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
    // Las carpetas analizadas (no unidades enteras) van al final, con el tamaño de su análisis.
    let mut folders: Vec<DriveInfo> = scanned
        .iter()
        .filter(|(r, _)| !out.iter().any(|d| &d.root == *r))
        .map(|(r, at)| {
            let size = state.tree(r).ok().map(|t| t.read().unwrap_or_else(|e| e.into_inner()).size(Tree::ROOT)).unwrap_or(0);
            DriveInfo {
                root: r.clone(),
                name: Path::new(r).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| r.clone()),
                total: size,
                available: 0,
                kind: "Carpeta".into(),
                removable: false,
                file_system: String::new(),
                scanned: true,
                scanned_at: *at,
                folder: true,
            }
        })
        .collect();
    folders.sort_by(|a, b| a.root.cmp(&b.root));
    out.extend(folders);
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
    /// Se leyó la MFT directamente (análisis rápido como administrador).
    fast: bool,
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
        if !progress.is_cancelled() {
            saved::after_scan(app.clone(), root.clone(), tree.clone());
        }
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
                fast: progress.fast.load(Ordering::Relaxed),
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
async fn get_folder(root: String, id: NodeId, limit: usize, storage: Option<Storage>, state: State<'_, AppState>) -> Result<FolderView, String> {
    let s = storage.unwrap_or_default();
    query(&state, &root, move |t| {
        let node = t.info_in(id, s).ok_or("Elemento no encontrado")?;
        Ok(FolderView {
            total_children: t.count_children(id, s),
            children: t.children_in(id, s, 0, limit),
            ancestors: t.ancestors_in(id, s),
            categories: t.categories_in(id, s),
            node,
        })
    })
    .await?
}

/// Vuelve a leer del disco solo una carpeta. Devuelve `None` si la carpeta ya no existe.
#[tauri::command]
async fn rescan_folder(root: String, id: NodeId, app: AppHandle, state: State<'_, AppState>) -> Result<Option<NodeId>, String> {
    let tree = state.tree(&root)?;
    let t = tree.clone();
    let r = tauri::async_runtime::spawn_blocking(move || scanner::rescan(&t, id, &Progress::default()))
        .await
        .map_err(|e| e.to_string())?;
    saved::schedule(&app, &root, tree);
    Ok(r)
}

/// Id de una ruta dentro del análisis de `root`, o `None` si ya no está.
#[tauri::command]
async fn locate(root: String, path: String, state: State<'_, AppState>) -> Result<Option<NodeId>, String> {
    query(&state, &root, move |t| {
        let rel = Path::new(&path).strip_prefix(t.root_path()).ok()?.to_path_buf();
        t.find(&rel)
    })
    .await
}

/// Lo que cambió en el disco desde el análisis anterior, o `None` si todavía no hay dos análisis.
#[tauri::command]
async fn get_changes(root: String, app: AppHandle) -> Option<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || saved::changes(&app, &root)).await.ok().flatten()
}

#[tauri::command]
async fn get_children(
    root: String,
    id: NodeId,
    offset: usize,
    limit: usize,
    storage: Option<Storage>,
    state: State<'_, AppState>,
) -> Result<Vec<NodeInfo>, String> {
    query(&state, &root, move |t| t.children_in(id, storage.unwrap_or_default(), offset, limit)).await
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
async fn move_to_trash(root: String, ids: Vec<NodeId>, app: AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let tree = state.tree(&root)?;
    let saved_tree = tree.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
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
    .map_err(|e| e.to_string())?;
    saved::schedule(&app, &root, saved_tree);
    r
}

/// Carpeta con varios niveles de hijos, para el mapa anidado y los anillos.
#[tauri::command]
async fn get_subtree(
    root: String,
    id: NodeId,
    depth: usize,
    storage: Option<Storage>,
    state: State<'_, AppState>,
) -> Result<Option<scanner::SubNode>, String> {
    query(&state, &root, move |t| t.subtree(id, depth.min(4), 60, storage.unwrap_or_default())).await
}

/// Archivos bajo `id` que cumplen los filtros de la vista Archivos, de mayor a menor.
#[tauri::command]
async fn find_files(root: String, id: NodeId, filter: scanner::FileFilter, limit: usize, state: State<'_, AppState>) -> Result<Vec<NodeInfo>, String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    query(&state, &root, move |t| t.find_files(id, &filter, now, limit)).await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DupProgress {
    candidates: u64,
    checked: u64,
    bytes: u64,
}

/// Busca archivos duplicados bajo `id`. El avance llega como eventos `dups://progress`.
#[tauri::command]
async fn find_duplicates(root: String, id: NodeId, min_size: u64, app: AppHandle, state: State<'_, AppState>) -> Result<Vec<scanner::DupGroup>, String> {
    let tree = state.tree(&root)?;
    let progress = Arc::new(Progress::default());
    {
        let mut d = state.dups.lock().unwrap();
        if let Some(old) = d.replace(progress.clone()) {
            old.cancel();
        }
    }
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (app, progress, done) = (app.clone(), progress.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let _ = app.emit(
                    "dups://progress",
                    DupProgress {
                        candidates: progress.files.load(Ordering::Relaxed),
                        checked: progress.dirs.load(Ordering::Relaxed),
                        bytes: progress.bytes.load(Ordering::Relaxed),
                    },
                );
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }
    let p = progress.clone();
    let r = tauri::async_runtime::spawn_blocking(move || scanner::duplicates(&tree, id, min_size, &p)).await;
    done.store(true, Ordering::Relaxed);
    let r = r.map_err(|e| e.to_string())?;
    if progress.is_cancelled() {
        return Err("Búsqueda cancelada".into());
    }
    Ok(r)
}

#[tauri::command]
fn cancel_duplicates(state: State<AppState>) {
    if let Some(p) = state.dups.lock().unwrap().take() {
        p.cancel();
    }
}

/// Guarda un informe de la carpeta `id` en `path`, en CSV o en HTML según la extensión.
#[tauri::command]
async fn export_report(root: String, id: NodeId, path: String, state: State<'_, AppState>) -> Result<(), String> {
    query(&state, &root, move |t| {
        let p = Path::new(&path);
        let html = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
        if html { scanner::export_html(t, id, p) } else { scanner::export_csv(t, id, 4, p) }.map_err(|e| format!("No se pudo guardar el informe: {e}"))
    })
    .await?
}

/// Borra elementos definitivamente, sin pasar por la papelera. Devuelve los bytes liberados.
#[tauri::command]
async fn delete_permanently(root: String, ids: Vec<NodeId>, app: AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let tree = state.tree(&root)?;
    let saved_tree = tree.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        let targets: Vec<(NodeId, PathBuf, bool)> = {
            let t = tree.read().unwrap_or_else(|e| e.into_inner());
            ids.into_iter().filter(|id| t.attached(*id) && *id != Tree::ROOT).map(|id| (id, t.path_of(id), t.is_dir(id))).collect()
        };
        let mut done = Vec::new();
        let mut failed = Vec::new();
        for (id, path, is_dir) in targets {
            let r = if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
            match r {
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
            Err(format!("No se pudo borrar:\n{}", failed.join("\n")))
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    saved::schedule(&app, &root, saved_tree);
    r
}

/// Vacía la papelera de reciclaje de todas las unidades.
#[tauri::command]
async fn empty_recycle_bin() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
        // S_OK, o la papelera ya estaba vacía.
        if hr == 0 || hr as u32 == 0x8000_FFFF {
            Ok(())
        } else {
            Err(format!("No se pudo vaciar la papelera (código {hr:#x})"))
        }
    }
    #[cfg(not(windows))]
    {
        Err("Solo disponible en Windows".into())
    }
}

/// Carpeta con la que se abrió la app (desde "Analizar con AppWindowsTreeSize" en el Explorador).
#[tauri::command]
fn startup_path() -> Option<String> {
    std::env::args().skip(1).find(|a| !a.starts_with('-') && Path::new(a).is_dir())
}

/// Agrega "Analizar con AppWindowsTreeSize" al menú del clic derecho de las carpetas y unidades
/// en el Explorador, para el usuario actual (no necesita permisos de administrador).
#[cfg(windows)]
#[cfg_attr(debug_assertions, allow(dead_code))]
fn register_explorer_menu() {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    let command = format!("\"{}\" \"%V\"", exe.display());
    for key in [r"HKCU\Software\Classes\Directory\shell\AppWindowsTreeSize", r"HKCU\Software\Classes\Drive\shell\AppWindowsTreeSize"] {
        let run = |args: &[&str]| {
            let _ = std::process::Command::new("reg").args(args).creation_flags(0x0800_0000).status();
        };
        run(&["add", key, "/ve", "/d", "Analizar con AppWindowsTreeSize", "/f"]);
        run(&["add", key, "/v", "Icon", "/d", &exe.display().to_string(), "/f"]);
        run(&["add", &format!(r"{key}\command"), "/ve", "/d", &command, "/f"]);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

/// Busca una versión nueva en GitHub Releases. `None` si ya es la última o no hay conexión.
#[tauri::command]
async fn check_update(app: AppHandle) -> Option<UpdateInfo> {
    use tauri_plugin_updater::UpdaterExt;
    // Sin clave pública configurada no se podría verificar la descarga: no se ofrece nada.
    let pubkey = app.config().plugins.0.get("updater").and_then(|u| u.get("pubkey")).and_then(|p| p.as_str()).unwrap_or("");
    if pubkey.is_empty() {
        return None;
    }
    let update = app.updater().ok()?.check().await.ok()??;
    Some(UpdateInfo { version: update.version.clone(), notes: update.body.clone() })
}

/// Descarga e instala la versión nueva y reinicia la app.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("No se pudo buscar la actualización: {e}"))?
        .ok_or("Ya tienes la última versión")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("No se pudo instalar la actualización: {e}"))?;
    app.restart();
}

/// La app se está ejecutando como administrador.
#[tauri::command]
fn is_admin() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Vuelve a abrir la app como administrador (Windows pide confirmación) y cierra esta.
#[tauri::command]
fn restart_as_admin(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let verb = wide(std::ffi::OsStr::new("runas"));
        let file = wide(exe.as_os_str());
        let r = unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
        };
        if r as isize <= 32 {
            return Err("No se pudo reiniciar como administrador (¿cancelaste el aviso de Windows?)".into());
        }
        app.exit(0);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Solo disponible en Windows".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            saved::load_all(app.handle().clone());
            // Solo en la app instalada: en modo desarrollo apuntaría al ejecutable de prueba.
            #[cfg(all(windows, not(debug_assertions)))]
            std::thread::spawn(register_explorer_menu);
            Ok(())
        })
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
            get_changes,
            locate,
            is_admin,
            restart_as_admin,
            find_files,
            get_subtree,
            find_duplicates,
            cancel_duplicates,
            export_report,
            delete_permanently,
            empty_recycle_bin,
            startup_path,
            check_update,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar AppWindowsTreeSize");
}
