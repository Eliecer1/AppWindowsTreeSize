import { tr } from "../i18n";
import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { CloudIcon, Legend, Ring, StackBar } from "../components";
import { CATEGORY, dateTime, driveLabel, driveLetter, int, pct, seconds, size } from "../format";
import type { CategorySize, Drive, NodeInfo } from "../types";

function ScanStatus({ ctx, drive }: { ctx: Ctx; drive: Drive }) {
  const p = ctx.scanning?.root === drive.root ? ctx.scanning : null;
  if (p) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <small className="muted">
          {tr("Leyendo archivos…", "Reading files…")} <span className="num">{int(p.files)}</span> {tr("archivos", "files")}, <span className="num">{size(p.bytes)}</span> · <span className="num">{seconds(p.elapsedMs)}</span>
        </small>
        <small className="muted">{tr("Ya puedes explorar: las carpetas se van llenando mientras avanza.", "You can explore already: folders fill in as the scan goes.")}</small>
        <div className="progress"><i style={{ width: `${pct(p.bytes, drive.total - drive.available)}%` }} /></div>
      </div>
    );
  }
  const done = ctx.lastScan[drive.root];
  if (done) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <small className="muted">
          {done.cancelled ? tr("Análisis cancelado", "Scan cancelled") : tr("Analizado", "Scanned")} · <span className="num">{int(done.files)}</span> {tr("archivos en", "files in")} <span className="num">{seconds(done.elapsedMs)}</span>
          {done.fast && <> · <span className="chip ok" title={tr("Se leyó la tabla de archivos de NTFS (MFT) directamente", "Read the NTFS file table (MFT) directly")}>{tr("Lectura rápida", "Fast scan")}</span></>}
          {done.errors > 0 && <> · {int(done.errors)} {tr("carpetas sin permiso de lectura", "folders could not be read")}</>}
        </small>
        {!ctx.isAdmin && !done.cancelled && <AdminHint ctx={ctx} errors={done.errors} />}
      </div>
    );
  }
  if (drive.scanned && drive.scannedAt) {
    return <small className="muted">{tr(`Análisis guardado del ${dateTime(drive.scannedAt)}. Analiza de nuevo para ver los cambios.`, `Saved scan from ${dateTime(drive.scannedAt)}. Scan again to see what changed.`)}</small>;
  }
  return <small className="muted">{drive.scanned ? tr("Análisis listo.", "Scan ready.") : tr("Todavía no se ha analizado este disco.", "This disk has not been scanned yet.")}</small>;
}

/** Invita a reiniciar como administrador: el análisis lee la MFT directamente y entra en todas las carpetas. */
function AdminHint({ ctx, errors }: { ctx: Ctx; errors: number }) {
  return (
    <div className="admin-hint">
      <span>
        {errors > 0 ? tr("Hay carpetas que no se pudieron leer. ", "Some folders could not be read. ") : ""}
        {tr("Como administrador el análisis es hasta 20 veces más rápido y entra en todas las carpetas.", "As administrator the scan is up to 20 times faster and reads every folder.")}
      </span>
      <button className="btn small" onClick={ctx.restartAsAdmin}>{tr("Reiniciar como administrador", "Restart as administrator")}</button>
    </div>
  );
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
  }, [api, primary?.root, primary?.scanned, version, ctx.live]);

  // Recorrer todo el árbol es lo más costoso: durante un análisis se hace cada ~5 segundos, no en cada actualización.
  const slowTick = Math.floor(ctx.live / 4);
  useEffect(() => {
    if (!primary?.scanned) return;
    const r = primary.root;
    api.largestFiles(r, 0, 10).then(setLargest).catch(() => {});
    api.cleanupSuggestions(r).then((s) => setSavings({
      total: s.reduce((a, x) => a + x.size, 0),
      safe: s.filter((x) => x.risk === "safe").length,
      review: s.filter((x) => x.risk !== "safe").length,
    })).catch(() => {});
  }, [api, primary?.root, primary?.scanned, version, slowTick]);

  if (!primary) {
    return <div className="view"><div className="card empty-state"><h3>{tr("Buscando discos…", "Looking for disks…")}</h3></div></div>;
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
        <div><h2>{tr("Tu espacio en disco", "Your disk space")}</h2><p>{tr("Analiza un disco para ver qué lo ocupa y qué puedes liberar.", "Scan a disk to see what fills it and what you can free up.")}</p></div>
        <div className="actions">
          {ctx.scanning
            ? <button className="btn" onClick={() => api.cancelScan()}>{tr("Cancelar análisis", "Cancel scan")}</button>
            : <>
                <button className="btn" onClick={ctx.scanFolder}>{tr("Analizar una carpeta…", "Scan a folder…")}</button>
                <button className="btn primary" onClick={() => ctx.startScan(primary.root)}>{primary.scanned ? tr("Analizar de nuevo", "Scan again") : `${tr("Analizar", "Scan")} ${driveLetter(primary.root)}`}</button>
              </>}
        </div>
      </div>

      <div className="bento">
        <section className="card b-drive">
          <div className="ring">
            <Ring used={used} total={primary.total} />
            <div className="c"><b>{Math.round(usage)}%</b><small>en uso</small></div>
          </div>
          <div className="drive-meta">
            <span className="eyebrow">{primary.removable ? tr("Unidad extraíble", "Removable drive") : tr("Disco local", "Local disk")}</span>
            <h3>
              {driveLabel(primary)} <span className="chip">{primary.kind}</span>
              {usage > 90 ? <span className="chip bad">{tr("Casi lleno", "Almost full")}</span> : usage > 75 ? <span className="chip warn">{tr("Poco espacio", "Low space")}</span> : null}
            </h3>
            <div className="stats">
              <div className="stat"><small>{tr("Usado", "Used")}</small><b>{size(used)}</b></div>
              <div className="stat"><small>{tr("Libre", "Free")}</small><b>{size(primary.available)}</b></div>
              <div className="stat"><small>{tr("Capacidad", "Capacity")}</small><b>{size(primary.total)}</b></div>
              {rootInfo && rootInfo.cloud > 0 && (
                <div className="stat cloud" title={tr(`${int(rootInfo.cloudFiles)} archivos de OneDrive u otra nube que no están descargados en este PC`, `${int(rootInfo.cloudFiles)} OneDrive or other cloud files not downloaded on this PC`)}>
                  <small><CloudIcon px={12} /> Solo en la nube</small><b>{size(rootInfo.cloud)}</b>
                </div>
              )}
            </div>
            <ScanStatus ctx={ctx} drive={primary} />
            {primary.scanned && <div><button className="btn" onClick={() => ctx.explore(primary.root)}>{tr("Explorar", "Explore")} {driveLetter(primary.root)}</button></div>}
          </div>
        </section>

        <section className="card b-save">
          <span className="eyebrow">{tr("Puedes liberar", "You can free up")}</span>
          {savings ? (
            <>
              <div className="bignum">{size(savings.total)}</div>
              <p style={{ margin: 0 }} className="muted">{tr(`${savings.safe} sugerencias seguras y ${savings.review} para revisar.`, `${savings.safe} safe suggestions and ${savings.review} to review.`)}</p>
              <div style={{ marginTop: "auto" }}><button className="btn primary" onClick={() => ctx.go("clean")}>{tr("Ver sugerencias", "See suggestions")}</button></div>
            </>
          ) : (
            <p style={{ margin: 0 }} className="muted">{tr(`Analiza ${driveLetter(primary.root)} y aquí verás cuánto espacio puedes recuperar sin riesgo.`, `Scan ${driveLetter(primary.root)} and you will see here how much space you can safely recover.`)}</p>
          )}
        </section>

        <section className="card b-types">
          <span className="eyebrow">{tr("Qué ocupa el disco", "What fills disk")} {driveLetter(primary.root)}</span>
          {cats.length ? (
            <>
              <div style={{ marginTop: 12 }}><StackBar parts={cats} total={catTotal} /></div>
              <Legend parts={cats} />
            </>
          ) : <p className="muted">{tr("El desglose por tipo de archivo aparece al terminar el análisis.", "The breakdown by file type appears when the scan finishes.")}</p>}
        </section>

        <section className="card b-drives">
          <span className="eyebrow">{tr("Otros discos y carpetas", "Other disks and folders")}</span>
          {others.length === 0 && <p className="muted" style={{ margin: 0 }}>{tr("No hay más discos conectados.", "No other disks connected.")}</p>}
          {others.map((d) => (
            <div className="mini-drive" key={d.root}>
              <header>
                <b>{d.folder ? d.name : driveLabel(d)}</b>
                <span className="chip">{d.removable ? tr("Extraíble", "Removable") : d.kind === "Carpeta" ? tr("Carpeta", "Folder") : d.kind === "Disco" ? tr("Disco", "Disk") : d.kind}</span>
              </header>
              {d.folder ? (
                <small className="muted" title={d.root}><span className="num">{size(d.total)}</span> {tr("en", "in")} {d.root}</small>
              ) : (
                <>
                  <div className="progress"><i style={{ width: `${pct(d.total - d.available, d.total)}%` }} /></div>
                  <small className="muted"><span className="num">{size(d.total - d.available)}</span> {tr("de", "of")} <span className="num">{size(d.total)}</span></small>
                </>
              )}
              {ctx.scanning?.root === d.root ? <ScanStatus ctx={ctx} drive={d} /> : (
                <div className="actions">
                  {d.scanned && <button className="btn" onClick={() => ctx.explore(d.root)}>{tr("Explorar", "Explore")}</button>}
                  <button className="btn" disabled={busy} onClick={() => ctx.startScan(d.root)}>{d.scanned ? tr("Analizar de nuevo", "Scan again") : tr("Analizar", "Scan")}</button>
                </div>
              )}
            </div>
          ))}
        </section>

        {largest.length > 0 && (
          <section className="card b-big">
            <span className="eyebrow">{tr("Archivos más pesados en", "Largest files on")} {driveLetter(primary.root)}</span>
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
