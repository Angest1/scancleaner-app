// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDeferredReorder } from "./useDeferredReorder";

interface RowItem {
  id: string;
  size: number;
}

describe("useDeferredReorder (anti-reorder-under-cursor UX)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("defers reordering while pointer hovers and flushes on pointer leave or 1s inactivity", () => {
    const initial: RowItem[] = [
      { id: "a", size: 300 },
      { id: "b", size: 200 },
      { id: "c", size: 100 },
    ];
    const getKey = (item: RowItem) => item.id;

    const { result, rerender } = renderHook(({ list }) => useDeferredReorder(list, getKey, 1000), {
      initialProps: { list: initial },
    });

    expect(result.current.items.map((x) => x.id)).toEqual(["a", "b", "c"]);

    // Pointer enters list container
    act(() => {
      result.current.containerProps.onPointerEnter();
    });

    // Live update arrives: "c" grew larger than "a", so backend returns ["c", "a", "b"]
    const reordered: RowItem[] = [
      { id: "c", size: 900 },
      { id: "a", size: 300 },
      { id: "b", size: 200 },
    ];
    rerender({ list: reordered });

    // Order MUST NOT jump under cursor, yet "c"'s live size updates immediately!
    expect(result.current.items.map((x) => x.id)).toEqual(["a", "b", "c"]);
    expect(result.current.items.find((x) => x.id === "c")?.size).toBe(900);

    // After pointer leaves, new order is applied cleanly
    act(() => {
      result.current.containerProps.onPointerLeave();
    });
    expect(result.current.items.map((x) => x.id)).toEqual(["c", "a", "b"]);

    // Enter again and verify 1000ms inactivity timer also flushes deferred reorder
    act(() => {
      result.current.containerProps.onPointerEnter();
    });
    const reordered2: RowItem[] = [
      { id: "b", size: 1500 },
      { id: "c", size: 900 },
      { id: "a", size: 300 },
    ];
    rerender({ list: reordered2 });
    expect(result.current.items.map((x) => x.id)).toEqual(["c", "a", "b"]);

    act(() => {
      vi.advanceTimersByTime(1050);
    });
    expect(result.current.items.map((x) => x.id)).toEqual(["b", "c", "a"]);
  });
});
