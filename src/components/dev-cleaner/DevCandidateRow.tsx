import { Lock, MoreHorizontal, Terminal } from "lucide-react";
import { useState } from "react";
import { formatBytes, formatUnixTs } from "../../lib/format";
import type { DevCandidate } from "../../lib/ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { DetailDrawer, DetailField } from "../ui/DetailDrawer";
import { PathDisplay } from "../ui/PathDisplay";

interface DevCandidateRowProps {
  item: DevCandidate;
  checked: boolean;
  locked: boolean;
  isElevated: boolean;
  onToggle: (item: DevCandidate) => void;
  onRelaunchAdmin: () => void;
}

export function DevCandidateRow({
  item,
  checked,
  locked,
  isElevated,
  onToggle,
  onRelaunchAdmin,
}: DevCandidateRowProps) {
  const [inspecting, setInspecting] = useState(false);

  return (
    <>
      <div
        className={`flex items-start justify-between gap-3 px-3 py-2.5 rounded-[8px] bg-canvas border transition-colors duration-140 ${
          checked ? "border-accent-border bg-accent-soft/25" : "border-border-subtle"
        } ${locked ? "opacity-65" : ""}`}
      >
        <label
          className={`flex items-start gap-2.5 flex-1 min-w-0 ${
            locked ? "cursor-not-allowed" : "cursor-pointer"
          }`}
        >
          <input
            type="checkbox"
            className="mt-0.5 accent-accent shrink-0"
            checked={checked}
            disabled={locked}
            onChange={() => onToggle(item)}
          />
          <div className="min-w-0 flex-1 flex flex-col gap-1">
            <div className="flex items-center gap-1.5 flex-wrap">
              <span className="font-medium text-[13px] text-fg-primary">{item.nombre}</span>
              <Badge tone={item.nivel === "seguro" ? "success" : "warning"}>{item.nivel}</Badge>
              {item.noGitRepo && <Badge tone="warning">Sin repo Git</Badge>}
              {item.projectAgeDays !== null && (
                <Badge>Sin cambios hace {item.projectAgeDays} d</Badge>
              )}
            </div>

            <PathDisplay path={item.path} maxChars={58} className="text-[11.5px] text-fg-muted" />

            <div className="text-[11.5px] text-fg-secondary truncate">
              <span className="text-accent font-medium">Regeneración:</span> {item.comoRegenerar}
            </div>

            {item.warning && <div className="text-[11.5px] text-warning-text">{item.warning}</div>}
          </div>
        </label>

        <div className="flex items-center gap-2 shrink-0">
          {item.lastModifiedTs > 0 && (
            <span className="hidden md:inline text-[11.5px] text-fg-muted tabular">
              {formatUnixTs(item.lastModifiedTs)}
            </span>
          )}
          <span className="mono font-medium text-[12.5px] text-fg-primary min-w-[74px] text-right">
            {item.sizeBytes > 0 ? formatBytes(item.sizeBytes) : "—"}
          </span>
          {item.requiresAdmin && !isElevated && (
            <Button variant="warning" size="sm" onClick={onRelaunchAdmin}>
              <Lock size={12} />
              Admin
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setInspecting(true)}
            title="Ver detalles y comando"
            aria-label={`Inspeccionar ${item.nombre}`}
          >
            <MoreHorizontal size={14} />
          </Button>
        </div>
      </div>

      {inspecting && (
        <DetailDrawer
          title={item.nombre}
          subtitle={`Categoría: ${item.categoria}`}
          badge={<Badge tone={item.nivel === "seguro" ? "success" : "warning"}>{item.nivel}</Badge>}
          onClose={() => setInspecting(false)}
          footer={<Button onClick={() => setInspecting(false)}>Cerrar</Button>}
        >
          <DetailField label="Tamaño estimado">
            <span className="mono text-[18px] font-semibold text-accent">
              {item.sizeBytes > 0 ? formatBytes(item.sizeBytes) : "Calculado al ejecutar"}
            </span>
          </DetailField>
          <DetailField label="Ruta completa">
            <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default">
              <PathDisplay path={item.path} maxChars={120} />
            </div>
          </DetailField>
          <DetailField label="Cómo regenerar">{item.comoRegenerar}</DetailField>
          {item.command && (
            <DetailField label="Comando oficial">
              <div className="p-2.5 rounded-[8px] bg-canvas border border-border-default mono text-[12px] text-accent flex items-center gap-2 break-all">
                <Terminal size={13} className="shrink-0" />
                <span>{item.command.join(" ")}</span>
              </div>
            </DetailField>
          )}
          {item.warning && (
            <DetailField label="Advertencia">
              <span className="text-warning-text">{item.warning}</span>
            </DetailField>
          )}
          {item.blockedReason && (
            <DetailField label="Motivo de bloqueo">
              <span className="text-danger-text">{item.blockedReason}</span>
            </DetailField>
          )}
        </DetailDrawer>
      )}
    </>
  );
}
