// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useStartupBootstrap } from "../../hooks/useStartupBootstrap";
import { type StartupBootstrapEvent, api } from "../../lib/ipc";
import { StartupSplash } from "./StartupSplash";

vi.mock("../../lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("../../lib/ipc")>("../../lib/ipc");
  return {
    ...actual,
    api: {
      ...actual.api,
      getStartupContext: vi.fn(),
      bootstrapStartup: vi.fn(),
      showMainWindow: vi.fn().mockResolvedValue(undefined),
    },
  };
});

describe("StartupSplash & useStartupBootstrap", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("skips splash completely on hot start when ready arrives in < 300ms", async () => {
    vi.mocked(api.getStartupContext).mockResolvedValue({
      elevation: { isElevated: false, username: "dev" },
      startupDrive: "C:\\",
      rulesPath: "rules.json",
      settings: {
        skipStartupAnimation: false,
        includeAds: false,
        startWithWindowsTray: false,
        uninstallWatchTimeoutMins: 10,
        driftThresholdPercent: 1,
        driftThresholdGb: 1,
      },
      serviceStatus: {
        installed: false,
        running: false,
        pipeReachable: false,
        binaryPath: "",
      },
      windowsAnimationsEnabled: true,
    });

    vi.mocked(api.bootstrapStartup).mockImplementation(async (onEvent) => {
      const ev: StartupBootstrapEvent = {
        kind: "ready",
        warm: true,
        partial: false,
        hasPreviousIndex: true,
        observations: [],
        summaries: [],
        elapsedMs: 42,
      };
      onEvent(ev);
    });

    const { result } = renderHook(() => useStartupBootstrap());
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(result.current.splashVisible).toBe(false);
    expect(result.current.homeMounted).toBe(true);
  });

  it("holds splash for at least 600ms once visible before closing", () => {
    const onMountHome = vi.fn();
    const onComplete = vi.fn();
    const now = Date.now();

    const { getByTestId } = render(
      <StartupSplash
        state={{ status: "success" }}
        globalRatio={1}
        phaseText="Finalizando…"
        totalFiles={1000}
        totalAllocatedBytes={4096}
        drives={[]}
        reducedMotion={false}
        minVisibleMs={600}
        visibleSinceMs={now}
        onMountHomeUnderneath={onMountHome}
        onComplete={onComplete}
        onRetry={vi.fn()}
        onEnterLastIndex={vi.fn()}
      />,
    );

    // At 400ms (< 600ms min hold), still in active stage and not completed
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(getByTestId("startup-splash").getAttribute("data-stage")).toBe("active");
    expect(onComplete).not.toHaveBeenCalled();

    // At 620ms (> 600ms min hold), enters converging
    act(() => {
      vi.advanceTimersByTime(220);
    });
    expect(getByTestId("startup-splash").getAttribute("data-stage")).toBe("converging");

    // At +320ms (total 940ms), enters tick and mounts Home underneath
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(getByTestId("startup-splash").getAttribute("data-stage")).toBe("tick");
    expect(onMountHome).toHaveBeenCalledTimes(1);

    // After full closing sequence (+750ms more), completes cleanly
    act(() => {
      vi.advanceTimersByTime(750);
    });
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("progress bar is strictly monotonic and never regresses on out-of-order updates", () => {
    const now = Date.now();
    const props = {
      state: { status: "loading" as const },
      phaseText: "Escaneando C:\\",
      totalFiles: 500,
      totalAllocatedBytes: 1024,
      drives: [],
      reducedMotion: false,
      visibleSinceMs: now,
      onMountHomeUnderneath: vi.fn(),
      onComplete: vi.fn(),
      onRetry: vi.fn(),
      onEnterLastIndex: vi.fn(),
    };

    const { getByRole, rerender } = render(<StartupSplash {...props} globalRatio={0.78} />);
    expect(getByRole("progressbar").getAttribute("aria-valuenow")).toBe("78");

    // Out-of-order lower value (0.35) MUST NOT regress bar below 78
    rerender(<StartupSplash {...props} globalRatio={0.35} />);
    expect(getByRole("progressbar").getAttribute("aria-valuenow")).toBe("78");

    // Higher value advances normally
    rerender(<StartupSplash {...props} globalRatio={0.91} />);
    expect(getByRole("progressbar").getAttribute("aria-valuenow")).toBe("91");
  });

  it("respects reducedMotion flag on splash root", () => {
    const { getByTestId } = render(
      <StartupSplash
        state={{ status: "loading" }}
        globalRatio={0.5}
        phaseText="Escaneando…"
        totalFiles={100}
        totalAllocatedBytes={4096}
        drives={[]}
        reducedMotion={true}
        visibleSinceMs={Date.now()}
        onMountHomeUnderneath={vi.fn()}
        onComplete={vi.fn()}
        onRetry={vi.fn()}
        onEnterLastIndex={vi.fn()}
      />,
    );
    expect(getByTestId("startup-splash").getAttribute("data-reduced-motion")).toBe("true");
  });

  it("allows skipping the closing sequence with click, Enter, and Escape", () => {
    const baseProps = {
      state: { status: "success" as const },
      globalRatio: 1,
      phaseText: "Listo",
      totalFiles: 10,
      totalAllocatedBytes: 4096,
      drives: [],
      reducedMotion: false,
      visibleSinceMs: Date.now() - 1000,
      onMountHomeUnderneath: vi.fn(),
      onRetry: vi.fn(),
      onEnterLastIndex: vi.fn(),
    };

    // 1. Click
    const onCompleteClick = vi.fn();
    const r1 = render(<StartupSplash {...baseProps} onComplete={onCompleteClick} />);
    fireEvent.click(r1.getByTestId("startup-splash"));
    expect(onCompleteClick).toHaveBeenCalledTimes(1);
    cleanup();

    // 2. Enter
    const onCompleteEnter = vi.fn();
    render(<StartupSplash {...baseProps} onComplete={onCompleteEnter} />);
    fireEvent.keyDown(window, { key: "Enter" });
    expect(onCompleteEnter).toHaveBeenCalledTimes(1);
    cleanup();

    // 3. Escape
    const onCompleteEsc = vi.fn();
    render(<StartupSplash {...baseProps} onComplete={onCompleteEsc} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCompleteEsc).toHaveBeenCalledTimes(1);
  });

  it("shows amber warning icon instead of green tick on partial scan", () => {
    const now = Date.now() - 1000;
    const { getByTestId, queryByTestId, getAllByText } = render(
      <StartupSplash
        state={{ status: "partial", observations: ["C:\\: 5 rutas sin acceso"] }}
        globalRatio={1}
        phaseText="Completado"
        totalFiles={500}
        totalAllocatedBytes={4096}
        drives={[]}
        reducedMotion={false}
        minVisibleMs={0}
        visibleSinceMs={now}
        onMountHomeUnderneath={vi.fn()}
        onComplete={vi.fn()}
        onRetry={vi.fn()}
        onEnterLastIndex={vi.fn()}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(320);
    });

    expect(getByTestId("splash-warning-icon")).toBeTruthy();
    expect(queryByTestId("splash-success-tick")).toBeNull();
    expect(getAllByText("Listo, con observaciones").length).toBeGreaterThanOrEqual(1);
  });
});
