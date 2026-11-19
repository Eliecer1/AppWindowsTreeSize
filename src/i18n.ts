// Idioma de la interfaz: español o inglés. Cada texto se escribe en los dos idiomas donde se usa,
// con tr("Español", "English"), así nunca falta una traducción.

export type Lang = "es" | "en";

function initial(): Lang {
  try {
    const v = localStorage.getItem("awts-lang");
    if (v === "es" || v === "en") return v;
  } catch { /* sin almacenamiento */ }
  return typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("es") ? "es" : "en";
}

let current: Lang = initial();
if (typeof document !== "undefined") document.documentElement.lang = current;

export const lang = () => current;

export function setLang(l: Lang) {
  current = l;
  document.documentElement.lang = l;
  try { localStorage.setItem("awts-lang", l); } catch { /* sin almacenamiento */ }
}

export const tr = (es: string, en: string) => (current === "en" ? en : es);

/** Formato de números del idioma actual. */
export const locale = () => (current === "en" ? "en-US" : "es-ES");
