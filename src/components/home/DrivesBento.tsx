import { FolderTree, HardDrive, Network, Play, RefreshCw, Square, Usb } from "lucide-react";
import { formatBytes, formatDurationMs } from "../../lib/format";
import type { DriveInfo, ScanProgress, ScanSummary } from "../../lib/ipc";
import { InlineActivityBanner, MiniLeapfrogLoader } from "../ui/ActionMotion";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Collapse } from "../ui/Collapse";
import { EmptyState, Skeleton } from "../ui/Feedback";
import { GlobalStorageCard } from "./GlobalStorageCard";

interface DrivesBentoProps {
  drives: DriveInfo[];
  summaries: ScanSummary[];
  loading: boolean;
  syncing: boolean;
  includeNetwork: boolean;
  scanningDrive: string | null;
  progress: ScanProgress | null;
  onExploreDrive: (rootPath: string) => void;
  onToggleNetwork: (include: boolean) => void;
  onRefresh: () => void;
  onStartScan: (rootPath: string) => void;
  onCancelScan: () => void;
}

export function DrivesBento({
  drives,
  summaries,
  loading,
  syncing,
  includeNetwork,
  scanningDrive,
  progress,
  onExploreDrive,
  onToggleNetwork,
  onRefresh,
  onStartScan,
  onCancelScan,
}: DrivesBentoProps) {
  const isBusy = loading || syncing || Boolean(scanningDrive);
  const indexedMap = new Map(summaries.map((s) => [s.rootPath.toUpperCase(), s]));

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
          <Button size="sm" onClick={onRefresh} disabled={isBusy}>
            <RefreshCw size={13} className={loading || syncing ? "animate-spin" : ""} />
            {syncing ? "Sincronizando…" : loading ? "Actualizando…" : "Actualizar"}
          </Button>
        </div>
      </div>

      <InlineActivityBanner
        active={syncing && !scanningDrive}
        delayMs={350}
        title="Sincronizando unidades e índice en vivo…"
        detail="Verificando espacio libre real y cambios recientes en tus volúmenes."
      />

      <Collapse open={Boolean(scanningDrive)}>
        {scanningDrive && (
          <Card className="border-accent-border">
            <div className="flex items-center justify-between flex-wrap gap-3 mb-2.5">
              <div className="flex items-center gap-3 min-w-0">
                <MiniLeapfrogLoader />
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
              </div>
              <Button variant="danger" size="sm" onClick={onCancelScan}>
                <Square size={12} />
                Cancelar
              </Button>
            </div>

            <div className="w-full h-2 rounded-full bg-elevated overflow-hidden relative">
              <div className="h-full w-2/5 rounded-full bg-accent sc-shimmer-track" />
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
      </Collapse>

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
          <GlobalStorageCard
            drives={drives}
            summaries={summaries}
            scanningDrive={scanningDrive}
            onExploreDrive={onExploreDrive}
          />

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
                    <span className="mono text-[11.5px] text-fg-muted truncate">
                      {formatBytes(drive.totalBytes)} · {usedPct.toFixed(1)}%{(() => {
                        const s = indexedMap.get(drive.rootPath.toUpperCase());
                        return s ? ` · ${s.stats.totalFiles.toLocaleString()} arch.` : "";
                      })()}
                    </span>
                    <div className="flex items-center gap-1.5 shrink-0">
                      {indexedMap.has(drive.rootPath.toUpperCase()) ? (
                        <>
                          <Button
                            variant="ghost"
                            size="icon"
                            disabled={!drive.available || Boolean(scanningDrive)}
                            onClick={() => onStartScan(drive.rootPath)}
                            title={`Reescanear ${drive.rootPath}`}
                            aria-label={`Reescanear ${drive.rootPath}`}
                          >
                            <RefreshCw size={12} />
                          </Button>
                          <Button
                            variant="primary"
                            size="sm"
                            disabled={!drive.available}
                            onClick={() => onExploreDrive(drive.rootPath)}
                          >
                            <FolderTree size={12} />
                            Explorar
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={!drive.available || Boolean(scanningDrive)}
                          onClick={() => onStartScan(drive.rootPath)}
                        >
                          <Play size={12} />
                          {drive.available ? "Escanear" : "No disponible"}
                        </Button>
                      )}
                    </div>
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
