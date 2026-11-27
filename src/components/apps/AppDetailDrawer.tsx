import { Trash2 } from "lucide-react";
import { formatBytes } from "../../lib/format";
import type { InstalledApp } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { DetailDrawer, DetailField } from "../ui/DetailDrawer";
import { PathDisplay } from "../ui/PathDisplay";

interface AppDetailDrawerProps {
  app: InstalledApp;
  onClose: () => void;
  onUninstall: (app: InstalledApp) => void;
}

export function AppDetailDrawer({ app, onClose, onUninstall }: AppDetailDrawerProps) {
  return (
    <DetailDrawer
      title={app.name}
      subtitle={app.publisher || "Editor desconocido"}
      badge={
        <Badge tone={app.source === "store_msix" ? "accent" : "neutral"}>
          {app.source === "store_msix" ? "Store / MSIX" : "Win32"}
        </Badge>
      }
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cerrar</Button>
          <Button
            variant="danger"
            onClick={() => {
              onUninstall(app);
              onClose();
            }}
          >
            <Trash2 size={13} />
            Desinstalar (nativo)
          </Button>
        </>
      }
    >
      <DetailField label="Tamaño">
        <span className="mono text-[18px] font-semibold text-accent">
          {app.sizeSource === "unknown" ? "No disponible" : formatBytes(app.sizeBytes)}
        </span>
        <span className="ml-2 text-[12px] text-fg-muted">
          (
          {app.sizeSource === "measured"
            ? "Medido en disco"
            : app.sizeSource === "estimated"
              ? "Estimado por Registro"
              : "Acceso restringido"}
          )
        </span>
      </DetailField>
      {app.version && (
        <DetailField label="Versión">
          <span className="mono">{app.version}</span>
        </DetailField>
      )}
      {app.installLocation && (
        <DetailField label="Carpeta de instalación">
          <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default">
            <PathDisplay path={app.installLocation} maxChars={90} />
          </div>
        </DetailField>
      )}
      {app.uninstallString && (
        <DetailField label="Comando de desinstalación nativo">
          <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default mono text-[11.5px] text-fg-secondary break-all">
            {app.uninstallString}
          </div>
        </DetailField>
      )}
    </DetailDrawer>
  );
}
