import { useState } from "react";
import { colorForCategory, formatBytes } from "../lib/format";
import type { CategoryBreakdown } from "../lib/ipc";
import { SegmentedControl } from "./ui/SegmentedControl";

interface CategoryChartsProps {
  breakdown: CategoryBreakdown | null;
}

export function CategoryCharts({ breakdown }: CategoryChartsProps) {
  const [mode, setMode] = useState<"type" | "origin">("type");

  if (!breakdown) {
    return (
      <div className="py-6 text-[12.5px] text-fg-muted">
        Escanea una unidad para visualizar el desglose por categorías.
      </div>
    );
  }

  const buckets = mode === "type" ? breakdown.byType : breakdown.byOrigin;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <SegmentedControl
          ariaLabel="Agrupar categorías"
          value={mode}
          onChange={setMode}
          options={[
            { value: "type", label: "Por tipo de archivo" },
            { value: "origin", label: "Por origen" },
          ]}
        />
        <span className="mono text-[12px] text-fg-secondary">
          Total medido:{" "}
          <strong className="text-fg-primary">{formatBytes(breakdown.totalAllocatedBytes)}</strong>
        </span>
      </div>

      {/* Stacked bar summary */}
      <div className="h-4 rounded-[6px] overflow-hidden flex bg-canvas border border-border-default">
        {buckets
          .filter((b) => b.percentage > 0.3)
          .map((b) => (
            <div
              key={b.key}
              title={`${b.label}: ${formatBytes(b.allocatedBytes)} (${b.percentage.toFixed(1)}%)`}
              className="h-full border-r border-canvas/70 last:border-r-0"
              style={{
                width: `${b.percentage}%`,
                backgroundColor: colorForCategory(b.key),
              }}
            />
          ))}
      </div>

      <div className="flex flex-col gap-2.5">
        {buckets.map((b) => (
          <div
            key={b.key}
            className="grid grid-cols-[160px_1fr_95px_54px] sm:grid-cols-[200px_1fr_110px_62px] items-center gap-3"
          >
            <div className="flex items-center gap-2 min-w-0">
              <span
                className="w-2.5 h-2.5 rounded-[3px] shrink-0"
                style={{ backgroundColor: colorForCategory(b.key) }}
              />
              <span className="font-medium text-[12.5px] text-fg-primary truncate">{b.label}</span>
            </div>

            <div className="w-full h-1.5 rounded-full bg-canvas overflow-hidden">
              <div
                className="h-full rounded-full"
                style={{
                  width: `${Math.min(100, b.percentage)}%`,
                  backgroundColor: colorForCategory(b.key),
                }}
              />
            </div>

            <span className="mono text-right text-[12px] text-fg-primary">
              {formatBytes(b.allocatedBytes)}
            </span>

            <span className="mono text-right text-[11.5px] text-fg-muted">
              {b.percentage.toFixed(1)}%
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
