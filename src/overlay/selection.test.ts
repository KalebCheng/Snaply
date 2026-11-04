import { describe, expect, it } from "vitest";
import { handlePoints, hitHandle, moveRect, resizeRect, roundRect } from "./selection";

describe("roundRect", () => {
  it("rounds each edge independently", () => {
    expect(roundRect({ x: 10.4, y: 10.6, width: 20.5, height: 30.4 })).toEqual({
      x: 10,
      y: 11,
      width: 21,
      height: 30,
    });
  });
});

describe("resizeRect", () => {
  const bounds = { width: 200, height: 200 };

  it("grows from the south-east handle", () => {
    expect(resizeRect({ x: 10, y: 10, width: 20, height: 20 }, "se", { x: 60, y: 70 }, bounds)).toEqual({
      x: 10,
      y: 10,
      width: 50,
      height: 60,
    });
  });

  it("keeps a minimum size when the handles cross over", () => {
    const r = resizeRect({ x: 10, y: 10, width: 20, height: 20 }, "nw", { x: 40, y: 40 }, bounds);
    expect(r.width).toBeGreaterThanOrEqual(2);
    expect(r.height).toBeGreaterThanOrEqual(2);
  });

  it("clamps to the viewport", () => {
    const r = resizeRect({ x: 10, y: 10, width: 20, height: 20 }, "se", { x: 500, y: 500 }, bounds);
    expect(r.x + r.width).toBeLessThanOrEqual(bounds.width);
    expect(r.y + r.height).toBeLessThanOrEqual(bounds.height);
  });
});

describe("moveRect", () => {
  const bounds = { width: 200, height: 200 };

  it("stops at the edges", () => {
    expect(moveRect({ x: 10, y: 10, width: 50, height: 50 }, -100, -100, bounds)).toMatchObject({
      x: 0,
      y: 0,
      width: 50,
      height: 50,
    });
    expect(moveRect({ x: 10, y: 10, width: 50, height: 50 }, 1000, 1000, bounds)).toMatchObject({
      x: 150,
      y: 150,
    });
  });
});

describe("hitHandle", () => {
  const rect = { x: 100, y: 100, width: 200, height: 100 };

  it("hits every corner", () => {
    const points = handlePoints(rect);
    expect(hitHandle(rect, points.nw)).toBe("nw");
    expect(hitHandle(rect, points.se)).toBe("se");
    expect(hitHandle(rect, points.sw)).toBe("sw");
    expect(hitHandle(rect, points.ne)).toBe("ne");
  });

  it("misses the middle of the selection", () => {
    expect(hitHandle(rect, { x: 200, y: 150 })).toBeNull();
  });
});
