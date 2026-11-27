import { ShieldCheck } from "lucide-react";
import { DrivesBento } from "../components/home/DrivesBento";
import { LauncherGrid, type LauncherTarget } from "../components/home/LauncherGrid";
import { Badge } from "../components/ui/Badge";
import type { DriveInfo, ScanProgress, ScanSummary } from "../lib/ipc";

interface HomeViewProps {
  username: string | null;
  isElevated: boolean;
  drives: DriveInfo[];
  summaries: ScanSummary[];
  loadingDrives: boolean;
  syncing: boolean;
  includeNetwork: boolean;
  scanningDrive: string | null;
  progress: ScanProgress | null;
  scanSummary: ScanSummary | null;
  onNavigate: (page: LauncherTarget) => void;
  onExploreDrive: (rootPath: string) => void;
  onToggleNetwork: (include: boolean) => void;
  onRefreshDrives: () => void;
  onStartScan: (rootPath: string) => void;
  onCancelScan: () => void;
}

export function HomeView({
  username,
  isElevated,
  drives,
  summaries,
  loadingDrives,
  syncing,
  includeNetwork,
  scanningDrive,
  progress,
  scanSummary,
  onNavigate,
  onExploreDrive,
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
            Todas tus unidades locales están listas para explorar y limpiar al instante.
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
          <LauncherGrid
            activeScanRoot={scanSummary?.rootPath ?? null}
            indexedCount={summaries.length}
            onNavigate={onNavigate}
          />
        </div>
      </div>

      {/* Drives Bento Grid */}
      <DrivesBento
        drives={drives}
        summaries={summaries}
        loading={loadingDrives}
        syncing={syncing}
        includeNetwork={includeNetwork}
        scanningDrive={scanningDrive}
        progress={progress}
        onExploreDrive={onExploreDrive}
        onToggleNetwork={onToggleNetwork}
        onRefresh={onRefreshDrives}
        onStartScan={onStartScan}
        onCancelScan={onCancelScan}
      />
    </div>
  );
}
