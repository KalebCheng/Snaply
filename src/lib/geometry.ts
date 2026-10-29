import type { Point, Rect, Shape } from "../types";

/** Build an arrow polygon with a head scaled from its length and stroke. */
export function arrowPolygon(from: Point, to: Point, stroke: number): Point[] {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len;
  const uy = dy / len;
  const head = Math.min(len * 0.42, stroke * 5.5 + 10);
  const halfWidth = Math.min(head * 0.52, stroke * 2.2 + 6);
  const baseX = to.x - ux * head;
  const baseY = to.y - uy * head;
  // Unit normal to the arrow direction.
  const px = -uy;
  const py = ux;
  // Slightly narrow the shaft before the arrow head.
  const shaft = stroke + 1.5;
  return [
    { x: from.x + px * shaft, y: from.y + py * shaft },
    { x: baseX + px * shaft, y: baseY + py * shaft },
    { x: baseX + px * halfWidth, y: baseY + py * halfWidth },
    { x: to.x, y: to.y },
    { x: baseX - px * halfWidth, y: baseY - py * halfWidth },
    { x: baseX - px * shaft, y: baseY - py * shaft },
    { x: from.x - px * shaft, y: from.y - py * shaft },
  ];
}

export function normalizeRect(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}

export function clampRect(r: Rect, width: number, height: number): Rect {
  const x = Math.max(0, Math.min(r.x, width));
  const y = Math.max(0, Math.min(r.y, height));
  return {
    x,
    y,
    width: Math.min(r.width, width - x),
    height: Math.min(r.height, height - y),
  };
}

export function rectContains(r: Rect, p: Point, pad = 0) {
  return (
    p.x >= r.x - pad && p.y >= r.y - pad && p.x <= r.x + r.width + pad && p.y <= r.y + r.height + pad
  );
}

/** Return the bounding rectangle for an annotation. */
export function shapeBounds(s: Shape): Rect {
  switch (s.kind) {
    case "rect":
    case "ellipse":
    case "blur":
      return { x: s.x, y: s.y, width: s.width, height: s.height };
    case "arrow":
      return normalizeRect(s.from, s.to);
    case "pen": {
      const xs = s.points.map((p) => p.x);
      const ys = s.points.map((p) => p.y);
      return {
        x: Math.min(...xs),
        y: Math.min(...ys),
        width: Math.max(...xs) - Math.min(...xs),
        height: Math.max(...ys) - Math.min(...ys),
      };
    }
    case "text":
      return {
        x: s.x,
        y: s.y - s.fontSize,
        width: Math.max(s.fontSize * s.text.length * 0.62, s.fontSize),
        height: s.fontSize * 1.4,
      };
    case "watermark":
      return { x: 0, y: 0, width: 0, height: 0 };
  }
}

/** Squared distance between two points. */
export function dist2(a: Point, b: Point) {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return dx * dx + dy * dy;
}

export const uid = () => Math.random().toString(36).slice(2, 9);
