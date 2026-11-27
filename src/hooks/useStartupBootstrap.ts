import { useCallback, useEffect, useRef, useState } from "react";
import type { StartupSplashState } from "../components/startup/StartupSplash";
import {
  type AppSettings,
  type DriveInfo,
  type DriveProgressItem,
  type IndexingServiceStatus,
  type ScanSummary,
  type StartupContext,
  api,
} from "../lib/ipc";

const HOT_START_SKIP_MS = 300;

export interface StartupBootstrapResult {
  startup: StartupContext | null;
  settings: AppSettings;
  serviceStatus: IndexingServiceStatus | null;
  drives: DriveInfo[];
  summaries: ScanSummary[];
  activeSummary: ScanSummary | null;
  splashVisible: boolean;
  splashState: StartupSplashState;
  splashVisibleSinceMs: number;
  homeMounted: boolean;
  homeEmerging: boolean;
  globalRatio: number | null;
  phaseText: string;
  totalFiles: number;
  totalAllocatedBytes: number;
  driveProgress: DriveProgressItem[];
  partialObservations: string[];
  reducedMotion: boolean;
  setSettings: (s: AppSettings) => void;
  setServiceStatus: (st: IndexingServiceStatus) => void;
  setDrives: (d: DriveInfo[]) => void;
  setActiveSummary: (s: ScanSummary | null) => void;
  upsertSummary: (s: ScanSummary) => void;
  refreshSummaries: () => void;
  mountHomeUnderneath: () => void;
  completeSplash: () => void;
  retryBootstrap: () => void;
  enterLastIndex: () => void;
}

const DEFAULT_SETTINGS: AppSettings = {
  skipStartupAnimation: false,
  includeAds: false,
  startWithWindowsTray: false,
  uninstallWatchTimeoutMins: 10,
  driftThresholdPercent: 1.0,
  driftThresholdGb: 1,
};

function prefersReducedMotionMedia(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useStartupBootstrap(): StartupBootstrapResult {
  const [startup, setStartup] = useState<StartupContext | null>(null);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [serviceStatus, setServiceStatus] = useState<IndexingServiceStatus | null>(null);
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [summaries, setSummaries] = useState<ScanSummary[]>([]);
  const [activeSummary, setActiveSummary] = useState<ScanSummary | null>(null);

  const [splashVisible, setSplashVisible] = useState(false);
  const [splashState, setSplashState] = useState<StartupSplashState>({ status: "loading" });
  const [splashVisibleSinceMs, setSplashVisibleSinceMs] = useState<number>(() => Date.now());
  const [homeMounted, setHomeMounted] = useState(false);
  const [homeEmerging, setHomeEmerging] = useState(false);

  const [globalRatio, setGlobalRatio] = useState<number | null>(null);
  const [phaseText, setPhaseText] = useState("Leyendo índice…");
  const [totalFiles, setTotalFiles] = useState(0);
  const [totalAllocatedBytes, setTotalAllocatedBytes] = useState(0);
  const [driveProgress, setDriveProgress] = useState<DriveProgressItem[]>([]);
  const [partialObservations, setPartialObservations] = useState<string[]>([]);
  const [cssReduced, setCssReduced] = useState(() => prefersReducedMotionMedia());
  const [winAnimEnabled, setWinAnimEnabled] = useState(true);

  const bootStartRef = useRef<number>(Date.now());
  const hotTimerRef = useRef<number | null>(null);
  const splashShownRef = useRef(false);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = (e: MediaQueryListEvent) => setCssReduced(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const showSplashNow = useCallback(() => {
    if (splashShownRef.current) return;
    splashShownRef.current = true;
    setSplashVisibleSinceMs(Date.now());
    setSplashVisible(true);
  }, []);

  const runBootstrap = useCallback(
    (forceSplash = false) => {
      bootStartRef.current = Date.now();
      splashShownRef.current = false;
      setSplashState({ status: "loading" });
      setGlobalRatio(null);
      setPhaseText("Leyendo índice…");
      setTotalFiles(0);
      setTotalAllocatedBytes(0);
      setDriveProgress([]);
      setHomeEmerging(false);

      if (hotTimerRef.current !== null) {
        window.clearTimeout(hotTimerRef.current);
      }

      // Check optional `?demo-splash=cold|partial` preview param (for visual verification / GIF capture)
      const params =
        typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
      const demoMode = params?.get("demo-splash");
      if (demoMode === "cold" || demoMode === "partial") {
        showSplashNow();
        setHomeMounted(false);
        const demoDrives: DriveInfo[] = [
          {
            rootPath: "C:\\",
            label: "Sistema",
            kind: "fixed",
            fsType: "ntfs",
            fsName: "NTFS",
            totalBytes: 240e9,
            freeBytes: 35e9,
            usedBytes: 205e9,
            volumeSerial: 1,
            available: true,
          },
          {
            rootPath: "E:\\",
            label: "Proyectos",
            kind: "fixed",
            fsType: "ntfs",
            fsName: "NTFS",
            totalBytes: 500e9,
            freeBytes: 45e9,
            usedBytes: 455e9,
            volumeSerial: 2,
            available: true,
          },
        ];
        setDrives(demoDrives);
        const steps = [
          { r: 0.14, p: "Leyendo índice…", f: 184200, b: 48e9 },
          { r: 0.36, p: "Escaneando C:\\ (MFT)…", f: 742800, b: 178e9 },
          { r: 0.62, p: "Escaneando E:\\ (MFT)…", f: 1620400, b: 412e9 },
          { r: 0.84, p: "Calculando categorías…", f: 2180900, b: 610e9 },
          { r: 0.96, p: "Preparando limpieza…", f: 2418730, b: 660e9 },
        ];
        steps.forEach((st, i) => {
          window.setTimeout(
            () => {
              setGlobalRatio(st.r);
              setPhaseText(st.p);
              setTotalFiles(st.f);
              setTotalAllocatedBytes(st.b);
              setDriveProgress([
                {
                  driveRoot: "C:\\",
                  label: "Sistema",
                  usedBytes: 205e9,
                  progressRatio: Math.min(1, st.r * 1.4),
                  phase: st.r >= 0.6 ? "Completado" : "Escaneando MFT…",
                  files: Math.round(st.f * 0.4),
                  allocatedBytes: Math.round(st.b * 0.32),
                  done: st.r >= 0.6,
                  stale: false,
                  engineMode: "mft_direct",
                },
                {
                  driveRoot: "E:\\",
                  label: "Proyectos",
                  usedBytes: 455e9,
                  progressRatio: st.r,
                  phase: st.p,
                  files: Math.round(st.f * 0.6),
                  allocatedBytes: Math.round(st.b * 0.68),
                  done: false,
                  stale: false,
                  engineMode: "mft_direct",
                },
              ]);
            },
            260 * (i + 1),
          );
        });
        window.setTimeout(() => {
          if (demoMode === "partial") {
            setPartialObservations(["C:\\: 12 carpetas sin acceso (modo respaldo)."]);
            setSplashState({
              status: "partial",
              observations: ["C:\\: 12 carpetas sin acceso (modo respaldo)."],
            });
          } else {
            setSplashState({ status: "success" });
          }
        }, 1600);
        return;
      }

      api
        .getStartupContext()
        .then((ctx) => {
          setStartup(ctx);
          setSettings(ctx.settings);
          setServiceStatus(ctx.serviceStatus);
          setWinAnimEnabled(ctx.windowsAnimationsEnabled);

          const skipAnim = ctx.settings.skipStartupAnimation;
          if (forceSplash && !skipAnim) {
            showSplashNow();
          } else if (!skipAnim) {
            // Hot-start rule: only show splash if initialization takes >= 300ms
            hotTimerRef.current = window.setTimeout(() => {
              showSplashNow();
            }, HOT_START_SKIP_MS);
          }

          return api.bootstrapStartup((ev) => {
            if (ev.kind === "drives_ready") {
              setDrives(ev.drives);
              // Reveal hidden Tauri window on first real data paint (zero white flash)
              api.showMainWindow().catch(() => {});
            } else if (ev.kind === "progress") {
              setGlobalRatio(ev.globalRatio);
              setPhaseText(ev.phase);
              setTotalFiles(ev.totalFiles);
              setTotalAllocatedBytes(ev.totalAllocatedBytes);
              setDriveProgress(ev.drives);
              if (!skipAnim && ev.globalRatio !== null && ev.globalRatio < 0.9) {
                showSplashNow();
              }
            } else if (ev.kind === "ready") {
              if (hotTimerRef.current !== null) {
                window.clearTimeout(hotTimerRef.current);
                hotTimerRef.current = null;
              }
              setSummaries(ev.summaries);
              if (ev.summaries.length > 0) {
                setActiveSummary(ev.summaries[0]);
              }
              setPartialObservations(ev.observations);
              api.showMainWindow().catch(() => {});

              const elapsed = Date.now() - bootStartRef.current;
              const shouldSkipSplash =
                skipAnim || (ev.warm && elapsed < HOT_START_SKIP_MS && !splashShownRef.current);

              if (shouldSkipSplash) {
                setSplashVisible(false);
                setHomeMounted(true);
                setHomeEmerging(true);
              } else {
                showSplashNow();
                setSplashState(
                  ev.partial
                    ? { status: "partial", observations: ev.observations }
                    : { status: "success" },
                );
              }
            } else if (ev.kind === "error") {
              if (hotTimerRef.current !== null) {
                window.clearTimeout(hotTimerRef.current);
              }
              api.showMainWindow().catch(() => {});
              showSplashNow();
              setSplashState({
                status: "error",
                message: ev.message,
                hasPreviousIndex: ev.hasPreviousIndex,
              });
            }
          });
        })
        .catch(() => {
          // Running outside Tauri (e.g. plain Vite browser preview)
          setHomeMounted(true);
          setSplashVisible(false);
        });
    },
    [showSplashNow],
  );

  useEffect(() => {
    runBootstrap(false);
    return () => {
      if (hotTimerRef.current !== null) {
        window.clearTimeout(hotTimerRef.current);
      }
    };
  }, [runBootstrap]);

  const mountHomeUnderneath = useCallback(() => {
    setHomeMounted(true);
    setHomeEmerging(true);
  }, []);

  const completeSplash = useCallback(() => {
    setHomeMounted(true);
    setSplashVisible(false);
  }, []);

  const enterLastIndex = useCallback(() => {
    setHomeMounted(true);
    setSplashVisible(false);
  }, []);

  const upsertSummary = useCallback((summary: ScanSummary) => {
    const key = summary.rootPath.toUpperCase();
    setSummaries((prev) => {
      const rest = prev.filter((s) => s.rootPath.toUpperCase() !== key);
      return [...rest, summary].sort((a, b) => a.rootPath.localeCompare(b.rootPath));
    });
    setActiveSummary(summary);
  }, []);

  const refreshSummaries = useCallback(() => {
    api
      .listIndexedSummaries()
      .then((list) => {
        if (list.length === 0) return;
        setSummaries(list);
        setActiveSummary((cur) => {
          if (!cur) return list[0];
          const updated = list.find((s) => s.rootPath.toUpperCase() === cur.rootPath.toUpperCase());
          return updated ?? cur;
        });
      })
      .catch(() => {});
  }, []);

  return {
    startup,
    settings,
    serviceStatus,
    drives,
    summaries,
    activeSummary,
    splashVisible,
    splashState,
    splashVisibleSinceMs,
    homeMounted,
    homeEmerging,
    globalRatio,
    phaseText,
    totalFiles,
    totalAllocatedBytes,
    driveProgress,
    partialObservations,
    reducedMotion: cssReduced || !winAnimEnabled,
    setSettings,
    setServiceStatus,
    setDrives,
    setActiveSummary,
    upsertSummary,
    refreshSummaries,
    mountHomeUnderneath,
    completeSplash,
    retryBootstrap: () => runBootstrap(true),
    enterLastIndex,
  };
}
