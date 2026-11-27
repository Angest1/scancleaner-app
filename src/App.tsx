import { useCallback, useEffect, useState } from "react";
import { AppHeader, type NavPage } from "./components/AppHeader";
import { AppSidebar } from "./components/AppSidebar";
import { AppStatusBanners } from "./components/AppStatusBanners";
import { CleanupModal } from "./components/CleanupModal";
import { StartupSplash } from "./components/startup/StartupSplash";
import { AmbientMesh } from "./components/ui/AmbientMesh";
import { ToastOverlay } from "./components/ui/ToastOverlay";
import { useAppLiveEvents } from "./hooks/useAppLiveEvents";
import { useGlobalCleanupModal } from "./hooks/useGlobalCleanupModal";
import { useStartupBootstrap } from "./hooks/useStartupBootstrap";
import { type DeletionJobStatus, type ScanProgress, type SyncStatusDto, api } from "./lib/ipc";
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
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
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
    refreshSummaries: boot.refreshSummaries,
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
          boot.upsertSummary(summary);
          setSyncRevision((r) => r + 1);
          if (!summary.stats.cancelled) {
            setPage("explorer");
          }
        })
        .catch((err) => showToast(String(err)))
        .finally(() => setScanningDrive(null));
    },
    [boot.upsertSummary, showToast],
  );

  const openDriveInExplorer = useCallback(
    (rootPath: string) => {
      const norm = rootPath.toUpperCase();
      const existing = boot.summaries.find((s) => s.rootPath.toUpperCase() === norm);
      if (existing) {
        boot.setActiveSummary(existing);
        api.setActiveDrive(existing.rootPath).catch(() => {});
        setPage("explorer");
      } else {
        startScan(rootPath);
      }
    },
    [boot.summaries, boot.setActiveSummary, startScan],
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

  const cleanup = useGlobalCleanupModal(() => {
    setSyncRevision((r) => r + 1);
    fetchDrives(includeNetwork);
  }, showToast);

  const [manualRefreshing, setManualRefreshing] = useState(false);

  const handleManualSync = useCallback(() => {
    if (manualRefreshing || syncStatus.syncing || Boolean(scanningDrive)) return;
    setManualRefreshing(true);
    setSyncStatus((prev) => ({ ...prev, syncing: true }));
    Promise.all([
      api.triggerFocusSync().catch(() => null),
      api
        .listDrives(includeNetwork)
        .then(boot.setDrives)
        .catch(() => {}),
    ])
      .then(([res]) => {
        boot.refreshSummaries();
        setSyncRevision((r) => r + 1);
        if (res?.systemFileNotice) {
          showToast(res.systemFileNotice);
        } else {
          showToast("Unidades e índice actualizados.");
        }
      })
      .finally(() => {
        setManualRefreshing(false);
        setSyncStatus((prev) => ({ ...prev, syncing: false }));
      });
  }, [
    manualRefreshing,
    syncStatus.syncing,
    scanningDrive,
    includeNetwork,
    boot.setDrives,
    boot.refreshSummaries,
    showToast,
  ]);

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
          className={`relative flex-1 flex flex-col xl:flex-row h-full overflow-hidden ${
            boot.homeEmerging ? "sc-home-emerging" : ""
          }`}
        >
          <AmbientMesh />

          <AppSidebar
            page={page}
            collapsed={sidebarCollapsed}
            scanningDrive={scanningDrive}
            activeDriveRoot={boot.activeSummary?.rootPath ?? null}
            syncStatus={syncStatus}
            elevation={boot.startup?.elevation ?? null}
            theme={theme}
            onSelectPage={handleSelectPage}
            onToggleCollapse={() => setSidebarCollapsed((v) => !v)}
            onManualSync={handleManualSync}
            onRelaunchAdmin={handleRelaunchAdmin}
            onToggleTheme={toggleTheme}
          />

          <div className="relative flex-1 min-w-0 flex flex-col h-full overflow-hidden">
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
                <AppStatusBanners
                  showPartial={!dismissedPartialBanner}
                  partialMessage={boot.partialObservations[0]}
                  activeBgJobs={activeBgJobs}
                  onGoSettings={handleSelectPage}
                  onDismissPartial={() => setDismissedPartialBanner(true)}
                />

                <div key={page} className="w-full animate-view-in">
                  {page === "home" && (
                    <HomeView
                      username={boot.startup?.elevation.username ?? null}
                      isElevated={boot.startup?.elevation.isElevated ?? false}
                      drives={boot.drives}
                      summaries={boot.summaries}
                      loadingDrives={loadingDrives}
                      syncing={syncStatus.syncing || manualRefreshing}
                      includeNetwork={includeNetwork}
                      scanningDrive={scanningDrive}
                      progress={progress}
                      scanSummary={boot.activeSummary}
                      onNavigate={(target) => handleSelectPage(target)}
                      onExploreDrive={openDriveInExplorer}
                      onToggleNetwork={handleToggleNetwork}
                      onRefreshDrives={handleManualSync}
                      onStartScan={startScan}
                      onCancelScan={() => api.cancelScan()}
                    />
                  )}

                  {page === "explorer" && (
                    <ExplorerView
                      scanSummary={boot.activeSummary}
                      summaries={boot.summaries}
                      drives={boot.drives}
                      scanningDrive={scanningDrive}
                      rulesPath={boot.startup?.rulesPath ?? ""}
                      isElevated={boot.startup?.elevation.isElevated ?? false}
                      syncRevision={syncRevision}
                      onSelectDrive={openDriveInExplorer}
                      onRescanDrive={startScan}
                      onGoHome={() => handleSelectPage("home")}
                      onRelaunchAdmin={handleRelaunchAdmin}
                      onQueueCleanup={(path, category) =>
                        cleanup.openCleanupPreview(
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
                        cleanup.openCleanupPreview(
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
                      onStartPreview={cleanup.openCleanupPreview}
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
          </div>

          <ToastOverlay message={toastMsg} onDismiss={() => setToastMsg(null)} />

          {cleanup.activePlan && (
            <CleanupModal
              plan={cleanup.activePlan}
              executing={cleanup.executingCleanup}
              result={cleanup.cleanupResult}
              onChangeMode={cleanup.handleChangePlanMode}
              onExecute={cleanup.handleExecutePlan}
              onClose={cleanup.closeModal}
            />
          )}
        </div>
      )}
    </div>
  );
}
