import {
  AppWindow,
  Code2,
  FolderTree,
  HardDrive,
  History,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { CleanupModal } from "./components/CleanupModal";
import { ElevationBanner } from "./components/ElevationBanner";
import {
  type CleanupMode,
  type CleanupPlan,
  type CleanupResult,
  type CleanupTargetInput,
  type DriveInfo,
  type ScanProgress,
  type ScanSummary,
  type StartupContext,
  api,
} from "./lib/ipc";
import { AppsView } from "./pages/AppsView";
import { CleanerView } from "./pages/CleanerView";
import { DevCleanerView } from "./pages/DevCleanerView";
import { DrivesView } from "./pages/DrivesView";
import { ExplorerView } from "./pages/ExplorerView";
import { HistoryView } from "./pages/HistoryView";
import { ProtectedPathsView } from "./pages/ProtectedPathsView";

type NavPage = "drives" | "explorer" | "apps" | "cleaner" | "dev_cleaner" | "protected" | "history";

export function App() {
  const [page, setPage] = useState<NavPage>("drives");
  const [startup, setStartup] = useState<StartupContext | null>(null);
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [loadingDrives, setLoadingDrives] = useState(false);
  const [includeNetwork, setIncludeNetwork] = useState(false);
  const [scanningDrive, setScanningDrive] = useState<string | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [scanSummary, setScanSummary] = useState<ScanSummary | null>(null);

  // Global cleanup modal state
  const [pendingTargets, setPendingTargets] = useState<CleanupTargetInput[]>([]);
  const [activePlan, setActivePlan] = useState<CleanupPlan | null>(null);
  const [executingCleanup, setExecutingCleanup] = useState(false);
  const [cleanupResult, setCleanupResult] = useState<CleanupResult | null>(null);

  const fetchDrives = useCallback((withNet: boolean) => {
    setLoadingDrives(true);
    api
      .listDrives(withNet)
      .then(setDrives)
      .finally(() => setLoadingDrives(false));
  }, []);

  const startScan = useCallback((rootPath: string) => {
    setScanningDrive(rootPath);
    setProgress(null);
    api
      .startScan(rootPath, (p) => setProgress(p))
      .then((summary) => {
        setScanSummary(summary);
        if (!summary.stats.cancelled) {
          setPage("explorer");
        }
      })
      .catch((err) => alert(String(err)))
      .finally(() => {
        setScanningDrive(null);
      });
  }, []);

  useEffect(() => {
    api.getStartupContext().then((ctx) => {
      setStartup(ctx);
      if (ctx.startupDrive) {
        startScan(ctx.startupDrive);
      }
    });
    fetchDrives(false);
  }, [fetchDrives, startScan]);

  const handleToggleNetwork = (include: boolean) => {
    setIncludeNetwork(include);
    fetchDrives(include);
  };

  const handleRelaunchAdmin = () => {
    const drive = scanSummary?.rootPath ?? scanningDrive ?? startup?.startupDrive ?? null;
    api.relaunchAsAdmin(drive).catch((err) => alert(String(err)));
  };

  const openCleanupPreview = (targets: CleanupTargetInput[], mode: CleanupMode) => {
    setPendingTargets(targets);
    setCleanupResult(null);
    api
      .previewCleanup(targets, mode)
      .then(setActivePlan)
      .catch((err) => alert(String(err)));
  };

  const handleChangePlanMode = (mode: CleanupMode) => {
    api
      .previewCleanup(pendingTargets, mode)
      .then(setActivePlan)
      .catch((err) => alert(String(err)));
  };

  const handleExecutePlan = (confirmedPermanent: boolean) => {
    if (!activePlan) return;
    setExecutingCleanup(true);
    api
      .executeCleanup(activePlan.planId, confirmedPermanent)
      .then((res) => {
        setCleanupResult(res);
        fetchDrives(includeNetwork);
      })
      .catch((err) => alert(String(err)))
      .finally(() => setExecutingCleanup(false));
  };

  const navItems: { id: NavPage; label: string; icon: typeof HardDrive }[] = [
    { id: "drives", label: "Unidades", icon: HardDrive },
    { id: "explorer", label: "Explorador", icon: FolderTree },
    { id: "apps", label: "Aplicaciones", icon: AppWindow },
    { id: "cleaner", label: "Limpieza segura", icon: Sparkles },
    { id: "dev_cleaner", label: "Limpieza Dev", icon: Code2 },
    { id: "protected", label: "Rutas protegidas", icon: ShieldCheck },
    { id: "history", label: "Historial y logs", icon: History },
  ];

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-icon">SC</div>
          <div>
            <div className="brand-title">ScanCleaner</div>
            <div className="brand-sub">Windows Disk Analyzer</div>
          </div>
        </div>

        <nav className="nav-list">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                type="button"
                className={`nav-btn ${page === item.id ? "active" : ""}`}
                onClick={() => setPage(item.id)}
              >
                <Icon size={17} />
                {item.label}
              </button>
            );
          })}
        </nav>

        <div style={{ fontSize: 11.5, color: "var(--text-muted)", padding: "6px 8px" }}>
          v1.0.0 · Papelera por defecto · Guardrails activos
        </div>
      </aside>

      <div className="main-area">
        <header className="topbar">
          <div style={{ fontWeight: 600, fontSize: 14, color: "var(--text-secondary)" }}>
            {scanSummary ? `Unidad activa: ${scanSummary.rootPath}` : "Listo para escanear"}
          </div>
          <ElevationBanner
            elevation={startup?.elevation ?? null}
            activeDrive={scanSummary?.rootPath ?? null}
            onRelaunch={handleRelaunchAdmin}
          />
        </header>

        <main className="content-scroll">
          {page === "drives" && (
            <DrivesView
              drives={drives}
              loading={loadingDrives}
              includeNetwork={includeNetwork}
              scanningDrive={scanningDrive}
              progress={progress}
              onToggleNetwork={handleToggleNetwork}
              onRefresh={() => fetchDrives(includeNetwork)}
              onStartScan={startScan}
              onCancelScan={() => api.cancelScan()}
            />
          )}

          {page === "explorer" && (
            <ExplorerView
              scanSummary={scanSummary}
              rulesPath={startup?.rulesPath ?? ""}
              isElevated={startup?.elevation.isElevated ?? false}
              onRelaunchAdmin={handleRelaunchAdmin}
              onQueueCleanup={(path, category) =>
                openCleanupPreview([{ path, category, isTempDirContents: false }], "recycle_bin")
              }
            />
          )}

          {page === "apps" && <AppsView />}

          {page === "cleaner" && (
            <CleanerView
              isElevated={startup?.elevation.isElevated ?? false}
              onRelaunchAdmin={handleRelaunchAdmin}
              onStartPreview={openCleanupPreview}
            />
          )}

          {page === "dev_cleaner" && (
            <DevCleanerView
              isElevated={startup?.elevation.isElevated ?? false}
              onRelaunchAdmin={handleRelaunchAdmin}
            />
          )}

          {page === "protected" && <ProtectedPathsView />}

          {page === "history" && <HistoryView />}
        </main>
      </div>

      {activePlan && (
        <CleanupModal
          plan={activePlan}
          executing={executingCleanup}
          result={cleanupResult}
          onChangeMode={handleChangePlanMode}
          onExecute={handleExecutePlan}
          onClose={() => {
            setActivePlan(null);
            setCleanupResult(null);
          }}
        />
      )}
    </div>
  );
}
