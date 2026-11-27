import { AlertTriangle, HardDrive, Plus, RefreshCw } from "lucide-react";
import { formatBytes, formatDurationMs } from "../../lib/format";
import type { DriveInfo, ScanSummary } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";

interface DriveSwitcherCardProps {
  scanSummary: ScanSummary;
  availableDrives: DriveInfo[];
  indexedSet: Set<string>;
  scanningDrive: string | null;
  skippedLinks: number;
  onSelectDrive: (rootPath: string) => void;
  onRescanDrive: (rootPath: string) => void;
  onToggleDenied: () => void;
}

export function DriveSwitcherCard({
  scanSummary,
  availableDrives,
  indexedSet,
  scanningDrive,
  skippedLinks,
  onSelectDrive,
  onRescanDrive,
  onToggleDenied,
}: DriveSwitcherCardProps) {
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div
          className="flex items-center gap-1.5 flex-wrap"
          role="tablist"
          aria-label="Unidades disponibles"
        >
          {availableDrives.map((d) => {
            const key = d.rootPath.toUpperCase();
            const isActive = scanSummary.rootPath.toUpperCase() === key;
            const isIndexed = indexedSet.has(key);
            const isScanning = scanningDrive?.toUpperCase() === key;
            return (
              <button
                key={d.rootPath}
                type="button"
                role="tab"
                aria-selected={isActive}
                disabled={Boolean(scanningDrive) && !isIndexed}
                onClick={() => onSelectDrive(d.rootPath)}
                title={
                  isIndexed
                    ? `Cambiar a ${d.rootPath} (${formatBytes(d.usedBytes)} usados)`
                    : `Indexar y abrir ${d.rootPath}`
                }
                className={`inline-flex items-center gap-1.5 h-8 px-3 rounded-full text-[12px] font-medium border cursor-pointer transition-colors duration-140 ${
                  isActive
                    ? "bg-accent text-white border-accent shadow-[0_2px_8px_-2px_rgba(59,110,245,0.35)]"
                    : isIndexed
                      ? "bg-elevated text-fg-primary border-border-default hover:border-accent-border hover:text-accent"
                      : "bg-canvas text-fg-secondary border-dashed border-border-default hover:border-accent-border hover:text-fg-primary"
                }`}
              >
                {isScanning ? (
                  <RefreshCw size={12} className="animate-spin shrink-0" />
                ) : isIndexed ? (
                  <HardDrive size={12} className="shrink-0" />
                ) : (
                  <Plus size={12} className="shrink-0" />
                )}
                <span className="mono">{d.rootPath.replace("\\", "")}</span>
                {d.label && <span className="hidden sm:inline opacity-85">({d.label})</span>}
                {!isIndexed && <span className="text-[10.5px] opacity-80">Escanear</span>}
              </button>
            );
          })}
        </div>

        <div className="flex items-center gap-2">
          <Button
            size="sm"
            disabled={Boolean(scanningDrive)}
            onClick={() => onRescanDrive(scanSummary.rootPath)}
            title={`Reescanear ${scanSummary.rootPath}`}
          >
            <RefreshCw
              size={12}
              className={
                scanningDrive?.toUpperCase() === scanSummary.rootPath.toUpperCase()
                  ? "animate-spin"
                  : ""
              }
            />
            Reescanear
          </Button>
          {scanSummary.stats.accessDeniedCount > 0 && (
            <Button variant="warning" size="sm" onClick={onToggleDenied}>
              <AlertTriangle size={13} />
              Sin acceso ({scanSummary.stats.accessDeniedCount})
            </Button>
          )}
        </div>
      </div>

      <div className="flex items-center gap-4 flex-wrap text-[13px] tabular pt-2 border-t border-border-subtle">
        <span className="text-[12px] text-fg-muted">
          <span className="mono text-fg-secondary font-medium">{scanSummary.rootPath}</span> (
          {scanSummary.fsType.toUpperCase()})
        </span>
        <span>
          <strong className="mono font-semibold text-fg-primary">
            {formatBytes(scanSummary.stats.totalAllocatedBytes)}
          </strong>{" "}
          <span className="text-fg-secondary">en disco</span>
        </span>
        <span className="text-fg-secondary">
          {scanSummary.stats.totalFiles.toLocaleString()} archivos
        </span>
        <span className="text-fg-secondary">
          {scanSummary.stats.totalDirs.toLocaleString()} carpetas
        </span>
        <span className="text-fg-secondary">
          Duración: <span className="mono">{formatDurationMs(scanSummary.stats.durationMs)}</span>
        </span>
        {scanSummary.stats.hardlinksDeduplicated > 0 && (
          <Badge tone="accent">
            {scanSummary.stats.hardlinksDeduplicated.toLocaleString()} hardlinks deduplicados
          </Badge>
        )}
        {skippedLinks > 0 && <Badge>{skippedLinks.toLocaleString()} enlaces omitidos</Badge>}
      </div>
    </Card>
  );
}
