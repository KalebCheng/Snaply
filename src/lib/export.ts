import type { Rect, Shape } from "../types";
import { blobToBytes } from "./decode";
import { drawShapeList } from "./render";

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

type Surface = {
  ctx: Ctx;
  encode: () => Promise<Blob>;
};

/**
 * Create a physical-resolution drawing surface.
 *
 * `OffscreenCanvas` (and `convertToBlob`) only exists on Safari 16.4+ /
 * WebKitGTK recent builds, so fall back to a detached `<canvas>` + `toBlob`
 * instead of failing the whole export on older runtimes. The canvas is sized in
 * source image pixels, so no DPR scaling is applied here.
 */
function createSurface(width: number, height: number): Surface {
  if (
    typeof OffscreenCanvas !== "undefined" &&
    typeof OffscreenCanvas.prototype.convertToBlob === "function"
  ) {
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("无法创建导出画布");
    return { ctx, encode: () => canvas.convertToBlob({ type: "image/png" }) };
  }

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("无法创建导出画布");
  return {
    ctx,
    encode: () =>
      new Promise<Blob>((resolve, reject) => {
        canvas.toBlob(
          (blob) => (blob ? resolve(blob) : reject(new Error("PNG 编码失败"))),
          "image/png",
        );
      }),
  };
}

/** Compose the export at the source image's physical resolution. */
export async function composeExport(
  base: ImageBitmap,
  shapes: Shape[],
  crop?: Rect | null,
): Promise<{ bytes: Uint8Array; width: number; height: number }> {
  const out = crop ?? { x: 0, y: 0, width: base.width, height: base.height };
  const w = Math.max(1, Math.round(out.width));
  const h = Math.max(1, Math.round(out.height));
  const { ctx, encode } = createSurface(w, h);
  ctx.drawImage(base, out.x, out.y, w, h, 0, 0, w, h);
  ctx.translate(-out.x, -out.y);
  drawShapeList(ctx, shapes, base, undefined, out);
  const blob = await encode();
  return { bytes: await blobToBytes(blob), width: w, height: h };
}

export function defaultFileName() {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `Snaply-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(
    d.getMinutes(),
  )}${p(d.getSeconds())}.png`;
}
