import { ArrowUpRight, ShieldAlert, ShieldCheck } from "lucide-react";
import type { ElevationStatus } from "../lib/ipc";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";

interface ElevationBannerProps {
  elevation: ElevationStatus | null;
  activeDrive: string | null;
  onRelaunch: () => void;
}

export function ElevationBanner({ elevation, activeDrive, onRelaunch }: ElevationBannerProps) {
  const isElevated = elevation?.isElevated ?? false;

  return (
    <div className="flex items-center gap-2 shrink-0">
      {isElevated ? (
        <Badge tone="warning" title="Ejecutando con privilegios elevados">
          <ShieldCheck size={12} />
          <span className="hidden sm:inline">Administrador</span> ({elevation?.username})
        </Badge>
      ) : (
        <>
          <Badge
            tone="neutral"
            className="hidden sm:inline-flex"
            title="Ejecución estándar sin privilegios"
          >
            <ShieldAlert size={12} className="text-fg-muted" />
            Estándar ({elevation?.username ?? "Usuario"})
          </Badge>
          <Button
            variant="warning"
            size="sm"
            onClick={onRelaunch}
            title={`Reiniciar como administrador${activeDrive ? ` (${activeDrive})` : ""}`}
          >
            <ArrowUpRight size={13} />
            <span>Admin</span>
          </Button>
        </>
      )}
    </div>
  );
}
