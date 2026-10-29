import type { Rect, Shape } from "../types";
import { arrowPolygon } from "./geometry";

/**
 * Annotation renderer using the source image's physical pixel coordinate space.
 */
type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

export function drawShape(ctx: Ctx, shape: Shape, base?: ImageBitmap, scope?: Rect) {
  switch (shape.kind) {
    case "rect":
    case "ellipse": {
      ctx.save();
      ctx.strokeStyle = shape.color;
      ctx.lineWidth = shape.strokeWidth;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      if (shape.kind === "rect") {
        ctx.strokeRect(shape.x, shape.y, shape.width, shape.height);
      } else {
        ctx.beginPath();
        ctx.ellipse(
          shape.x + shape.width / 2,
          shape.y + shape.height / 2,
          Math.abs(shape.width / 2),
          Math.abs(shape.height / 2),
          0,
          0,
          Math.PI * 2,
        );
        ctx.stroke();
      }
      ctx.restore();
      break;
    }
    case "arrow": {
      const pts = arrowPolygon(shape.from, shape.to, shape.strokeWidth);
      ctx.save();
      ctx.fillStyle = shape.color;
      ctx.beginPath();
      pts.forEach((p, i) => (i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
      ctx.closePath();
      ctx.fill();
      ctx.restore();
      break;
    }
    case "pen": {
      if (shape.points.length < 2) break;
      ctx.save();
      ctx.strokeStyle = shape.color;
      ctx.lineWidth = shape.strokeWidth;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(shape.points[0].x, shape.points[0].y);
      // Midpoint quadratic curves smooth freehand strokes.
      for (let i = 1; i < shape.points.length - 1; i++) {
        const p = shape.points[i];
        const n = shape.points[i + 1];
        ctx.quadraticCurveTo(p.x, p.y, (p.x + n.x) / 2, (p.y + n.y) / 2);
      }
      const last = shape.points[shape.points.length - 1];
      ctx.lineTo(last.x, last.y);
      ctx.stroke();
      ctx.restore();
      break;
    }
    case "text": {
      ctx.save();
      ctx.fillStyle = shape.color;
      ctx.font = `600 ${shape.fontSize}px -apple-system, "SF Pro Text", "Segoe UI", system-ui, sans-serif`;
      ctx.textBaseline = "top";
      shape.text.split("\n").forEach((line, i) => {
        ctx.fillText(line, shape.x, shape.y + i * shape.fontSize * 1.25);
      });
      ctx.restore();
      break;
    }
    case "blur": {
      if (!base) break;
      mosaic(ctx, base, shape);
      break;
    }
    case "watermark": {
      drawWatermark(ctx, shape, scope);
      break;
    }
  }
}

/** Build a mosaic from the original image pixels. */
export function mosaic(ctx: Ctx, base: ImageBitmap, r: Rect, cell = 12) {
  if (r.width < 1 || r.height < 1) return;
  // Downsample then upscale with nearest-neighbor filtering.
  const tw = Math.max(1, Math.round(r.width / cell));
  const th = Math.max(1, Math.round(r.height / cell));
  const small = new OffscreenCanvas(tw, th);
  const sctx = small.getContext("2d")!;
  sctx.imageSmoothingEnabled = true;
  sctx.drawImage(base, r.x, r.y, r.width, r.height, 0, 0, tw, th);
  ctx.save();
  ctx.beginPath();
  ctx.rect(r.x, r.y, r.width, r.height);
  ctx.clip();
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(small, r.x, r.y, r.width, r.height);
  ctx.restore();
}

/** Draw a diagonal tiled watermark inside the requested scope. */
export function drawWatermark(
  ctx: Ctx,
  w: { text: string; fontSize: number; opacity: number; angle: number; color: string },
  scope?: Rect,
) {
  const text = w.text.trim();
  if (!text) return;
  const canvas = ctx.canvas;
  const width = scope?.width ?? ("width" in canvas ? canvas.width : 0);
  const height = scope?.height ?? ("height" in canvas ? canvas.height : 0);
  if (!width || !height) return;
  const originX = scope?.x ?? 0;
  const originY = scope?.y ?? 0;
  ctx.save();
  ctx.globalAlpha = Math.min(1, Math.max(0.02, w.opacity));
  ctx.fillStyle = w.color;
  ctx.font = `600 ${w.fontSize}px -apple-system, "SF Pro Text", "Segoe UI", system-ui, sans-serif`;
  const metrics = ctx.measureText(text);
  const stepX = metrics.width + w.fontSize * 2.4;
  const stepY = w.fontSize * 3.2;
  ctx.translate(originX + width / 2, originY + height / 2);
  ctx.rotate((w.angle * Math.PI) / 180);
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const diagonal = Math.hypot(width, height);
  for (let y = -diagonal; y <= diagonal; y += stepY) {
    const offset = (Math.round(y / stepY) % 2) * (stepX / 2);
    for (let x = -diagonal + offset; x <= diagonal; x += stepX) {
      ctx.fillText(text, x, y);
    }
  }
  ctx.restore();
}

export function drawShapeList(
  ctx: Ctx,
  shapes: Shape[],
  base?: ImageBitmap,
  draft?: Shape | null,
  scope?: Rect,
) {
  for (const s of shapes) drawShape(ctx, s, base, scope);
  if (draft) drawShape(ctx, draft, base, scope);
}
