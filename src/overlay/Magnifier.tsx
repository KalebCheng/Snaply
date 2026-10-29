import { useEffect, useRef, useState } from "react";
import type { Point } from "../types";
import { api, isTauri } from "../lib/ipc";

type Props = {
  source: ImageBitmap | null;
  /** Cursor position in screen coordinates. Null hides the magnifier. */
  point: Point | null;
  viewport: { width: number; height: number };
  zoom?: number;
  width?: number;
  previewHeight?: number;
};

/**
 * Rectangular pixel inspector with a color readout and screen coordinates.
 */
export function Magnifier({
  source,
  point,
  viewport,
  zoom = 9,
  width = 220,
  previewHeight = 120,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const copiedTimerRef = useRef<number | null>(null);
  const [rgb, setRgb] = useState<[number, number, number] | null>(null);
  const [format, setFormat] = useState<"hex" | "rgb">("hex");
  const [copied, setCopied] = useState(false);
  const panelHeight = previewHeight + 96;
  const colorText = rgb
    ? format === "hex"
      ? `#${rgb.map((value) => value.toString(16).padStart(2, "0")).join("").toUpperCase()}`
      : `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`
    : "";

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !source || !point) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = previewHeight * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, width, previewHeight);

    // Sample physical pixels so the preview matches the exported image.
    const scaleX = source.width / viewport.width;
    const scaleY = source.height / viewport.height;
    const px = point.x * scaleX;
    const py = point.y * scaleY;
    const sampleWidth = width / zoom;
    const sampleHeight = previewHeight / zoom;
    ctx.drawImage(
      source,
      px - sampleWidth / 2,
      py - sampleHeight / 2,
      sampleWidth,
      sampleHeight,
      0,
      0,
      width,
      previewHeight,
    );

    const sample = document.createElement("canvas");
    sample.width = 1;
    sample.height = 1;
    const sampleCtx = sample.getContext("2d");
    if (!sampleCtx) return;
    sampleCtx.imageSmoothingEnabled = false;
    sampleCtx.drawImage(source, px, py, 1, 1, 0, 0, 1, 1);
    const [r, g, b] = sampleCtx.getImageData(0, 0, 1, 1).data;
    setRgb([r, g, b]);
  }, [previewHeight, point, source, viewport.height, viewport.width, width, zoom]);

  useEffect(() => {
    if (!point) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Shift" && !event.repeat) {
        event.preventDefault();
        event.stopImmediatePropagation();
        setFormat((current) => (current === "hex" ? "rgb" : "hex"));
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c" && colorText) {
        event.preventDefault();
        event.stopImmediatePropagation();
        const copy = isTauri()
          ? api.copyText(colorText)
          : navigator.clipboard.writeText(colorText);
        void copy
          .then(() => {
            setCopied(true);
            if (copiedTimerRef.current != null) window.clearTimeout(copiedTimerRef.current);
            copiedTimerRef.current = window.setTimeout(() => setCopied(false), 1200);
          })
          .catch(() => setCopied(false));
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [colorText, point]);

  useEffect(
    () => () => {
      if (copiedTimerRef.current != null) window.clearTimeout(copiedTimerRef.current);
    },
    [],
  );

  if (!point) return null;

  const gap = 18;
  const margin = 8;
  const showLeft = point.x + gap + width + margin > viewport.width;
  const showAbove = point.y + gap + panelHeight + margin > viewport.height;
  const left = clamp(
    showLeft ? point.x - gap - width : point.x + gap,
    margin,
    viewport.width - width - margin,
  );
  const top = clamp(
    showAbove ? point.y - gap - panelHeight : point.y + gap,
    margin,
    viewport.height - panelHeight - margin,
  );

  return (
    <div className="magnifier" style={{ left, top, width }} aria-hidden>
      <div className="magnifier-preview" style={{ height: previewHeight }}>
        <canvas ref={canvasRef} style={{ width, height: previewHeight }} />
        <div
          className="magnifier-grid"
          style={{
            backgroundSize: `${zoom}px ${zoom}px, ${zoom}px ${zoom}px`,
          }}
        />
        <div className="magnifier-crosshair" />
        <div className="magnifier-center" />
      </div>
      <div className="magnifier-info">
        <div className="magnifier-info-row">
          <span className="magnifier-info-label">坐标</span>
          <span className="magnifier-info-value tnum">
            {Math.round(point.x)}, {Math.round(point.y)}
          </span>
        </div>
        <div className="magnifier-info-row">
          <span className="magnifier-swatch" style={{ background: colorText }} />
          <span className="magnifier-color tnum">{colorText}</span>
          {copied && <span className="magnifier-copied">已复制</span>}
        </div>
        <div className="magnifier-shortcut">
          按 <kbd>⌘C</kbd> 复制颜色值
        </div>
        <div className="magnifier-shortcut">
          按 <kbd>Shift</kbd> 切换 RGB/HEX
        </div>
      </div>
    </div>
  );
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}
