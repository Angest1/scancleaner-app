import { AlertTriangle, CheckCircle2, RefreshCw } from "lucide-react";
import type { SyncStatusDto } from "../lib/ipcTypes";

interface SyncStatusPillProps {
  status: SyncStatusDto;
  onManualSync: () => void;
  iconOnly?: boolean;
  className?: string;
}

/**
 * Unified 32px (`h-8 rounded-[9px]`) auto-sync status control shared between
 * AppHeader and AppSidebar for consistent height, radius, and tactile feedback.
 */
export function SyncStatusPill({
  status,
  onManualSync,
  iconOnly = false,
  className = "",
}: SyncStatusPillProps) {
  const hasObs = status.observations.length > 0;
  const isBusy = status.syncing;

  const title = hasObs
    ? status.observations[status.observations.length - 1]
    : isBusy
      ? "Aplicando cambios incrementales en segundo plano…"
      : "Índice sincronizado automáticamente. Haz clic para verificar ahora.";

  const toneClass = hasObs
    ? "bg-warning-soft text-warning-text border-warning-border"
    : isBusy
      ? "bg-accent-soft text-accent border-accent-border"
      : "bg-elevated/85 text-fg-secondary border-border-default hover:text-fg-primary hover:border-border-strong";

  return (
    <button
      type="button"
      onClick={onManualSync}
      disabled={isBusy}
      title={title}
      aria-label={isBusy ? "Sincronizando índice" : "Índice actualizado, sincronizar ahora"}
      className={`inline-flex items-center justify-center gap-1.5 h-8 ${
        iconOnly ? "w-8 px-0" : "px-2.5"
      } rounded-[9px] text-[12px] font-medium border transition-[background-color,color,border-color,transform] duration-140 active:scale-[0.97] ${
        isBusy ? "cursor-wait opacity-85" : "cursor-pointer"
      } ${toneClass} ${className}`}
    >
      {isBusy ? (
        <RefreshCw size={13} className="animate-spin shrink-0" aria-hidden="true" />
      ) : hasObs ? (
        <AlertTriangle size={13} className="shrink-0" aria-hidden="true" />
      ) : (
        <CheckCircle2 size={13} className="text-success shrink-0" aria-hidden="true" />
      )}
      {!iconOnly && (
        <span className="truncate">
          {isBusy ? "Sincronizando…" : hasObs ? "Observación" : "Actualizado"}
        </span>
      )}
    </button>
  );
}
