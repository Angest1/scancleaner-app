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
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Collapse } from "./ui/Collapse";
import { AlertBanner, Skeleton } from "./ui/Feedback";
import { Modal } from "./ui/Modal";

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
      <div className="flex flex-col gap-2.5 py-2">
        <Skeleton className="h-5 w-48" />
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-20 w-full" />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <Lock size={15} className="text-warning" />
          <span className="font-semibold text-[14px] text-fg-primary">Sistema (no eliminable)</span>
          <Badge tone="warning">Protegido</Badge>
        </div>
        <span className="mono text-[12.5px] font-semibold text-accent">
          Total: {formatBytes(report.totalBytes)}
        </span>
      </div>

      {feedback && <AlertBanner tone="info">{feedback}</AlertBanner>}

      {report.files.length === 0 ? (
        <div className="py-4 text-[12.5px] text-fg-muted">
          No se detectaron <code>pagefile.sys</code>, <code>swapfile.sys</code> ni{" "}
          <code>hiberfil.sys</code> en esta unidad.
        </div>
      ) : (
        <div className="flex flex-col gap-2.5">
          {report.files.map((file: SystemFileEntry) => {
            const isExpanded = expandedId === file.id;
            return (
              <div
                key={file.id}
                className="rounded-[8px] bg-canvas border border-border-default px-3.5 py-3 flex flex-col gap-2.5"
              >
                <div className="flex items-start justify-between flex-wrap gap-3">
                  <div className="flex-1 min-w-[240px]">
                    <div className="flex items-center gap-2 flex-wrap">
                      <Lock size={13} className="text-warning shrink-0" />
                      <span className="mono font-semibold text-[13.5px] text-fg-primary">
                        {file.name}
                      </span>
                      <span className="mono text-[11.5px] text-fg-muted">{file.path}</span>
                    </div>
                    <p className="m-0 mt-1.5 text-[12.5px] text-fg-secondary leading-relaxed">
                      {file.explanation}
                    </p>
                  </div>

                  <div className="flex items-center gap-2.5 shrink-0">
                    <span className="mono font-semibold text-[13.5px] text-fg-primary">
                      {formatBytes(file.sizeBytes)}
                    </span>
                    {file.canAdjust ? (
                      <Button
                        variant={isExpanded ? "default" : "primary"}
                        size="sm"
                        onClick={() => setExpandedId(isExpanded ? null : file.id)}
                      >
                        <SlidersHorizontal size={13} />
                        {isExpanded ? "Ocultar" : "Ajustar"}
                      </Button>
                    ) : (
                      <Badge title="Administrado exclusivamente por Windows; no requiere ni admite ajuste manual.">
                        <Lock size={10} />
                        Solo Windows
                      </Badge>
                    )}
                  </div>
                </div>

                {file.adjustKind === "pagefile" && (
                  <Collapse open={isExpanded}>
                    <div className="border-t border-border-subtle pt-2.5 flex items-center gap-3 flex-wrap">
                      <Button variant="primary" size="sm" onClick={handleOpenVirtualMemory}>
                        <ExternalLink size={13} />
                        Abrir memoria virtual
                      </Button>
                      <span className="text-[12px] text-accent">
                        {report.virtualMemoryInstruction}
                      </span>
                    </div>
                  </Collapse>
                )}

                {file.adjustKind === "hiberfil" && (
                  <Collapse open={isExpanded}>
                    <div className="border-t border-border-subtle pt-2.5 flex flex-col gap-2.5">
                      {report.hibernation.available && (
                        <div className="text-[12.5px]">
                          <span className="font-medium text-fg-secondary">
                            Estado de hibernación:{" "}
                          </span>
                          <span className="text-accent font-medium">
                            {report.hibernation.label}
                          </span>
                          {report.hibernation.detail && (
                            <span className="text-fg-muted ml-1.5">
                              ({report.hibernation.detail})
                            </span>
                          )}
                        </div>
                      )}

                      <div className="flex flex-col gap-2">
                        {POWERCFG_COMMANDS.map((item) => (
                          <div
                            key={item.action}
                            className="rounded-[7px] bg-shell border border-border-subtle px-3 py-2 flex items-center justify-between flex-wrap gap-2.5"
                          >
                            <div className="flex-1 min-w-[220px]">
                              <div className="mono text-[12px] text-accent flex items-center gap-1.5">
                                <Terminal size={12} />
                                {item.cmd}
                              </div>
                              <div className="text-[11.5px] text-fg-secondary mt-0.5">
                                {item.desc}
                              </div>
                            </div>
                            <div className="flex gap-1.5">
                              <Button size="sm" onClick={() => handleCopy(item.cmd)}>
                                {copiedCmd === item.cmd ? <Check size={12} /> : <Copy size={12} />}
                                {copiedCmd === item.cmd ? "Copiado" : "Copiar"}
                              </Button>
                              <Button
                                variant="warning"
                                size="sm"
                                onClick={() =>
                                  setConfirmAction({ action: item.action, cmd: item.cmd })
                                }
                              >
                                Ejecutar como admin
                              </Button>
                            </div>
                          </div>
                        ))}
                      </div>

                      <div>
                        <Button
                          size="sm"
                          onClick={() => api.openPowerOptions().catch((err) => alert(String(err)))}
                        >
                          <ExternalLink size={13} />
                          Abrir opciones de energía
                        </Button>
                      </div>
                    </div>
                  </Collapse>
                )}
              </div>
            );
          })}
        </div>
      )}

      {confirmAction && (
        <Modal
          title={
            <span className="inline-flex items-center gap-2">
              <ShieldAlert size={16} className="text-warning" />
              Confirmar ejecución elevada
            </span>
          }
          maxWidthClass="max-w-[480px]"
          onClose={() => setConfirmAction(null)}
          footer={
            <>
              <Button onClick={() => setConfirmAction(null)}>Cancelar</Button>
              <Button variant="warning" onClick={handleRunPowercfg}>
                Confirmar y ejecutar
              </Button>
            </>
          }
        >
          <p className="m-0 text-[13px] text-fg-secondary">
            Se solicitará elevación UAC de Windows para ejecutar:
          </p>
          <div className="mono px-3 py-2 rounded-[7px] bg-canvas border border-border-default text-accent text-[12.5px]">
            {confirmAction.cmd}
          </div>
        </Modal>
      )}
    </div>
  );
}
