import {
  Check,
  Copy,
  ExternalLink,
  Lock,
  ShieldAlert,
  SlidersHorizontal,
  Terminal,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { formatBytes } from "../lib/format";
import { type PowercfgAction, type SystemFileEntry, type SystemFilesReport, api } from "../lib/ipc";

interface SystemFilesPanelProps {
  driveRoot: string | null;
}

const POWERCFG_COMMANDS: {
  action: PowercfgAction;
  cmd: string;
  desc: string;
}[] = [
  {
    action: "reduced",
    cmd: "powercfg /h /type reduced",
    desc: "Reduce el archivo y mantiene el inicio rápido, pero deshabilita la hibernación completa.",
  },
  {
    action: "off",
    cmd: "powercfg /h off",
    desc: "Elimina hiberfil.sys y desactiva tanto la hibernación como el inicio rápido.",
  },
  {
    action: "on",
    cmd: "powercfg /h on",
    desc: "Reactiva la hibernación y el inicio rápido.",
  },
];

export function SystemFilesPanel({ driveRoot }: SystemFilesPanelProps) {
  const [report, setReport] = useState<SystemFilesReport | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [copiedCmd, setCopiedCmd] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<{
    action: PowercfgAction;
    cmd: string;
  } | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);

  const load = useCallback(() => {
    api
      .systemFilesReport(driveRoot)
      .then(setReport)
      .catch(() => {});
  }, [driveRoot]);

  useEffect(() => {
    load();
  }, [load]);

  const handleCopy = (cmd: string) => {
    navigator.clipboard?.writeText(cmd).then(() => {
      setCopiedCmd(cmd);
      setTimeout(() => setCopiedCmd(null), 1800);
    });
  };

  const handleOpenVirtualMemory = () => {
    api
      .openVirtualMemory()
      .then(() =>
        setFeedback(
          "Opciones de rendimiento abiertas. Sigue: Pestaña Opciones avanzadas → Memoria virtual → Cambiar.",
        ),
      )
      .catch((err) => alert(String(err)));
  };

  const handleOpenPowerOptions = () => {
    api.openPowerOptions().catch((err) => alert(String(err)));
  };

  const handleRunPowercfg = () => {
    if (!confirmAction) return;
    api
      .runPowercfgElevated(confirmAction.action)
      .then((ran) => {
        setFeedback(`Comando lanzado como administrador: ${ran}`);
        setConfirmAction(null);
        setTimeout(load, 1500);
      })
      .catch((err) => alert(String(err)));
  };

  if (!report) {
    return (
      <div style={{ padding: 20, color: "var(--text-muted)" }}>
        Consultando archivos de sistema...
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          flexWrap: "wrap",
          gap: 8,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <Lock size={16} color="#f59e0b" />
          <span style={{ fontWeight: 700, fontSize: 15 }}>Sistema (no eliminable)</span>
          <span className="badge badge-warning">Protegido</span>
        </div>
        <span className="mono" style={{ fontSize: 13, color: "var(--accent)", fontWeight: 600 }}>
          Total: {formatBytes(report.totalBytes)}
        </span>
      </div>

      {feedback && (
        <div
          className="card"
          style={{
            padding: "8px 12px",
            borderColor: "rgba(56, 189, 248, 0.45)",
            background: "rgba(56, 189, 248, 0.08)",
            fontSize: 12.5,
          }}
        >
          {feedback}
        </div>
      )}

      {report.files.length === 0 ? (
        <div style={{ padding: 16, color: "var(--text-muted)", fontSize: 13 }}>
          No se detectaron <code>pagefile.sys</code>, <code>swapfile.sys</code> ni{" "}
          <code>hiberfil.sys</code> en esta unidad.
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          {report.files.map((file: SystemFileEntry) => {
            const isExpanded = expandedId === file.id;
            return (
              <div
                key={file.id}
                style={{
                  background: "#0f172a",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  padding: "12px 14px",
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "flex-start",
                    gap: 12,
                    flexWrap: "wrap",
                  }}
                >
                  <div style={{ flex: 1, minWidth: 240 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <Lock size={14} color="#f59e0b" />
                      <span className="mono" style={{ fontWeight: 700, fontSize: 14 }}>
                        {file.name}
                      </span>
                      <span className="mono" style={{ fontSize: 11.5, color: "var(--text-muted)" }}>
                        {file.path}
                      </span>
                    </div>
                    <p
                      style={{
                        margin: "6px 0 0",
                        fontSize: 12.5,
                        color: "var(--text-secondary)",
                        lineHeight: 1.45,
                      }}
                    >
                      {file.explanation}
                    </p>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                    <span className="mono" style={{ fontWeight: 700, fontSize: 14 }}>
                      {formatBytes(file.sizeBytes)}
                    </span>
                    {file.canAdjust ? (
                      <button
                        type="button"
                        className="btn btn-primary"
                        onClick={() => setExpandedId(isExpanded ? null : file.id)}
                      >
                        <SlidersHorizontal size={13} />
                        {isExpanded ? "Ocultar" : "Ajustar"}
                      </button>
                    ) : (
                      <span
                        className="badge"
                        title="Administrado exclusivamente por Windows; no requiere ni admite ajuste manual."
                      >
                        <Lock size={11} style={{ marginRight: 4 }} />
                        Solo Windows
                      </span>
                    )}
                  </div>
                </div>

                {isExpanded && file.adjustKind === "pagefile" && (
                  <div
                    style={{
                      borderTop: "1px solid var(--border)",
                      paddingTop: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 8,
                    }}
                  >
                    <div
                      style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <button
                        type="button"
                        className="btn btn-primary"
                        onClick={handleOpenVirtualMemory}
                      >
                        <ExternalLink size={13} />
                        Abrir memoria virtual
                      </button>
                      <span style={{ fontSize: 12.5, color: "#93c5fd" }}>
                        {report.virtualMemoryInstruction}
                      </span>
                    </div>
                  </div>
                )}

                {isExpanded && file.adjustKind === "hiberfil" && (
                  <div
                    style={{
                      borderTop: "1px solid var(--border)",
                      paddingTop: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 10,
                    }}
                  >
                    {report.hibernation.available && (
                      <div style={{ fontSize: 12.5 }}>
                        <strong>Estado actual de hibernación:</strong>{" "}
                        <span style={{ color: "#38bdf8" }}>{report.hibernation.label}</span>
                        {report.hibernation.detail && (
                          <span style={{ color: "var(--text-muted)", marginLeft: 6 }}>
                            ({report.hibernation.detail})
                          </span>
                        )}
                      </div>
                    )}

                    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                      {POWERCFG_COMMANDS.map((item) => (
                        <div
                          key={item.action}
                          style={{
                            background: "#020617",
                            border: "1px solid var(--border)",
                            borderRadius: 6,
                            padding: "8px 10px",
                            display: "flex",
                            justifyContent: "space-between",
                            alignItems: "center",
                            gap: 10,
                            flexWrap: "wrap",
                          }}
                        >
                          <div style={{ flex: 1, minWidth: 220 }}>
                            <div
                              className="mono"
                              style={{
                                fontSize: 12.5,
                                color: "#38bdf8",
                                display: "flex",
                                alignItems: "center",
                                gap: 6,
                              }}
                            >
                              <Terminal size={12} />
                              {item.cmd}
                            </div>
                            <div
                              style={{
                                fontSize: 11.5,
                                color: "var(--text-secondary)",
                                marginTop: 2,
                              }}
                            >
                              {item.desc}
                            </div>
                          </div>
                          <div style={{ display: "flex", gap: 6 }}>
                            <button
                              type="button"
                              className="btn"
                              style={{ padding: "4px 8px" }}
                              onClick={() => handleCopy(item.cmd)}
                            >
                              {copiedCmd === item.cmd ? <Check size={12} /> : <Copy size={12} />}
                              {copiedCmd === item.cmd ? "Copiado" : "Copiar"}
                            </button>
                            <button
                              type="button"
                              className="btn btn-warning"
                              style={{ padding: "4px 8px" }}
                              onClick={() =>
                                setConfirmAction({ action: item.action, cmd: item.cmd })
                              }
                            >
                              Ejecutar como administrador
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>

                    <div>
                      <button type="button" className="btn" onClick={handleOpenPowerOptions}>
                        <ExternalLink size={13} />
                        Abrir opciones de energía
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {confirmAction && (
        <div className="modal-backdrop">
          <div className="modal-panel" style={{ maxWidth: 480 }}>
            <div
              style={{
                padding: "14px 18px",
                borderBottom: "1px solid var(--border)",
                fontWeight: 700,
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <ShieldAlert size={18} color="#f59e0b" />
              Confirmar ejecución elevada
            </div>
            <div style={{ padding: 18, fontSize: 13, lineHeight: 1.5 }}>
              Se solicitará elevación UAC de Windows para ejecutar:
              <div
                className="mono"
                style={{
                  marginTop: 10,
                  padding: "8px 10px",
                  background: "#0f172a",
                  borderRadius: 6,
                  color: "#38bdf8",
                }}
              >
                {confirmAction.cmd}
              </div>
            </div>
            <div
              style={{
                padding: "12px 18px",
                borderTop: "1px solid var(--border)",
                display: "flex",
                justifyContent: "flex-end",
                gap: 8,
              }}
            >
              <button type="button" className="btn" onClick={() => setConfirmAction(null)}>
                Cancelar
              </button>
              <button type="button" className="btn btn-warning" onClick={handleRunPowercfg}>
                Confirmar y ejecutar
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
