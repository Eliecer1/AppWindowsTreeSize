import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { NodeIcon } from "../components";
import { dateTime, delta, driveLetter, size } from "../format";
import type { Change, Changes as ChangesData } from "../types";

const d = (c: Change) => c.new - c.old;

/** Qué cambió en un disco entre los dos últimos análisis: lo que creció, lo nuevo y lo que se borró. */
export function Changes({ ctx }: { ctx: Ctx }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((x) => x.scanned);
  const root = scanned.find((x) => x.root === ctx.activeRoot)?.root ?? scanned[0]?.root;
  const [data, setData] = useState<ChangesData | null | undefined>(undefined);
  const [path, setPath] = useState<Change[]>([]);

  useEffect(() => {
    if (!root) return;
    let live = true;
    api.getChanges(root).then((c) => { if (live) { setData(c); setPath(c ? [c.tree] : []); } }).catch(() => live && setData(null));
    return () => { live = false; };
  }, [api, root, version]);

  const head = (
    <div className="head">
      <div><h2>Cambios</h2><p>Qué creció, qué es nuevo y qué se borró entre los dos últimos análisis.</p></div>
      <div className="actions">
        {scanned.map((x) => (
          <button key={x.root} className={`btn ${x.root === root ? "primary" : ""}`} onClick={() => ctx.setActiveRoot(x.root)}>{driveLetter(x.root)}</button>
        ))}
      </div>
    </div>
  );

  if (!root || data === null) {
    return (
      <div className="view">
        {head}
        <div className="card empty-state">
          <h3>{root ? "Todavía no hay con qué comparar" : "Primero analiza un disco"}</h3>
          <p>La app guarda cada análisis. La próxima vez que analices {root ? driveLetter(root) : "el disco"}, aquí verás qué carpetas crecieron, qué archivos son nuevos y qué se borró desde la vez anterior.</p>
          {root && <button className="btn primary" disabled={!!ctx.scanning} onClick={() => ctx.startScan(root)}>Analizar {driveLetter(root)} de nuevo</button>}
        </div>
      </div>
    );
  }
  if (!data || !path.length) return <div className="view">{head}</div>;

  const cur = path[path.length - 1];
  const kids = [...cur.children].sort((a, b) => Math.abs(d(b)) - Math.abs(d(a)));
  const biggest = Math.max(1, ...kids.map((k) => Math.abs(d(k))));
  const grew = kids.filter((k) => d(k) > 0).reduce((a, k) => a + d(k), 0);
  const shrank = kids.filter((k) => d(k) < 0).reduce((a, k) => a - d(k), 0);
  const fullPath = (upto: Change[]) => {
    const [first, ...rest] = upto;
    return rest.reduce((p, c) => (p.endsWith("\\") || p.endsWith("/") ? p : p + "\\") + c.name, first.name);
  };
  const explore = async (c: Change) => {
    // Un archivo se muestra dentro de su carpeta.
    const id = await api.locate(root, fullPath(c.isDir ? [...path, c] : path)).catch(() => null);
    if (id === null) { ctx.toast(`${c.name} ya no está en el análisis actual.`); return; }
    ctx.explore(root, id);
  };

  return (
    <div className="view">
      {head}
      <div className="card changes-sum">
        <div>
          <span className="eyebrow">Del {dateTime(data.from)} al {dateTime(data.to)}</span>
          <div className={`bignum ${d(cur) > 0 ? "up" : "down"}`}>{delta(d(cur))}</div>
          <p className="muted" style={{ margin: 0 }}>
            {path.length === 1 ? driveLetter(root) : cur.name} pasó de {size(cur.old)} a {size(cur.new)}.
            {grew > 0 && <> Creció {size(grew)}</>}{shrank > 0 && <>{grew > 0 ? " y se" : " Se"} liberaron {size(shrank)}</>}{(grew > 0 || shrank > 0) && "."}
          </p>
        </div>
      </div>

      <nav className="crumbs" aria-label="Ruta">
        {path.map((c, i) => (
          <span key={i} style={{ display: "contents" }}>
            {i > 0 && <span className="sep">›</span>}
            <button onClick={() => setPath(path.slice(0, i + 1))}>{i === 0 ? driveLetter(root) : c.name}</button>
          </span>
        ))}
      </nav>

      <div className="card list changes-list" role="list">
        {kids.length === 0 && <p className="muted" style={{ padding: 10 }}>Dentro de esta carpeta no hay cambios de más de 1 MB.</p>}
        {kids.map((k, i) => {
          const v = d(k);
          return (
            <div key={i} role="listitem" className="row change-row">
              <button className="nm" disabled={!k.isDir || !k.children.length} onClick={() => setPath([...path, k])} title={k.isDir && k.children.length ? "Ver qué cambió dentro" : undefined}>
                <NodeIcon node={{ isDir: k.isDir, category: "other" }} px={16} />
                <span className="t">{k.name}</span>
                {!k.existed && <span className="chip warn">Nuevo</span>}
                {!k.exists && <span className="chip ok">Borrado</span>}
              </button>
              <span className="muted num">{k.existed ? size(k.old) : "—"} → {k.exists ? size(k.new) : "—"}</span>
              <b className={`num ${v > 0 ? "up" : "down"}`}>{delta(v)}</b>
              {k.exists ? <button className="btn small" onClick={() => explore(k)}>Explorar</button> : <span />}
              <span className="bar"><i style={{ width: `${(Math.abs(v) / biggest) * 100}%`, background: v > 0 ? "var(--warn)" : "var(--ok)" }} /></span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
