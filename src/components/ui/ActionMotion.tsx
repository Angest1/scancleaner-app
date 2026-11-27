import { CheckCircle2, Sparkles, Terminal } from "lucide-react";
import { useEffect, useState } from "react";
import { formatBytes } from "../../lib/format";
import type { CleanupProgressEvent } from "../../lib/ipcTypes";

const MINI_DOTS = [
  { id: "accent", color: "var(--accent)", left: 0 },
  { id: "success", color: "var(--success)", left: 12 },
  { id: "violet", color: "#a855f7", left: 24 },
  { id: "warning", color: "var(--warning)", left: 36 },
  { id: "teal", color: "#14b8a6", left: 48 },
];

export function MiniLeapfrogLoader({ label }: { label?: string }) {
  return (
    <div className="inline-flex items-center gap-3">
      <div className="relative h-6 w-[58px] shrink-0" aria-hidden="true">
        {MINI_DOTS.map((d, idx) => (
          <span
            key={d.id}
            className={`absolute bottom-1 w-2 h-2 rounded-full ${
              idx === 0 ? "sc-mini-dot-leaper" : "sc-mini-dot-shifter"
            }`}
            style={{
              left: `${d.left}px`,
              backgroundColor: d.color,
            }}
          />
        ))}
      </div>
      {label && <span className="text-[12.5px] font-medium text-fg-secondary">{label}</span>}
    </div>
  );
}

interface ActionProgressStageProps {
  title: string;
  subtitle?: string;
  progress: CleanupProgressEvent | null;
  totalItemsFallback: number;
  totalBytesFallback: number;
}

export function ActionProgressStage({
  title,
  subtitle,
  progress,
  totalItemsFallback,
  totalBytesFallback,
}: ActionProgressStageProps) {
  const [elapsedSec, setElapsedSec] = useState(0);

  useEffect(() => {
    const start = Date.now();
    const id = window.setInterval(() => {
      setElapsedSec(Math.floor((Date.now() - start) / 1000));
    }, 500);
    return () => window.clearInterval(id);
  }, []);

  const total = Math.max(1, progress?.totalItems ?? totalItemsFallback);
  const completed = Math.min(total, progress?.completedItems ?? 0);
  const ratio = Math.max(0.06, Math.min(1, completed / total));
  const isCommand = progress?.phase === "command";
  const isFinalizing = progress?.phase === "finalizing";

  return (
    <output
      aria-live="polite"
      className="flex flex-col items-center text-center gap-4 py-5 px-4 rounded-[12px] bg-canvas border border-accent-border"
    >
      {/* Orbital badge + leapfrog dots */}
      <div className="relative flex flex-col items-center gap-3">
        <div className="relative w-14 h-14 grid place-items-center">
          <span
            aria-hidden="true"
            className="absolute inset-0 rounded-full bg-accent-soft sc-pulse-halo"
          />
          <svg
            width="56"
            height="56"
            viewBox="0 0 56 56"
            aria-hidden="true"
            className="absolute inset-0 sc-orbit-ring"
          >
            <circle
              cx="28"
              cy="28"
              r="24"
              fill="none"
              stroke="var(--border-default)"
              strokeWidth="2.5"
            />
            <circle
              cx="28"
              cy="28"
              r="24"
              fill="none"
              stroke="var(--accent)"
              strokeWidth="3"
              strokeLinecap="round"
              strokeDasharray="48 110"
            />
          </svg>
          <span className="relative z-10 text-accent">
            {isCommand ? (
              <Terminal size={20} />
            ) : isFinalizing ? (
              <CheckCircle2 size={20} className="text-success" />
            ) : (
              <Sparkles size={20} />
            )}
          </span>
        </div>

        <MiniLeapfrogLoader />
      </div>

      {/* Live current step description */}
      <div className="flex flex-col gap-1 max-w-full">
        <div className="text-[14.5px] font-semibold text-fg-primary">
          {progress?.currentLabel || title}
        </div>
        <div className="text-[12px] text-fg-secondary">
          {subtitle ?? "Ejecutando de forma segura con guardrails y auditoría activa…"}
        </div>
        {progress?.currentPath && (
          <div
            className="mono text-[11px] text-fg-muted truncate max-w-[500px] mt-0.5 px-2.5 py-1 rounded-[6px] bg-elevated border border-border-subtle"
            title={progress.currentPath}
          >
            {progress.currentPath}
          </div>
        )}
      </div>

      {/* Determinate GPU-composited progress bar */}
      <div className="w-full max-w-[440px] flex flex-col gap-1.5">
        <div className="h-2 w-full rounded-full bg-elevated overflow-hidden relative">
          <div
            className="h-full w-full rounded-full bg-accent sc-progress-fill"
            style={{ transform: `scaleX(${ratio})` }}
          />
          <div
            aria-hidden="true"
            className="absolute inset-y-0 left-0 w-1/3 bg-white/30 sc-shimmer-track"
          />
        </div>
        <div className="flex items-center justify-between text-[11.5px] text-fg-secondary mono">
          <span>
            {completed} / {total} elementos
          </span>
          <span>
            {formatBytes(progress?.freedBytesSoFar ?? 0)} / {formatBytes(totalBytesFallback)}
          </span>
          <span>{elapsedSec}s</span>
        </div>
      </div>
    </output>
  );
}

interface InlineActivityBannerProps {
  active: boolean;
  title: string;
  detail?: string;
  variant?: "floating" | "centered";
  delayMs?: number;
  minVisibleMs?: number;
}

/**
 * Anti-flash, zero-layout-shift activity indicator:
 * - Waits `delayMs` (default 260ms) before showing so sub-250ms operations NEVER flicker.
 * - Once shown, stays visible at least `minVisibleMs` (default 520ms) and exits with a smooth GPU transition.
 * - `floating` (default) renders in a fixed bottom pill so it NEVER pushes page content down.
 * - `centered` renders cleanly inside an empty content slot during initial cold loads.
 */
export function InlineActivityBanner({
  active,
  title,
  detail,
  variant = "floating",
  delayMs = 260,
  minVisibleMs = 520,
}: InlineActivityBannerProps) {
  const [mounted, setMounted] = useState(false);
  const [visible, setVisible] = useState(false);
  const shownAtRef = useState(() => ({ ts: 0 }))[0];

  useEffect(() => {
    let showTimer: number | undefined;
    let hideTimer: number | undefined;
    let unmountTimer: number | undefined;

    if (active) {
      showTimer = window.setTimeout(() => {
        shownAtRef.ts = Date.now();
        setMounted(true);
        window.requestAnimationFrame(() => setVisible(true));
      }, delayMs);
    } else if (mounted) {
      const elapsed = Date.now() - shownAtRef.ts;
      const waitRemaining = Math.max(0, minVisibleMs - elapsed);
      hideTimer = window.setTimeout(() => {
        setVisible(false);
        unmountTimer = window.setTimeout(() => setMounted(false), 220);
      }, waitRemaining);
    }

    return () => {
      if (showTimer !== undefined) window.clearTimeout(showTimer);
      if (hideTimer !== undefined) window.clearTimeout(hideTimer);
      if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    };
  }, [active, delayMs, minVisibleMs, mounted, shownAtRef]);

  if (!mounted) return null;

  if (variant === "centered") {
    return (
      <output
        aria-live="polite"
        data-visible={visible ? "true" : "false"}
        className="sc-centered-activity rounded-[14px] bg-panel border border-border-default shadow-[var(--shadow-neu)] px-6 py-10 flex flex-col items-center justify-center text-center gap-3.5"
      >
        <MiniLeapfrogLoader />
        <div className="flex flex-col gap-1 max-w-[420px]">
          <div className="text-[14px] font-semibold text-fg-primary">{title}</div>
          {detail && <div className="text-[12px] text-fg-secondary">{detail}</div>}
        </div>
        <div className="h-1.5 w-full max-w-[260px] rounded-full bg-elevated overflow-hidden relative">
          <div className="h-full w-2/5 rounded-full bg-accent sc-shimmer-track" />
        </div>
      </output>
    );
  }

  return (
    <output
      aria-live="polite"
      data-open={visible ? "true" : "false"}
      className="sc-presence-bar fixed bottom-5 right-5 z-90 pointer-events-none max-w-[380px] rounded-[12px] bg-tile/95 backdrop-blur-md border border-border-default px-4 py-2.5 text-[12.5px] text-fg-primary shadow-[var(--shadow-floating)] flex flex-col gap-2 overflow-hidden"
    >
      <div className="flex items-center gap-3 min-w-0">
        <MiniLeapfrogLoader />
        <div className="min-w-0">
          <div className="text-[12.5px] font-medium text-fg-primary truncate">{title}</div>
          {detail && <div className="text-[11px] text-fg-secondary truncate">{detail}</div>}
        </div>
      </div>
      <div className="h-1 w-full rounded-full bg-elevated overflow-hidden relative">
        <div className="h-full w-2/5 rounded-full bg-accent sc-shimmer-track" />
      </div>
    </output>
  );
}
