import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { apiPromise, type Api } from "./api";
import { Toast } from "./components";
import { CommandPalette } from "./views/CommandPalette";
import { Clean } from "./views/Clean";
import { Explore } from "./views/Explore";
import { Home } from "./views/Home";
import { Types } from "./views/Types";
import type { Drive, NodeInfo, ScanDone, ScanProgress } from "./types";

export type View = "home" | "explore" | "clean" | "types";

/** Lo que comparten todas las vistas. */
export interface Ctx {
  api: Api;
  drives: Drive[];
  primary: Drive | undefined;
  scanning: ScanProgress | null;
  lastScan: Record<string, ScanDone>;
  /** Sube cada vez que cambian los datos (análisis nuevo o algo enviado a la papelera). */
  version: number;
  bump: () => void;
  /** Sube cada poco mientras hay un análisis en curso, para que las vistas muestren los tamaños en vivo. */
  live: number;
  startScan: (root: string) => void;
  explore: (root: string, id?: number, select?: NodeInfo) => void;
  go: (v: View) => void;
  toast: (text: string, error?: boolean) => void;
  activeRoot: string | undefined;
  setActiveRoot: (root: string) => void;
}

const NAV: { view: View; label: string; icon: ReactNode }[] = [
  { view: "home", label: "Inicio", icon: <path d="M3 3h8v10H3zM13 3h8v6h-8zM13 11h8v10h-8zM3 15h8v6H3z" /> },
  { view: "explore", label: "Explorar", icon: <><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M12 3v18M12 12h9" /></> },
  { view: "clean", label: "Limpiar", icon: <path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /> },
  { view: "types", label: "Tipos de archivo", icon: <><circle cx="12" cy="12" r="9" /><path d="M12 3v9l7 5" /></> },
];

function readView(): View {
  try {
    const v = localStorage.getItem("awts-view");
    if (v === "home" || v === "explore" || v === "clean" || v === "types") return v;
  } catch { /* almacenamiento no disponible */ }
  return "home";
}

export default function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [drives, setDrives] = useState<Drive[]>([]);
  const [view, setView] = useState<View>(readView);
  const [scanning, setScanning] = useState<ScanProgress | null>(null);
  const [lastScan, setLastScan] = useState<Record<string, ScanDone>>({});
  const [version, setVersion] = useState(0);
  const [live, setLive] = useState(0);
  const [target, setTarget] = useState<{ root: string; id: number; select?: NodeInfo; at: number } | null>(null);
  const [activeRoot, setActiveRoot] = useState<string | undefined>();
  const [msg, setMsg] = useState<{ text: string; error?: boolean; at: number } | null>(null);
  const [palette, setPalette] = useState(false);

  const toast = useCallback((text: string, error = false) => setMsg({ text, error, at: Date.now() }), []);
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => { apiPromise.then(setApi); }, []);

  const refreshDrives = useCallback(async () => {
    if (!api) return;
    try { setDrives(await api.listDrives()); } catch (e) { toast(String(e), true); }
  }, [api, toast]);

  useEffect(() => {
    if (!api) return;
    refreshDrives();
    const subs = [
      api.onProgress((p) => setScanning(p)),
      api.onDone((d) => {
        setScanning(null);
        setLastScan((s) => ({ ...s, [d.root]: d }));
        setActiveRoot(d.root);
        refreshDrives();
        bump();
      }),
    ];
    return () => { subs.forEach((s) => s.then((off) => off())); };
  }, [api, refreshDrives, bump]);

  // Mientras se analiza, el árbol ya se puede consultar y va creciendo: las vistas se refrescan solas.
  const isScanning = scanning !== null;
  useEffect(() => {
    if (!isScanning) return;
    const t = setInterval(() => setLive((n) => n + 1), 1200);
    return () => clearInterval(t);
  }, [isScanning]);

  const go = useCallback((v: View) => {
    setView(v);
    try { localStorage.setItem("awts-view", v); } catch { /* sin almacenamiento */ }
  }, []);

  const primary = useMemo(() => drives.find((d) => d.root.toUpperCase().startsWith("C:")) ?? drives[0], [drives]);
  const root = activeRoot ?? drives.find((d) => d.scanned)?.root ?? primary?.root;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalette(true); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!api) return null;

  const ctx: Ctx = {
    api, drives, primary, scanning, lastScan, version, bump, live, toast, go,
    activeRoot: root,
    setActiveRoot,
    startScan: async (r) => {
      try {
        setScanning({ root: r, files: 0, dirs: 0, bytes: 0, elapsedMs: 0 });
        await api.startScan(r);
        setActiveRoot(r);
        refreshDrives();
      } catch (e) { setScanning(null); toast(String(e), true); }
    },
    explore: (r, id = 0, select) => {
      setActiveRoot(r);
      setTarget({ root: r, id, select, at: Date.now() });
      go("explore");
    },
  };

  const toggleTheme = () => {
    const el = document.documentElement;
    const cur = el.dataset.theme ?? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
    el.dataset.theme = cur === "dark" ? "light" : "dark";
  };

  return (
    <div className="window">
      <header className="titlebar">
        <img className="logo" src="/icon.png" alt="" />
        <h1>AppWindowsTreeSize</h1>
        {api.isDemo && <span className="demo-badge" title="Abierto en el navegador: se muestran datos de ejemplo">Datos de ejemplo</span>}
        <div className="sp" />
        <button className="search" onClick={() => setPalette(true)} aria-label="Buscar archivos y carpetas">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
          <span>Buscar archivos y carpetas…</span><kbd>Ctrl K</kbd>
        </button>
        <button className="iconbtn" onClick={toggleTheme} aria-label="Cambiar tema" title="Cambiar tema">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z" /></svg>
        </button>
      </header>
      <div className="body">
        <nav className="side" aria-label="Secciones">
          {NAV.map((n) => (
            <button key={n.view} aria-current={view === n.view ? "page" : undefined} onClick={() => go(n.view)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">{n.icon}</svg>
              {n.label}
            </button>
          ))}
          <div className="foot">
            {scanning ? <>Analizando {scanning.root}…</> : api.isDemo ? <>Vista previa con datos de ejemplo.</> : <>Listo.</>}
          </div>
        </nav>
        <main>
          {view === "home" && <Home ctx={ctx} />}
          {view === "explore" && <Explore ctx={ctx} target={target} />}
          {view === "clean" && <Clean ctx={ctx} />}
          {view === "types" && <Types ctx={ctx} />}
        </main>
      </div>
      {palette && root && <CommandPalette ctx={ctx} root={root} onClose={() => setPalette(false)} />}
      <Toast msg={msg} />
    </div>
  );
}
