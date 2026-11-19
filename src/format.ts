import { locale, tr } from "./i18n";
import type { Category } from "./types";

export const CATEGORY: Record<Category, { name: string; color: string }> = {
  video: { get name() { return tr("Videos", "Videos"); }, color: "var(--t-video)" },
  image: { get name() { return tr("Imágenes", "Images"); }, color: "var(--t-image)" },
  app: { get name() { return tr("Programas y juegos", "Apps and games"); }, color: "var(--t-app)" },
  archive: { get name() { return tr("Instaladores y comprimidos", "Installers and archives"); }, color: "var(--t-archive)" },
  system: { get name() { return tr("Sistema", "System"); }, color: "var(--t-system)" },
  docs: { get name() { return tr("Documentos", "Documents"); }, color: "var(--t-docs)" },
  code: { get name() { return tr("Código y dependencias", "Code and dependencies"); }, color: "var(--t-code)" },
  cache: { get name() { return tr("Caché y temporales", "Cache and temp files"); }, color: "var(--t-cache)" },
  other: { get name() { return tr("Otros", "Other"); }, color: "var(--t-other)" },
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
  return `${tr(s.replace(".", ","), s)} ${UNITS[i]}`;
}

export const int = (n: number) => n.toLocaleString(locale());

export const pct = (part: number, total: number) => (total > 0 ? (part / total) * 100 : 0);

export const fmtPct = (v: number) => { const t = v >= 10 ? v.toFixed(0) : v.toFixed(1); return tr(`${t.replace(".", ",")} %`, `${t}%`); };

export const seconds = (ms: number) => { const t = (ms / 1000).toFixed(1); return `${tr(t.replace(".", ","), t)} s`; };

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
const MONTHS_EN = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export function date(unixSecs: number): string {
  if (!unixSecs) return "—";
  const d = new Date(unixSecs * 1000);
  return tr(`${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`, `${MONTHS_EN[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`);
}

/** Fecha con hora: "12 nov 2026, 18:05". Si es de hoy o de ayer, lo dice así. */
export function dateTime(unixSecs: number): string {
  if (!unixSecs) return "—";
  const d = new Date(unixSecs * 1000);
  const hm = `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
  const today = new Date();
  const yesterday = new Date(today.getTime() - 86400000);
  const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  if (same(d, today)) return tr(`hoy a las ${hm}`, `today at ${hm}`);
  if (same(d, yesterday)) return tr(`ayer a las ${hm}`, `yesterday at ${hm}`);
  return `${date(unixSecs)}, ${hm}`;
}

/** Diferencia con signo: "+1,2 GB" o "−300 MB". */
export const delta = (bytes: number) => (bytes >= 0 ? "+" : "−") + size(Math.abs(bytes));

/** "C:\\" → "C:" */
export const driveLetter = (root: string) => {
  const r = root.replace(/[\\/]+$/, "") || root;
  // Una carpeta analizada ("D:\\Fotos\\2024") se nombra por su última parte.
  const parts = r.split(/[\\/]/);
  return parts.length > 1 ? parts[parts.length - 1] : r;
};

export function driveLabel(d: { root: string; name: string }) {
  const letter = driveLetter(d.root);
  return d.name && d.name !== letter ? `${d.name} (${letter})` : tr(`Disco local (${letter})`, `Local disk (${letter})`);
}
