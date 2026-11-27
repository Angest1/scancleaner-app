import { ArrowUpRight, ShieldAlert, ShieldCheck } from "lucide-react";
import type { ElevationStatus } from "../lib/ipc";

interface ElevationBannerProps {
  elevation: ElevationStatus | null;
  activeDrive: string | null;
  onRelaunch: () => void;
  layout?: "inline" | "sidebar" | "icon";
}

/**
 * Unified 32px (`h-8 rounded-[9px]`) elevation status & UAC relaunch control.
 * Adapts cleanly across AppHeader (`inline`), expanded AppSidebar (`sidebar`),
 * and collapsed icon-rail (`icon`) without mismatched heights or nested badges.
 */
export function ElevationBanner({
  elevation,
  activeDrive,
  onRelaunch,
  layout = "inline",
}: ElevationBannerProps) {
  const isElevated = elevation?.isElevated ?? false;
  const username = elevation?.username ?? "Usuario";

  if (layout === "icon") {
    return isElevated ? (
      <div
        title={`Administrador (${username})`}
        className="w-8 h-8 rounded-[9px] bg-warning-soft border border-warning-border text-warning grid place-items-center"
      >
        <ShieldCheck size={14} aria-hidden="true" />
      </div>
    ) : (
      <button
        type="button"
        onClick={onRelaunch}
        title={`Estándar (${username}) — Clic para reiniciar como administrador`}
        aria-label="Reiniciar como administrador"
        className="w-8 h-8 rounded-[9px] bg-warning-soft border border-warning-border text-warning-text hover:bg-warning/20 grid place-items-center cursor-pointer transition-[background-color,transform] duration-140 active:scale-[0.97]"
      >
        <ShieldAlert size={14} aria-hidden="true" />
      </button>
    );
  }

  if (layout === "sidebar") {
    if (isElevated) {
      return (
        <div
          title="Ejecutando con privilegios elevados"
          className="w-full h-9 px-2.5 rounded-[9px] bg-warning-soft border border-warning-border flex items-center gap-2 text-[12px] font-medium text-warning-text"
        >
          <ShieldCheck size={14} className="shrink-0" aria-hidden="true" />
          <span className="truncate">Admin · {username}</span>
        </div>
      );
    }

    return (
      <div className="w-full h-9 pl-2.5 pr-1 rounded-[9px] bg-elevated/85 border border-border-default flex items-center justify-between gap-2">
        <span
          className="flex items-center gap-1.5 text-[12px] text-fg-secondary min-w-0"
          title="Ejecución estándar sin privilegios"
        >
          <ShieldAlert size={13} className="text-fg-muted shrink-0" aria-hidden="true" />
          <span className="truncate">{username}</span>
        </span>
        <button
          type="button"
          onClick={onRelaunch}
          title={`Reiniciar como administrador${activeDrive ? ` (${activeDrive})` : ""}`}
          className="inline-flex items-center gap-1 h-7 px-2 rounded-[7px] text-[11.5px] font-medium bg-warning-soft text-warning-text border border-warning-border hover:bg-warning/20 cursor-pointer shrink-0 transition-[background-color,transform] duration-140 active:scale-[0.97]"
        >
          <ArrowUpRight size={12} aria-hidden="true" />
          <span>Admin</span>
        </button>
      </div>
    );
  }

  // Default inline header layout — single cohesive 32px control
  if (isElevated) {
    return (
      <div
        title="Ejecutando con privilegios elevados"
        className="inline-flex items-center gap-1.5 h-8 px-2.5 rounded-[9px] bg-warning-soft border border-warning-border text-[12px] font-medium text-warning-text shrink-0"
      >
        <ShieldCheck size={13} className="shrink-0" aria-hidden="true" />
        <span className="hidden sm:inline">Admin</span>
        <span className="hidden md:inline opacity-80">· {username}</span>
      </div>
    );
  }

  return (
    <div className="inline-flex items-center h-8 rounded-[9px] bg-elevated/85 border border-border-default p-0.5 gap-1 shrink-0">
      <span
        className="hidden sm:inline-flex items-center gap-1.5 pl-2 pr-1 text-[12px] text-fg-secondary"
        title="Ejecución estándar sin privilegios"
      >
        <ShieldAlert size={13} className="text-fg-muted shrink-0" aria-hidden="true" />
        <span>{username}</span>
      </span>
      <button
        type="button"
        onClick={onRelaunch}
        title={`Reiniciar como administrador${activeDrive ? ` (${activeDrive})` : ""}`}
        className="inline-flex items-center gap-1 h-7 px-2 rounded-[7px] text-[11.5px] font-medium bg-warning-soft text-warning-text border border-warning-border hover:bg-warning/20 cursor-pointer transition-[background-color,transform] duration-140 active:scale-[0.97]"
      >
        <ArrowUpRight size={12} aria-hidden="true" />
        <span>Admin</span>
      </button>
    </div>
  );
}
