import { tr } from "../i18n";
import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { NodeIcon } from "../components";
import { date, driveLetter, int, size } from "../format";
import type { DupGroup, DupProgress } from "../types";

const MB = 1024 ** 2;

/** Archivos con exactamente el mismo contenido en varios sitios. */
export function Duplicates({ ctx }: { ctx: Ctx }) {
  const { api, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = scanned.find((d) => d.root === ctx.activeRoot)?.root ?? scanned[0]?.root;
  const [minSize, setMinSize] = useState(10 * MB);
  const [groups, setGroups] = useState<DupGroup[] | null>(null);
  const [busy, setBusy] = useState<DupProgress | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    const sub = api.onDupProgress((p) => setBusy((b) => (b ? p : b)));
    return () => { sub.then((off) => off()); };
  }, [api]);
  // Cambiar de disco descarta los resultados anteriores.
  useEffect(() => { setGroups(null); setChecked(new Set()); }, [root]);

  const search = async () => {
    if (!root) return;
    setBusy({ candidates: 0, checked: 0, bytes: 0 });
    setConfirm(false);
    try {
      const g = await api.findDuplicates(root, 0, minSize);
      setGroups(g);
      setChecked(new Set());
    } catch (e) {
      ctx.toast(String(e), true);
    } finally {
      setBusy(null);
    }
  };

  /** Marca todas las copias menos la más reciente de cada grupo. */
  const keepNewest = () => {
    const ids = new Set<number>();
    for (const g of groups ?? []) {
      const newest = [...g.files].sort((a, b) => b.modified - a.modified)[0];
      g.files.forEach((f) => f.id !== newest.id && ids.add(f.id));
    }
    setChecked(ids);
  };
  const toggle = (id: number) => { setConfirm(false); setChecked((c) => { const n = new Set(c); if (n.has(id)) n.delete(id); else n.add(id); return n; }); };

  if (!root) {
    return (
      <div className="view">
        <div className="head"><div><h2>{tr("Duplicados", "Duplicates")}</h2></div></div>
        <div className="card empty-state"><h3>{tr("Primero analiza un disco", "Scan a drive first")}</h3><p>{tr("Después podrás buscar archivos repetidos: fotos, videos o instaladores que tienes guardados más de una vez.", "Then you can look for repeated files: photos, videos or installers saved more than once.")}</p></div>
      </div>
    );
  }

  const all = (groups ?? []).flatMap((g) => g.files);
  const sel = all.filter((f) => checked.has(f.id));
  const selSize = sel.reduce((a, f) => a + f.size, 0);
  // Nunca se borran todas las copias de un grupo.
  const wipesGroup = (groups ?? []).some((g) => g.files.every((f) => checked.has(f.id)));
  const wasted = (groups ?? []).reduce((a, g) => a + g.wasted, 0);

  const trash = async () => {
    setConfirm(false);
    try {
      const freed = await api.moveToTrash(root, sel.map((f) => f.id));
      ctx.toast(tr(`Se enviaron ${sel.length} copias a la papelera. Liberaste ${size(freed)}.`, `Moved ${sel.length} copies to the Recycle Bin. You freed ${size(freed)}.`));
      setGroups((gs) => (gs ?? []).map((g) => ({ ...g, files: g.files.filter((f) => !checked.has(f.id)) })).filter((g) => g.files.length > 1)
        .map((g) => ({ ...g, wasted: g.size * (g.files.length - 1) })));
      setChecked(new Set());
      ctx.bump();
    } catch (e) { ctx.toast(String(e), true); }
  };

  return (
    <div className="view">
      <div className="head">
        <div><h2>{tr("Duplicados", "Duplicates")}</h2><p>{tr(`Archivos con exactamente el mismo contenido en ${driveLetter(root)}. Se compara el contenido, no solo el nombre.`, `Files with exactly the same content on ${driveLetter(root)}. Content is compared, not just the name.`)}</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} disabled={!!busy} onClick={() => ctx.setActiveRoot(d.root)}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>

      <div className="card filters">
        <label>{tr("Archivos de al menos", "Files of at least")}
          <select value={minSize} disabled={!!busy} onChange={(e) => setMinSize(Number(e.target.value))}>
            {[1 * MB, 10 * MB, 100 * MB, 1024 * MB].map((s) => <option key={s} value={s}>{size(s)}</option>)}
          </select>
        </label>
        {busy ? (
          <>
            <span className="muted">
              {tr("Comparando…", "Comparing…")} <span className="num">{int(busy.checked)}</span> {tr("de", "of")} <span className="num">{int(busy.candidates)}</span> {tr("archivos", "files")}, <span className="num">{size(busy.bytes)}</span> {tr("leídos", "read")}
            </span>
            <button className="btn" onClick={() => api.cancelDuplicates()}>{tr("Cancelar", "Cancel")}</button>
          </>
        ) : (
          <button className="btn primary" onClick={search}>{groups ? tr("Buscar de nuevo", "Search again") : tr("Buscar duplicados", "Find duplicates")}</button>
        )}
        <div className="sp" />
        {groups && groups.length > 0 && (
          <div className="filters-sum"><b className="num">{size(wasted)}</b><small className="muted">{tr("repetidos en", "repeated in")} {groups.length} {tr("grupos", "groups")}</small></div>
        )}
      </div>

      {groups && groups.length > 0 && (
        <div className="card selbar">
          {sel.length > 0 ? <span><b>{sel.length}</b> {tr("copias seleccionadas", "copies selected")} · <b className="num">{size(selSize)}</b></span> : <span className="muted">{tr("Marca las copias que sobran, o deja que la app elija.", "Tick the extra copies, or let the app choose.")}</span>}
          <button className="btn" onClick={keepNewest}>{tr("Dejar solo la más reciente de cada grupo", "Keep only the newest in each group")}</button>
          {sel.length > 0 && (confirm ? (
            <>
              <button className="btn primary" disabled={wipesGroup} onClick={trash}>{tr("Sí, enviar a la papelera", "Yes, move to Recycle Bin")}</button>
              <button className="btn" onClick={() => setConfirm(false)}>{tr("Cancelar", "Cancel")}</button>
            </>
          ) : (
            <button className="btn primary" disabled={wipesGroup} title={wipesGroup ? tr("Deja al menos una copia de cada archivo", "Keep at least one copy of each file") : undefined} onClick={() => setConfirm(true)}>{tr("Enviar a la papelera", "Move to Recycle Bin")}</button>
          ))}
          {wipesGroup && <span className="chip bad">{tr("Deja al menos una copia de cada archivo", "Keep at least one copy of each file")}</span>}
        </div>
      )}

      {groups && groups.length === 0 && (
        <div className="card empty-state"><h3>{tr(`No hay duplicados de más de ${size(minSize)}`, `No duplicates over ${size(minSize)}`)}</h3><p>{tr("Prueba con un tamaño menor si quieres revisar archivos más pequeños.", "Try a smaller size to check smaller files.")}</p></div>
      )}

      <div className="dup-groups">
        {(groups ?? []).map((g, gi) => (
          <section className="card dup" key={gi}>
            <header><b>{g.files[0].name}</b><span className="muted">{g.files.length} {tr("copias de", "copies of")} {size(g.size)}</span><b className="num up">{size(g.wasted)} {tr("repetidos", "repeated")}</b></header>
            {g.files.map((f) => (
              <label key={f.id} className={`row dup-row ${checked.has(f.id) ? "sel" : ""}`}>
                <input type="checkbox" checked={checked.has(f.id)} onChange={() => toggle(f.id)} />
                <NodeIcon node={f} px={16} />
                <span className="t" title={f.path}>{f.path}</span>
                <span className="muted num">{date(f.modified)}</span>
                <button className="btn small" onClick={(e) => { e.preventDefault(); ctx.explore(root, f.parent ?? 0, f); }}>{tr("Ver", "Show")}</button>
              </label>
            ))}
          </section>
        ))}
      </div>
    </div>
  );
}
