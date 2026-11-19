import { tr } from "../i18n";
import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { driveLetter, size } from "../format";
import type { Risk, Suggestion } from "../types";

const GROUPS: { risk: Risk; title: () => string; chip: [() => string, string] }[] = [
  { risk: "safe", title: () => tr("Seguro de borrar", "Safe to delete"), chip: [() => tr("Seguro", "Safe"), "ok"] },
  { risk: "review", title: () => tr("Revisa antes de borrar", "Review before deleting"), chip: [() => tr("Revisar", "Review"), "warn"] },
  { risk: "careful", title: () => tr("Con cuidado", "Be careful"), chip: [() => tr("Cuidado", "Careful"), "bad"] },
];

/** Las sugerencias llegan del motor en español; en inglés se muestran con estos textos. */
const EN: Record<string, [string, string, string?]> = {
  temp: ["Temporary files", "Files Windows and apps leave in Temp and no longer use."],
  "windows-old": ["Previous Windows installation", "The Windows.old folder left after an upgrade. Only needed to roll back."],
  "windows-update": ["Windows Update downloads", "Update files that are already installed."],
  "browser-cache": ["Browser cache", "Web page copies from Chrome, Edge, Brave and Firefox. They are downloaded again when needed."],
  "app-cache": ["Discord, Teams, Slack, Spotify and VS Code cache", "Temporary files from these apps. They are rebuilt automatically; the first launch may be a little slower."],
  "crash-dumps": ["Crash dumps", "Reports Windows saves when an app or the system crashes. Only useful to diagnose failures."],
  "delivery-optimization": ["Delivery Optimization", "Copies of updates Windows shares with other PCs. Safe to delete."],
  "dev-cache": ["npm, pip and Yarn cache", "Downloaded packages that can be fetched again."],
  "package-cache": ["NuGet, Gradle, Cargo and Maven packages", "Downloaded project dependencies. They are fetched again on the next build, which can take a while."],
  "shader-cache": ["Shader cache", "Games rebuild it the next time they start."],
  iso: ["ISO images in Downloads", "Installation disc images. Check whether you still need them."],
  installers: ["Installers in Downloads", ".exe and .msi files usually not needed after installing."],
  "node-modules": ["node_modules folders", "Project dependencies. Restore them with npm install."],
  hibernation: ["Hibernation file", "Turning off hibernation frees hiberfil.sys, but removes Hibernate from the power menu.", "Open a terminal as administrator and run: powercfg /hibernate off"],
  "recycle-bin": ["Empty the Recycle Bin", "Files you already deleted that still take space. Once emptied they cannot be recovered."],
};
const title = (s: Suggestion) => tr(s.title, EN[s.key]?.[0] ?? s.title);
const desc = (s: Suggestion) => tr(s.description, EN[s.key]?.[1] ?? s.description);
const hint = (s: Suggestion) => tr(s.manualHint ?? "", EN[s.key]?.[2] ?? s.manualHint ?? "");

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
        <div className="head"><div><h2>{tr("Limpiar", "Clean up")}</h2></div></div>
        <div className="card empty-state">
          <h3>{tr("Primero analiza un disco", "Scan a disk first")}</h3>
          <p>{tr("Las sugerencias de limpieza salen del análisis: temporales, cachés, instaladores viejos, Windows.old y más.", "Clean-up suggestions come from the scan: temp files, caches, old installers, Windows.old and more.")}</p>
          {ctx.primary && <button className="btn primary" disabled={!!ctx.scanning} onClick={() => ctx.startScan(ctx.primary!.root)}>{tr("Analizar", "Scan")} {driveLetter(ctx.primary.root)}</button>}
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
      let got = 0;
      const ids = selected.filter((s) => !s.action).flatMap((s) => s.ids);
      if (ids.length) got += await api.moveToTrash(root, ids);
      const bin = selected.find((s) => s.action === "empty-recycle-bin");
      if (bin) { await api.emptyRecycleBin(); got += bin.size; }
      setFreed((f) => f + got);
      ctx.toast(tr(`Listo: liberaste ${size(got)}.`, `Done: you freed ${size(got)}.`));
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
        <div><h2>{tr("Limpiar", "Clean up")}</h2><p>{tr(`Sugerencias para ${driveLetter(root)}, ordenadas de más seguras a más delicadas.`, `Suggestions for ${driveLetter(root)}, from safest to most delicate.`)}</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} onClick={() => ctx.setActiveRoot(d.root)}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>
      <div className="clean">
        <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
          {items.length === 0 && <div className="card empty-state"><h3>{tr("No encontramos nada que limpiar", "Nothing to clean up")}</h3><p>{tr("Este disco no tiene temporales ni cachés grandes.", "This disk has no large temp files or caches.")}</p></div>}
          {GROUPS.map((g) => {
            const list = items.filter((s) => s.risk === g.risk);
            if (!list.length) return null;
            return (
              <div className="group" key={g.risk}>
                <h3>{g.title()}</h3>
                {list.map((s) => s.manualHint ? (
                  <div className="sug manual" key={s.key}>
                    <span />
                    <span className="t">{title(s)}</span>
                    <span className="r"><b>{size(s.size)}</b><span className={`chip ${g.chip[1]}`}>{g.chip[0]()}</span></span>
                    <span className="d">{desc(s)}</span>
                    <span />
                    <span className="hint">{hint(s)}</span>
                  </div>
                ) : (
                  <label className="sug" key={s.key} htmlFor={`s-${s.key}`}>
                    <input type="checkbox" id={`s-${s.key}`} checked={checked.has(s.key)} onChange={() => toggle(s.key)} />
                    <span className="t">{title(s)}</span>
                    <span className="r"><b>{size(s.size)}</b><span className={`chip ${g.chip[1]}`}>{g.chip[0]()}</span></span>
                    <span className="d">{desc(s)}</span>
                  </label>
                ))}
              </div>
            );
          })}
        </div>
        <aside className="card summary">
          <span className="eyebrow">{tr("Seleccionado", "Selected")}</span>
          <div className="bignum">{size(total)}</div>
          <p style={{ margin: 0 }} className="muted">{tr(`${selected.length} grupos. Todo va a la papelera de reciclaje, así que puedes recuperarlo (salvo si eliges vaciarla).`, `${selected.length} groups. Everything goes to the Recycle Bin, so you can recover it (unless you choose to empty it).`)}</p>
          {confirm ? (
            <div className="confirm">
              <b>{tr(`¿Liberar ${size(total)}?`, `Free up ${size(total)}?`)}</b>
              <span>
                {selected.some((s) => !s.action) && <>{tr(`Se enviarán a la papelera ${selected.reduce((a, s) => a + s.ids.length, 0)} carpetas y archivos. `, `${selected.reduce((a, s) => a + s.ids.length, 0)} folders and files will go to the Recycle Bin. `)}</>}
                {selected.some((s) => s.action === "empty-recycle-bin") && <>{tr("La papelera de reciclaje se vaciará y lo que tiene ya no se podrá recuperar.", "The Recycle Bin will be emptied and its contents can no longer be recovered.")}</>}
              </span>
              <div className="actions">
                <button className="btn primary" disabled={working} onClick={run}>{working ? tr("Liberando…", "Freeing…") : tr("Sí, liberar", "Yes, free up")}</button>
                <button className="btn" disabled={working} onClick={() => setConfirm(false)}>{tr("Cancelar", "Cancel")}</button>
              </div>
            </div>
          ) : (
            <button className="btn primary" disabled={!total} onClick={() => setConfirm(true)}>{tr("Liberar espacio", "Free up space")}</button>
          )}
          {freed > 0 && <div className="freed">{tr("Liberaste", "You freed")} <b className="num">{size(freed)}</b> {tr("en esta sesión.", "this session.")}</div>}
        </aside>
      </div>
    </div>
  );
}
