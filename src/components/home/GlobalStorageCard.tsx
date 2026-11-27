import { FolderTree } from "lucide-react";
import { formatBytes } from "../../lib/format";
import type { DriveInfo, ScanSummary } from "../../lib/ipc";
import { HorizontalBarChart } from "../charts/HorizontalBarChart";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";

interface GlobalStorageCardProps {
  drives: DriveInfo[];
  summaries: ScanSummary[];
  scanningDrive: string | null;
  onExploreDrive: (rootPath: string) => void;
}

export function GlobalStorageCard({
  drives,
  summaries,
  scanningDrive,
  onExploreDrive,
}: GlobalStorageCardProps) {
  const totalBytes = drives.reduce((acc, d) => acc + d.totalBytes, 0);
  const usedBytes = drives.reduce((acc, d) => acc + d.usedBytes, 0);
  const freeBytes = drives.reduce((acc, d) => acc + d.freeBytes, 0);
  const globalPct = totalBytes > 0 ? Math.min(100, (usedBytes / totalBytes) * 100) : 0;
  const indexedMap = new Map(summaries.map((s) => [s.rootPath.toUpperCase(), s]));
  const primaryDrive = drives.find((d) => d.available);

  const radius = 34;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - globalPct / 100);

  return (
    <Card className="lg:col-span-4 flex flex-col justify-between gap-4 animate-fade-up">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-[12px] font-medium text-fg-muted">Capacidad global</div>
          <div className="mono text-[22px] font-semibold text-fg-primary mt-0.5 leading-tight">
            {formatBytes(freeBytes)}
          </div>
          <div className="text-[12px] text-fg-secondary mt-0.5">
            disponibles de <span className="mono">{formatBytes(totalBytes)}</span>
          </div>
        </div>

        <div className="relative w-[84px] h-[84px] shrink-0 grid place-items-center">
          <svg width="84" height="84" viewBox="0 0 84 84" aria-hidden="true" className="-rotate-90">
            <circle
              cx="42"
              cy="42"
              r={radius}
              fill="none"
              stroke="var(--border-default)"
              strokeWidth="7"
            />
            <circle
              cx="42"
              cy="42"
              r={radius}
              fill="none"
              stroke={
                globalPct > 90
                  ? "var(--danger)"
                  : globalPct > 75
                    ? "var(--warning)"
                    : "var(--accent)"
              }
              strokeWidth="7"
              strokeLinecap="round"
              strokeDasharray={circumference}
              strokeDashoffset={dashOffset}
              className="transition-[stroke-dashoffset] duration-300 ease-out"
            />
          </svg>
          <span className="absolute mono text-[13px] font-semibold text-fg-primary">
            {globalPct.toFixed(0)}%
          </span>
        </div>
      </div>

      <div className="flex flex-col gap-2.5 pt-3 border-t border-border-subtle">
        {drives.length > 0 && (
          <div className="pb-1">
            <div className="text-[11px] font-medium text-fg-muted mb-1.5">
              Comparativa de uso por unidad
            </div>
            <HorizontalBarChart
              maxItems={4}
              onSelectItem={onExploreDrive}
              items={drives
                .filter((d) => d.available && d.totalBytes > 0)
                .map((d) => {
                  const pct = (d.usedBytes / d.totalBytes) * 100;
                  return {
                    key: d.rootPath,
                    label: d.label ? `${d.rootPath.replace("\\", "")} ${d.label}` : d.rootPath,
                    value: d.usedBytes,
                    valueLabel: `${formatBytes(d.usedBytes)} (${pct.toFixed(0)}%)`,
                    color:
                      pct > 90 ? "var(--danger)" : pct > 75 ? "var(--warning)" : "var(--accent)",
                  };
                })}
            />
          </div>
        )}

        <div className="flex items-center justify-between gap-2">
          <span className="text-[12px] text-fg-secondary tabular">
            {summaries.length}/{drives.length} unidades indexadas
          </span>
          {primaryDrive && (
            <Button
              variant="primary"
              size="sm"
              disabled={Boolean(scanningDrive)}
              onClick={() => onExploreDrive(primaryDrive.rootPath)}
            >
              <FolderTree size={12} />
              Abrir Explorador
            </Button>
          )}
        </div>

        {drives.length > 1 && (
          <div className="flex items-center gap-1.5 flex-wrap">
            {drives
              .filter((d) => d.available)
              .map((d) => {
                const isIndexed = indexedMap.has(d.rootPath.toUpperCase());
                return (
                  <button
                    key={d.rootPath}
                    type="button"
                    onClick={() => onExploreDrive(d.rootPath)}
                    title={
                      isIndexed
                        ? `Explorar ${d.rootPath} al instante`
                        : `Escanear y abrir ${d.rootPath}`
                    }
                    className="inline-flex items-center gap-1 h-6.5 px-2 rounded-full text-[11px] font-medium mono bg-elevated border border-border-default text-fg-primary hover:border-accent-border hover:text-accent cursor-pointer transition-colors"
                  >
                    <span
                      className={`w-1.5 h-1.5 rounded-full ${
                        isIndexed ? "bg-success" : "bg-fg-muted"
                      }`}
                    />
                    {d.rootPath.replace("\\", "")}
                  </button>
                );
              })}
          </div>
        )}
      </div>
    </Card>
  );
}
