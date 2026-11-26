import { HardDrive, Network, Play, RefreshCw, Square, Usb } from "lucide-react";
import { formatBytes, formatDurationMs } from "../../lib/format";
import type { DriveInfo, ScanProgress } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState, Skeleton } from "../ui/Feedback";

interface DrivesBentoProps {
  drives: DriveInfo[];
  loading: boolean;
  includeNetwork: boolean;
  scanningDrive: string | null;
  progress: ScanProgress | null;
  onToggleNetwork: (include: boolean) => void;
  onRefresh: () => void;
  onStartScan: (rootPath: string) => void;
  onCancelScan: () => void;
}

export function DrivesBento({
  drives,
  loading,
  includeNetwork,
  scanningDrive,
  progress,
  onToggleNetwork,
  onRefresh,
  onStartScan,
  onCancelScan,
}: DrivesBentoProps) {
  const totalBytes = drives.reduce((acc, d) => acc + d.totalBytes, 0);
  const usedBytes = drives.reduce((acc, d) => acc + d.usedBytes, 0);
  const freeBytes = drives.reduce((acc, d) => acc + d.freeBytes, 0);
  const globalPct = totalBytes > 0 ? Math.min(100, (usedBytes / totalBytes) * 100) : 0;
  const primaryDrive = drives.find((d) => d.available);

  // SVG Donut ring geometry
  const radius = 34;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference * (1 - globalPct / 100);

  return (
    <section aria-label="Unidades de almacenamiento" className="flex flex-col gap-4">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h2 className="m-0 text-[16px] font-semibold tracking-[-0.02em] text-fg-primary">
            Unidades de almacenamiento
          </h2>
          <p className="m-0 mt-0.5 text-[12.5px] text-fg-secondary">
            Estado en tiempo real y escaneo rápido en un clic.
          </p>
        </div>

        <div className="flex items-center gap-3 flex-wrap">
          <label className="inline-flex items-center gap-2 text-[12.5px] text-fg-secondary cursor-pointer select-none">
            <input
              type="checkbox"
              className="accent-accent"
              checked={includeNetwork}
              onChange={(e) => onToggleNetwork(e.target.checked)}
            />
            Incluir unidades de red
          </label>
          <Button size="sm" onClick={onRefresh} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            Actualizar
          </Button>
        </div>
      </div>

      {scanningDrive && (
        <Card className="border-accent-border animate-fade-up">
          <div className="flex items-center justify-between flex-wrap gap-3 mb-2.5">
            <div className="min-w-0">
              <div className="font-semibold text-[14px] text-fg-primary">
                Escaneando unidad <span className="mono text-accent">{scanningDrive}</span>
              </div>
              <div className="mono text-[12px] text-fg-secondary mt-0.5">
                {progress
                  ? `${progress.scannedFiles.toLocaleString()} archivos · ${progress.scannedDirs.toLocaleString()} carpetas · ${formatBytes(progress.totalAllocatedBytes)} · ${formatDurationMs(progress.elapsedMs)}`
                  : "Iniciando enumeración de archivos…"}
              </div>
            </div>
            <Button variant="danger" size="sm" onClick={onCancelScan}>
              <Square size={12} />
              Cancelar
            </Button>
          </div>

          <div className="w-full h-2 rounded-full bg-elevated overflow-hidden relative">
            <div className="h-full w-2/5 rounded-full bg-accent animate-scan-bar" />
          </div>

          {progress?.currentPath && (
            <div
              className="mono mt-2 text-[11.5px] text-fg-muted truncate"
              title={progress.currentPath}
            >
              {progress.currentPath}
            </div>
          )}
        </Card>
      )}

      {loading && drives.length === 0 ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {[0, 1, 2].map((i) => (
            <Card key={i} className="flex flex-col gap-3">
              <div className="flex items-center gap-3">
                <Skeleton className="w-10 h-10 rounded-[12px]" />
                <div className="flex-1 flex flex-col gap-1.5">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-20" />
                </div>
              </div>
              <Skeleton className="h-2 w-full rounded-full" />
              <Skeleton className="h-8 w-full rounded-[9px]" />
            </Card>
          ))}
        </div>
      ) : drives.length === 0 ? (
        <EmptyState
          icon={<HardDrive size={18} />}
          title="No se detectaron unidades"
          description="Actualiza la lista o habilita las unidades de red si deseas analizar volúmenes remotos."
          action={
            <Button variant="primary" onClick={onRefresh}>
              <RefreshCw size={13} />
              Actualizar unidades
            </Button>
          }
        />
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-12 gap-4">
          {/* Global storage summary Bento tile */}
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
                <svg
                  width="84"
                  height="84"
                  viewBox="0 0 84 84"
                  aria-hidden="true"
                  className="-rotate-90"
                >
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

            <div className="flex items-center justify-between gap-2 pt-3 border-t border-border-subtle">
              <span className="text-[12px] text-fg-secondary tabular">
                {drives.length} {drives.length === 1 ? "unidad detectada" : "unidades detectadas"}
              </span>
              {primaryDrive && (
                <Button
                  variant="primary"
                  size="sm"
                  disabled={Boolean(scanningDrive)}
                  onClick={() => onStartScan(primaryDrive.rootPath)}
                >
                  <Play size={12} />
                  Escanear {primaryDrive.rootPath}
                </Button>
              )}
            </div>
          </Card>

          {/* Individual drive Bento cards */}
          <div className="lg:col-span-8 grid grid-cols-1 sm:grid-cols-2 gap-4">
            {drives.map((drive, idx) => {
              const usedPct =
                drive.totalBytes > 0
                  ? Math.min(100, (drive.usedBytes / drive.totalBytes) * 100)
                  : 0;
              const isCritical = usedPct > 90;
              const isHigh = usedPct > 75 && !isCritical;
              const Icon =
                drive.kind === "removable" ? Usb : drive.kind === "network" ? Network : HardDrive;

              return (
                <Card
                  key={drive.rootPath}
                  interactive
                  style={{ animationDelay: `${(idx + 1) * 40}ms` }}
                  className="flex flex-col justify-between gap-3.5 animate-fade-up"
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2.5 min-w-0">
                      <div className="w-10 h-10 rounded-[12px] bg-accent-soft border border-accent-border grid place-items-center text-accent shrink-0">
                        <Icon size={18} />
                      </div>
                      <div className="min-w-0">
                        <div className="font-semibold text-[14px] text-fg-primary truncate">
                          {drive.label ? `${drive.label} (${drive.rootPath})` : drive.rootPath}
                        </div>
                        <div className="text-[11.5px] text-fg-muted">
                          {drive.kind === "fixed"
                            ? "Disco fijo"
                            : drive.kind === "removable"
                              ? "Unidad removible"
                              : "Unidad de red"}
                        </div>
                      </div>
                    </div>
                    <Badge tone="neutral">{drive.fsName || "N/D"}</Badge>
                  </div>

                  <div className="flex flex-col gap-1.5">
                    <div className="w-full h-2 rounded-full bg-elevated overflow-hidden">
                      <div
                        className={`h-full rounded-full transition-[width] duration-250 ${
                          isCritical ? "bg-danger" : isHigh ? "bg-warning" : "bg-accent"
                        }`}
                        style={{ width: `${usedPct}%` }}
                      />
                    </div>
                    <div className="flex items-center justify-between text-[12px] text-fg-secondary tabular">
                      <span>
                        Usado:{" "}
                        <span className="mono font-medium text-fg-primary">
                          {formatBytes(drive.usedBytes)}
                        </span>
                      </span>
                      <span>
                        Libre:{" "}
                        <span className="mono font-medium text-fg-primary">
                          {formatBytes(drive.freeBytes)}
                        </span>
                      </span>
                    </div>
                  </div>

                  <div className="flex items-center justify-between gap-2 pt-1">
                    <span className="mono text-[11.5px] text-fg-muted">
                      {formatBytes(drive.totalBytes)} · {usedPct.toFixed(1)}%
                    </span>
                    <Button
                      variant="primary"
                      size="sm"
                      disabled={!drive.available || Boolean(scanningDrive)}
                      onClick={() => onStartScan(drive.rootPath)}
                    >
                      <Play size={12} />
                      {drive.available ? "Escanear" : "No disponible"}
                    </Button>
                  </div>
                </Card>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}
