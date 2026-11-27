import { BarChart3, Clock, Layers } from "lucide-react";
import { useState } from "react";
import { colorForCategory, formatBytes } from "../../lib/format";
import type { ScanSnapshot } from "../../lib/ipc";
import { DonutChart } from "../charts/DonutChart";
import { HorizontalBarChart } from "../charts/HorizontalBarChart";
import { SnapshotTrendChart } from "../charts/SnapshotTrendChart";
import { Card } from "../ui/Card";
import { SegmentedControl } from "../ui/SegmentedControl";

interface HistoryAnalyticsBentoProps {
  snapshots: ScanSnapshot[];
}

export function HistoryAnalyticsBento({ snapshots }: HistoryAnalyticsBentoProps) {
  const [rightMode, setRightMode] = useState<"dirs" | "duration">("dirs");
  if (snapshots.length === 0) return null;

  const latest = snapshots[0];
  const recentForDuration = [...snapshots].slice(0, 6).reverse();

  const categorySegments =
    latest.categories.length > 0
      ? latest.categories
          .filter((c) => c.allocatedBytes > 0)
          .slice(0, 6)
          .map((c) => ({
            key: c.key,
            label: c.label,
            value: c.allocatedBytes,
            valueLabel: formatBytes(c.allocatedBytes),
            color: colorForCategory(c.key),
          }))
      : [
          {
            key: "used",
            label: "Usado en disco",
            value: Math.max(0, latest.totalDiskBytes - latest.freeDiskBytes),
            valueLabel: formatBytes(Math.max(0, latest.totalDiskBytes - latest.freeDiskBytes)),
            color: "var(--accent)",
          },
          {
            key: "free",
            label: "Espacio libre",
            value: latest.freeDiskBytes,
            valueLabel: formatBytes(latest.freeDiskBytes),
            color: "var(--success)",
          },
        ];

  return (
    <div className="grid grid-cols-1 xl:grid-cols-12 gap-4 items-stretch">
      {/* Left: Chronological Snapshot Trend Chart */}
      <Card className="xl:col-span-6 flex flex-col justify-between gap-2">
        <SnapshotTrendChart snapshots={snapshots} />
      </Card>

      {/* Center/Right: Latest Snapshot Category Donut */}
      <Card className="xl:col-span-3 flex flex-col justify-between gap-3">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5 text-[12.5px] font-semibold text-fg-primary">
            <Layers size={14} className="text-accent shrink-0" />
            <span>Último snapshot ({latest.driveRoot})</span>
          </div>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <DonutChart
            size={126}
            strokeWidth={15}
            centerValue={formatBytes(latest.scannedAllocatedBytes)}
            centerLabel={`${latest.fileCount.toLocaleString()} arch.`}
            segments={categorySegments}
          />
        </div>
      </Card>

      {/* Far Right: Top Directories or Scan Duration Trend */}
      <Card className="xl:col-span-3 flex flex-col justify-between gap-3">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div className="flex items-center gap-1.5 text-[12.5px] font-semibold text-fg-primary">
            {rightMode === "dirs" ? (
              <BarChart3 size={14} className="text-accent shrink-0" />
            ) : (
              <Clock size={14} className="text-accent shrink-0" />
            )}
            <span>{rightMode === "dirs" ? "Top carpetas" : "Tiempos de escaneo"}</span>
          </div>
          <SegmentedControl
            ariaLabel="Gráfico complementario"
            value={rightMode}
            onChange={setRightMode}
            options={[
              { value: "dirs", label: "Top" },
              { value: "duration", label: "Tiempo" },
            ]}
          />
        </div>

        <div className="flex-1 flex flex-col justify-center">
          {rightMode === "dirs" ? (
            <HorizontalBarChart
              maxItems={5}
              emptyText="Sin carpetas registradas en el último snapshot."
              items={latest.topDirs.slice(0, 5).map((d, idx) => ({
                key: `${d.name}-${idx}`,
                label: d.name,
                value: d.allocatedBytes,
                valueLabel: formatBytes(d.allocatedBytes),
                color: ["#3b82f6", "#8b5cf6", "#10b981", "#f59e0b", "#06b6d4"][idx % 5],
              }))}
            />
          ) : (
            <HorizontalBarChart
              maxItems={6}
              emptyText="Sin tiempos registrados."
              items={recentForDuration.map((s) => ({
                key: String(s.id),
                label: `#${s.id} · ${s.driveRoot}`,
                sublabel: new Date(s.scannedAt).toLocaleDateString("es-ES", {
                  day: "2-digit",
                  month: "short",
                }),
                value: Math.max(1, s.durationMs),
                valueLabel: `${(s.durationMs / 1000).toFixed(1)}s`,
                color: s.durationMs > 15_000 ? "var(--warning)" : "var(--accent)",
              }))}
            />
          )}
        </div>
      </Card>
    </div>
  );
}
