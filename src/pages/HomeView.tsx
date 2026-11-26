import { ShieldCheck } from "lucide-react";
import { DrivesBento } from "../components/home/DrivesBento";
import { LauncherGrid, type LauncherTarget } from "../components/home/LauncherGrid";
import { Badge } from "../components/ui/Badge";
import type { DriveInfo, ScanProgress, ScanSummary } from "../lib/ipc";

interface HomeViewProps {
  username: string | null;
  isElevated: boolean;
  drives: DriveInfo[];
  loadingDrives: boolean;
  includeNetwork: boolean;
  scanningDrive: string | null;
  progress: ScanProgress | null;
  scanSummary: ScanSummary | null;
  onNavigate: (page: LauncherTarget) => void;
  onToggleNetwork: (include: boolean) => void;
  onRefreshDrives: () => void;
  onStartScan: (rootPath: string) => void;
  onCancelScan: () => void;
}

export function HomeView({
  username,
  isElevated,
  drives,
  loadingDrives,
  includeNetwork,
  scanningDrive,
  progress,
  scanSummary,
  onNavigate,
  onToggleNetwork,
  onRefreshDrives,
  onStartScan,
  onCancelScan,
}: HomeViewProps) {
  return (
    <div className="flex flex-col gap-9 py-2">
      {/* Hero Welcome + Proton-inspired Launcher */}
      <div className="flex flex-col items-center text-center gap-6 pt-2">
        <div className="flex flex-col items-center gap-2 animate-fade-up">
          <h1 className="m-0 text-[26px] sm:text-[30px] font-bold tracking-[-0.03em] text-fg-primary leading-tight">
            {username ? `Bienvenido, ${username}` : "Bienvenido a ScanCleaner"}
          </h1>
          <p className="m-0 text-[14px] text-fg-secondary text-pretty">
            Selecciona una herramienta o escanea una unidad para liberar espacio de forma segura.
          </p>
          <div className="flex items-center justify-center gap-2 flex-wrap mt-1">
            <Badge tone="success">
              <ShieldCheck size={11} />
              Guardrails activos · Papelera por defecto
            </Badge>
            {isElevated && <Badge tone="warning">Privilegios de Administrador</Badge>}
          </div>
        </div>

        <div className="w-full max-w-[920px]">
          <LauncherGrid activeScanRoot={scanSummary?.rootPath ?? null} onNavigate={onNavigate} />
        </div>
      </div>

      {/* Drives Bento Grid */}
      <DrivesBento
        drives={drives}
        loading={loadingDrives}
        includeNetwork={includeNetwork}
        scanningDrive={scanningDrive}
        progress={progress}
        onToggleNetwork={onToggleNetwork}
        onRefresh={onRefreshDrives}
        onStartScan={onStartScan}
        onCancelScan={onCancelScan}
      />
    </div>
  );
}
