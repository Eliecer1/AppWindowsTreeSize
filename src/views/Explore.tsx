import { useCallback, useEffect, useRef, useState } from "react";
import type { Ctx } from "../App";
import { CloudIcon, CloudNote, FileStatus, NodeIcon, RefreshIcon, StackBar, Sunburst, Treemap } from "../components";
import { CATEGORY, date, driveLetter, fmtPct, int, pct, size } from "../format";
import type { FolderView, NodeInfo, SubNode } from "../types";

const PAGE = 300;
type Mode = "blocks" | "rings";

function readMode(): Mode {
  try { return localStorage.getItem("awts-explore-mode") === "rings" ? "rings" : "blocks"; } catch { return "blocks"; }
}

export function Explore({ ctx, target }: { ctx: Ctx; target: { root: string; id: number; select?: NodeInfo; at: number } | null }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = ctx.activeRoot && scanned.some((d) => d.root === ctx.activeRoot) ? ctx.activeRoot : scanned[0]?.root;
  const [folderId, setFolderId] = useState(0);
  const [folder, setFolder] = useState<FolderView | null>(null);
  const [children, setChildren] = useState<NodeInfo[]>([]);
  const [sub, setSub] = useState<SubNode | null>(null);
  const [sel, setSel] = useState<NodeInfo | null>(null);
  // Selección múltiple (Ctrl y Mayús), por id.
  const [multi, setMulti] = useState<Set<number>>(new Set());
  const anchor = useRef<number | null>(null);
  const [confirm, setConfirm] = useState<null | "trash" | "delete">(null);
  const [refreshing, setRefreshing] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; item: NodeInfo } | null>(null);
  const [mode, setModeState] = useState<Mode>(readMode);
  // Historial para Atrás y Adelante.
  const back = useRef<number[]>([]);
  const fwd = useRef<number[]>([]);

  const setMode = (m: Mode) => { setModeState(m); try { localStorage.setItem("awts-explore-mode", m); } catch { /* sin almacenamiento */ } };

  const navigate = useCallback((id: number, push = true) => {
    setFolderId((cur) => {
      if (push && cur !== id) { back.current.push(cur); fwd.current = []; }
      return id;
    });
    setSel(null);
    setMulti(new Set());
    setConfirm(null);
    setMenu(null);
  }, []);

  useEffect(() => {
    if (!target) return;
    navigate(target.id);
    setSel(target.select ?? null);
  }, [target, navigate]);

  useEffect(() => {
    if (!root) return;
    let live = true;
    api.getFolder(root, folderId, PAGE)
      .then((f) => { if (live) { setFolder(f); setChildren(f.children); } })
      .catch(() => { if (live && folderId !== 0) setFolderId(0); });
    api.getSubtree(root, folderId, mode === "rings" ? 3 : 2).then((s) => { if (live) setSub(s); }).catch(() => {});
    return () => { live = false; };
  }, [api, root, folderId, version, ctx.live, mode]);

  const n = folder?.node;
  const picked = children.filter((c) => multi.has(c.id));

  const reveal = async (item: NodeInfo) => {
    try {
      await api.openInExplorer(item.path, item.isDir);
      if (api.isDemo) ctx.toast(`En la app de escritorio se abre el Explorador en ${item.path}`);
    } catch (e) { ctx.toast(String(e), true); }
  };
  const copyPath = async (item: NodeInfo) => {
    try { await navigator.clipboard.writeText(item.path); ctx.toast("Ruta copiada."); } catch { ctx.toast("No se pudo copiar la ruta.", true); }
  };
  /** Lo que afectan los botones de papelera y borrar: la selección múltiple o el elemento elegido. */
  const targets = (): NodeInfo[] => (picked.length > 1 ? picked : sel ? [sel] : n && n.id !== 0 ? [n] : []);

  const remove = async (how: "trash" | "delete") => {
    if (!root || !n) return;
    const list = targets();
    setConfirm(null);
    try {
      const ids = list.map((x) => x.id);
      const freed = how === "trash" ? await api.moveToTrash(root, ids) : await api.deletePermanently(root, ids);
      const what = list.length === 1 ? list[0].name : `${list.length} elementos`;
      ctx.toast(how === "trash" ? `${what} se envió a la papelera. Liberaste ${size(freed)}.` : `${what} se borró definitivamente. Liberaste ${size(freed)}.`);
      if (list.some((x) => x.id === n.id) && n.parent !== null) navigate(n.parent, false);
      setSel(null);
      setMulti(new Set());
      ctx.bump();
    } catch (e) { ctx.toast(String(e), true); }
  };

  const refresh = async (item?: NodeInfo) => {
    if (!root || !n) return;
    const f = item ?? n;
    setConfirm(null);
    // La raíz es el disco entero: eso es un análisis completo, con su barra de progreso.
    if (f.id === 0) { ctx.startScan(root); return; }
    setRefreshing(true);
    try {
      const id = await api.rescanFolder(root, f.id);
      if (id === null) {
        ctx.toast(`${f.name} ya no existe. Se quitó de la lista.`);
        if (f.id === n.id) navigate(n.parent ?? 0, false);
      } else {
        const now = (await api.getFolder(root, id, 0)).node;
        ctx.toast(now.size === f.size ? `${f.name} está al día: ${size(now.size)}.` : `${f.name} actualizada: antes ${size(f.size)}, ahora ${size(now.size)}.`);
      }
      ctx.bump();
    } catch (e) { ctx.toast(String(e), true); }
    finally { setRefreshing(false); }
  };

  const exportReport = async () => {
    if (!root || !n) return;
    try {
      const base = (n.id === 0 ? driveLetter(root).replace(":", "") : n.name).replace(/[\\/:*?"<>|]/g, "_");
      const path = await api.pickSavePath(`Informe ${base}.html`);
      if (!path) return;
      await api.exportReport(root, n.id, path);
      ctx.toast("Informe guardado.");
    } catch (e) { ctx.toast(String(e), true); }
  };

  const open = (c: NodeInfo) => {
    setConfirm(null);
    if (c.isDir) navigate(c.id); else setSel(c);
  };

  /** Clic en una fila: normal abre, Ctrl suma o quita de la selección, Mayús selecciona un rango. */
  const click = (c: NodeInfo, e: React.MouseEvent) => {
    setMenu(null);
    const i = children.findIndex((x) => x.id === c.id);
    if (e.ctrlKey || e.metaKey) {
      setMulti((m) => {
        const next = new Set(m);
        if (next.size === 0 && sel) next.add(sel.id);
        if (next.has(c.id)) next.delete(c.id); else next.add(c.id);
        return next;
      });
      setSel(c);
      anchor.current = i;
      return;
    }
    if (e.shiftKey && anchor.current !== null) {
      const [a, b] = [Math.min(anchor.current, i), Math.max(anchor.current, i)];
      setMulti(new Set(children.slice(a, b + 1).map((x) => x.id)));
      setSel(c);
      return;
    }
    anchor.current = i;
    setMulti(new Set());
    open(c);
  };

  const showMenu = (item: NodeInfo, x: number, y: number) => {
    if (!multi.has(item.id)) { setMulti(new Set()); setSel(item); }
    setMenu({ x: Math.min(x, window.innerWidth - 240), y: Math.min(y, window.innerHeight - 300), item });
  };

  // Teclado: flechas para moverse, Intro para entrar, Retroceso para subir, Alt+flechas para Atrás y Adelante,
  // Supr para la papelera, Mayús+Supr para borrar, Ctrl+A para seleccionar todo, Esc para cancelar.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)) return;
      if (document.querySelector(".palette")) return;
      if (!n) return;
      const idx = sel ? children.findIndex((c) => c.id === sel.id) : -1;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const next = children[Math.max(0, Math.min(children.length - 1, idx + (e.key === "ArrowDown" ? 1 : -1)))];
        if (next) {
          setSel(next);
          if (e.shiftKey && anchor.current !== null) {
            const j = children.indexOf(next);
            const [a, b] = [Math.min(anchor.current, j), Math.max(anchor.current, j)];
            setMulti(new Set(children.slice(a, b + 1).map((x) => x.id)));
          } else { anchor.current = children.indexOf(next); setMulti(new Set()); }
          document.getElementById(`row-${next.id}`)?.scrollIntoView({ block: "nearest" });
        }
      } else if (e.key === "Enter" && sel) {
        e.preventDefault();
        if (sel.isDir) navigate(sel.id); else reveal(sel);
      } else if ((e.key === "Backspace" && !e.altKey) || (e.key === "ArrowUp" && e.altKey)) {
        e.preventDefault();
        if (n.parent !== null) navigate(n.parent);
      } else if (e.key === "ArrowLeft" && e.altKey) {
        e.preventDefault();
        const prev = back.current.pop();
        if (prev !== undefined) { fwd.current.push(n.id); navigate(prev, false); }
      } else if (e.key === "ArrowRight" && e.altKey) {
        e.preventDefault();
        const next = fwd.current.pop();
        if (next !== undefined) { back.current.push(n.id); navigate(next, false); }
      } else if (e.key === "Delete") {
        e.preventDefault();
        if (targets().length && !ctx.scanning) setConfirm(e.shiftKey ? "delete" : "trash");
      } else if (e.key.toLowerCase() === "a" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        setMulti(new Set(children.map((c) => c.id)));
      } else if (e.key === "Escape") {
        setMenu(null); setConfirm(null); setMulti(new Set());
      } else if (e.key === "F5") {
        e.preventDefault();
        refresh();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    return () => { window.removeEventListener("click", close); window.removeEventListener("scroll", close, true); };
  }, [menu]);

  if (!root) {
    return (
      <div className="view">
        <div className="head"><div><h2>Explorar</h2></div></div>
        <div className="card empty-state">
          <h3>Primero analiza un disco</h3>
          <p>Cuando termine el análisis podrás recorrer cada carpeta, ver qué ocupa más y abrirla en el Explorador de Windows.</p>
          {ctx.primary && <button className="btn primary" disabled={!!ctx.scanning} onClick={() => ctx.startScan(ctx.primary!.root)}>Analizar {driveLetter(ctx.primary.root)}</button>}
        </div>
      </div>
    );
  }
  if (!folder || !n) return null;

  const live = ctx.scanning?.root === root ? ctx.scanning : null;
  const shown = sel ?? n;
  const biggest = children[0]?.size || 1;
  // En cuanto la carpeta tiene archivos, la lista gana una columna que dice dónde está cada uno.
  const hasFiles = children.some((c) => !c.isDir);
  const many = picked.length > 1;
  const pickedSize = picked.reduce((a, c) => a + c.size, 0);
  const tlist = targets();
  const tsize = tlist.reduce((a, c) => a + c.size, 0);
  const tname = tlist.length === 1 ? tlist[0].name : `${tlist.length} elementos`;
  const nested = new Map((sub?.children ?? []).map((c) => [c.id, c]));

  return (
    <div className="view">
      <div className="head">
        <div><h2>Explorar</h2><p>Haz clic para entrar, clic derecho para más opciones. Ctrl y Mayús seleccionan varios.</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} title={d.root}
              onClick={() => { ctx.setActiveRoot(d.root); back.current = []; fwd.current = []; navigate(0, false); }}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>

      <div className="crumbs-bar">
        <nav className="crumbs" aria-label="Ruta">
          <button aria-label="Atrás (Alt+←)" title="Atrás (Alt+←)" disabled={!back.current.length}
            onClick={() => { const p = back.current.pop(); if (p !== undefined) { fwd.current.push(n.id); navigate(p, false); } }}>‹</button>
          {folder.ancestors.map((a, i) => (
            <span key={a.id} style={{ display: "contents" }}>
              {i > 0 && <span className="sep">›</span>}
              <button onClick={() => navigate(a.id)}>{i === 0 ? driveLetter(root) : a.name}</button>
            </span>
          ))}
        </nav>
        <div className="viewtoggle" role="group" aria-label="Vista">
          <button aria-pressed={mode === "blocks"} onClick={() => setMode("blocks")}>Bloques</button>
          <button aria-pressed={mode === "rings"} onClick={() => setMode("rings")}>Anillos</button>
        </div>
        <button className="btn" onClick={exportReport} title="Guarda un informe de esta carpeta en HTML o CSV">Exportar</button>
        {live ? (
          <span className="live-chip" role="status">
            <RefreshIcon spin /> Analizando… <span className="num">{int(live.files)}</span> archivos, <span className="num">{size(live.bytes)}</span>.
          </span>
        ) : (
          <button className="btn refresh" disabled={refreshing || !!ctx.scanning} onClick={() => refresh()}
            title={n.id === 0 ? "Vuelve a analizar todo (F5)" : "Vuelve a leer solo esta carpeta y lo que tiene dentro (F5)"}>
            <RefreshIcon spin={refreshing || !!ctx.scanning} />
            {refreshing ? "Refrescando…" : n.id === 0 ? "Analizar de nuevo" : "Refrescar carpeta"}
          </button>
        )}
      </div>

      <div className="explore">
        <div className={`card list ${hasFiles ? "with-status" : ""}`} role="list">
          {children.length === 0 && <p className="muted" style={{ padding: 10 }}>Esta carpeta está vacía.</p>}
          {hasFiles && (
            <div className="row head-row" aria-hidden="true">
              <span style={{ width: 16 }} /><span>Nombre</span><span>Estado</span><span>Tamaño</span>
            </div>
          )}
          {children.map((c) => (
            <button key={c.id} id={`row-${c.id}`} role="listitem" className={`row ${multi.has(c.id) ? "multi" : sel?.id === c.id ? "sel" : ""}`}
              onClick={(e) => click(c, e)} onContextMenu={(e) => { e.preventDefault(); showMenu(c, e.clientX, e.clientY); }} title={c.path}>
              <NodeIcon node={c} px={16} />
              <span className="nm">{c.name}{!c.isDir && c.ext && <small>.{c.ext}</small>}</span>
              {hasFiles && <span className="st">{!c.isDir && <FileStatus node={c} />}</span>}
              {c.cloudOnly
                ? <span className="sz cloud" title={`Pesa ${size(c.logical)}, pero está solo en la nube`}>{size(c.logical)}</span>
                : <span className="sz">{size(c.size)}{c.cloud > 0 && <span className="cloud" style={{ marginLeft: 5 }} title={`${size(c.cloud)} más están solo en la nube`}><CloudIcon px={12} /></span>}</span>}
              <span className="bar"><i style={{ width: `${(c.size / biggest) * 100}%`, background: CATEGORY[c.category].color }} /></span>
            </button>
          ))}
          {children.length < folder.totalChildren && (
            <div className="more"><button className="btn" onClick={async () => {
              const more = await api.getChildren(root, n.id, children.length, PAGE);
              setChildren((c) => [...c, ...more]);
            }}>Mostrar más ({int(folder.totalChildren - children.length)} restantes)</button></div>
          )}
        </div>

        <div className="tm-wrap">
          {mode === "rings" && sub
            ? <Sunburst root={sub} onPick={(s) => { if (s.isDir) navigate(s.id); else { const c = children.find((x) => x.id === s.id); if (c) setSel(c); } }} />
            : <Treemap items={children} onPick={open} nested={nested} onPickId={(id) => navigate(id)} onMenu={showMenu} />}
          <div className="card details">
            <div style={{ minWidth: 0 }}>
              {many ? (
                <>
                  <h3>{picked.length} elementos seleccionados</h3>
                  <p><span className="num">{size(pickedSize)}</span> en disco · {fmtPct(pct(pickedSize, folder.ancestors[0].size))} del espacio analizado</p>
                </>
              ) : (
                <>
                  <h3>{shown.id === 0 ? driveLetter(root) : shown.name}</h3>
                  <p>
                    <span className="num">{size(shown.size)}</span> en disco · {fmtPct(pct(shown.size, folder.ancestors[0].size))} del espacio analizado ·{" "}
                    {shown.isDir ? <>{int(shown.files)} archivos{shown.dirs > 0 && <> en {int(shown.dirs)} carpetas</>}</> : <>{shown.ext ? `.${shown.ext}` : "sin extensión"} · {CATEGORY[shown.category].name}</>}
                    {" "}· modificado {date(shown.modified)}
                  </p>
                  <CloudNote node={shown} />
                  {shown.hardlink && <p className="cloud-note">Este archivo tiene otro nombre en otra carpeta (enlace duro). Su espacio ({size(shown.logical)}) se contó allí, así que aquí no suma.</p>}
                  {shown.id === n.id && <div style={{ marginTop: 10, maxWidth: 420 }}><StackBar parts={folder.categories} total={n.size} /></div>}
                </>
              )}
            </div>
            <div className="actions">
              {!many && <button className="btn" onClick={() => reveal(shown)}>Abrir en el Explorador</button>}
              {tlist.length > 0 && tlist.every((x) => x.id !== 0) && (
                <button className="btn" disabled={!!live} title={live ? "Espera a que termine el análisis" : "Supr"} onClick={() => setConfirm("trash")}>Enviar a la papelera</button>
              )}
            </div>
            {confirm && !live && tlist.length > 0 && (
              <div className={`confirm ${confirm === "delete" ? "danger" : ""}`} style={{ gridColumn: "1 / -1" }}>
                <b>{confirm === "trash" ? `¿Enviar ${tname} a la papelera?` : `¿Borrar ${tname} definitivamente?`}</b>
                <span>
                  {tsize > 0 ? `Se liberarán ${size(tsize)}.` : "No liberará espacio en tu disco."}{" "}
                  {confirm === "trash" ? "Podrás recuperarlo desde la papelera de reciclaje." : "No pasa por la papelera: no se podrá recuperar."}
                </span>
                {tlist.some((x) => x.cloud > 0) && (
                  <span><b>Ojo:</b> hay archivos en OneDrive u otra nube. Borrarlos aquí también los borra de la nube, y no libera espacio por esa parte.</span>
                )}
                <div className="actions">
                  <button className="btn primary" onClick={() => remove(confirm)}>{confirm === "trash" ? "Sí, enviar a la papelera" : "Sí, borrar definitivamente"}</button>
                  <button className="btn" onClick={() => setConfirm(null)}>Cancelar</button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {menu && (
        <div className="ctxmenu" style={{ left: menu.x, top: menu.y }} role="menu" onClick={(e) => e.stopPropagation()}>
          {many ? <button disabled>{picked.length} elementos · {size(pickedSize)}</button> : (
            <>
              {menu.item.isDir && <button role="menuitem" onClick={() => { setMenu(null); navigate(menu.item.id); }}>Entrar <kbd>Intro</kbd></button>}
              <button role="menuitem" onClick={() => { setMenu(null); reveal(menu.item); }}>Abrir en el Explorador</button>
              <button role="menuitem" onClick={() => { setMenu(null); copyPath(menu.item); }}>Copiar ruta</button>
              {menu.item.isDir && <button role="menuitem" disabled={!!ctx.scanning || refreshing} onClick={() => { setMenu(null); refresh(menu.item); }}>Refrescar esta carpeta</button>}
            </>
          )}
          <hr />
          <button role="menuitem" disabled={!!live} onClick={() => { setMenu(null); setConfirm("trash"); }}>Enviar a la papelera <kbd>Supr</kbd></button>
          <button role="menuitem" className="danger" disabled={!!live} onClick={() => { setMenu(null); setConfirm("delete"); }}>Borrar definitivamente <kbd>Mayús+Supr</kbd></button>
        </div>
      )}
    </div>
  );
}
