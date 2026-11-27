import { AlertTriangle, CheckCircle2, RefreshCw } from "lucide-react";
import type { SyncStatusDto } from "../lib/ipcTypes";

interface SyncStatusPillProps {
  status: SyncStatusDto;
  onManualSync: () => void;
}

/**
 * Discreet topbar auto-sync indicator ("Sincronizando…" / "Actualizado" / observation)
 * with a non-intrusive backup refresh trigger. Never blocks UI or spawns modals.
 */
export function SyncStatusPill({ status, onManualSync }: SyncStatusPillProps) {
  const hasObs = status.observations.length > 0;
  const isBusy = status.syncing || status.pendingBatches > 0;

  const title = hasObs
    ? status.observations[status.observations.length - 1]
    : isBusy
      ? "Aplicando cambios incrementales en segundo plano…"
      : "Índice sincronizado automáticamente. Haz clic para forzar verificación.";

  return (
    <button
      type="button"
      onClick={onManualSync}
      title={title}
      aria-label={isBusy ? "Sincronizando índice" : "Índice actualizado, sincronizar ahora"}
      className={`inline-flex items-center gap-1.5 h-8 px-2.5 rounded-full text-[11.5px] font-medium border cursor-pointer transition-colors duration-150 ${
        hasObs
          ? "bg-warning-soft text-warning-text border-warning-border"
          : isBusy
            ? "bg-accent-soft text-accent border-accent-border"
            : "bg-elevated/80 text-fg-secondary border-border-default hover:text-fg-primary"
      }`}
    >
      {isBusy ? (
        <RefreshCw size={12} className="animate-spin shrink-0" aria-hidden="true" />
      ) : hasObs ? (
        <AlertTriangle size={12} className="shrink-0" aria-hidden="true" />
      ) : (
        <CheckCircle2 size={12} className="text-success shrink-0" aria-hidden="true" />
      )}
      <span className="hidden md:inline">
        {isBusy ? "Sincronizando…" : hasObs ? "Observación" : "Actualizado"}
      </span>
    </button>
  );
}
