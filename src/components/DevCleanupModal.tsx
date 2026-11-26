import { ShieldBan, Terminal, Trash2 } from "lucide-react";
import { formatBytes } from "../lib/format";
import type { DevCleanupPlan, DevCleanupResult } from "../lib/ipc";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { AlertBanner } from "./ui/Feedback";
import { Modal } from "./ui/Modal";
import { SegmentedControl } from "./ui/SegmentedControl";

interface DevCleanupModalProps {
  plan: DevCleanupPlan;
  executing: boolean;
  result: DevCleanupResult | null;
  useRecycleBin: boolean;
  onToggleRecycleBin: (useRecycle: boolean) => void;
  onExecute: () => void;
  onClose: () => void;
}

export function DevCleanupModal({
  plan,
  executing,
  result,
  useRecycleBin,
  onToggleRecycleBin,
  onExecute,
  onClose,
}: DevCleanupModalProps) {
  const canExecute = plan.items.length > 0 && !executing;

  const footer = result ? (
    <Button variant="primary" onClick={onClose}>
      Cerrar
    </Button>
  ) : (
    <>
      <Button onClick={onClose}>Cancelar</Button>
      <Button variant="danger" disabled={!canExecute} onClick={onExecute}>
        <Trash2 size={14} />
        {executing ? "Ejecutando limpieza..." : `Ejecutar (${plan.items.length} reglas)`}
      </Button>
    </>
  );

  return (
    <Modal
      title={result ? "Resultado de Limpieza Dev" : "Dry-run obligatorio — Limpieza Dev"}
      maxWidthClass="max-w-[780px]"
      onClose={onClose}
      footer={footer}
    >
      {result ? (
        <>
          <AlertBanner
            tone="success"
            action={<Badge tone="success">Log guardado en Historial</Badge>}
          >
            <div className="font-semibold text-fg-primary">
              Espacio liberado real:{" "}
              <span className="mono">{formatBytes(result.totalFreedBytes)}</span>
            </div>
            <div className="text-[12px] text-fg-secondary mt-0.5 tabular">
              Estimado previo: {formatBytes(result.totalEstimatedBytes)} · Completados:{" "}
              {result.succeededCount} · Omitidos: {result.skippedCount} · Fallidos:{" "}
              {result.failedCount}
            </div>
          </AlertBanner>

          <div className="max-h-[320px] overflow-auto flex flex-col gap-1.5">
            {result.outcomes.map((o) => (
              <div
                key={o.id}
                className="px-3 py-2 rounded-[7px] bg-canvas border border-border-subtle flex items-center justify-between gap-3"
              >
                <div className="min-w-0">
                  <div className="font-medium text-[12.5px] text-fg-primary truncate">
                    {o.nombre}
                  </div>
                  <div className="mono text-[11.5px] text-fg-muted truncate">
                    {o.path} · {o.methodLabel}
                  </div>
                  {o.detail && (
                    <div className="text-[11.5px] text-warning-text mt-0.5">{o.detail}</div>
                  )}
                </div>
                <div className="text-right shrink-0 flex flex-col items-end gap-1">
                  <div className="mono text-[12px] font-medium text-fg-primary">
                    {formatBytes(o.freedBytes)}{" "}
                    <span className="text-fg-muted font-normal">
                      / est. {formatBytes(o.estimatedBytes)}
                    </span>
                  </div>
                  <Badge
                    tone={
                      o.status === "failed" || o.status === "blocked"
                        ? "danger"
                        : o.status === "skipped"
                          ? "warning"
                          : "success"
                    }
                  >
                    {o.status}
                  </Badge>
                </div>
              </div>
            ))}
          </div>
        </>
      ) : (
        <>
          <div className="px-4 py-3 rounded-[8px] bg-canvas border border-border-default flex items-center justify-between flex-wrap gap-3">
            <div>
              <div className="text-[11.5px] text-fg-muted">Espacio estimado a recuperar</div>
              <div className="mono text-[22px] font-semibold text-accent leading-tight mt-0.5">
                {formatBytes(plan.totalEstimatedBytes)}
              </div>
            </div>

            <div className="flex items-center gap-2.5 flex-wrap">
              <span className="text-[12px] text-fg-secondary">Artefactos regenerables</span>
              <SegmentedControl
                ariaLabel="Método para artefactos regenerables"
                value={useRecycleBin ? "recycle" : "permanent"}
                onChange={(v) => onToggleRecycleBin(v === "recycle")}
                options={[
                  { value: "permanent", label: "Permanente (Recomendado)" },
                  { value: "recycle", label: "Papelera" },
                ]}
              />
            </div>
          </div>

          {plan.warnings.map((w) => (
            <AlertBanner key={w} tone="warning">
              {w}
            </AlertBanner>
          ))}

          {plan.blockedItems.length > 0 && (
            <div className="px-3.5 py-3 rounded-[8px] bg-danger-soft border border-danger-border">
              <div className="flex items-center gap-1.5 font-semibold text-[12.5px] text-fg-primary mb-1.5">
                <ShieldBan size={14} className="text-danger" />
                Elementos bloqueados ({plan.blockedItems.length})
              </div>
              <div className="flex flex-col gap-1">
                {plan.blockedItems.map((b) => (
                  <div key={b.id} className="text-[12px] text-danger-text break-all">
                    <strong>{b.nombre}</strong> (<span className="mono">{b.path}</span>) —{" "}
                    {b.reason}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="max-h-[280px] overflow-auto flex flex-col gap-1.5">
            {plan.items.map((item) => (
              <div
                key={item.id}
                className="flex items-start justify-between gap-3 px-3 py-2 rounded-[7px] bg-canvas border border-border-subtle text-[12px]"
              >
                <div className="min-w-0 flex flex-col gap-0.5">
                  <div className="flex items-center gap-1.5 flex-wrap">
                    <span className="font-medium text-fg-primary">{item.nombre}</span>
                    <Badge tone={item.nivel === "seguro" ? "success" : "warning"}>
                      {item.nivel}
                    </Badge>
                  </div>
                  <div className="mono text-[11.5px] text-fg-muted break-all">{item.path}</div>
                  <div className="flex items-center gap-1.5 text-[11.5px] text-accent mt-0.5">
                    <Terminal size={12} className="shrink-0" />
                    <span className="mono truncate">
                      {item.effectiveMethod === "comando-oficial" && item.command
                        ? item.command.join(" ")
                        : item.effectiveMethod}
                    </span>
                  </div>
                </div>
                <span className="mono text-accent font-medium shrink-0">
                  {formatBytes(item.estimatedBytes)}
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </Modal>
  );
}
