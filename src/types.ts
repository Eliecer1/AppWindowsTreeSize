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
}

export interface NodeInfo {
  id: number;
  parent: number | null;
  name: string;
  path: string;
  isDir: boolean;
  size: number;
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
}
