import type { Changes, Drive, DupGroup, DupProgress, FileFilter, FolderView, NodeInfo, ScanDone, ScanProgress, SubNode, Suggestion } from "./types";
import { demoApi } from "./demo";

/** Lo que la interfaz necesita del motor. En la app de escritorio lo cumple Rust; en el navegador, datos de ejemplo. */
export interface Api {
  isDemo: boolean;
  listDrives(): Promise<Drive[]>;
  startScan(root: string): Promise<void>;
  cancelScan(): Promise<void>;
  onProgress(cb: (p: ScanProgress) => void): Promise<() => void>;
  onDone(cb: (d: ScanDone) => void): Promise<() => void>;
  getFolder(root: string, id: number, limit: number): Promise<FolderView>;
  getChildren(root: string, id: number, offset: number, limit: number): Promise<NodeInfo[]>;
  /** Vuelve a leer una sola carpeta del disco. Devuelve null si ya no existe. */
  rescanFolder(root: string, id: number): Promise<number | null>;
  largestFiles(root: string, id: number, limit: number): Promise<NodeInfo[]>;
  search(root: string, query: string, limit: number): Promise<NodeInfo[]>;
  cleanupSuggestions(root: string): Promise<Suggestion[]>;
  openInExplorer(path: string, isDir: boolean): Promise<void>;
  moveToTrash(root: string, ids: number[]): Promise<number>;
  /** Un análisis guardado terminó de abrirse al iniciar la app. */
  onLoaded(cb: (root: string) => void): Promise<() => void>;
  /** Hay una comparación nueva con el análisis anterior. */
  onChanges(cb: (root: string) => void): Promise<() => void>;
  /** Qué cambió desde el análisis anterior; null si todavía no hay dos análisis. */
  getChanges(root: string): Promise<Changes | null>;
  /** Id de una ruta dentro del análisis, o null si ya no está. */
  locate(root: string, path: string): Promise<number | null>;
  isAdmin(): Promise<boolean>;
  /** Vuelve a abrir la app como administrador (Windows pide confirmación). */
  restartAsAdmin(): Promise<void>;
  getSubtree(root: string, id: number, depth: number): Promise<SubNode | null>;
  findFiles(root: string, id: number, filter: FileFilter, limit: number): Promise<NodeInfo[]>;
  findDuplicates(root: string, id: number, minSize: number): Promise<DupGroup[]>;
  cancelDuplicates(): Promise<void>;
  onDupProgress(cb: (p: DupProgress) => void): Promise<() => void>;
  /** Guarda un informe CSV o HTML (según la extensión de `path`). */
  exportReport(root: string, id: number, path: string): Promise<void>;
  deletePermanently(root: string, ids: number[]): Promise<number>;
  emptyRecycleBin(): Promise<void>;
  /** Carpeta con la que se abrió la app desde el clic derecho del Explorador. */
  startupPath(): Promise<string | null>;
  /** Pide una carpeta con el diálogo de Windows. */
  pickFolder(): Promise<string | null>;
  /** Pide dónde guardar un informe. */
  pickSavePath(defaultName: string): Promise<string | null>;
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    isDemo: false,
    listDrives: () => invoke("list_drives"),
    startScan: (root) => invoke("start_scan", { root }),
    cancelScan: () => invoke("cancel_scan"),
    onProgress: (cb) => listen<ScanProgress>("scan://progress", (e) => cb(e.payload)),
    onDone: (cb) => listen<ScanDone>("scan://done", (e) => cb(e.payload)),
    getFolder: (root, id, limit) => invoke("get_folder", { root, id, limit }),
    getChildren: (root, id, offset, limit) => invoke("get_children", { root, id, offset, limit }),
    rescanFolder: (root, id) => invoke("rescan_folder", { root, id }),
    largestFiles: (root, id, limit) => invoke("largest_files", { root, id, limit }),
    search: (root, query, limit) => invoke("search", { root, queryText: query, limit }),
    cleanupSuggestions: (root) => invoke("cleanup_suggestions", { root }),
    openInExplorer: (path, isDir) => invoke("open_in_explorer", { path, isDir }),
    moveToTrash: (root, ids) => invoke("move_to_trash", { root, ids }),
    onLoaded: (cb) => listen<{ root: string }>("scan://loaded", (e) => cb(e.payload.root)),
    onChanges: (cb) => listen<string>("scan://changes", (e) => cb(e.payload)),
    getChanges: (root) => invoke("get_changes", { root }),
    locate: (root, path) => invoke("locate", { root, path }),
    isAdmin: () => invoke("is_admin"),
    restartAsAdmin: () => invoke("restart_as_admin"),
    getSubtree: (root, id, depth) => invoke("get_subtree", { root, id, depth }),
    findFiles: (root, id, filter, limit) => invoke("find_files", { root, id, filter, limit }),
    findDuplicates: (root, id, minSize) => invoke("find_duplicates", { root, id, minSize }),
    cancelDuplicates: () => invoke("cancel_duplicates"),
    onDupProgress: (cb) => listen<DupProgress>("dups://progress", (e) => cb(e.payload)),
    exportReport: (root, id, path) => invoke("export_report", { root, id, path }),
    deletePermanently: (root, ids) => invoke("delete_permanently", { root, ids }),
    emptyRecycleBin: () => invoke("empty_recycle_bin"),
    startupPath: () => invoke("startup_path"),
    pickFolder: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const r = await open({ directory: true, multiple: false, title: "Elige la carpeta que quieres analizar" });
      return typeof r === "string" ? r : null;
    },
    pickSavePath: async (defaultName) => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      return save({
        title: "Guardar informe",
        defaultPath: defaultName,
        filters: [{ name: "Página web", extensions: ["html"] }, { name: "Hoja de cálculo (CSV)", extensions: ["csv"] }],
      });
    },
  };
}

export const apiPromise: Promise<Api> = inTauri ? tauriApi() : Promise.resolve(demoApi());
