import { useEffect, useRef, useState } from "react";
import type { Ctx } from "../App";
import { NodeIcon } from "../components";
import { size } from "../format";
import type { NodeInfo } from "../types";

/** Búsqueda instantánea (Ctrl+K) por nombre, o por extensión si empieza con un punto. */
export function CommandPalette({ ctx, root, onClose }: { ctx: Ctx; root: string; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [res, setRes] = useState<NodeInfo[]>([]);
  const [idx, setIdx] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => { input.current?.focus(); }, []);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      ctx.api.search(root, q, 12)
        .then((r) => { if (live) { setRes(r); setIdx(0); setError(null); } })
        .catch((e) => { if (live) { setRes([]); setError(String(e)); } });
    }, 120);
    return () => { live = false; clearTimeout(t); };
  }, [ctx.api, root, q]);

  const pick = (n: NodeInfo) => {
    onClose();
    if (n.isDir) ctx.explore(root, n.id);
    else ctx.explore(root, n.parent ?? 0, n);
  };

  return (
    <div className="palette-bg" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-label="Buscar">
        <input ref={input} value={q} onChange={(e) => setQ(e.target.value)} placeholder="Busca por nombre o extensión (ej. .iso, Temp, node_modules)"
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") { setIdx((i) => Math.min(i + 1, res.length - 1)); e.preventDefault(); }
            else if (e.key === "ArrowUp") { setIdx((i) => Math.max(i - 1, 0)); e.preventDefault(); }
            else if (e.key === "Enter" && res[idx]) pick(res[idx]);
            else if (e.key === "Escape") onClose();
          }} />
        <ul role="listbox">
          {error && <div className="empty">{error.includes("analizado") ? "Analiza un disco para poder buscar en él." : error}</div>}
          {!error && q.trim() && res.length === 0 && <div className="empty">No hay resultados para “{q}”.</div>}
          {!error && !q.trim() && <div className="empty">Escribe para buscar entre todos los archivos analizados.</div>}
          {res.map((n, i) => (
            <li key={n.id} role="option" aria-selected={i === idx} onClick={() => pick(n)} onMouseEnter={() => setIdx(i)}>
              <NodeIcon node={n} />
              <span style={{ minWidth: 0 }}>{n.name}<small>{n.path}</small></span>
              <span className="sz">{size(n.size)}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
