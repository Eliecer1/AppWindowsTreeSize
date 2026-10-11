import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { driveLetter, size } from "../format";
import type { Risk, Suggestion } from "../types";

const GROUPS: { risk: Risk; title: string; chip: [string, string] }[] = [
  { risk: "safe", title: "Seguro de borrar", chip: ["Seguro", "ok"] },
  { risk: "review", title: "Revisa antes de borrar", chip: ["Revisar", "warn"] },
  { risk: "careful", title: "Con cuidado", chip: ["Cuidado", "bad"] },
];

export function Clean({ ctx }: { ctx: Ctx }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = scanned.find((d) => d.root === ctx.activeRoot)?.root ?? scanned[0]?.root;
  const [items, setItems] = useState<Suggestion[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [working, setWorking] = useState(false);
  const [freed, setFreed] = useState(0);

  useEffect(() => {
    if (!root) return;
    api.cleanupSuggestions(root).then((s) => {
      setItems(s);
      // Las sugerencias seguras empiezan marcadas; las demás las decide la persona.
      setChecked(new Set(s.filter((x) => x.risk === "safe" && !x.manualHint).map((x) => x.key)));
    }).catch((e) => ctx.toast(String(e), true));
  }, [api, root, version, ctx.toast]);

  if (!root) {
    return (
      <div className="view">
        <div className="head"><div><h2>Limpiar</h2></div></div>
        <div className="card empty-state">
          <h3>Primero analiza un disco</h3>
          <p>Las sugerencias de limpieza salen del análisis: temporales, cachés, instaladores viejos, Windows.old y más.</p>
          {ctx.primary && <button className="btn primary" disabled={!!ctx.scanning} onClick={() => ctx.startScan(ctx.primary!.root)}>Analizar {driveLetter(ctx.primary.root)}</button>}
        </div>
      </div>
    );
  }
  if (!items) return null;

  const selected = items.filter((s) => checked.has(s.key) && !s.manualHint);
  const total = selected.reduce((a, s) => a + s.size, 0);
  const toggle = (key: string) => {
    setConfirm(false);
    setChecked((c) => { const n = new Set(c); if (n.has(key)) n.delete(key); else n.add(key); return n; });
  };
  const run = async () => {
    setWorking(true);
    try {
      const got = await api.moveToTrash(root, selected.flatMap((s) => s.ids));
      setFreed((f) => f + got);
      ctx.toast(`Listo: liberaste ${size(got)}. Todo está en la papelera de reciclaje.`);
      ctx.bump();
    } catch (e) {
      ctx.toast(String(e), true);
    } finally {
      setWorking(false);
      setConfirm(false);
    }
  };

  return (
    <div className="view">
      <div className="head">
        <div><h2>Limpiar</h2><p>Sugerencias para {driveLetter(root)}, ordenadas de más seguras a más delicadas.</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} onClick={() => ctx.setActiveRoot(d.root)}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>
      <div className="clean">
        <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
          {items.length === 0 && <div className="card empty-state"><h3>No encontramos nada que limpiar</h3><p>Este disco no tiene temporales ni cachés grandes.</p></div>}
          {GROUPS.map((g) => {
            const list = items.filter((s) => s.risk === g.risk);
            if (!list.length) return null;
            return (
              <div className="group" key={g.risk}>
                <h3>{g.title}</h3>
                {list.map((s) => s.manualHint ? (
                  <div className="sug manual" key={s.key}>
                    <span />
                    <span className="t">{s.title}</span>
                    <span className="r"><b>{size(s.size)}</b><span className={`chip ${g.chip[1]}`}>{g.chip[0]}</span></span>
                    <span className="d">{s.description}</span>
                    <span />
                    <span className="hint">{s.manualHint}</span>
                  </div>
                ) : (
                  <label className="sug" key={s.key} htmlFor={`s-${s.key}`}>
                    <input type="checkbox" id={`s-${s.key}`} checked={checked.has(s.key)} onChange={() => toggle(s.key)} />
                    <span className="t">{s.title}</span>
                    <span className="r"><b>{size(s.size)}</b><span className={`chip ${g.chip[1]}`}>{g.chip[0]}</span></span>
                    <span className="d">{s.description}</span>
                  </label>
                ))}
              </div>
            );
          })}
        </div>
        <aside className="card summary">
          <span className="eyebrow">Seleccionado</span>
          <div className="bignum">{size(total)}</div>
          <p style={{ margin: 0 }} className="muted">{selected.length} grupos. Todo va a la papelera de reciclaje, así que puedes recuperarlo.</p>
          {confirm ? (
            <div className="confirm">
              <b>¿Liberar {size(total)}?</b>
              <span>Se enviarán a la papelera {selected.reduce((a, s) => a + s.ids.length, 0)} carpetas y archivos.</span>
              <div className="actions">
                <button className="btn primary" disabled={working} onClick={run}>{working ? "Liberando…" : "Sí, liberar"}</button>
                <button className="btn" disabled={working} onClick={() => setConfirm(false)}>Cancelar</button>
              </div>
            </div>
          ) : (
            <button className="btn primary" disabled={!total} onClick={() => setConfirm(true)}>Liberar espacio</button>
          )}
          {freed > 0 && <div className="freed">Liberaste <b className="num">{size(freed)}</b> en esta sesión.</div>}
        </aside>
      </div>
    </div>
  );
}
