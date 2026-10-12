import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { CloudIcon, Legend, Ring, StackBar } from "../components";
import { CATEGORY, driveLabel, driveLetter, int, pct, seconds, size } from "../format";
import type { CategorySize, Drive, NodeInfo } from "../types";

function ScanStatus({ ctx, drive }: { ctx: Ctx; drive: Drive }) {
  const p = ctx.scanning?.root === drive.root ? ctx.scanning : null;
  if (p) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <small className="muted">
          Leyendo archivos… <span className="num">{int(p.files)}</span> archivos, <span className="num">{size(p.bytes)}</span> · <span className="num">{seconds(p.elapsedMs)}</span>
        </small>
        <div className="progress"><i style={{ width: `${pct(p.bytes, drive.total - drive.available)}%` }} /></div>
      </div>
    );
  }
  const done = ctx.lastScan[drive.root];
  if (done) {
    return (
      <small className="muted">
        {done.cancelled ? "Análisis cancelado" : "Analizado"} · <span className="num">{int(done.files)}</span> archivos en <span className="num">{seconds(done.elapsedMs)}</span>
        {done.errors > 0 && <> · {int(done.errors)} carpetas sin permiso de lectura</>}
      </small>
    );
  }
  return <small className="muted">{drive.scanned ? "Análisis listo." : "Todavía no se ha analizado este disco."}</small>;
}

export function Home({ ctx }: { ctx: Ctx }) {
  const { api, primary, drives, version } = ctx;
  const [cats, setCats] = useState<CategorySize[]>([]);
  const [rootInfo, setRootInfo] = useState<NodeInfo | null>(null);
  const [largest, setLargest] = useState<NodeInfo[]>([]);
  const [savings, setSavings] = useState<{ total: number; safe: number; review: number } | null>(null);

  useEffect(() => {
    if (!primary?.scanned) return;
    const r = primary.root;
    api.getFolder(r, 0, 0).then((f) => { setCats(f.categories); setRootInfo(f.node); }).catch(() => {});
    api.largestFiles(r, 0, 10).then(setLargest).catch(() => {});
    api.cleanupSuggestions(r).then((s) => setSavings({
      total: s.reduce((a, x) => a + x.size, 0),
      safe: s.filter((x) => x.risk === "safe").length,
      review: s.filter((x) => x.risk !== "safe").length,
    })).catch(() => {});
  }, [api, primary?.root, primary?.scanned, version]);

  if (!primary) {
    return <div className="view"><div className="card empty-state"><h3>Buscando discos…</h3></div></div>;
  }

  const used = primary.total - primary.available;
  const usage = pct(used, primary.total);
  const busy = ctx.scanning !== null;
  const others = drives.filter((d) => d.root !== primary.root);
  const catTotal = cats.reduce((a, c) => a + c.size, 0);
  const top = largest[0]?.size ?? 1;

  return (
    <div className="view">
      <div className="head">
        <div><h2>Tu espacio en disco</h2><p>Analiza un disco para ver qué lo ocupa y qué puedes liberar.</p></div>
        <div className="actions">
          {ctx.scanning
            ? <button className="btn" onClick={() => api.cancelScan()}>Cancelar análisis</button>
            : <button className="btn primary" onClick={() => ctx.startScan(primary.root)}>{primary.scanned ? "Analizar de nuevo" : `Analizar ${driveLetter(primary.root)}`}</button>}
        </div>
      </div>

      <div className="bento">
        <section className="card b-drive">
          <div className="ring">
            <Ring used={used} total={primary.total} />
            <div className="c"><b>{Math.round(usage)}%</b><small>en uso</small></div>
          </div>
          <div className="drive-meta">
            <span className="eyebrow">{primary.removable ? "Unidad extraíble" : "Disco local"}</span>
            <h3>
              {driveLabel(primary)} <span className="chip">{primary.kind}</span>
              {usage > 90 ? <span className="chip bad">Casi lleno</span> : usage > 75 ? <span className="chip warn">Poco espacio</span> : null}
            </h3>
            <div className="stats">
              <div className="stat"><small>Usado</small><b>{size(used)}</b></div>
              <div className="stat"><small>Libre</small><b>{size(primary.available)}</b></div>
              <div className="stat"><small>Capacidad</small><b>{size(primary.total)}</b></div>
              {rootInfo && rootInfo.cloud > 0 && (
                <div className="stat cloud" title={`${int(rootInfo.cloudFiles)} archivos de OneDrive u otra nube que no están descargados en este PC`}>
                  <small><CloudIcon px={12} /> Solo en la nube</small><b>{size(rootInfo.cloud)}</b>
                </div>
              )}
            </div>
            <ScanStatus ctx={ctx} drive={primary} />
            {primary.scanned && <div><button className="btn" onClick={() => ctx.explore(primary.root)}>Explorar {driveLetter(primary.root)}</button></div>}
          </div>
        </section>

        <section className="card b-save">
          <span className="eyebrow">Puedes liberar</span>
          {savings ? (
            <>
              <div className="bignum">{size(savings.total)}</div>
              <p style={{ margin: 0 }} className="muted">{savings.safe} sugerencias seguras y {savings.review} para revisar.</p>
              <div style={{ marginTop: "auto" }}><button className="btn primary" onClick={() => ctx.go("clean")}>Ver sugerencias</button></div>
            </>
          ) : (
            <p style={{ margin: 0 }} className="muted">Analiza {driveLetter(primary.root)} y aquí verás cuánto espacio puedes recuperar sin riesgo.</p>
          )}
        </section>

        <section className="card b-types">
          <span className="eyebrow">Qué ocupa el disco {driveLetter(primary.root)}</span>
          {cats.length ? (
            <>
              <div style={{ marginTop: 12 }}><StackBar parts={cats} total={catTotal} /></div>
              <Legend parts={cats} />
            </>
          ) : <p className="muted">El desglose por tipo de archivo aparece al terminar el análisis.</p>}
        </section>

        <section className="card b-drives">
          <span className="eyebrow">Otros discos</span>
          {others.length === 0 && <p className="muted" style={{ margin: 0 }}>No hay más discos conectados.</p>}
          {others.map((d) => (
            <div className="mini-drive" key={d.root}>
              <header>
                <b>{driveLabel(d)}</b>
                <span className="chip">{d.removable ? "Extraíble" : d.kind}</span>
              </header>
              <div className="progress"><i style={{ width: `${pct(d.total - d.available, d.total)}%` }} /></div>
              <small className="muted"><span className="num">{size(d.total - d.available)}</span> de <span className="num">{size(d.total)}</span></small>
              {ctx.scanning?.root === d.root ? <ScanStatus ctx={ctx} drive={d} /> : (
                <div className="actions">
                  {d.scanned && <button className="btn" onClick={() => ctx.explore(d.root)}>Explorar</button>}
                  <button className="btn" disabled={busy} onClick={() => ctx.startScan(d.root)}>{d.scanned ? "Analizar de nuevo" : "Analizar"}</button>
                </div>
              )}
            </div>
          ))}
        </section>

        {largest.length > 0 && (
          <section className="card b-big">
            <span className="eyebrow">Archivos más pesados en {driveLetter(primary.root)}</span>
            <div className="mini-list">
              {largest.map((f) => (
                <button key={f.id} className="mini-row" onClick={() => ctx.explore(primary.root, f.parent ?? 0, f)} title={f.path}>
                  <span><i className="dot" style={{ background: CATEGORY[f.category].color, marginRight: 8 }} />{f.name}</span>
                  <b className="num">{size(f.size)}</b>
                  <small>{f.path}</small>
                  <div className="bar"><i style={{ width: `${(f.size / top) * 100}%`, background: CATEGORY[f.category].color }} /></div>
                </button>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
