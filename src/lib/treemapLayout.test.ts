import { describe, expect, it } from "vitest";
import { formatBytes, formatDurationMs, formatSignedBytes } from "./format";
import { layoutSquarifiedTreemap } from "./treemapLayout";

describe("layoutSquarifiedTreemap", () => {
  it("preserves total area and stays within canvas bounds", () => {
    const items = [
      { id: "a", value: 600 },
      { id: "b", value: 300 },
      { id: "c", value: 100 },
    ];
    const width = 800;
    const height = 400;
    const rects = layoutSquarifiedTreemap(items, width, height);

    expect(rects).toHaveLength(3);
    const totalArea = rects.reduce((sum, r) => sum + r.w * r.h, 0);
    expect(totalArea).toBeCloseTo(width * height, 1);

    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(0);
      expect(r.y).toBeGreaterThanOrEqual(0);
      expect(r.x + r.w).toBeLessThanOrEqual(width + 0.01);
      expect(r.y + r.h).toBeLessThanOrEqual(height + 0.01);
    }
  });

  it("handles empty or zero-value lists safely", () => {
    expect(layoutSquarifiedTreemap([], 500, 300)).toEqual([]);
    expect(layoutSquarifiedTreemap([{ id: "z", value: 0 }], 500, 300)).toEqual([]);
  });
});

describe("format helpers", () => {
  it("formats bytes, signed deltas and durations", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1024 ** 3 * 2.5)).toBe("2.50 GB");
    expect(formatSignedBytes(2048)).toBe("+2.00 KB");
    expect(formatSignedBytes(-1024)).toBe("-1.00 KB");
    expect(formatDurationMs(450)).toBe("450 ms");
    expect(formatDurationMs(65_000)).toBe("1m 5s");
  });
});
