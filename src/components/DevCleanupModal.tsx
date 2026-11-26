import { AlertTriangle, CheckCircle2, ShieldBan, Terminal, Trash2, X } from "lucide-react";
import { formatBytes } from "../lib/format";
import type { DevCleanupPlan, DevCleanupResult } from "../lib/ipc";

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

  return (
    <div className="modal-backdrop">
      <div className="modal-panel" style={{ maxWidth: 780 }}>
        <div
          style={{
            padding: "14px 18px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <div style={{ fontWeight: 700, fontSize: 15 }}>
            {result ? "Resultado de Limpieza Dev" : "Dry-run obligatorio — Limpieza Dev"}
          </div>
          <button type="button" className="btn" style={{ padding: 6 }} onClick={onClose}>
            <X size={15} />
          </button>
        </div>

        <div
          style={{
            padding: 18,
            overflow: "auto",
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          {result ? (
            <>
              <div
                className="card"
                style={{
                  borderColor: "rgba(34, 197, 94, 0.45)",
                  background: "rgba(34, 197, 94, 0.08)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  flexWrap: "wrap",
                  gap: 12,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                  <CheckCircle2 size={24} color="#22c55e" />
                  <div>
                    <div style={{ fontWeight: 700, fontSize: 15 }}>
                      Espacio liberado real: {formatBytes(result.totalFreedBytes)}
                    </div>
                    <div style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>
                      Estimado previo: {formatBytes(result.totalEstimatedBytes)} · Completados:{" "}
                      {result.succeededCount} · Omitidos: {result.skippedCount} · Fallidos:{" "}
                      {result.failedCount}
                    </div>
                  </div>
                </div>
                <span className="badge badge-success">Log guardado en Historial</span>
              </div>

              <div
                style={{
                  maxHeight: 320,
                  overflow: "auto",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                {result.outcomes.map((o) => (
                  <div
                    key={o.id}
                    className="card"
                    style={{
                      padding: "8px 12px",
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: 10,
                    }}
                  >
                    <div style={{ overflow: "hidden" }}>
                      <div style={{ fontWeight: 600, fontSize: 13 }}>{o.nombre}</div>
                      <div className="mono" style={{ fontSize: 11.5, color: "var(--text-muted)" }}>
                        {o.path} · {o.methodLabel}
                      </div>
                      {o.detail && (
                        <div style={{ fontSize: 11.5, color: "#fcd34d", marginTop: 2 }}>
                          {o.detail}
                        </div>
                      )}
                    </div>
                    <div style={{ textAlign: "right", flexShrink: 0 }}>
                      <div className="mono" style={{ fontSize: 12.5, fontWeight: 600 }}>
                        {formatBytes(o.freedBytes)}{" "}
                        <span style={{ color: "var(--text-muted)", fontWeight: 400 }}>
                          / est. {formatBytes(o.estimatedBytes)}
                        </span>
                      </div>
                      <span
                        className={`badge ${
                          o.status === "failed" || o.status === "blocked"
                            ? "badge-danger"
                            : o.status === "skipped"
                              ? "badge-warning"
                              : "badge-success"
                        }`}
                      >
                        {o.status}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <>
              <div
                className="card"
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  flexWrap: "wrap",
                  gap: 12,
                }}
              >
                <div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
                    Espacio estimado a recuperar
                  </div>
                  <div className="mono" style={{ fontSize: 22, fontWeight: 700, color: "#38bdf8" }}>
                    {formatBytes(plan.totalEstimatedBytes)}
                  </div>
                </div>

                <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                  <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
                    Artefactos regenerables:
                  </span>
                  <button
                    type="button"
                    className={`btn ${!useRecycleBin ? "btn-primary" : ""}`}
                    onClick={() => onToggleRecycleBin(false)}
                  >
                    Borrado permanente (Recomendado)
                  </button>
                  <button
                    type="button"
                    className={`btn ${useRecycleBin ? "btn-primary" : ""}`}
                    onClick={() => onToggleRecycleBin(true)}
                  >
                    Enviar a Papelera
                  </button>
                </div>
              </div>

              {plan.warnings.map((w) => (
                <div
                  key={w}
                  className="card"
                  style={{
                    borderColor: "rgba(245, 158, 11, 0.5)",
                    background: "rgba(245, 158, 11, 0.08)",
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    fontSize: 12.5,
                  }}
                >
                  <AlertTriangle size={16} color="#f59e0b" />
                  <span>{w}</span>
                </div>
              ))}

              {plan.blockedItems.length > 0 && (
                <div className="card" style={{ borderColor: "rgba(239, 68, 68, 0.4)" }}>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      fontWeight: 600,
                      marginBottom: 6,
                    }}
                  >
                    <ShieldBan size={15} color="#ef4444" />
                    Elementos bloqueados ({plan.blockedItems.length})
                  </div>
                  {plan.blockedItems.map((b) => (
                    <div key={b.id} style={{ fontSize: 12, color: "#fca5a5" }}>
                      <strong>{b.nombre}</strong> (<span className="mono">{b.path}</span>) —{" "}
                      {b.reason}
                    </div>
                  ))}
                </div>
              )}

              <div
                style={{
                  maxHeight: 280,
                  overflow: "auto",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                {plan.items.map((item) => (
                  <div
                    key={item.id}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: 12,
                      padding: "8px 10px",
                      background: "#0f172a",
                      borderRadius: 6,
                      fontSize: 12,
                    }}
                  >
                    <div style={{ overflow: "hidden" }}>
                      <div style={{ fontWeight: 600 }}>
                        {item.nombre}{" "}
                        <span
                          className={`badge ${
                            item.nivel === "seguro" ? "badge-success" : "badge-warning"
                          }`}
                          style={{ marginLeft: 6 }}
                        >
                          {item.nivel}
                        </span>
                      </div>
                      <div
                        className="mono"
                        style={{
                          fontSize: 11.5,
                          color: "var(--text-muted)",
                          wordBreak: "break-all",
                        }}
                      >
                        {item.path}
                      </div>
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 6,
                          marginTop: 3,
                          color: "#93c5fd",
                          fontSize: 11.5,
                        }}
                      >
                        <Terminal size={12} />
                        <span className="mono">
                          {item.effectiveMethod === "comando-oficial" && item.command
                            ? item.command.join(" ")
                            : item.effectiveMethod}
                        </span>
                      </div>
                    </div>
                    <span className="mono" style={{ color: "var(--accent)", flexShrink: 0 }}>
                      {formatBytes(item.estimatedBytes)}
                    </span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <div
          style={{
            padding: "12px 18px",
            borderTop: "1px solid var(--border)",
            display: "flex",
            justifyContent: "flex-end",
            gap: 10,
          }}
        >
          {result ? (
            <button type="button" className="btn btn-primary" onClick={onClose}>
              Cerrar
            </button>
          ) : (
            <>
              <button type="button" className="btn" onClick={onClose}>
                Cancelar
              </button>
              <button
                type="button"
                className="btn btn-danger"
                disabled={!canExecute}
                onClick={onExecute}
              >
                <Trash2 size={14} />
                {executing ? "Ejecutando limpieza..." : `Ejecutar (${plan.items.length} reglas)`}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
