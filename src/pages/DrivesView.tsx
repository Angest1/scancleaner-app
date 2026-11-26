import { HardDrive, Network, Play, RefreshCw, Square, Usb } from "lucide-react";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { EmptyState, Skeleton } from "../components/ui/Feedback";
import { formatBytes, formatDurationMs } from "../lib/format";
import type { DriveInfo, ScanProgress } from "../lib/ipc";

interface DrivesViewProps {
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

export function DrivesView({
  drives,
  loading,
  includeNetwork,
  scanningDrive,
  progress,
  onToggleNetwork,
  onRefresh,
  onStartScan,
  onCancelScan,
}: DrivesViewProps) {
  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[18px] font-semibold tracking-[-0.02em] text-fg-primary">
            Unidades de almacenamiento
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary">
            Selecciona una unidad para analizar su estructura, categorías y oportunidades de
            limpieza.
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
          <Button onClick={onRefresh} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            Actualizar
          </Button>
        </div>
      </div>

      {scanningDrive && (
        <Card className="border-accent-border bg-panel">
          <div className="flex items-center justify-between flex-wrap gap-3 mb-2.5">
            <div className="min-w-0">
              <div className="font-semibold text-[14px] text-fg-primary">
                Escaneando unidad <span className="mono text-accent">{scanningDrive}</span>
              </div>
              <div className="mono text-[12px] text-fg-secondary mt-0.5">
                {progress
                  ? `${progress.scannedFiles.toLocaleString()} archivos · ${progress.scannedDirs.toLocaleString()} carpetas · ${formatBytes(progress.totalAllocatedBytes)} · ${formatDurationMs(progress.elapsedMs)}`
                  : "Iniciando enumeración de archivos..."}
              </div>
            </div>
            <Button variant="danger" size="sm" onClick={onCancelScan}>
              <Square size={12} />
              Cancelar
            </Button>
          </div>

          <div className="w-full h-1.5 rounded-full bg-canvas overflow-hidden relative">
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
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5">
          {[0, 1, 2].map((i) => (
            <Card key={i} className="flex flex-col gap-3">
              <div className="flex items-center gap-3">
                <Skeleton className="w-9 h-9 rounded-[8px]" />
                <div className="flex-1 flex flex-col gap-1.5">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-20" />
                </div>
              </div>
              <Skeleton className="h-2 w-full rounded-full" />
              <Skeleton className="h-8 w-full rounded-[7px]" />
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
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3.5">
          {drives.map((drive) => {
            const usedPct =
              drive.totalBytes > 0 ? Math.min(100, (drive.usedBytes / drive.totalBytes) * 100) : 0;
            const isCritical = usedPct > 90;
            const isHigh = usedPct > 75 && !isCritical;
            const Icon =
              drive.kind === "removable" ? Usb : drive.kind === "network" ? Network : HardDrive;

            return (
              <Card key={drive.rootPath} className="flex flex-col gap-3.5">
                <div className="flex items-start justify-between gap-2">
                  <div className="flex items-center gap-2.5 min-w-0">
                    <div className="w-9 h-9 rounded-[8px] bg-elevated border border-border-subtle grid place-items-center text-accent shrink-0">
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
                  <div className="w-full h-1.5 rounded-full bg-canvas overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-[width] duration-200 ${
                        isCritical ? "bg-danger" : isHigh ? "bg-warning" : "bg-accent"
                      }`}
                      style={{ width: `${usedPct}%` }}
                    />
                  </div>
                  <div className="flex items-center justify-between text-[12px] text-fg-secondary tabular">
                    <span>
                      Usado:{" "}
                      <span className="mono text-fg-primary">{formatBytes(drive.usedBytes)}</span>
                    </span>
                    <span>
                      Libre:{" "}
                      <span className="mono text-fg-primary">{formatBytes(drive.freeBytes)}</span>
                    </span>
                  </div>
                  <div className="text-[11.5px] text-fg-muted mono">
                    Total: {formatBytes(drive.totalBytes)} ({usedPct.toFixed(1)}%)
                  </div>
                </div>

                <Button
                  variant="primary"
                  className="w-full"
                  disabled={!drive.available || Boolean(scanningDrive)}
                  onClick={() => onStartScan(drive.rootPath)}
                >
                  <Play size={13} />
                  {drive.available ? "Escanear unidad" : "No disponible"}
                </Button>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
