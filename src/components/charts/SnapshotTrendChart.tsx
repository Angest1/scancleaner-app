import { useState } from "react";
import { formatBytes } from "../../lib/format";
import type { ScanSnapshot } from "../../lib/ipc";

interface SnapshotTrendChartProps {
  snapshots: ScanSnapshot[];
}

export function SnapshotTrendChart({ snapshots }: SnapshotTrendChartProps) {
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  // Order oldest -> newest for chronological chart
  const ordered = [...snapshots].reverse().slice(-14);
  if (ordered.length < 2) {
    return (
      <div className="py-5 text-center text-[12px] text-fg-muted">
        Se necesitan al menos 2 snapshots para trazar la tendencia histórica de espacio.
      </div>
    );
  }

  const width = 560;
  const height = 178;
  const padL = 62;
  const padR = 14;
  const padT = 16;
  const padB = 26;
  const plotW = width - padL - padR;
  const plotH = height - padT - padB;

  const values = ordered.map((s) => s.scannedAllocatedBytes);
  const minVal = Math.min(...values);
  const maxVal = Math.max(...values);
  const span = Math.max(maxVal - minVal, maxVal * 0.15, 1);
  const yMin = Math.max(0, minVal - span * 0.15);
  const yMax = maxVal + span * 0.1;
  const yRange = Math.max(1, yMax - yMin);

  const pts = ordered.map((s, idx) => {
    const x = padL + (idx / Math.max(1, ordered.length - 1)) * plotW;
    const y = padT + plotH - ((s.scannedAllocatedBytes - yMin) / yRange) * plotH;
    return { x, y, snap: s };
  });

  const linePath = pts
    .map((p, idx) => `${idx === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`)
    .join(" ");
  const areaPath = `${linePath} L${pts[pts.length - 1].x.toFixed(1)},${(padT + plotH).toFixed(1)} L${pts[0].x.toFixed(1)},${(padT + plotH).toFixed(1)} Z`;

  const active = hoveredIdx !== null ? pts[hoveredIdx] : pts[pts.length - 1];

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between flex-wrap gap-2 text-[12px]">
        <span className="text-fg-secondary">
          Evolución de espacio medido ({ordered.length} snapshots)
        </span>
        {active && (
          <span className="mono text-[11.5px] text-fg-primary">
            <strong>{active.snap.driveRoot}</strong> ·{" "}
            {formatBytes(active.snap.scannedAllocatedBytes)}{" "}
            <span className="text-fg-muted">
              ({new Date(active.snap.scannedAt).toLocaleDateString("es-ES")})
            </span>
          </span>
        )}
      </div>

      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="w-full h-auto max-h-[195px] overflow-visible"
        role="img"
        aria-label="Gráfico de evolución de espacio en snapshots"
      >
        <defs>
          <linearGradient id="sc-snap-area" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.32" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0.02" />
          </linearGradient>
        </defs>

        {/* Horizontal gridlines */}
        {[0, 0.5, 1].map((t) => {
          const y = padT + plotH * (1 - t);
          const v = yMin + yRange * t;
          return (
            <g key={t}>
              <line
                x1={padL}
                y1={y}
                x2={width - padR}
                y2={y}
                stroke="var(--border-subtle)"
                strokeDasharray="3 3"
              />
              <text
                x={padL - 8}
                y={y + 3.5}
                textAnchor="end"
                fill="var(--text-muted)"
                fontSize="10"
                fontFamily="JetBrains Mono, monospace"
              >
                {formatBytes(v)}
              </text>
            </g>
          );
        })}

        <path d={areaPath} fill="url(#sc-snap-area)" />
        <path
          d={linePath}
          fill="none"
          stroke="var(--accent)"
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {pts.map((p, idx) => {
          const isHot = hoveredIdx === idx;
          return (
            <g key={p.snap.id}>
              <circle
                cx={p.x}
                cy={p.y}
                r={isHot ? 5.5 : 3.5}
                fill={isHot ? "var(--accent)" : "var(--surface-tile)"}
                stroke="var(--accent)"
                strokeWidth="2"
                className="cursor-pointer transition-opacity"
                onMouseEnter={() => setHoveredIdx(idx)}
                onMouseLeave={() => setHoveredIdx(null)}
              >
                <title>
                  {p.snap.driveRoot} — {formatBytes(p.snap.scannedAllocatedBytes)} (
                  {new Date(p.snap.scannedAt).toLocaleString("es-ES")})
                </title>
              </circle>
              {(idx === 0 || idx === pts.length - 1 || idx === Math.floor(pts.length / 2)) && (
                <text
                  x={p.x}
                  y={height - 6}
                  textAnchor={idx === 0 ? "start" : idx === pts.length - 1 ? "end" : "middle"}
                  fill="var(--text-muted)"
                  fontSize="10"
                >
                  {new Date(p.snap.scannedAt).toLocaleDateString("es-ES", {
                    day: "2-digit",
                    month: "short",
                  })}
                </text>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
