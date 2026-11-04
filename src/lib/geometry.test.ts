import { describe, expect, it } from "vitest";
import {
  arrowPolygon,
  clampRect,
  clampToImage,
  normalizeRect,
  rectContains,
  shapeBounds,
} from "./geometry";
import type { Rect } from "../types";

describe("normalizeRect", () => {
  it("handles a drag towards the top-left", () => {
    const r = normalizeRect({ x: 100, y: 80 }, { x: 20, y: 10 });
    expect(r).toEqual({ x: 20, y: 10, width: 80, height: 70 });
  });

  it("keeps a zero-size drag at the origin point", () => {
    expect(normalizeRect({ x: 5, y: 6 }, { x: 5, y: 6 })).toEqual({ x: 5, y: 6, width: 0, height: 0 });
  });
});

describe("clampRect", () => {
  it("clamps to the image bounds", () => {
    expect(clampRect({ x: -10, y: -10, width: 300, height: 300 }, 200, 100)).toEqual({
      x: 0,
      y: 0,
      width: 200,
      height: 100,
    });
  });
});

describe("clampToImage", () => {
  it("keeps a crop one pixel inside the image", () => {
    expect(clampToImage({ x: 199.6, y: 99.7, width: 40, height: 40 }, 200, 100)).toEqual({
      x: 199,
      y: 99,
      width: 1,
      height: 1,
    });
  });

  it("never returns a zero or negative crop", () => {
    const crop = clampToImage({ x: -50, y: -50, width: 0, height: 0 }, 100, 100);
    expect(crop.x).toBe(0);
    expect(crop.y).toBe(0);
    expect(crop.width).toBeGreaterThanOrEqual(1);
    expect(crop.height).toBeGreaterThanOrEqual(1);
  });

  it("rounds a fractional selection without leaving the image", () => {
    const crop = clampToImage({ x: 10.4, y: 20.6, width: 30.2, height: 40.9 }, 100, 100);
    expect(crop).toEqual({ x: 10, y: 21, width: 30, height: 41 });
    expect(crop.x + crop.width).toBeLessThanOrEqual(100);
    expect(crop.y + crop.height).toBeLessThanOrEqual(100);
  });

  it("treats non-finite values as zero", () => {
    const crop = clampToImage(
      { x: Number.NaN, y: Number.POSITIVE_INFINITY, width: Number.NaN, height: Number.NaN },
      100,
      100,
    );
    expect(crop).toEqual({ x: 0, y: 0, width: 1, height: 1 });
  });
});

describe("rectContains", () => {
  const rect: Rect = { x: 10, y: 10, width: 100, height: 50 };
  it("accepts points inside and on the border", () => {
    expect(rectContains(rect, { x: 10, y: 10 })).toBe(true);
    expect(rectContains(rect, { x: 110, y: 60 })).toBe(true);
    expect(rectContains(rect, { x: 60, y: 30 })).toBe(true);
  });
  it("rejects points outside", () => {
    expect(rectContains(rect, { x: 9.9, y: 30 })).toBe(false);
    expect(rectContains(rect, { x: 60, y: 61 })).toBe(false);
  });
});

describe("arrowPolygon", () => {
  it("stays finite for a degenerate zero-length arrow", () => {
    const points = arrowPolygon({ x: 5, y: 5 }, { x: 5, y: 5 }, 4);
    expect(points).toHaveLength(7);
    for (const point of points) {
      expect(Number.isFinite(point.x)).toBe(true);
      expect(Number.isFinite(point.y)).toBe(true);
    }
  });
});

describe("shapeBounds", () => {
  it("returns the rectangle of a rect shape", () => {
    expect(
      shapeBounds({ id: "a", kind: "rect", x: 1, y: 2, width: 3, height: 4, color: "#fff", strokeWidth: 1 }),
    ).toEqual({ x: 1, y: 2, width: 3, height: 4 });
  });
});
