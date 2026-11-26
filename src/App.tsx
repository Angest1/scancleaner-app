import {
  AppWindow,
  Code2,
  FolderTree,
  History,
  Home,
  Menu,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { CleanupModal } from "./components/CleanupModal";
import { ElevationBanner } from "./components/ElevationBanner";
import { AmbientMesh } from "./components/ui/AmbientMesh";
import { BrandMark } from "./components/ui/BrandMark";
import { Button } from "./components/ui/Button";
import { ThemeToggle } from "./components/ui/ThemeToggle";
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
import { useTheme } from "./lib/theme";
import { AppsView } from "./pages/AppsView";
import { CleanerView } from "./pages/CleanerView";
import { DevCleanerView } from "./pages/DevCleanerView";
import { ExplorerView } from "./pages/ExplorerView";
import { HistoryView } from "./pages/HistoryView";
import { HomeView } from "./pages/HomeView";
import { ProtectedPathsView } from "./pages/ProtectedPathsView";

type NavPage = "home" | "explorer" | "cleaner" | "dev_cleaner" | "apps" | "protected" | "history";

const NAV_ITEMS: { id: NavPage; label: string; icon: typeof Home }[] = [
  { id: "home", label: "Inicio", icon: Home },
  { id: "explorer", label: "Explorador", icon: FolderTree },
  { id: "cleaner", label: "Limpieza", icon: Sparkles },
  { id: "dev_cleaner", label: "Dev", icon: Code2 },
  { id: "apps", label: "Apps", icon: AppWindow },
  { id: "protected", label: "Protegidas", icon: ShieldCheck },
  { id: "history", label: "Historial", icon: History },
];

export function App() {
  const { resolved: theme, toggleTheme } = useTheme();
  const [page, setPage] = useState<NavPage>("home");
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
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

  const handleSelectPage = (next: NavPage) => {
    setPage(next);
    setMobileMenuOpen(false);
  };

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-canvas text-fg-primary flex flex-col">
      <AmbientMesh />

      {/* Floating top navigation bar (100% width reclaimed — no sidebar) */}
      <header className="relative z-30 shrink-0 h-14 px-4 sm:px-6 bg-shell backdrop-blur-md border-b border-border-subtle">
        <div className="h-full max-w-[1440px] mx-auto flex items-center justify-between gap-3">
          {/* Brand button (always returns to Home) */}
          <div className="flex items-center gap-3 min-w-0">
            <button
              type="button"
              onClick={() => handleSelectPage("home")}
              className="flex items-center gap-2.5 bg-transparent border-0 p-1 -ml-1 rounded-[10px] cursor-pointer hover:opacity-90 transition-opacity"
              title="Ir a Inicio"
            >
              <BrandMark size={30} />
              <div className="text-left leading-tight">
                <div className="text-[14.5px] font-bold tracking-[-0.02em] text-fg-primary">
                  ScanCleaner
                </div>
                <div className="text-[10.5px] text-fg-muted hidden sm:block">
                  {scanningDrive
                    ? `Escaneando ${scanningDrive}`
                    : scanSummary
                      ? `Unidad: ${scanSummary.rootPath}`
                      : "Windows Disk Suite"}
                </div>
              </div>
            </button>
          </div>

          {/* Center pill navigation (desktop) */}
          <nav
            aria-label="Navegación principal"
            className="hidden lg:flex items-center gap-0.5 p-1 rounded-full bg-elevated/90 border border-border-default shadow-[0_2px_10px_-4px_rgba(32,28,76,0.12)]"
          >
            {NAV_ITEMS.map((item) => {
              const Icon = item.icon;
              const active = page === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  aria-current={active ? "page" : undefined}
                  onClick={() => handleSelectPage(item.id)}
                  className={`inline-flex items-center gap-1.5 h-7.5 px-3 rounded-full text-[12px] font-medium cursor-pointer border transition-[background-color,color,border-color,box-shadow] duration-140 ${
                    active
                      ? "bg-tile text-accent border-accent-border shadow-[0_2px_8px_-2px_rgba(59,110,245,0.25)]"
                      : "bg-transparent text-fg-secondary border-transparent hover:text-fg-primary hover:bg-panel/60"
                  }`}
                >
                  <Icon size={13} className="shrink-0" />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </nav>

          {/* Right controls: UAC + Theme toggle + compact nav menu */}
          <div className="flex items-center gap-2 shrink-0">
            <ElevationBanner
              elevation={startup?.elevation ?? null}
              activeDrive={scanSummary?.rootPath ?? null}
              onRelaunch={handleRelaunchAdmin}
            />
            <ThemeToggle resolved={theme} onToggle={toggleTheme} />
            <Button
              variant="default"
              size="icon"
              className="lg:hidden"
              onClick={() => setMobileMenuOpen((v) => !v)}
              aria-label={mobileMenuOpen ? "Cerrar menú" : "Abrir menú"}
            >
              {mobileMenuOpen ? <X size={15} /> : <Menu size={15} />}
            </Button>
          </div>
        </div>

        {/* Compact dropdown nav for narrow windows (<1024px) */}
        {mobileMenuOpen && (
          <div className="lg:hidden absolute left-4 right-4 top-[60px] z-50 rounded-[14px] bg-tile border border-border-default p-2 grid grid-cols-2 sm:grid-cols-4 gap-1.5 animate-popover-in shadow-[var(--shadow-floating)]">
            {NAV_ITEMS.map((item) => {
              const Icon = item.icon;
              const active = page === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  aria-current={active ? "page" : undefined}
                  onClick={() => handleSelectPage(item.id)}
                  className={`flex items-center gap-2 h-9 px-3 rounded-[9px] text-[12.5px] font-medium border cursor-pointer ${
                    active
                      ? "bg-accent-soft text-accent border-accent-border"
                      : "bg-transparent text-fg-secondary border-transparent hover:bg-elevated hover:text-fg-primary"
                  }`}
                >
                  <Icon size={14} />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </div>
        )}
      </header>

      {/* Full-width scrollable main workspace */}
      <main className="relative z-10 flex-1 overflow-auto px-4 py-5 sm:px-6 lg:px-8">
        <div key={page} className="w-full max-w-[1400px] mx-auto animate-view-in">
          {page === "home" && (
            <HomeView
              username={startup?.elevation.username ?? null}
              isElevated={startup?.elevation.isElevated ?? false}
              drives={drives}
              loadingDrives={loadingDrives}
              includeNetwork={includeNetwork}
              scanningDrive={scanningDrive}
              progress={progress}
              scanSummary={scanSummary}
              onNavigate={(target) => handleSelectPage(target)}
              onToggleNetwork={handleToggleNetwork}
              onRefreshDrives={() => fetchDrives(includeNetwork)}
              onStartScan={startScan}
              onCancelScan={() => api.cancelScan()}
            />
          )}

          {page === "explorer" && (
            <ExplorerView
              scanSummary={scanSummary}
              rulesPath={startup?.rulesPath ?? ""}
              isElevated={startup?.elevation.isElevated ?? false}
              onGoHome={() => handleSelectPage("home")}
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
        </div>
      </main>

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
