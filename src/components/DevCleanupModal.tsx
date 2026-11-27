import { listen } from "@tauri-apps/api/event";
import { ShieldBan, Terminal, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { formatBytes } from "../lib/format";
import type { CleanupProgressEvent, DevCleanupPlan, DevCleanupResult } from "../lib/ipc";
import { ActionProgressStage } from "./ui/ActionMotion";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { AlertBanner } from "./ui/Feedback";
import { Modal } from "./ui/Modal";

interface DevCleanupModalProps {
  plan: DevCleanupPlan;
  executing: boolean;
  result: DevCleanupResult | null;
  useRecycleBin: boolean;
  onToggleRecycleBin: (val: boolean) => void;
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
  const [liveProgress, setLiveProgress] = useState<CleanupProgressEvent | null>(null);

  useEffect(() => {
    if (!executing) {
      setLiveProgress(null);
      return;
    }
    let unsub: (() => void) | null = null;
    let active = true;
    listen<CleanupProgressEvent>("dev-cleanup-progress", (ev) => {
      if (active && ev.payload && ev.payload.planId === plan.planId) {
        setLiveProgress(ev.payload);
      }
    })
      .then((fn) => {
        if (active) unsub = fn;
        else fn();
      })
      .catch(() => {});

    return () => {
      active = false;
      unsub?.();
    };
  }, [executing, plan.planId]);

  const footer = result ? (
    <Button variant="primary" onClick={onClose}>
      Cerrar
    </Button>
  ) : executing ? (
    <div className="text-[12px] text-fg-muted">
      Limpieza en progreso — espera a que finalicen los comandos oficiales…
    </div>
  ) : (
    <>
      <Button onClick={onClose}>Cancelar</Button>
      <Button variant="danger" disabled={plan.items.length === 0} onClick={onExecute}>
        <Trash2 size={14} />
        Confirmar y ejecutar Limpieza Dev
      </Button>
    </>
  );

  return (
    <Modal
      title={
        result
          ? "Resultado — Limpieza Dev"
          : executing
            ? "Ejecutando Limpieza Dev…"
            : "Dry-run obligatorio — Limpieza Dev"
      }
      onClose={executing ? () => {} : onClose}
      footer={footer}
      maxWidthClass="max-w-[740px]"
    >
      {executing && !result ? (
        <div key="executing" className="animate-stage-swap">
          <ActionProgressStage
            title="Ejecutando plan de Limpieza Dev…"
            subtitle="Las carpetas grandes se vacían en segundo plano y los comandos oficiales se ejecutan con protección anti-bloqueo."
            progress={liveProgress}
            totalItemsFallback={plan.items.length}
            totalBytesFallback={plan.totalEstimatedBytes}
          />
        </div>
      ) : result ? (
        <div key="result" className="animate-stage-swap flex flex-col gap-3.5">
          <AlertBanner tone={result.failedCount > 0 ? "warning" : "success"}>
            <div className="font-semibold text-fg-primary">
              Espacio liberado real:{" "}
              <span className="mono">
                {formatBytes(result.realFreedBytes ?? result.totalFreedBytes)}
              </span>{" "}
              <span className="text-[12px] font-normal text-fg-secondary">
                (estimado: {formatBytes(result.totalEstimatedBytes)})
              </span>
            </div>
            <div className="text-[12px] text-fg-secondary mt-0.5 tabular">
              Completados: {result.succeededCount} · Omitidos/Bloqueados: {result.skippedCount} ·
              Fallidos: {result.failedCount}
            </div>
            {result.discrepancyExplanation && (
              <div className="text-[11.5px] text-warning-text mt-1">
                {result.discrepancyExplanation}
              </div>
            )}
          </AlertBanner>

          <div className="max-h-[300px] overflow-auto flex flex-col gap-1.5">
            {result.outcomes.map((o) => (
              <div
                key={o.id}
                className="px-3 py-2 rounded-[7px] bg-canvas border border-border-subtle flex items-start justify-between gap-3"
              >
                <div className="min-w-0">
                  <div className="font-medium text-[12.5px] text-fg-primary">{o.nombre}</div>
                  <div className="mono text-[11.5px] text-fg-muted break-all">{o.path}</div>
                  <div className="mono text-[11px] text-fg-secondary mt-0.5">
                    Método: {o.methodLabel} · Liberado: {formatBytes(o.freedBytes)}
                  </div>
                  {o.detail && (
                    <div className="text-[11.5px] text-warning-text mt-0.5">{o.detail}</div>
                  )}
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
            ))}
          </div>
        </div>
      ) : (
        <div key="preview" className="animate-stage-swap flex flex-col gap-3.5">
          <div className="px-4 py-3 rounded-[8px] bg-canvas border border-border-default flex items-center justify-between gap-3 flex-wrap">
            <div>
              <div className="text-[11.5px] text-fg-muted">Espacio estimado a recuperar</div>
              <div className="mono text-[22px] font-semibold text-accent leading-tight mt-0.5">
                {formatBytes(plan.totalEstimatedBytes)}
              </div>
            </div>
            <div className="text-right text-[12.5px] tabular">
              <div className="text-fg-primary font-medium">
                {plan.items.length} elementos listos
              </div>
              {plan.blockedItems.length > 0 && (
                <div className="text-danger-text text-[12px] mt-0.5">
                  {plan.blockedItems.length} bloqueados por lista negra
                </div>
              )}
            </div>
          </div>

          <label className="flex items-center gap-2 text-[12.5px] text-fg-secondary cursor-pointer select-none">
            <input
              type="checkbox"
              className="accent-accent"
              checked={useRecycleBin}
              onChange={(e) => onToggleRecycleBin(e.target.checked)}
            />
            Enviar borrados de carpetas a la Papelera cuando la regla lo permita (más lento en
            árboles grandes como <code className="mono">node_modules</code>)
          </label>

          {plan.warnings.map((w) => (
            <AlertBanner key={w} tone="warning">
              {w}
            </AlertBanner>
          ))}

          {plan.blockedItems.length > 0 && (
            <div className="px-3.5 py-3 rounded-[8px] bg-danger-soft border border-danger-border">
              <div className="flex items-center gap-1.5 font-semibold text-[12.5px] text-fg-primary mb-1.5">
                <ShieldBan size={14} className="text-danger" />
                Bloqueados por lista negra inmutable
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
                className="px-3 py-2 rounded-[7px] bg-canvas border border-border-subtle flex items-start justify-between gap-3"
              >
                <div className="min-w-0">
                  <div className="font-medium text-[12.5px] text-fg-primary flex items-center gap-1.5 flex-wrap">
                    <span>{item.nombre}</span>
                    <Badge tone={item.nivel === "seguro" ? "success" : "warning"}>
                      {item.nivel === "seguro" ? "Seguro" : "Revisar"}
                    </Badge>
                  </div>
                  <div className="mono text-[11.5px] text-fg-muted break-all">{item.path}</div>
                  <div className="text-[11px] text-fg-secondary mt-0.5 flex items-center gap-1.5 flex-wrap">
                    {item.effectiveMethod === "comando-oficial" && item.command ? (
                      <>
                        <Terminal size={11} className="text-accent shrink-0" />
                        <code className="mono">{item.command.join(" ")}</code>
                      </>
                    ) : item.effectiveMethod === "papelera" ? (
                      <span>Papelera de reciclaje</span>
                    ) : (
                      <span>Borrado permanente (staging + vaciado en segundo plano)</span>
                    )}
                  </div>
                </div>
                <span className="mono font-medium text-[12px] text-accent shrink-0">
                  {formatBytes(item.estimatedBytes)}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </Modal>
  );
}
