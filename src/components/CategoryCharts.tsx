import { useState } from "react";
import { colorForCategory, formatBytes } from "../lib/format";
import type { CategoryBreakdown } from "../lib/ipc";
import { DonutChart } from "./charts/DonutChart";
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
      <div className="h-3.5 rounded-[6px] overflow-hidden flex bg-canvas border border-border-default">
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

      <div className="grid grid-cols-1 lg:grid-cols-12 gap-5 items-center">
        <div className="lg:col-span-5 p-3 rounded-[10px] bg-canvas/60 border border-border-subtle">
          <DonutChart
            size={150}
            strokeWidth={18}
            centerValue={formatBytes(breakdown.totalAllocatedBytes)}
            centerLabel={mode === "type" ? "Por tipo" : "Por origen"}
            segments={buckets.map((b) => ({
              key: b.key,
              label: b.label,
              value: b.allocatedBytes,
              valueLabel: formatBytes(b.allocatedBytes),
              color: colorForCategory(b.key),
            }))}
          />
        </div>

        <div className="lg:col-span-7 flex flex-col gap-2.5">
          {buckets.map((b) => (
            <div
              key={b.key}
              className="grid grid-cols-[140px_1fr_88px_50px] sm:grid-cols-[180px_1fr_104px_56px] items-center gap-3"
            >
              <div className="flex items-center gap-2 min-w-0">
                <span
                  className="w-2.5 h-2.5 rounded-[3px] shrink-0"
                  style={{ backgroundColor: colorForCategory(b.key) }}
                />
                <span className="font-medium text-[12.5px] text-fg-primary truncate">
                  {b.label}
                </span>
              </div>

              <div className="w-full h-2 rounded-full bg-canvas overflow-hidden">
                <div
                  className="h-full w-full rounded-full sc-bar-grow"
                  style={{
                    transform: `scaleX(${Math.max(0.01, Math.min(1, b.percentage / 100))})`,
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
    </div>
  );
}
