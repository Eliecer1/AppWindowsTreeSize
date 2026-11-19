import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { CATEGORY, pct, size } from "./format";
import type { CategorySize, NodeInfo, SubNode } from "./types";

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

export function CloudIcon({ px = 14 }: { px?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={px} height={px} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7 18h10a4 4 0 0 0 .6-7.95A6 6 0 0 0 6.1 9.2 4.5 4.5 0 0 0 7 18z" />
    </svg>
  );
}

export function RefreshIcon({ px = 15, spin = false }: { px?: number; spin?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" width={px} height={px} aria-hidden="true" className={spin ? "spin" : undefined} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M20 11a8 8 0 0 0-14.6-4.5L4 8" />
      <path d="M4 3.5V8h4.5" />
      <path d="M4 13a8 8 0 0 0 14.6 4.5L20 16" />
      <path d="M20 20.5V16h-4.5" />
    </svg>
  );
}

/** Estado de un archivo: descargado en el disco o solo en la nube. */
export function FileStatus({ node }: { node: Pick<NodeInfo, "cloudOnly" | "hardlink"> }) {
  if (node.hardlink) {
    return (
      <span className="chip status link-st" title="Es otro nombre de un archivo que ya se contó en otra carpeta. No ocupa espacio extra.">
        <svg viewBox="0 0 24 24" width={12} height={12} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7" />
        </svg>
        Ya contado
      </span>
    );
  }
  return node.cloudOnly ? (
    <span className="chip status cloud-st" title="No está descargado: no ocupa espacio en tu disco"><CloudIcon px={12} /> En la nube</span>
  ) : (
    <span className="chip status disk-st" title="Está guardado en tu disco y ocupa espacio">
      <svg viewBox="0 0 24 24" width={12} height={12} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="13" width="18" height="7" rx="2" /><path d="M5 13l2.5-8h9L19 13M7 16.5h.01" />
      </svg>
      En el disco
    </span>
  );
}

/** Aviso de cuánto de una carpeta o archivo está solo en la nube. */
export function CloudNote({ node }: { node: { isDir: boolean; cloud: number; cloudFiles: number; cloudOnly: boolean; logical: number } }) {
  if (node.cloud <= 0) return null;
  return (
    <p className="cloud-note">
      <CloudIcon />
      {node.cloudOnly
        ? <>Está solo en la nube: pesa {size(node.logical)}, pero no ocupa espacio en tu disco.</>
        : <>Además hay {size(node.cloud)} en {node.cloudFiles.toLocaleString("es-ES")} archivos que están solo en la nube y no ocupan espacio en tu disco.</>}
    </p>
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

/** Mapa de bloques. Con `nested`, los bloques grandes de carpetas muestran dentro lo que contienen. */
export function Treemap({ items, onPick, nested, onPickId, onMenu }: {
  items: NodeInfo[];
  onPick: (n: NodeInfo) => void;
  nested?: Map<number, SubNode>;
  onPickId?: (id: number) => void;
  onMenu?: (n: NodeInfo, x: number, y: number) => void;
}) {
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
      {rects.map(({ item, x, y, w, h }) => {
        const sub = nested?.get(item.id);
        const inner = sub && w > 150 && h > 110 ? squarify(sub.children.filter((c) => c.size > 0).slice(0, 40), 6, 40, w - 12, h - 46) : [];
        return (
          <button key={item.id} className={`tile ${w < 70 || h < 44 ? "small" : ""} ${inner.length ? "nest" : ""}`} title={`${item.name} · ${size(item.size)}`}
            style={{ left: x, top: y, width: w, height: h, background: CATEGORY[item.category].color }} onClick={() => onPick(item)}
            onContextMenu={onMenu ? (e) => { e.preventDefault(); onMenu(item, e.clientX, e.clientY); } : undefined}>
            <b>{item.name}</b>
            <span>{size(item.size)}</span>
            {inner.map((r) => (
              <i key={r.item.id} className={`sub ${r.w < 60 || r.h < 30 ? "small" : ""}`} title={`${r.item.name} · ${size(r.item.size)}`}
                style={{ left: r.x, top: r.y, width: r.w, height: r.h }}
                onClick={(e) => { if (r.item.isDir && onPickId) { e.stopPropagation(); onPickId(r.item.id); } }}>
                <em>{r.item.name}</em>
              </i>
            ))}
          </button>
        );
      })}
    </div>
  );
}

/** Anillos: el centro es la carpeta actual y cada anillo es un nivel más adentro. */
export function Sunburst({ root, onPick }: { root: SubNode; onPick: (n: SubNode) => void }) {
  const [hover, setHover] = useState<SubNode | null>(null);
  const S = 520, C = S / 2, R0 = 70, W = (C - R0 - 4) / 4;
  const arcs: { n: SubNode; a0: number; a1: number; depth: number }[] = [];
  const walk = (n: SubNode, a0: number, a1: number, depth: number) => {
    const total = n.size || 1;
    let a = a0;
    for (const c of n.children) {
      const span = ((a1 - a0) * c.size) / total;
      if (span > 0.004) {
        arcs.push({ n: c, a0: a, a1: a + span, depth });
        if (depth < 3) walk(c, a, a + span, depth + 1);
      }
      a += span;
    }
  };
  walk(root, 0, Math.PI * 2, 0);
  const pt = (r: number, a: number) => `${C + r * Math.sin(a)} ${C - r * Math.cos(a)}`;
  const path = (a0: number, a1: number, r0: number, r1: number) => {
    const large = a1 - a0 > Math.PI ? 1 : 0;
    // Un arco completo no se puede dibujar de una vez: se deja un hueco mínimo.
    const e = a1 - a0 >= Math.PI * 2 ? a1 - 0.0001 : a1;
    return `M ${pt(r0, a0)} L ${pt(r1, a0)} A ${r1} ${r1} 0 ${large} 1 ${pt(r1, e)} L ${pt(r0, e)} A ${r0} ${r0} 0 ${large} 0 ${pt(r0, a0)} Z`;
  };
  const shown = hover ?? root;
  return (
    <div className="sunburst">
      <svg viewBox={`0 0 ${S} ${S}`} role="img" aria-label="Anillos de espacio">
        {arcs.map(({ n, a0, a1, depth }) => (
          <path key={`${depth}-${n.id}`} d={path(a0, a1, R0 + depth * W + 1, R0 + (depth + 1) * W - 1)} fill={CATEGORY[n.category].color}
            opacity={1 - depth * 0.16} className={n.isDir ? "dir" : ""} onMouseEnter={() => setHover(n)} onMouseLeave={() => setHover(null)} onClick={() => onPick(n)}>
            <title>{`${n.name} · ${size(n.size)}`}</title>
          </path>
        ))}
        <circle cx={C} cy={C} r={R0 - 4} className="hub" />
        <text x={C} y={C - 4} textAnchor="middle" className="hub-name">{shown.name.length > 18 ? shown.name.slice(0, 17) + "…" : shown.name}</text>
        <text x={C} y={C + 18} textAnchor="middle" className="hub-size">{size(shown.size)}</text>
      </svg>
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
