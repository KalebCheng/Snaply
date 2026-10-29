import type { Rect, Shape } from "../types";
import { blobToBytes } from "./decode";
import { drawShapeList } from "./render";

/** Compose the export at the source image's physical resolution. */
export async function composeExport(
  base: ImageBitmap,
  shapes: Shape[],
  crop?: Rect | null,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  const out = crop ?? { x: 0, y: 0, width: base.width, height: base.height };
  const w = Math.max(1, Math.round(out.width));
  const h = Math.max(1, Math.round(out.height));
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(base, out.x, out.y, w, h, 0, 0, w, h);
  ctx.translate(-out.x, -out.y);
  drawShapeList(ctx, shapes, base, undefined, out);
  const blob = await canvas.convertToBlob({ type: "image/png" });
  return { bytes: await blobToBytes(blob), width: w, height: h };
}

export function defaultFileName() {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `Snaply-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(
    d.getMinutes(),
  )}${p(d.getSeconds())}.png`;
}

/** Convert a physical crop rectangle into display coordinates. */
export function toImageRect(display: Rect, scale: number): Rect {
  return {
    x: Math.round(display.x / scale),
    y: Math.round(display.y / scale),
    width: Math.round(display.width / scale),
    height: Math.round(display.height / scale),
  };
}
