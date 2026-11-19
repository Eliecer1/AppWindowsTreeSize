// Tipos que comparten la interfaz y el motor en Rust (ver src-tauri/src/lib.rs).

export type Category = "video" | "image" | "app" | "archive" | "system" | "docs" | "code" | "cache" | "other";

export interface Drive {
  root: string;
  name: string;
  total: number;
  available: number;
  kind: string;
  removable: boolean;
  fileSystem: string;
  scanned: boolean;
  /** Fecha (segundos Unix) del análisis abierto, que puede venir de un análisis guardado. */
  scannedAt?: number | null;
  /** Es una carpeta analizada, no una unidad entera. */
  folder?: boolean;
}

export interface NodeInfo {
  id: number;
  parent: number | null;
  name: string;
  path: string;
  isDir: boolean;
  /** Espacio que ocupa realmente en el disco. */
  size: number;
  /** Tamaño que muestra el Explorador de Windows. */
  logical: number;
  /** Bytes que están solo en la nube (OneDrive y similares) y no ocupan espacio local. */
  cloud: number;
  cloudFiles: number;
  /** Archivo no descargado: está solo en la nube. */
  cloudOnly: boolean;
  /** Otro nombre (enlace duro) de un archivo cuyo espacio ya se contó en otra carpeta. */
  hardlink: boolean;
  files: number;
  dirs: number;
  modified: number;
  ext: string;
  category: Category;
  hasChildren: boolean;
}

export interface CategorySize {
  category: Category;
  size: number;
}

export interface FolderView {
  node: NodeInfo;
  ancestors: NodeInfo[];
  children: NodeInfo[];
  totalChildren: number;
  categories: CategorySize[];
}

export interface ScanProgress {
  root: string;
  files: number;
  dirs: number;
  bytes: number;
  elapsedMs: number;
}

export interface ScanDone extends ScanProgress {
  errors: number;
  cancelled: boolean;
  /** Se leyó la tabla MFT directamente (análisis rápido como administrador). */
  fast?: boolean;
}

export type Risk = "safe" | "review" | "careful";

export interface Suggestion {
  key: string;
  title: string;
  description: string;
  risk: Risk;
  size: number;
  ids: number[];
  manualHint: string | null;
  /** Acción especial en lugar de la papelera, por ejemplo "empty-recycle-bin". */
  action?: string | null;
}

/** Qué archivos se miran: todos, solo los guardados en este PC o solo los que están en la nube. */
export type Storage = "all" | "local" | "cloud";

export interface FileFilter {
  minSize?: number;
  olderThanDays?: number;
  category?: Category | null;
  ext?: string | null;
  storage?: Storage;
}

export interface DupGroup {
  size: number;
  wasted: number;
  files: NodeInfo[];
}

export interface DupProgress {
  candidates: number;
  checked: number;
  bytes: number;
}

/** Un elemento que cambió entre dos análisis. */
export interface Change {
  name: string;
  isDir: boolean;
  old: number;
  new: number;
  existed: boolean;
  exists: boolean;
  children: Change[];
}

export interface Changes {
  root: string;
  from: number;
  to: number;
  tree: Change;
}

/** Una carpeta con varios niveles de hijos, para el mapa anidado y los anillos. */
export interface SubNode {
  id: number;
  name: string;
  size: number;
  category: Category;
  isDir: boolean;
  children: SubNode[];
}
