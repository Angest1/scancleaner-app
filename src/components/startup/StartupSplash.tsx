import { AlertTriangle, RotateCcw, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../../lib/format";
import type { DriveProgressItem } from "../../lib/ipc";
import { BrandMark } from "../ui/BrandMark";
import { Button } from "../ui/Button";
import { LeapfrogDots } from "./LeapfrogDots";

export type StartupSplashState =
  | { status: "loading" }
  | { status: "success" }
  | { status: "partial"; observations: string[] }
  | { status: "error"; message: string; hasPreviousIndex: boolean };

interface StartupSplashProps {
  state: StartupSplashState;
  globalRatio: number | null;
  phaseText: string;
  totalFiles: number;
  totalAllocatedBytes: number;
  drives: DriveProgressItem[];
  reducedMotion: boolean;
  minVisibleMs?: number;
  visibleSinceMs: number;
  onMountHomeUnderneath: () => void;
  onComplete: () => void;
  onRetry: () => void;
  onEnterLastIndex: () => void;
}

type ClosingStage = "active" | "converging" | "tick" | "exiting" | "done";

const numberFormatter = new Intl.NumberFormat(undefined);

export function StartupSplash({
  state,
  globalRatio,
  phaseText,
  totalFiles,
  totalAllocatedBytes,
  drives,
  reducedMotion,
  minVisibleMs = 600,
  visibleSinceMs,
  onMountHomeUnderneath,
  onComplete,
  onRetry,
  onEnterLastIndex,
}: StartupSplashProps) {
  // Progress bar rule: NEVER regress even if out-of-order updates arrive
  const [monotonicPct, setMonotonicPct] = useState<number>(0);
  const [closingStage, setClosingStage] = useState<ClosingStage>("active");
  const [announcedReady, setAnnouncedReady] = useState(false);
  const completedRef = useRef(false);

  // Throttled aria-live phase string so screen readers aren't spammed
  const [livePhase, setLivePhase] = useState(phaseText);
  const lastLiveUpdateRef = useRef<number>(0);

  useEffect(() => {
    const now = Date.now();
    if (now - lastLiveUpdateRef.current >= 450 || !livePhase) {
      lastLiveUpdateRef.current = now;
      setLivePhase(phaseText);
    }
  }, [phaseText, livePhase]);

  useEffect(() => {
    if (state.status === "success" || state.status === "partial") {
      setMonotonicPct(100);
      return;
    }
    if (globalRatio !== null && Number.isFinite(globalRatio)) {
      const nextPct = Math.max(0, Math.min(99, Math.round(globalRatio * 100)));
      setMonotonicPct((prev) => Math.max(prev, nextPct));
    }
  }, [globalRatio, state.status]);

  const finishImmediately = useCallback(() => {
    if (completedRef.current) return;
    completedRef.current = true;
    onMountHomeUnderneath();
    setClosingStage("done");
    onComplete();
  }, [onComplete, onMountHomeUnderneath]);

  // Orchestrate the 900–1200ms closing sequence once scan finishes (honoring >=600ms min display time)
  useEffect(() => {
    if (state.status !== "success" && state.status !== "partial") {
      setClosingStage("active");
      setAnnouncedReady(false);
      completedRef.current = false;
      return;
    }

    const elapsed = Date.now() - visibleSinceMs;
    const waitMin = Math.max(0, minVisibleMs - elapsed);

    if (reducedMotion) {
      const t1 = window.setTimeout(() => {
        setClosingStage("tick");
        setAnnouncedReady(true);
        onMountHomeUnderneath();
      }, waitMin);
      const t2 = window.setTimeout(() => {
        setClosingStage("exiting");
      }, waitMin + 320);
      const t3 = window.setTimeout(() => {
        finishImmediately();
      }, waitMin + 520);
      return () => {
        window.clearTimeout(t1);
        window.clearTimeout(t2);
        window.clearTimeout(t3);
      };
    }

    // Standard choreographed closing sequence (300ms converge + 400ms tick + 350ms emerge = 1050ms)
    const tConverge = window.setTimeout(() => {
      setClosingStage("converging");
    }, waitMin);

    const tTick = window.setTimeout(() => {
      setClosingStage("tick");
      setAnnouncedReady(true);
      // Mount & render Home underneath during step 2 so step 3 has zero frame drops
      onMountHomeUnderneath();
    }, waitMin + 300);

    const tExit = window.setTimeout(() => {
      setClosingStage("exiting");
    }, waitMin + 700);

    const tDone = window.setTimeout(() => {
      finishImmediately();
    }, waitMin + 1050);

    return () => {
      window.clearTimeout(tConverge);
      window.clearTimeout(tTick);
      window.clearTimeout(tExit);
      window.clearTimeout(tDone);
    };
  }, [
    state.status,
    visibleSinceMs,
    minVisibleMs,
    reducedMotion,
    onMountHomeUnderneath,
    finishImmediately,
  ]);

  // Allow skipping the closing sequence with click, Enter, or Escape
  const canSkipClosing =
    (state.status === "success" || state.status === "partial") && closingStage !== "done";

  useEffect(() => {
    if (!canSkipClosing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === "Escape") {
        e.preventDefault();
        finishImmediately();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [canSkipClosing, finishImmediately]);

  const isIndeterminate = state.status === "loading" && globalRatio === null;
  const isPartial = state.status === "partial";
  const isError = state.status === "error";
  const showTickVisual = closingStage === "tick" || closingStage === "exiting";

  const readyHeading = isPartial ? "Listo, con observaciones" : "¡Listo!";

  const effectivePhaseDisplay = useMemo(() => {
    if (monotonicPct >= 99 && state.status === "loading" && phaseText.includes("Escaneando")) {
      return "Finalizando categorías y limpieza…";
    }
    return phaseText;
  }, [monotonicPct, state.status, phaseText]);

  if (closingStage === "done") return null;

  return (
    <div
      data-testid="startup-splash"
      data-reduced-motion={reducedMotion ? "true" : "false"}
      data-stage={closingStage}
      data-status={state.status}
      onClick={() => {
        if (canSkipClosing) finishImmediately();
      }}
      onKeyDown={(e) => {
        if (canSkipClosing && (e.key === "Enter" || e.key === "Escape")) {
          e.preventDefault();
          finishImmediately();
        }
      }}
      className={`fixed inset-0 z-100 flex flex-col items-center justify-center bg-canvas text-fg-primary px-6 select-none ${
        closingStage === "exiting" ? "sc-splash-exiting pointer-events-none" : ""
      } ${canSkipClosing ? "cursor-pointer" : ""}`}
    >
      <div
        className={`w-full max-w-[440px] flex flex-col items-center text-center gap-6 ${
          closingStage === "converging" ? "sc-splash-stage-converging" : ""
        }`}
      >
        {/* Top centered logo & brand name */}
        <div
          className={`flex flex-col items-center gap-2.5 transition-opacity duration-240 ${
            showTickVisual ? "opacity-0" : "opacity-100"
          }`}
        >
          <BrandMark size={48} />
          <div className="text-[20px] font-bold tracking-[-0.025em] text-fg-primary" translate="no">
            ScanCleaner
          </div>
        </div>

        {/* Center focal area: leapfrog dots -> converging circle -> green tick / amber warning / error */}
        <div className="min-h-[96px] flex flex-col items-center justify-center gap-3">
          {isError ? (
            <div className="flex flex-col items-center gap-3" role="alert">
              <div className="w-16 h-16 rounded-full bg-danger-soft border border-danger-border grid place-items-center text-danger">
                <XCircle size={32} aria-hidden="true" />
              </div>
              <div className="text-[16px] font-semibold text-fg-primary">
                No se pudo completar el escaneo
              </div>
              <p className="m-0 text-[13px] text-fg-secondary max-w-[360px] text-pretty">
                {state.message}
              </p>
              <div className="flex items-center gap-2.5 mt-2 flex-wrap justify-center">
                <Button variant="primary" onClick={onRetry}>
                  <RotateCcw size={13} />
                  Reintentar
                </Button>
                {state.hasPreviousIndex && (
                  <Button variant="default" onClick={onEnterLastIndex}>
                    Entrar con el último índice
                  </Button>
                )}
              </div>
            </div>
          ) : showTickVisual ? (
            <div className="flex flex-col items-center gap-3">
              <div
                data-testid={isPartial ? "splash-warning-icon" : "splash-success-tick"}
                className={`w-[68px] h-[68px] rounded-full grid place-items-center sc-tick-circle ${
                  isPartial ? "bg-warning text-white" : "bg-success text-white"
                }`}
              >
                {isPartial ? (
                  <AlertTriangle size={32} aria-hidden="true" />
                ) : (
                  <svg width="34" height="34" viewBox="0 0 36 36" fill="none" aria-hidden="true">
                    <path
                      d="M8.5 18.5L15 25L27.5 11.5"
                      stroke="#ffffff"
                      strokeWidth="3.5"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      className="sc-tick-stroke"
                    />
                  </svg>
                )}
              </div>
              <div className="text-[18px] font-semibold tracking-[-0.02em] text-fg-primary sc-ready-text">
                {readyHeading}
              </div>
              <div className="text-[11.5px] text-fg-muted">Pulsa clic, Enter o Esc para omitir</div>
            </div>
          ) : (
            <LeapfrogDots
              converging={closingStage === "converging"}
              reducedMotion={reducedMotion}
            />
          )}
        </div>

        {/* Determinate / Indeterminate progress bar + phase text + multi-drive sub-lines */}
        {!isError && (
          <div
            className={`w-full flex flex-col gap-2.5 transition-opacity duration-240 ${
              closingStage === "active" ? "opacity-100" : "opacity-0 pointer-events-none"
            }`}
          >
            <div
              role="progressbar"
              tabIndex={0}
              aria-label="Progreso de inicialización"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={isIndeterminate ? undefined : monotonicPct}
              className="w-full h-2 rounded-full bg-elevated border border-border-subtle overflow-hidden relative"
            >
              {isIndeterminate ? (
                <div className="h-full w-2/5 rounded-full bg-accent animate-scan-bar" />
              ) : (
                <div
                  className="h-full w-full rounded-full bg-accent sc-progress-fill"
                  style={{ transform: `scaleX(${monotonicPct / 100})` }}
                />
              )}
            </div>

            <div className="flex items-center justify-between gap-2 text-[12.5px]">
              <span className="text-fg-primary font-medium truncate">{effectivePhaseDisplay}</span>
              {!isIndeterminate && (
                <span className="mono text-[12px] text-fg-secondary shrink-0 tabular">
                  {monotonicPct}%
                </span>
              )}
            </div>

            <div className="mono text-[11.5px] text-fg-muted tabular">
              {totalFiles > 0
                ? `${numberFormatter.format(totalFiles)} archivos · ${formatBytes(totalAllocatedBytes)}`
                : "Preparando unidades locales…"}
            </div>

            {/* Multi-drive sub-lines when scanning more than 1 local drive */}
            {drives.length > 1 && (
              <div className="mt-1 pt-2.5 border-t border-border-subtle flex flex-col gap-1.5 text-left">
                {drives.map((d) => {
                  const pct = d.done
                    ? 100
                    : d.progressRatio !== null
                      ? Math.round(d.progressRatio * 100)
                      : null;
                  return (
                    <div
                      key={d.driveRoot}
                      className="flex items-center justify-between gap-2 text-[11.5px]"
                    >
                      <span className="mono text-fg-secondary shrink-0">
                        {d.driveRoot} {d.label ? `(${d.label})` : ""}
                      </span>
                      <span className="text-fg-muted truncate flex-1 text-right">{d.phase}</span>
                      <span className="mono text-fg-primary w-9 text-right tabular shrink-0">
                        {pct !== null ? `${pct}%` : "…"}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Screen reader polite announcements: phase throttled, "¡Listo!" announced strictly once */}
      <div className="sr-only" aria-live="polite" aria-atomic="true">
        {announcedReady ? readyHeading : livePhase}
      </div>
    </div>
  );
}
