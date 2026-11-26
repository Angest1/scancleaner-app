import { AlertTriangle, CheckCircle2, ShieldBan, Trash2, X } from "lucide-react";
import { useState } from "react";
import { formatBytes } from "../lib/format";
import type { CleanupMode, CleanupPlan, CleanupResult } from "../lib/ipc";

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

  return (
    <div className="modal-backdrop">
      <div className="modal-panel">
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
            {result ? "Resultado de limpieza" : "Vista previa de limpieza (Dry-run)"}
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
                  gap: 12,
                }}
              >
                <CheckCircle2 size={24} color="#22c55e" />
                <div>
                  <div style={{ fontWeight: 700 }}>
                    Espacio liberado: {formatBytes(result.freedBytes)}
                  </div>
                  <div style={{ fontSize: 12.5, color: "var(--text-secondary)" }}>
                    Completados: {result.succeededCount} · Fallidos: {result.failedCount} ·
                    Bloqueados: {result.skippedCount}
                  </div>
                </div>
              </div>

              <div
                style={{
                  maxHeight: 280,
                  overflow: "auto",
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                {result.outcomes.map((o) => (
                  <div
                    key={o.path}
                    className="card"
                    style={{
                      padding: "8px 12px",
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 10,
                    }}
                  >
                    <div style={{ overflow: "hidden" }}>
                      <div className="mono" style={{ fontSize: 12, wordBreak: "break-all" }}>
                        {o.path}
                      </div>
                      {o.detail && (
                        <div style={{ fontSize: 11.5, color: "#fca5a5", marginTop: 2 }}>
                          {o.detail}
                        </div>
                      )}
                    </div>
                    <span
                      className={`badge ${
                        o.status === "failed" || o.status === "blocked"
                          ? "badge-danger"
                          : "badge-success"
                      }`}
                    >
                      {o.status}
                    </span>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <>
              {plan.mode !== "empty_recycle_bin" && (
                <div style={{ display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" }}>
                  <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>Método:</span>
                  <button
                    type="button"
                    className={`btn ${plan.mode === "recycle_bin" ? "btn-primary" : ""}`}
                    onClick={() => {
                      setConfirmPermanent(false);
                      onChangeMode("recycle_bin");
                    }}
                  >
                    Enviar a la Papelera (Seguro, por defecto)
                  </button>
                  <button
                    type="button"
                    className={`btn ${plan.mode === "permanent" ? "btn-danger" : ""}`}
                    onClick={() => onChangeMode("permanent")}
                  >
                    Borrado permanente
                  </button>
                </div>
              )}

              <div
                className="card"
                style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
              >
                <div>
                  <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Resumen a liberar</div>
                  <div className="mono" style={{ fontSize: 22, fontWeight: 700, color: "#38bdf8" }}>
                    {formatBytes(plan.totalBytes)}
                  </div>
                </div>
                <div style={{ textAlign: "right", fontSize: 13 }}>
                  <div>{plan.items.length} elementos listos</div>
                  {plan.blockedItems.length > 0 && (
                    <div style={{ color: "#fca5a5" }}>
                      {plan.blockedItems.length} bloqueados por seguridad
                    </div>
                  )}
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
                    Rutas bloqueadas por Guardrails
                  </div>
                  {plan.blockedItems.map((b) => (
                    <div key={b.path} style={{ fontSize: 12, color: "#fca5a5" }}>
                      <span className="mono">{b.path}</span> — {b.reason}
                    </div>
                  ))}
                </div>
              )}

              <div
                style={{
                  maxHeight: 240,
                  overflow: "auto",
                  display: "flex",
                  flexDirection: "column",
                  gap: 5,
                }}
              >
                {plan.items.map((item) => (
                  <div
                    key={item.path}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 10,
                      padding: "6px 10px",
                      background: "#0f172a",
                      borderRadius: 6,
                      fontSize: 12,
                    }}
                  >
                    <span className="mono" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                      {item.path}
                    </span>
                    <span className="mono" style={{ color: "var(--accent)", flexShrink: 0 }}>
                      {formatBytes(item.bytes)}
                    </span>
                  </div>
                ))}
              </div>

              {plan.mode === "permanent" && (
                <label
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    fontSize: 13,
                    color: "#fca5a5",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={confirmPermanent}
                    onChange={(e) => setConfirmPermanent(e.target.checked)}
                  />
                  Confirmo que deseo eliminar estos elementos permanentemente sin pasar por la
                  Papelera.
                </label>
              )}
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
                className={`btn ${plan.mode === "recycle_bin" ? "btn-primary" : "btn-danger"}`}
                disabled={!canExecute}
                onClick={() => onExecute(confirmPermanent)}
              >
                <Trash2 size={14} />
                {executing ? "Eliminando..." : "Confirmar y ejecutar limpieza"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
