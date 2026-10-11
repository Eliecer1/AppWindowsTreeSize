import { useEffect, useState } from "react";
import type { Ctx } from "../App";
import { CATEGORY, driveLetter, fmtPct, pct, size } from "../format";
import type { CategorySize } from "../types";

export function Types({ ctx }: { ctx: Ctx }) {
  const { api, version, drives } = ctx;
  const scanned = drives.filter((d) => d.scanned);
  const root = scanned.find((d) => d.root === ctx.activeRoot)?.root ?? scanned[0]?.root;
  const [cats, setCats] = useState<CategorySize[]>([]);

  useEffect(() => {
    if (root) api.getFolder(root, 0, 0).then((f) => setCats(f.categories)).catch(() => {});
  }, [api, root, version]);

  if (!root) {
    return (
      <div className="view">
        <div className="head"><div><h2>Tipos de archivo</h2></div></div>
        <div className="card empty-state">
          <h3>Primero analiza un disco</h3>
          <p>Verás cuánto ocupan tus videos, fotos, juegos, instaladores y archivos temporales.</p>
          {ctx.primary && <button className="btn primary" disabled={!!ctx.scanning} onClick={() => ctx.startScan(ctx.primary!.root)}>Analizar {driveLetter(ctx.primary.root)}</button>}
        </div>
      </div>
    );
  }

  const total = cats.reduce((a, c) => a + c.size, 0);
  const R = 110;
  const C = 2 * Math.PI * R;
  let off = 0;

  return (
    <div className="view">
      <div className="head">
        <div><h2>Tipos de archivo</h2><p>Cuánto ocupa cada tipo en {driveLetter(root)}</p></div>
        <div className="actions">
          {scanned.map((d) => (
            <button key={d.root} className={`btn ${d.root === root ? "primary" : ""}`} onClick={() => ctx.setActiveRoot(d.root)}>{driveLetter(d.root)}</button>
          ))}
        </div>
      </div>
      <div className="types">
        <div className="card donut">
          <svg width="280" height="280" viewBox="0 0 280 280" style={{ maxWidth: "100%" }}>
            {cats.map((c) => {
              const len = (c.size / (total || 1)) * C;
              const el = (
                <circle key={c.category} cx="140" cy="140" r={R} fill="none" stroke={CATEGORY[c.category].color} strokeWidth="34"
                  strokeDasharray={`${Math.max(len - 3, 0)} ${C}`} strokeDashoffset={-off} transform="rotate(-90 140 140)">
                  <title>{`${CATEGORY[c.category].name}: ${size(c.size)}`}</title>
                </circle>
              );
              off += len;
              return el;
            })}
          </svg>
          <div className="c"><div className="bignum" style={{ fontSize: 28 }}>{size(total)}</div><small className="muted">en {cats.length} tipos</small></div>
        </div>
        <div className="card">
          {cats.map((c) => (
            <div className="trow" key={c.category}>
              <i className="dot" style={{ background: CATEGORY[c.category].color }} />
              <span>{CATEGORY[c.category].name}</span>
              <span className="sz">{size(c.size)}</span>
              <span className="pc">{fmtPct(pct(c.size, total))}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
