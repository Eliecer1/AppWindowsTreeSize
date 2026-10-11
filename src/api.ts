import type { Drive, FolderView, NodeInfo, ScanDone, ScanProgress, Suggestion } from "./types";
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
  largestFiles(root: string, id: number, limit: number): Promise<NodeInfo[]>;
  search(root: string, query: string, limit: number): Promise<NodeInfo[]>;
  cleanupSuggestions(root: string): Promise<Suggestion[]>;
  openInExplorer(path: string, isDir: boolean): Promise<void>;
  moveToTrash(root: string, ids: number[]): Promise<number>;
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
    largestFiles: (root, id, limit) => invoke("largest_files", { root, id, limit }),
    search: (root, query, limit) => invoke("search", { root, query, limit }),
    cleanupSuggestions: (root) => invoke("cleanup_suggestions", { root }),
    openInExplorer: (path, isDir) => invoke("open_in_explorer", { path, isDir }),
    moveToTrash: (root, ids) => invoke("move_to_trash", { root, ids }),
  };
}

export const apiPromise: Promise<Api> = inTauri ? tauriApi() : Promise.resolve(demoApi());
