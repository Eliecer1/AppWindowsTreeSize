import type { Category } from "./types";

export const CATEGORY: Record<Category, { name: string; color: string }> = {
  video: { name: "Videos", color: "var(--t-video)" },
  image: { name: "Imágenes", color: "var(--t-image)" },
  app: { name: "Programas y juegos", color: "var(--t-app)" },
  archive: { name: "Instaladores y comprimidos", color: "var(--t-archive)" },
  system: { name: "Sistema", color: "var(--t-system)" },
  docs: { name: "Documentos", color: "var(--t-docs)" },
  code: { name: "Código y dependencias", color: "var(--t-code)" },
  cache: { name: "Caché y temporales", color: "var(--t-cache)" },
  other: { name: "Otros", color: "var(--t-other)" },
};

const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** Tamaño legible en unidades de Windows (base 1024), con coma decimal. */
export function size(bytes: number): string {
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 ? String(Math.round(v)) : v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2);
  return `${s.replace(".", ",")} ${UNITS[i]}`;
}

export const int = (n: number) => n.toLocaleString("es-ES");

export const pct = (part: number, total: number) => (total > 0 ? (part / total) * 100 : 0);

export const fmtPct = (v: number) => `${(v >= 10 ? v.toFixed(0) : v.toFixed(1)).replace(".", ",")} %`;

export const seconds = (ms: number) => `${(ms / 1000).toFixed(1).replace(".", ",")} s`;

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
export function date(unixSecs: number): string {
  if (!unixSecs) return "—";
  const d = new Date(unixSecs * 1000);
  return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** "C:\\" → "C:" */
export const driveLetter = (root: string) => root.replace(/[\\/]+$/, "") || root;

export function driveLabel(d: { root: string; name: string }) {
  const letter = driveLetter(d.root);
  return d.name && d.name !== letter ? `${d.name} (${letter})` : `Disco local (${letter})`;
}
