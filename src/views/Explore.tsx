import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { CloudIcon, CloudNote, NodeIcon, StackBar, Treemap } from "../components";
import { CATEGORY, date, driveLetter, fmtPct, int, pct, size } from "../format";
import type { FolderView, NodeInfo } from "../types";

const PAGE = 300;

export function Explore({ ctx, target }: { ctx: Ctx; target: { root: string; id: number; select?: NodeInfo; at: number } | null }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = ctx.activeRoot && scanned.some((d) => d.root === ctx.activeRoot) ? ctx.activeRoot : scanned[0]?.root;
  const [folderId, setFolderId] = useState(0);
  const [folder, setFolder] = useState<FolderView | null>(null);
  const [children, setChildren] = useState<NodeInfo[]>([]);
  const [sel, setSel] = useState<NodeInfo | null>(null);
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    if (!target) return;
    setFolderId(target.id);
    setSel(target.select ?? null);
  }, [target]);

  useEffect(() => {
    if (!root) return;
    let live = true;
    api.getFolder(root, folderId, PAGE)
      .then((f) => { if (live) { setFolder(f); setChildren(f.children); } })
      .catch(() => { if (live && folderId !== 0) setFolderId(0); });
    return () => { live = false; };
  }, [api, root, folderId, version]);

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
  if (!folder) return null;

  const n = folder.node;
  const shown = sel ?? n;
  const biggest = children[0]?.size || 1;

  const open = (c: NodeInfo) => {
    setConfirm(false);
    if (c.isDir) { setFolderId(c.id); setSel(null); } else setSel(c);
  };
  const reveal = async () => {
    try {
      await api.openInExplorer(shown.path, shown.isDir);
      if (api.isDemo) ctx.toast(`En la app de escritorio se abre el Explorador en ${shown.path}`);
    } catch (e) { ctx.toast(String(e), true); }
  };
  const trash = async () => {
    setConfirm(false);
    try {
      const freed = await api.moveToTrash(root, [shown.id]);
      ctx.toast(`${shown.name} se envió a la papelera. Liberaste ${size(freed)}.`);
      if (shown.id === n.id && n.parent !== null) setFolderId(n.parent);
      setSel(null);
      ctx.bump();
    } catch (e) { ctx.toast(String(e), true); }
  };
  const loadMore = async () => {
    const more = await api.getChildren(root, n.id, children.length, PAGE);
    setChildren((c) => [...c, ...more]);
  };

  return (
    <div className="view">
      <div className="head">
        <div><h2>Explorar</h2><p>Haz clic en un bloque para entrar. Cada color es un tipo de archivo.</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`}
              onClick={() => { ctx.setActiveRoot(d.root); setFolderId(0); setSel(null); }}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>

      <nav className="crumbs" aria-label="Ruta">
        {folder.ancestors.map((a, i) => (
          <span key={a.id} style={{ display: "contents" }}>
            {i > 0 && <span className="sep">›</span>}
            <button onClick={() => { setFolderId(a.id); setSel(null); }}>{i === 0 ? driveLetter(root) : a.name}</button>
          </span>
        ))}
      </nav>

      <div className="explore">
        <div className="card list" role="list">
          {children.length === 0 && <p className="muted" style={{ padding: 10 }}>Esta carpeta está vacía.</p>}
          {children.map((c) => (
            <button key={c.id} role="listitem" className={`row ${sel?.id === c.id ? "sel" : ""}`} onClick={() => open(c)} title={c.path}>
              <NodeIcon node={c} px={16} />
              <span className="nm">{c.name}{!c.isDir && c.ext && <small>.{c.ext}</small>}</span>
              {c.cloudOnly
                ? <span className="sz cloud" title={`Pesa ${size(c.logical)}, pero está solo en la nube`}><CloudIcon /> {size(c.logical)}</span>
                : <span className="sz">{size(c.size)}{c.cloud > 0 && <span className="cloud" style={{ marginLeft: 5 }} title={`${size(c.cloud)} más están solo en la nube`}><CloudIcon px={12} /></span>}</span>}
              <span className="bar"><i style={{ width: `${(c.size / biggest) * 100}%`, background: CATEGORY[c.category].color }} /></span>
            </button>
          ))}
          {children.length < folder.totalChildren && (
            <div className="more"><button className="btn" onClick={loadMore}>Mostrar más ({int(folder.totalChildren - children.length)} restantes)</button></div>
          )}
        </div>

        <div className="tm-wrap">
          <Treemap items={children} onPick={open} />
          <div className="card details">
            <div style={{ minWidth: 0 }}>
              <h3>{shown.id === 0 ? driveLetter(root) : shown.name}</h3>
              <p>
                <span className="num">{size(shown.size)}</span> en disco · {fmtPct(pct(shown.size, folder.ancestors[0].size))} del espacio analizado ·{" "}
                {shown.isDir ? <>{int(shown.files)} archivos{shown.dirs > 0 && <> en {int(shown.dirs)} carpetas</>}</> : <>{shown.ext ? `.${shown.ext}` : "sin extensión"} · {CATEGORY[shown.category].name}</>}
                {" "}· modificado {date(shown.modified)}
              </p>
              <CloudNote node={shown} />
              {shown.id === n.id && <div style={{ marginTop: 10, maxWidth: 420 }}><StackBar parts={folder.categories} total={n.size} /></div>}
            </div>
            <div className="actions">
              <button className="btn" onClick={reveal}>Abrir en el Explorador</button>
              {shown.id !== 0 && <button className="btn" onClick={() => setConfirm(true)}>Enviar a la papelera</button>}
            </div>
            {confirm && (
              <div className="confirm" style={{ gridColumn: "1 / -1" }}>
                <b>¿Enviar {shown.name} a la papelera?</b>
                <span>{shown.size > 0 ? `Se liberarán ${size(shown.size)}.` : "No liberará espacio en tu disco."} Podrás recuperarlo desde la papelera de reciclaje.</span>
                {shown.cloud > 0 && (
                  <span><b>Ojo:</b> {shown.cloudOnly ? "este archivo está" : `${int(shown.cloudFiles)} archivos están`} en OneDrive u otra nube. Borrarlo aquí también lo borra de la nube, y no libera espacio por esa parte.</span>
                )}
                <div className="actions">
                  <button className="btn primary" onClick={trash}>Sí, enviar a la papelera</button>
                  <button className="btn" onClick={() => setConfirm(false)}>Cancelar</button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
