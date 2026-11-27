import {
  BellOff,
  CheckCircle2,
  Database,
  RefreshCw,
  Server,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { AlertBanner } from "../components/ui/Feedback";
import { api } from "../lib/ipc";
import type { AppSettings, IndexingServiceStatus } from "../lib/ipcTypes";

interface SettingsViewProps {
  settings: AppSettings;
  serviceStatus: IndexingServiceStatus | null;
  onSettingsChange: (next: AppSettings) => void;
  onServiceStatusChange: (next: IndexingServiceStatus) => void;
  onRescanAll: () => void;
}

export function SettingsView({
  settings,
  serviceStatus,
  onSettingsChange,
  onServiceStatusChange,
  onRescanAll,
}: SettingsViewProps) {
  const [saving, setSaving] = useState(false);
  const [svcBusy, setSvcBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .serviceStatus()
      .then(onServiceStatusChange)
      .catch(() => {});
  }, [onServiceStatusChange]);

  const updateSettings = (patch: Partial<AppSettings>) => {
    const next = { ...settings, ...patch };
    setSaving(true);
    setError(null);
    setNotice(null);
    api
      .saveAppSettings(next)
      .then((saved) => {
        onSettingsChange(saved);
        setNotice("Preferencias guardadas.");
      })
      .catch((e) => setError(String(e)))
      .finally(() => setSaving(false));
  };

  const handleInstallService = () => {
    setSvcBusy(true);
    setError(null);
    setNotice(null);
    api
      .serviceInstall()
      .then((st) => {
        onServiceStatusChange(st);
        setNotice(
          st.running
            ? "Servicio de indexación MFT/USN instalado y activo."
            : "Solicitud UAC enviada; verifica el estado del servicio en unos segundos.",
        );
      })
      .catch((e) => setError(String(e)))
      .finally(() => setSvcBusy(false));
  };

  const handleUninstallService = () => {
    setSvcBusy(true);
    setError(null);
    setNotice(null);
    api
      .serviceUninstall()
      .then((st) => {
        onServiceStatusChange(st);
        setNotice("Servicio de indexación desinstalado correctamente.");
      })
      .catch((e) => setError(String(e)))
      .finally(() => setSvcBusy(false));
  };

  return (
    <div className="flex flex-col gap-5 max-w-[920px] mx-auto">
      <div className="flex items-start justify-between flex-wrap gap-3">
        <div>
          <h1 className="m-0 text-[20px] font-semibold tracking-[-0.025em] text-fg-primary">
            Ajustes
          </h1>
          <p className="m-0 mt-1 text-[13px] text-fg-secondary text-pretty">
            Controla el servicio opcional de indexación MFT/USN, el arranque en bandeja y el
            comportamiento de la pantalla de inicio.
          </p>
        </div>

        <Button onClick={onRescanAll}>
          <RefreshCw size={13} />
          Reescanear todas las unidades
        </Button>
      </div>

      {notice && <AlertBanner tone="success">{notice}</AlertBanner>}
      {error && <AlertBanner tone="danger">{error}</AlertBanner>}

      {/* Indexing Service (Section 4) */}
      <Card className="flex flex-col gap-4">
        <div className="flex items-start justify-between flex-wrap gap-3">
          <div className="flex items-start gap-3">
            <div className="w-10 h-10 rounded-[12px] bg-accent-soft border border-accent-border grid place-items-center text-accent shrink-0">
              <Server size={18} aria-hidden="true" />
            </div>
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <h2 className="m-0 text-[15px] font-semibold text-fg-primary">
                  Servicio de indexación MFT / USN (Opcional)
                </h2>
                {serviceStatus?.running ? (
                  <Badge tone="success">
                    <CheckCircle2 size={11} />
                    Activo
                  </Badge>
                ) : serviceStatus?.installed ? (
                  <Badge tone="warning">Instalado (detenido)</Badge>
                ) : (
                  <Badge tone="neutral">No instalado</Badge>
                )}
              </div>
              <p className="m-0 mt-1 text-[12.5px] text-fg-secondary text-pretty max-w-[62ch]">
                Permite leer la tabla maestra NTFS (<code className="mono">$MFT</code>) y el{" "}
                <code className="mono">USN Change Journal</code> sin pedir UAC cada vez que abres la
                app. Requiere un único aviso UAC al instalar.
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {serviceStatus?.installed ? (
              <Button variant="danger" onClick={handleUninstallService} disabled={svcBusy}>
                <Trash2 size={13} />
                {svcBusy ? "Desinstalando…" : "Desinstalar servicio"}
              </Button>
            ) : (
              <Button variant="primary" onClick={handleInstallService} disabled={svcBusy}>
                <ShieldCheck size={13} />
                {svcBusy ? "Instalando…" : "Instalar servicio (1 UAC)"}
              </Button>
            )}
          </div>
        </div>

        <div className="rounded-[10px] bg-canvas border border-border-subtle px-3.5 py-2.5 text-[12px] text-fg-secondary flex flex-col gap-1">
          <div>
            <strong>Modelo de seguridad:</strong> el servicio es estrictamente de{" "}
            <strong>solo lectura de metadatos</strong> (no posee ninguna operación de borrado ni
            escritura). El Named Pipe (<code className="mono">\\.\pipe\ScanCleanerIndexSvc_v1</code>
            ) restringe su ACL exclusivamente a <code className="mono">SYSTEM</code> y a tu SID de
            usuario.
          </div>
          {serviceStatus?.binaryPath && (
            <div className="mono text-[11px] text-fg-muted truncate">
              Binario aislado: {serviceStatus.binaryPath}
            </div>
          )}
        </div>
      </Card>

      {/* Startup, Tray & Motion preferences */}
      <Card className="flex flex-col gap-4">
        <h2 className="m-0 text-[15px] font-semibold text-fg-primary">
          Arranque, bandeja y rendimiento
        </h2>

        <label className="flex items-start justify-between gap-4 cursor-pointer py-1">
          <div className="flex items-start gap-2.5">
            <BellOff size={16} className="text-fg-secondary mt-0.5 shrink-0" aria-hidden="true" />
            <div>
              <div className="text-[13.5px] font-medium text-fg-primary">
                Iniciar con Windows en la bandeja del sistema
              </div>
              <div className="text-[12px] text-fg-secondary">
                Mantiene el índice de tus discos actualizado en segundo plano con prioridad de I/O
                reducida. Desactivado por defecto.
              </div>
            </div>
          </div>
          <input
            type="checkbox"
            className="mt-1 accent-accent w-4 h-4 shrink-0"
            checked={settings.startWithWindowsTray}
            disabled={saving}
            onChange={(e) => updateSettings({ startWithWindowsTray: e.target.checked })}
          />
        </label>

        <div className="border-t border-border-subtle" />

        <label className="flex items-start justify-between gap-4 cursor-pointer py-1">
          <div className="flex items-start gap-2.5">
            <Sparkles size={16} className="text-fg-secondary mt-0.5 shrink-0" aria-hidden="true" />
            <div>
              <div className="text-[13.5px] font-medium text-fg-primary">
                Omitir animación de inicio
              </div>
              <div className="text-[12px] text-fg-secondary">
                Entra directamente a la pantalla principal sin reproducir la secuencia de cierre
                animada.
              </div>
            </div>
          </div>
          <input
            type="checkbox"
            className="mt-1 accent-accent w-4 h-4 shrink-0"
            checked={settings.skipStartupAnimation}
            disabled={saving}
            onChange={(e) => updateSettings({ skipStartupAnimation: e.target.checked })}
          />
        </label>

        <div className="border-t border-border-subtle" />

        <label className="flex items-start justify-between gap-4 cursor-pointer py-1">
          <div className="flex items-start gap-2.5">
            <Database size={16} className="text-fg-secondary mt-0.5 shrink-0" aria-hidden="true" />
            <div>
              <div className="text-[13.5px] font-medium text-fg-primary">
                Incluir Alternate Data Streams (ADS) en el total NTFS
              </div>
              <div className="text-[12px] text-fg-secondary">
                Excluidos por defecto para coincidir con el Explorador de Windows. Actívalo si
                deseas contabilizar streams nombrados adicionales en lecturas MFT.
              </div>
            </div>
          </div>
          <input
            type="checkbox"
            className="mt-1 accent-accent w-4 h-4 shrink-0"
            checked={settings.includeAds}
            disabled={saving}
            onChange={(e) => updateSettings({ includeAds: e.target.checked })}
          />
        </label>

        <div className="border-t border-border-subtle" />

        <div className="flex items-center justify-between flex-wrap gap-3 py-1">
          <div>
            <div className="text-[13.5px] font-medium text-fg-primary">
              Timeout de vigilancia de desinstaladores nativos
            </div>
            <div className="text-[12px] text-fg-secondary">
              Tiempo máximo vigilando el Registro / PackageManager antes de reconciliar restos.
            </div>
          </div>
          <label className="inline-flex items-center gap-2 text-[12.5px] text-fg-secondary">
            <input
              type="number"
              name="uninstall-timeout-mins"
              min={1}
              max={60}
              className="w-[72px] h-8 px-2 rounded-[8px] bg-canvas border border-border-default mono text-[12.5px] text-fg-primary"
              value={settings.uninstallWatchTimeoutMins}
              onChange={(e) =>
                updateSettings({
                  uninstallWatchTimeoutMins: Math.max(
                    1,
                    Math.min(60, Number(e.target.value) || 10),
                  ),
                })
              }
            />
            minutos
          </label>
        </div>
      </Card>

      <div className="text-[11.5px] text-fg-muted text-center">
        ScanCleaner · 100% local · Cero telemetría · Logs locales en{" "}
        <code className="mono">%APPDATA%\ScanCleaner\logs</code>
      </div>
    </div>
  );
}
