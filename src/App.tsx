import { Trash2, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { AppHeader, type NavPage } from "./components/AppHeader";
import { CleanupModal } from "./components/CleanupModal";
import { StartupSplash } from "./components/startup/StartupSplash";
import { AmbientMesh } from "./components/ui/AmbientMesh";
import { Button } from "./components/ui/Button";
import { AlertBanner } from "./components/ui/Feedback";
import { useAppLiveEvents } from "./hooks/useAppLiveEvents";
import { useStartupBootstrap } from "./hooks/useStartupBootstrap";
import {
  type CleanupMode,
  type CleanupPlan,
  type CleanupResult,
  type CleanupTargetInput,
  type DeletionJobStatus,
  type ScanProgress,
  type SyncStatusDto,
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
import { SettingsView } from "./pages/SettingsView";

export function App() {
  const { resolved: theme, toggleTheme } = useTheme();
  const boot = useStartupBootstrap();

  const [page, setPage] = useState<NavPage>("home");
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const [loadingDrives, setLoadingDrives] = useState(false);
  const [includeNetwork, setIncludeNetwork] = useState(false);
  const [scanningDrive, setScanningDrive] = useState<string | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);

  // Live auto-sync & background deletion state
  const [syncStatus, setSyncStatus] = useState<SyncStatusDto>({
    syncing: false,
    pendingBatches: 0,
    lastSyncedTs: null,
    lastOrigin: null,
    observations: [],
    revision: 0,
  });
  const [syncRevision, setSyncRevision] = useState(0);
  const [bgJobs, setBgJobs] = useState<DeletionJobStatus[]>([]);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const [dismissedPartialBanner, setDismissedPartialBanner] = useState(false);

  // Global cleanup modal state
  const [pendingTargets, setPendingTargets] = useState<CleanupTargetInput[]>([]);
  const [activePlan, setActivePlan] = useState<CleanupPlan | null>(null);
  const [executingCleanup, setExecutingCleanup] = useState(false);
  const [cleanupResult, setCleanupResult] = useState<CleanupResult | null>(null);

  const showToast = useCallback((msg: string) => {
    setToastMsg(msg);
  }, []);

  useEffect(() => {
    if (!toastMsg) return;
    const t = window.setTimeout(() => setToastMsg(null), 5000);
    return () => window.clearTimeout(t);
  }, [toastMsg]);

  const fetchDrives = useCallback(
    (withNet: boolean) => {
      setLoadingDrives(true);
      api
        .listDrives(withNet)
        .then(boot.setDrives)
        .catch(() => {})
        .finally(() => setLoadingDrives(false));
    },
    [boot.setDrives],
  );

  useAppLiveEvents({
    includeNetwork,
    fetchDrives,
    setSyncRevision,
    setSyncStatus,
    setBgJobs,
    showToast,
  });

  const startScan = useCallback(
    (rootPath: string) => {
      setScanningDrive(rootPath);
      setProgress(null);
      api
        .startScan(rootPath, (p) => setProgress(p))
        .then((summary) => {
          boot.setActiveSummary(summary);
          setSyncRevision((r) => r + 1);
          if (!summary.stats.cancelled) {
            setPage("explorer");
          }
        })
        .catch((err) => showToast(String(err)))
        .finally(() => setScanningDrive(null));
    },
    [boot.setActiveSummary, showToast],
  );

  const handleToggleNetwork = (include: boolean) => {
    setIncludeNetwork(include);
    fetchDrives(include);
  };

  const handleRelaunchAdmin = () => {
    const drive =
      boot.activeSummary?.rootPath ?? scanningDrive ?? boot.startup?.startupDrive ?? null;
    api.relaunchAsAdmin(drive).catch((err) => showToast(String(err)));
  };

  const openCleanupPreview = (targets: CleanupTargetInput[], mode: CleanupMode) => {
    setPendingTargets(targets);
    setCleanupResult(null);
    api
      .previewCleanup(targets, mode)
      .then(setActivePlan)
      .catch((err) => showToast(String(err)));
  };

  const handleChangePlanMode = (mode: CleanupMode) => {
    api
      .previewCleanup(pendingTargets, mode)
      .then(setActivePlan)
      .catch((err) => showToast(String(err)));
  };

  const handleExecutePlan = (confirmedPermanent: boolean) => {
    if (!activePlan) return;
    setExecutingCleanup(true);
    api
      .executeCleanup(activePlan.planId, confirmedPermanent)
      .then((res) => {
        setCleanupResult(res);
        setSyncRevision((r) => r + 1);
        fetchDrives(includeNetwork);
      })
      .catch((err) => showToast(String(err)))
      .finally(() => setExecutingCleanup(false));
  };

  const handleManualSync = () => {
    api
      .triggerFocusSync()
      .then((res) => {
        setSyncRevision((r) => r + 1);
        fetchDrives(includeNetwork);
        if (res.systemFileNotice) showToast(res.systemFileNotice);
      })
      .catch(() => {});
  };

  const handleSelectPage = (next: NavPage) => {
    setPage(next);
    setMobileMenuOpen(false);
  };

  const activeBgJobs = bgJobs.filter(
    (j) => j.phase === "queued" || j.phase === "staging" || j.phase === "deleting_background",
  );

  return (
    <div
      className="relative h-screen w-screen overflow-hidden bg-canvas text-fg-primary flex flex-col"
      data-reduced-motion={boot.reducedMotion ? "true" : "false"}
    >
      {/* Startup Loading Screen (cold start or warm > 300ms) */}
      {boot.splashVisible && (
        <StartupSplash
          state={boot.splashState}
          globalRatio={boot.globalRatio}
          phaseText={boot.phaseText}
          totalFiles={boot.totalFiles}
          totalAllocatedBytes={boot.totalAllocatedBytes}
          drives={boot.driveProgress}
          reducedMotion={boot.reducedMotion}
          visibleSinceMs={boot.splashVisibleSinceMs}
          onMountHomeUnderneath={boot.mountHomeUnderneath}
          onComplete={boot.completeSplash}
          onRetry={boot.retryBootstrap}
          onEnterLastIndex={boot.enterLastIndex}
        />
      )}

      {/* Main App Shell — mounts underneath during tick step 2 for zero-jank transition */}
      {boot.homeMounted && (
        <div
          className={`relative flex-1 flex flex-col h-full overflow-hidden ${
            boot.homeEmerging ? "sc-home-emerging" : ""
          }`}
        >
          <AmbientMesh />

          <AppHeader
            page={page}
            mobileMenuOpen={mobileMenuOpen}
            scanningDrive={scanningDrive}
            activeDriveRoot={boot.activeSummary?.rootPath ?? null}
            syncStatus={syncStatus}
            elevation={boot.startup?.elevation ?? null}
            theme={theme}
            onSelectPage={handleSelectPage}
            onToggleMobileMenu={() => setMobileMenuOpen((v) => !v)}
            onManualSync={handleManualSync}
            onRelaunchAdmin={handleRelaunchAdmin}
            onToggleTheme={toggleTheme}
          />

          <main className="relative z-10 flex-1 overflow-auto px-4 py-5 sm:px-6 lg:px-8">
            <div className="w-full max-w-[1400px] mx-auto flex flex-col gap-4">
              {/* Partial scan observation banner */}
              {!dismissedPartialBanner && boot.partialObservations.length > 0 && (
                <AlertBanner
                  tone="warning"
                  action={
                    <div className="flex items-center gap-1.5">
                      <Button size="sm" onClick={() => handleSelectPage("settings")}>
                        Configurar servicio MFT
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setDismissedPartialBanner(true)}
                      >
                        Ocultar
                      </Button>
                    </div>
                  }
                >
                  {boot.partialObservations[0]}
                </AlertBanner>
              )}

              {/* Active background deletion strip (Section 7) */}
              {activeBgJobs.map((job) => (
                <AlertBanner
                  key={job.jobId}
                  tone="info"
                  action={
                    <Button
                      size="sm"
                      variant="danger"
                      onClick={() => api.cancelDeletionJob(job.jobId).catch(() => {})}
                    >
                      Cancelar
                    </Button>
                  }
                >
                  <span className="inline-flex items-center gap-1.5">
                    <Trash2 size={13} aria-hidden="true" />
                    Vaciando en segundo plano ({job.completedItems}/{job.totalItems} lotes)…
                  </span>
                </AlertBanner>
              ))}

              <div key={page} className="w-full animate-view-in">
                {page === "home" && (
                  <HomeView
                    username={boot.startup?.elevation.username ?? null}
                    isElevated={boot.startup?.elevation.isElevated ?? false}
                    drives={boot.drives}
                    loadingDrives={loadingDrives}
                    includeNetwork={includeNetwork}
                    scanningDrive={scanningDrive}
                    progress={progress}
                    scanSummary={boot.activeSummary}
                    onNavigate={(target) => handleSelectPage(target)}
                    onToggleNetwork={handleToggleNetwork}
                    onRefreshDrives={() => fetchDrives(includeNetwork)}
                    onStartScan={startScan}
                    onCancelScan={() => api.cancelScan()}
                  />
                )}

                {page === "explorer" && (
                  <ExplorerView
                    scanSummary={boot.activeSummary}
                    rulesPath={boot.startup?.rulesPath ?? ""}
                    isElevated={boot.startup?.elevation.isElevated ?? false}
                    syncRevision={syncRevision}
                    onGoHome={() => handleSelectPage("home")}
                    onRelaunchAdmin={handleRelaunchAdmin}
                    onQueueCleanup={(path, category) =>
                      openCleanupPreview(
                        [{ path, category, isTempDirContents: false }],
                        "recycle_bin",
                      )
                    }
                    onToast={showToast}
                  />
                )}

                {page === "apps" && (
                  <AppsView
                    syncRevision={syncRevision}
                    onQueueLeftoverCleanup={(path) =>
                      openCleanupPreview(
                        [{ path, category: "dev_artifact", isTempDirContents: false }],
                        "recycle_bin",
                      )
                    }
                  />
                )}

                {page === "cleaner" && (
                  <CleanerView
                    isElevated={boot.startup?.elevation.isElevated ?? false}
                    syncRevision={syncRevision}
                    onRelaunchAdmin={handleRelaunchAdmin}
                    onStartPreview={openCleanupPreview}
                  />
                )}

                {page === "dev_cleaner" && (
                  <DevCleanerView
                    isElevated={boot.startup?.elevation.isElevated ?? false}
                    syncRevision={syncRevision}
                    onRelaunchAdmin={handleRelaunchAdmin}
                  />
                )}

                {page === "protected" && <ProtectedPathsView />}

                {page === "history" && <HistoryView />}

                {page === "settings" && (
                  <SettingsView
                    settings={boot.settings}
                    serviceStatus={boot.serviceStatus}
                    onSettingsChange={boot.setSettings}
                    onServiceStatusChange={boot.setServiceStatus}
                    onRescanAll={boot.retryBootstrap}
                  />
                )}
              </div>
            </div>
          </main>

          {/* Non-blocking polite toast */}
          {toastMsg && (
            <output
              aria-live="polite"
              className="fixed bottom-5 right-5 z-90 max-w-[380px] rounded-[12px] bg-tile border border-border-default px-4 py-2.5 text-[12.5px] text-fg-primary shadow-[var(--shadow-floating)] flex items-center gap-3 animate-popover-in"
            >
              <span className="flex-1">{toastMsg}</span>
              <button
                type="button"
                onClick={() => setToastMsg(null)}
                aria-label="Cerrar notificación"
                className="text-fg-muted hover:text-fg-primary bg-transparent border-0 p-0.5 cursor-pointer"
              >
                <X size={13} />
              </button>
            </output>
          )}

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
      )}
    </div>
  );
}
