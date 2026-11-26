import { useState } from "react";
import { colorForCategory, formatBytes } from "../lib/format";
import type { CategoryBreakdown } from "../lib/ipc";

interface CategoryChartsProps {
  breakdown: CategoryBreakdown | null;
}

export function CategoryCharts({ breakdown }: CategoryChartsProps) {
  const [mode, setMode] = useState<"type" | "origin">("type");

  if (!breakdown) {
    return (
      <div style={{ padding: 24, color: "var(--text-muted)" }}>
        Escanea una unidad para visualizar el desglose por categorías.
      </div>
    );
  }

  const buckets = mode === "type" ? breakdown.byType : breakdown.byOrigin;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className={`btn ${mode === "type" ? "btn-primary" : ""}`}
            onClick={() => setMode("type")}
          >
            Por tipo de archivo
          </button>
          <button
            type="button"
            className={`btn ${mode === "origin" ? "btn-primary" : ""}`}
            onClick={() => setMode("origin")}
          >
            Por origen
          </button>
        </div>
        <span className="mono" style={{ color: "var(--text-secondary)", fontSize: 12.5 }}>
          Total medido: {formatBytes(breakdown.totalAllocatedBytes)}
        </span>
      </div>

      {/* Stacked bar summary */}
      <div
        style={{
          height: 22,
          borderRadius: 6,
          overflow: "hidden",
          display: "flex",
          background: "#0f172a",
          border: "1px solid var(--border)",
        }}
      >
        {buckets
          .filter((b) => b.percentage > 0.3)
          .map((b) => (
            <div
              key={b.key}
              title={`${b.label}: ${formatBytes(b.allocatedBytes)} (${b.percentage.toFixed(1)}%)`}
              style={{
                width: `${b.percentage}%`,
                background: colorForCategory(b.key),
              }}
            />
          ))}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {buckets.map((b) => (
          <div
            key={b.key}
            style={{
              display: "grid",
              gridTemplateColumns: "210px 1fr 120px 70px",
              alignItems: "center",
              gap: 12,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: 3,
                  background: colorForCategory(b.key),
                  flexShrink: 0,
                }}
              />
              <span style={{ fontWeight: 500 }}>{b.label}</span>
            </div>

            <div className="progress-track">
              <div
                style={{
                  height: "100%",
                  width: `${Math.min(100, b.percentage)}%`,
                  background: colorForCategory(b.key),
                }}
              />
            </div>

            <span className="mono" style={{ textAlign: "right", fontSize: 12.5 }}>
              {formatBytes(b.allocatedBytes)}
            </span>

            <span style={{ textAlign: "right", color: "var(--text-muted)", fontSize: 12 }}>
              {b.percentage.toFixed(1)}%
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
