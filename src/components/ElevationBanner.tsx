import { ArrowUpRight, ShieldAlert, ShieldCheck } from "lucide-react";
import type { ElevationStatus } from "../lib/ipc";

interface ElevationBannerProps {
  elevation: ElevationStatus | null;
  activeDrive: string | null;
  onRelaunch: () => void;
}

export function ElevationBanner({ elevation, activeDrive, onRelaunch }: ElevationBannerProps) {
  const isElevated = elevation?.isElevated ?? false;

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      {isElevated ? (
        <span className="badge badge-warning" title="Ejecutando con privilegios elevados">
          <ShieldCheck size={14} />
          Modo Administrador ({elevation?.username})
        </span>
      ) : (
        <>
          <span className="badge badge-info" title="Ejecución estándar sin privilegios">
            <ShieldAlert size={14} />
            Modo estándar ({elevation?.username ?? "Usuario"})
          </span>
          <button
            type="button"
            className="btn btn-warning"
            onClick={onRelaunch}
            title={`Reiniciar como administrador${activeDrive ? ` (${activeDrive})` : ""}`}
          >
            <ArrowUpRight size={14} />
            Reiniciar como admin
          </button>
        </>
      )}
    </div>
  );
}
