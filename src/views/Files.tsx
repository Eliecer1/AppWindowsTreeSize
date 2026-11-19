import { tr } from "../i18n";
import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { FileStatus, NodeIcon } from "../components";
import { CATEGORY, date, driveLetter, size } from "../format";
import type { Category, FileFilter, NodeInfo } from "../types";

const MB = 1024 ** 2;
const SIZES = [0, 10 * MB, 100 * MB, 1024 * MB];
const AGES: { days: number; label: () => string }[] = [
  { days: 0, label: () => tr("Cualquier fecha", "Any date") },
  { days: 180, label: () => tr("Sin cambios en 6 meses", "Unchanged for 6 months") },
  { days: 365, label: () => tr("Sin cambios en 1 año", "Unchanged for 1 year") },
  { days: 730, label: () => tr("Sin cambios en 2 años", "Unchanged for 2 years") },
];

/** Los archivos más grandes con filtros: tamaño, antigüedad, tipo y extensión. */
export function Files({ ctx }: { ctx: Ctx }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = scanned.find((d) => d.root === ctx.activeRoot)?.root ?? scanned[0]?.root;
  const [filter, setFilter] = useState<FileFilter>({ minSize: 10 * MB, olderThanDays: 0, category: null, ext: "" });
  const [items, setItems] = useState<NodeInfo[] | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    if (!root) return;
    let live = true;
    const t = setTimeout(() => {
      api.findFiles(root, 0, filter, 300).then((r) => { if (live) { setItems(r); setChecked(new Set()); } }).catch((e) => ctx.toast(String(e), true));
    }, 200);
    return () => { live = false; clearTimeout(t); };
  }, [api, root, filter, version, ctx.toast]);

  const set = (patch: Partial<FileFilter>) => { setConfirm(false); setFilter((f) => ({ ...f, ...patch })); };
  const toggle = (id: number) => setChecked((c) => { const n = new Set(c); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  if (!root) {
    return (
      <div className="view">
        <div className="head"><div><h2>{tr("Archivos", "Files")}</h2></div></div>
        <div className="card empty-state"><h3>{tr("Primero analiza un disco", "Scan a drive first")}</h3><p>{tr("Aquí podrás buscar los archivos más grandes, los que llevan años sin tocarse y filtrar por tipo.", "Here you can find the largest files, the ones untouched for years, and filter by type.")}</p></div>
      </div>
    );
  }

  const list = items ?? [];
  const sel = list.filter((f) => checked.has(f.id));
  const selSize = sel.reduce((a, f) => a + f.size, 0);
  const total = list.reduce((a, f) => a + f.size, 0);

  const trash = async () => {
    setConfirm(false);
    try {
      const freed = await api.moveToTrash(root, sel.map((f) => f.id));
      ctx.toast(tr(`Se enviaron ${sel.length} archivos a la papelera. Liberaste ${size(freed)}.`, `Moved ${sel.length} files to the Recycle Bin. You freed ${size(freed)}.`));
      ctx.bump();
    } catch (e) { ctx.toast(String(e), true); }
  };

  return (
    <div className="view">
      <div className="head">
        <div><h2>{tr("Archivos", "Files")}</h2><p>{tr(`Los archivos más grandes de ${driveLetter(root)}. Usa los filtros para encontrar lo que ya no usas.`, `The largest files on ${driveLetter(root)}. Use the filters to find what you no longer use.`)}</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} onClick={() => ctx.setActiveRoot(d.root)}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>

      <div className="card filters">
        <label>{tr("Tamaño mínimo", "Minimum size")}
          <select value={filter.minSize} onChange={(e) => set({ minSize: Number(e.target.value) })}>
            {SIZES.map((s) => <option key={s} value={s}>{s ? size(s) : tr("Cualquiera", "Any")}</option>)}
          </select>
        </label>
        <label>{tr("Antigüedad", "Age")}
          <select value={filter.olderThanDays} onChange={(e) => set({ olderThanDays: Number(e.target.value) })}>
            {AGES.map((a) => <option key={a.days} value={a.days}>{a.label()}</option>)}
          </select>
        </label>
        <label>{tr("Tipo", "Type")}
          <select value={filter.category ?? ""} onChange={(e) => set({ category: (e.target.value || null) as Category | null })}>
            <option value="">{tr("Todos", "All")}</option>
            {(Object.keys(CATEGORY) as Category[]).map((c) => <option key={c} value={c}>{CATEGORY[c].name}</option>)}
          </select>
        </label>
        <label>{tr("Extensión", "Extension")}
          <input placeholder={tr("por ejemplo .mp4", "for example .mp4")} value={filter.ext ?? ""} onChange={(e) => set({ ext: e.target.value })} />
        </label>
        <div className="sp" />
        <div className="filters-sum">
          <b className="num">{size(total)}</b>
          <small className="muted">{tr("en", "in")} {list.length}{list.length === 300 ? "+" : ""} {tr("archivos", "files")}</small>
        </div>
      </div>

      {sel.length > 0 && (
        <div className="card selbar">
          <span><b>{sel.length}</b> {tr("seleccionados", "selected")} · <b className="num">{size(selSize)}</b></span>
          {confirm ? (
            <>
              <span className="muted">{tr("¿Enviarlos a la papelera? Podrás recuperarlos desde allí.", "Move them to the Recycle Bin? You can restore them from there.")}</span>
              <button className="btn primary" onClick={trash}>{tr("Sí, enviar a la papelera", "Yes, move to Recycle Bin")}</button>
              <button className="btn" onClick={() => setConfirm(false)}>{tr("Cancelar", "Cancel")}</button>
            </>
          ) : (
            <>
              <button className="btn primary" onClick={() => setConfirm(true)}>{tr("Enviar a la papelera", "Move to Recycle Bin")}</button>
              <button className="btn" onClick={() => setChecked(new Set())}>{tr("Quitar selección", "Clear selection")}</button>
            </>
          )}
        </div>
      )}

      <div className="card list files-list" role="list">
        {items && list.length === 0 && <p className="muted" style={{ padding: 10 }}>{tr("Ningún archivo cumple estos filtros.", "No files match these filters.")}</p>}
        {list.map((f) => (
          <div key={f.id} role="listitem" className={`row file-row ${checked.has(f.id) ? "sel" : ""}`}>
            <input type="checkbox" checked={checked.has(f.id)} onChange={() => toggle(f.id)} aria-label={tr(`Seleccionar ${f.name}`, `Select ${f.name}`)} />
            <button className="nm" title={f.path} onClick={() => ctx.explore(root, f.parent ?? 0, f)}>
              <NodeIcon node={f} px={16} />
              <span className="t">{f.name}<small>{f.path}</small></span>
            </button>
            <span className="muted num">{date(f.modified)}</span>
            <span className="st"><FileStatus node={f} /></span>
            <b className="num">{size(f.size)}</b>
          </div>
        ))}
      </div>
    </div>
  );
}
