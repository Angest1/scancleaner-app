import { ShieldBan, Trash2 } from "lucide-react";
import { useState } from "react";
import { formatBytes } from "../lib/format";
import type { CleanupMode, CleanupPlan, CleanupResult } from "../lib/ipc";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { AlertBanner } from "./ui/Feedback";
import { Modal } from "./ui/Modal";
import { SegmentedControl } from "./ui/SegmentedControl";

interface CleanupModalProps {
  plan: CleanupPlan;
  executing: boolean;
  result: CleanupResult | null;
  onChangeMode: (mode: CleanupMode) => void;
  onExecute: (confirmedPermanent: boolean) => void;
  onClose: () => void;
}

export function CleanupModal({
  plan,
  executing,
  result,
  onChangeMode,
  onExecute,
  onClose,
}: CleanupModalProps) {
  const [confirmPermanent, setConfirmPermanent] = useState(false);
  const canExecute =
    plan.items.length > 0 && !executing && (plan.mode !== "permanent" || confirmPermanent);

  const footer = result ? (
    <Button variant="primary" onClick={onClose}>
      Cerrar
    </Button>
  ) : (
    <>
      <Button onClick={onClose}>Cancelar</Button>
      <Button
        variant={plan.mode === "recycle_bin" ? "primary" : "danger"}
        disabled={!canExecute}
        onClick={() => onExecute(confirmPermanent)}
      >
        <Trash2 size={14} />
        {executing ? "Eliminando..." : "Confirmar y ejecutar limpieza"}
      </Button>
    </>
  );

  return (
    <Modal
      title={result ? "Resultado de limpieza" : "Vista previa de limpieza (Dry-run)"}
      onClose={onClose}
      footer={footer}
    >
      {result ? (
        <>
          <AlertBanner tone="success">
            <div className="font-semibold text-fg-primary">
              Espacio liberado: <span className="mono">{formatBytes(result.freedBytes)}</span>
            </div>
            <div className="text-[12px] text-fg-secondary mt-0.5 tabular">
              Completados: {result.succeededCount} · Fallidos: {result.failedCount} · Bloqueados:{" "}
              {result.skippedCount}
            </div>
          </AlertBanner>

          <div className="max-h-[280px] overflow-auto flex flex-col gap-1.5">
            {result.outcomes.map((o) => (
              <div
                key={o.path}
                className="px-3 py-2 rounded-[7px] bg-canvas border border-border-subtle flex items-center justify-between gap-3"
              >
                <div className="min-w-0">
                  <div className="mono text-[12px] text-fg-primary break-all">{o.path}</div>
                  {o.detail && (
                    <div className="text-[11.5px] text-danger-text mt-0.5">{o.detail}</div>
                  )}
                </div>
                <Badge
                  tone={o.status === "failed" || o.status === "blocked" ? "danger" : "success"}
                >
                  {o.status}
                </Badge>
              </div>
            ))}
          </div>
        </>
      ) : (
        <>
          {plan.mode !== "empty_recycle_bin" && (
            <div className="flex items-center justify-between gap-3 flex-wrap">
              <span className="text-[12.5px] text-fg-secondary">Método de eliminación</span>
              <SegmentedControl
                ariaLabel="Método de eliminación"
                value={plan.mode === "permanent" ? "permanent" : "recycle_bin"}
                onChange={(mode) => {
                  if (mode === "recycle_bin") setConfirmPermanent(false);
                  onChangeMode(mode);
                }}
                options={[
                  { value: "recycle_bin", label: "Papelera (Seguro)" },
                  { value: "permanent", label: "Borrado permanente" },
                ]}
              />
            </div>
          )}

          <div className="px-4 py-3 rounded-[8px] bg-canvas border border-border-default flex items-center justify-between gap-3">
            <div>
              <div className="text-[11.5px] text-fg-muted">Resumen a liberar</div>
              <div className="mono text-[22px] font-semibold text-accent leading-tight mt-0.5">
                {formatBytes(plan.totalBytes)}
              </div>
            </div>
            <div className="text-right text-[12.5px] tabular">
              <div className="text-fg-primary font-medium">
                {plan.items.length} elementos listos
              </div>
              {plan.blockedItems.length > 0 && (
                <div className="text-danger-text text-[12px] mt-0.5">
                  {plan.blockedItems.length} bloqueados por seguridad
                </div>
              )}
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
                Rutas bloqueadas por Guardrails
              </div>
              <div className="flex flex-col gap-1">
                {plan.blockedItems.map((b) => (
                  <div key={b.path} className="text-[12px] text-danger-text break-all">
                    <span className="mono">{b.path}</span> — {b.reason}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="max-h-[240px] overflow-auto flex flex-col gap-1">
            {plan.items.map((item) => (
              <div
                key={item.path}
                className="flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-[6px] bg-canvas border border-border-subtle text-[12px]"
              >
                <span className="mono text-fg-secondary truncate" title={item.path}>
                  {item.path}
                </span>
                <span className="mono text-accent font-medium shrink-0">
                  {formatBytes(item.bytes)}
                </span>
              </div>
            ))}
          </div>

          {plan.mode === "permanent" && (
            <label className="flex items-start gap-2.5 px-3 py-2.5 rounded-[8px] bg-danger-soft border border-danger-border text-[12.5px] text-danger-text cursor-pointer">
              <input
                type="checkbox"
                className="mt-0.5 accent-danger"
                checked={confirmPermanent}
                onChange={(e) => setConfirmPermanent(e.target.checked)}
              />
              <span>
                Confirmo que deseo eliminar estos elementos permanentemente sin pasar por la
                Papelera.
              </span>
            </label>
          )}
        </>
      )}
    </Modal>
  );
}
