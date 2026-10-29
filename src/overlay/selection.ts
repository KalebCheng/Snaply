import type { Rect } from "../types";
import { normalizeRect } from "../lib/geometry";
import type { Point } from "../types";

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export const HANDLE_SIZE = 16;
const MIN_SIZE = 2;

export function handlePoints(r: Rect): Record<Handle, Point> {
  const { x, y, width: w, height: h } = r;
  return {
    nw: { x, y },
    n: { x: x + w / 2, y },
    ne: { x: x + w, y },
    e: { x: x + w, y: y + h / 2 },
    se: { x: x + w, y: y + h },
    s: { x: x + w / 2, y: y + h },
    sw: { x, y: y + h },
    w: { x, y: y + h / 2 },
  };
}

export const HANDLE_CURSOR: Record<Handle, string> = {
  nw: "nwse-resize",
  n: "ns-resize",
  ne: "nesw-resize",
  e: "ew-resize",
  se: "nwse-resize",
  s: "ns-resize",
  sw: "nesw-resize",
  w: "ew-resize",
};

export function hitHandle(rect: Rect, p: Point, tolerance = HANDLE_SIZE / 2 + 2): Handle | null {
  for (const [k, pt] of Object.entries(handlePoints(rect)) as [Handle, Point][]) {
    if (Math.abs(p.x - pt.x) <= tolerance && Math.abs(p.y - pt.y) <= tolerance) return k;
  }
  return null;
}

export function resizeRect(rect: Rect, handle: Handle, p: Point, bounds: { width: number; height: number }): Rect {
  const x1 = rect.x;
  const y1 = rect.y;
  const x2 = rect.x + rect.width;
  const y2 = rect.y + rect.height;
  let nx1 = x1;
  let ny1 = y1;
  let nx2 = x2;
  let ny2 = y2;

  if (handle.includes("w")) nx1 = clamp(p.x, 0, bounds.width);
  if (handle.includes("e")) nx2 = clamp(p.x, 0, bounds.width);
  if (handle.includes("n")) ny1 = clamp(p.y, 0, bounds.height);
  if (handle.includes("s")) ny2 = clamp(p.y, 0, bounds.height);

  const r = normalizeRect({ x: nx1, y: ny1 }, { x: nx2, y: ny2 });
  if (r.width < MIN_SIZE) {
    if (handle.includes("w")) r.x = r.x + r.width - MIN_SIZE;
    r.width = MIN_SIZE;
  }
  if (r.height < MIN_SIZE) {
    if (handle.includes("n")) r.y = r.y + r.height - MIN_SIZE;
    r.height = MIN_SIZE;
  }
  return r;
}

export function moveRect(rect: Rect, dx: number, dy: number, bounds: { width: number; height: number }): Rect {
  return {
    x: clamp(rect.x + dx, 0, bounds.width - rect.width),
    y: clamp(rect.y + dy, 0, bounds.height - rect.height),
    width: rect.width,
    height: rect.height,
  };
}

export function clamp(v: number, min: number, max: number) {
  return Math.max(min, Math.min(v, max));
}

export function roundRect(r: Rect): Rect {
  return {
    x: Math.round(r.x),
    y: Math.round(r.y),
    width: Math.round(r.width),
    height: Math.round(r.height),
  };
}
