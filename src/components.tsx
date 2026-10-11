import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { CATEGORY, pct, size } from "./format";
import type { CategorySize, NodeInfo } from "./types";

export function Ring({ used, total, size: px = 150 }: { used: number; total: number; size?: number }) {
  const r = px / 2 - 10;
  const c = 2 * Math.PI * r;
  const p = total > 0 ? Math.min(used / total, 1) : 0;
  const color = p > 0.9 ? "var(--bad)" : p > 0.75 ? "var(--warn)" : "var(--accent)";
  return (
    <svg width={px} height={px} viewBox={`0 0 ${px} ${px}`} aria-hidden="true">
      <circle cx={px / 2} cy={px / 2} r={r} fill="none" stroke="var(--panel-2)" strokeWidth={14} />
      <circle cx={px / 2} cy={px / 2} r={r} fill="none" stroke={color} strokeWidth={14} strokeLinecap="round"
        strokeDasharray={`${c * p} ${c}`} transform={`rotate(-90 ${px / 2} ${px / 2})`} />
    </svg>
  );
}

export function StackBar({ parts, total }: { parts: CategorySize[]; total: number }) {
  return (
    <div className="stackbar">
      {parts.map((p) => (
        <i key={p.category} style={{ width: `${pct(p.size, total)}%`, background: CATEGORY[p.category].color }}
          title={`${CATEGORY[p.category].name}: ${size(p.size)}`} />
      ))}
    </div>
  );
}

export function Legend({ parts }: { parts: CategorySize[] }) {
  return (
    <div className="legend">
      {parts.map((p) => (
        <span key={p.category}>
          <i className="dot" style={{ background: CATEGORY[p.category].color }} />
          {CATEGORY[p.category].name} <b className="num">{size(p.size)}</b>
        </span>
      ))}
    </div>
  );
}

export function NodeIcon({ node, px = 18 }: { node: Pick<NodeInfo, "isDir" | "category">; px?: number }) {
  const c = CATEGORY[node.category].color;
  return node.isDir ? (
    <svg viewBox="0 0 24 24" width={px} height={px} aria-hidden="true">
      <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2h8.8A1.5 1.5 0 0 1 21 8.5v9A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z" fill={c} opacity=".9" />
    </svg>
  ) : (
    <svg viewBox="0 0 24 24" width={px} height={px} aria-hidden="true" fill="none" stroke={c} strokeWidth="1.6">
      <path d="M6 2.5h8l5 5v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-17a1 1 0 0 1 1-1z" />
      <path d="M14 2.5v5h5" />
    </svg>
  );
}

interface Rect<T> { item: T; x: number; y: number; w: number; h: number }

/** Distribución "squarified": bloques lo más cuadrados posible, proporcionales al tamaño. */
export function squarify<T extends { size: number }>(items: T[], x: number, y: number, w: number, h: number): Rect<T>[] {
  const total = items.reduce((a, i) => a + i.size, 0);
  if (!total || w <= 0 || h <= 0) return [];
  const scale = (w * h) / total;
  const rest = items.map((item) => ({ item, a: item.size * scale }));
  const out: Rect<T>[] = [];
  let row: typeof rest = [];
  const worst = (r: typeof rest, s: number) => {
    const sum = r.reduce((a, b) => a + b.a, 0);
    const mx = Math.max(...r.map((z) => z.a));
    const mn = Math.min(...r.map((z) => z.a));
    return Math.max((s * s * mx) / (sum * sum), (sum * sum) / (s * s * mn));
  };
  const place = () => {
    const sum = row.reduce((a, b) => a + b.a, 0);
    if (w >= h) {
      const rw = sum / h;
      let cy = y;
      for (const r of row) { const rh = r.a / rw; out.push({ item: r.item, x, y: cy, w: rw, h: rh }); cy += rh; }
      x += rw; w -= rw;
    } else {
      const rh = sum / w;
      let cx = x;
      for (const r of row) { const rw = r.a / rh; out.push({ item: r.item, x: cx, y, w: rw, h: rh }); cx += rw; }
      y += rh; h -= rh;
    }
    row = [];
  };
  while (rest.length) {
    const s = Math.min(w, h);
    const c = rest[0];
    if (!row.length || worst([...row, c], s) <= worst(row, s)) { row.push(c); rest.shift(); }
    else place();
  }
  if (row.length) place();
  return out;
}

export function Treemap({ items, onPick }: { items: NodeInfo[]; onPick: (n: NodeInfo) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const rects = squarify(items.filter((i) => i.size > 0).slice(0, 80), 0, 0, box.w, box.h);
  return (
    <div className="treemap" ref={ref} aria-label="Mapa de espacio">
      {rects.map(({ item, x, y, w, h }) => (
        <button key={item.id} className={`tile ${w < 70 || h < 44 ? "small" : ""}`} title={`${item.name} · ${size(item.size)}`}
          style={{ left: x, top: y, width: w, height: h, background: CATEGORY[item.category].color }} onClick={() => onPick(item)}>
          <b>{item.name}</b>
          <span>{size(item.size)}</span>
        </button>
      ))}
    </div>
  );
}

export function Toast({ msg }: { msg: { text: string; error?: boolean; at: number } | null }) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!msg) return;
    setOn(true);
    const t = setTimeout(() => setOn(false), msg.error ? 5000 : 2800);
    return () => clearTimeout(t);
  }, [msg]);
  return <div className={`toast ${on ? "on" : ""} ${msg?.error ? "error" : ""}`} role="status">{msg?.text}</div>;
}
