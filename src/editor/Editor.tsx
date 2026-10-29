import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Point, Rect, Shape, ToolId, WatermarkShape } from "../types";
import { clampRect, normalizeRect, shapeBounds, uid } from "../lib/geometry";
import { drawShapeList } from "../lib/render";
import { api } from "../lib/ipc";
import { composeExport, defaultFileName } from "../lib/export";
import { Toolbar } from "./Toolbar";
import { Inspector } from "./Inspector";
import "./editor.css";

type Props = {
  image: ImageBitmap;
  initialPath: string;
  /** Annotations created in the overlay before opening the editor. */
  initialShapes?: Shape[];
  initialColor?: string;
  initialStroke?: number;
  initialFontSize?: number;
  /** Inline editing mode: fill the viewport and hide the close button. */
  inline?: boolean;
  onCommitted?: (shapes: Shape[]) => void;
  onDone: () => void;
  onClose?: () => void;
};

type Toast = { id: number; text: string; action?: { label: string; run: () => void } };

const DEFAULT_COLOR = "#FF453A";
const DEFAULT_STROKE = 4;

export function Editor({
  image,
  initialPath,
  initialShapes,
  initialColor,
  initialStroke,
  initialFontSize,
  inline,
  onCommitted,
  onDone,
  onClose,
}: Props) {
  const initialWatermark = initialShapes?.find((s): s is WatermarkShape => s.kind === "watermark");
  const [shapes, setShapes] = useState<Shape[]>(initialShapes ?? []);
  const [redoStack, setRedoStack] = useState<Shape[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tool, setTool] = useState<ToolId>("select");
  // Pointer callbacks can close over an older render, so keep the latest tool in a ref.
  const toolRef = useRef<ToolId>(tool);
  toolRef.current = tool;
  const [color, setColor] = useState(initialColor ?? DEFAULT_COLOR);
  const [strokeWidth, setStrokeWidth] = useState(initialStroke ?? DEFAULT_STROKE);
  const [fontSize, setFontSize] = useState(initialFontSize ?? (image.height > 1200 ? 40 : 24));
  // Watermark text, opacity, angle, and size.
  const [watermarkText, setWatermarkText] = useState(initialWatermark?.text ?? "Snaply");
  const [watermarkFontSize, setWatermarkFontSize] = useState(initialWatermark?.fontSize ?? 48);
  const [watermarkOpacity, setWatermarkOpacity] = useState(initialWatermark?.opacity ?? 0.18);
  const [watermarkAngle] = useState(initialWatermark?.angle ?? -30);
  const [cropRect, setCropRect] = useState<Rect | null>(null);
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState<Point>({ x: 0, y: 0 });
  const [cursor, setCursor] = useState<Point | null>(null);
  const [draft, setDraft] = useState<Shape | null>(null);
  const [textDraft, setTextDraft] = useState<{ x: number; y: number; value: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [savedPath, setSavedPath] = useState(initialPath);
  const [stageSize, setStageSize] = useState({ width: window.innerWidth, height: window.innerHeight });
  const stageRef = useRef<HTMLDivElement>(null);
  const baseCanvasRef = useRef<HTMLCanvasElement>(null);
  const artCanvasRef = useRef<HTMLCanvasElement>(null);
  const gesture = useRef<
    | { kind: "shape"; origin: Point }
    | { kind: "move"; id: string; origin: Point; start: Shape }
    | { kind: "crop"; origin: Point }
    | { kind: "pan"; origin: Point; start: Point }
    | null
  >(null);

  const crop = useMemo(() => cropRect, [cropRect]);
  const natural = useMemo(
    () => (crop ? { width: crop.width, height: crop.height } : { width: image.width, height: image.height }),
    [crop, image],
  );

  useEffect(() => {
    const onResize = () => setStageSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  /** Fit the image while leaving room for the toolbar and top bar. */
  const fitZoom = useCallback(() => {
    const padX = 64;
    const padTop = 56;
    const padBottom = 168;
    const zw = (stageSize.width - padX * 2) / natural.width;
    const zh = (stageSize.height - padTop - padBottom) / natural.height;
    return Math.min(1, Math.max(0.05, Math.min(zw, zh)));
  }, [natural, stageSize]);

  useEffect(() => {
    setZoom(fitZoom());
    setOffset({ x: 0, y: 0 });
  }, [fitZoom]);

  // Base image
  useEffect(() => {
    const canvas = baseCanvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(natural.width * zoom * dpr);
    const h = Math.round(natural.height * zoom * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.style.width = `${natural.width * zoom}px`;
    canvas.style.height = `${natural.height * zoom}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, 0, 0);
    ctx.clearRect(0, 0, natural.width, natural.height);
    const src = crop ?? { x: 0, y: 0, width: image.width, height: image.height };
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(image, src.x, src.y, src.width, src.height, 0, 0, natural.width, natural.height);
  }, [image, crop, natural, zoom]);

  // Annotation layer
  useEffect(() => {
    const canvas = artCanvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(natural.width * zoom * dpr);
    canvas.height = Math.round(natural.height * zoom * dpr);
    canvas.style.width = `${natural.width * zoom}px`;
    canvas.style.height = `${natural.height * zoom}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, 0, 0);
    ctx.clearRect(0, 0, natural.width, natural.height);
    const ox = crop?.x ?? 0;
    const oy = crop?.y ?? 0;
    ctx.translate(-ox, -oy);
    const visible = shapes.filter((s) => !isOutside(s, crop));
    drawShapeList(ctx, visible, image, draft, {
      x: ox,
      y: oy,
      width: natural.width,
      height: natural.height,
    });
    if (selectedId) drawSelectionFrame(ctx, visible.find((s) => s.id === selectedId), zoom);
  }, [image, shapes, draft, crop, natural, zoom, selectedId]);

  // Undo and redo
  useEffect(() => {
    onCommitted?.(shapes);
  }, [shapes, onCommitted]);

  const push = (s: Shape) => {
    setShapes((prev) => [...prev, s]);
    setRedoStack([]);
  };
  const undo = useCallback(() => {
    setShapes((prev) => {
      if (!prev.length) return prev;
      const last = prev[prev.length - 1];
      setRedoStack((r) => [...r, last]);
      return prev.slice(0, -1);
    });
    setSelectedId(null);
  }, []);
  const redo = useCallback(() => {
    setRedoStack((prev) => {
      if (!prev.length) return prev;
      const last = prev[prev.length - 1];
      setShapes((s) => [...s, last]);
      return prev.slice(0, -1);
    });
  }, []);
  const removeSelected = useCallback(() => {
    if (!selectedId) return;
    setShapes((prev) => prev.filter((s) => s.id !== selectedId));
    setSelectedId(null);
  }, [selectedId]);

  // Export actions
  const showToast = (text: string, action?: Toast["action"]) => {
    const id = Date.now();
    setToast({ id, text, action });
    window.setTimeout(() => setToast((t) => (t?.id === id ? null : t)), action ? 5200 : 2600);
  };

  const copy = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try {
      const { bytes } = await composeExport(image, shapes, crop);
      await api.copyImage(bytes);
      showToast("已复制到剪贴板");
    } catch (e) {
      showToast(`复制失败：${String(e)}`);
    } finally {
      setBusy(false);
    }
  }, [busy, image, shapes, crop]);

  const save = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try {
      const { bytes } = await composeExport(image, shapes, crop);
      const path = pickSavePath(savedPath);
      await api.saveImage(path, bytes);
      setSavedPath(path);
      showToast("已保存", { label: "在文件夹中显示", run: () => void api.revealInFolder(path) });
    } catch (e) {
      showToast(`保存失败：${String(e)}`);
    } finally {
      setBusy(false);
    }
  }, [busy, image, shapes, crop, savedPath]);

  const copyAndClose = useCallback(async () => {
    await copy();
    onDone();
  }, [copy, onDone]);

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (textDraft) {
        if (e.key === "Escape") setTextDraft(null);
        return;
      }
      if (mod && e.key.toLowerCase() === "c") {
        e.preventDefault();
        void copy();
      } else if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      } else if (mod && !e.shiftKey && e.key.toLowerCase() === "z") {
        e.preventDefault();
        undo();
      } else if (mod && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z"))) {
        e.preventDefault();
        redo();
      } else if (mod && e.key === "0") {
        e.preventDefault();
        setZoom(fitZoom());
        setOffset({ x: 0, y: 0 });
      } else if (e.key === "Escape") {
        if (cropRect) setCropRect(null);
        else if (selectedId) setSelectedId(null);
        else if (tool !== "select") setTool("select");
        else onDone();
      } else if ((e.key === "Backspace" || e.key === "Delete") && selectedId) {
        removeSelected();
      } else if (e.key === "Enter") {
        e.preventDefault();
        void copyAndClose();
      } else if (e.key === "v" || e.key === "V") setTool("select");
      else if (e.key === "r" || e.key === "R") setTool("rect");
      else if (e.key === "o" || e.key === "O") setTool("ellipse");
      else if (e.key === "a" || e.key === "A") setTool("arrow");
      else if (e.key === "p" || e.key === "P") setTool("pen");
      else if (e.key === "t" || e.key === "T") setTool("text");
      else if (e.key === "b" || e.key === "B") setTool("blur");
      else if (e.key === "w" || e.key === "W") setTool("watermark");
      else if (e.key === "x" || e.key === "X") setTool("crop");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [copy, save, undo, redo, fitZoom, selectedId, tool, cropRect, textDraft, removeSelected, copyAndClose, onDone]);

  // Coordinate conversion
  const toImage = (clientX: number, clientY: number): Point => {
    const rect = stageRef.current?.getBoundingClientRect();
    const originX = rect ? rect.left + rect.width / 2 - (natural.width * zoom) / 2 : window.innerWidth / 2;
    const originY = rect ? rect.top + rect.height / 2 - (natural.height * zoom) / 2 : window.innerHeight / 2;
    const x = (clientX - originX) / zoom + (crop?.x ?? 0);
    const y = (clientY - originY) / zoom + (crop?.y ?? 0);
    return { x, y };
  };


  // Pointer interaction
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button === 0 || e.button === 1) e.currentTarget.setPointerCapture(e.pointerId);
    if (e.button === 1 || e.altKey) {
      gesture.current = { kind: "pan", origin: { x: e.clientX, y: e.clientY }, start: offset };
      return;
    }
    if (e.button !== 0) return;
    const p = toImage(e.clientX, e.clientY);

    const active = toolRef.current;
    if (active === "select") {
      const hit = hitShape([...shapes].reverse(), p);
      setSelectedId(hit?.id ?? null);
      if (hit) {
        gesture.current = { kind: "move", id: hit.id, origin: p, start: hit };
      }
      return;
    }
    if (active === "crop") {
      gesture.current = { kind: "crop", origin: p };
      setCropRect({ x: p.x, y: p.y, width: 0, height: 0 });
      return;
    }
    if (active === "text") {
      setTextDraft({ x: p.x, y: p.y, value: "" });
      return;
    }
    gesture.current = { kind: "shape", origin: p };
    setSelectedId(null);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    const p = toImage(e.clientX, e.clientY);
    setCursor(p);
    if (!g) return;
    if (g.kind === "pan") {
      setOffset({ x: g.start.x + (e.clientX - g.origin.x), y: g.start.y + (e.clientY - g.origin.y) });
      return;
    }
    if (g.kind === "crop") {
      const r = normalizeRect(g.origin, p);
      setCropRect(clampTo(r, crop ?? { x: 0, y: 0, width: image.width, height: image.height }));
      return;
    }
    if (g.kind === "move") {
      const dx = p.x - g.origin.x;
      const dy = p.y - g.origin.y;
      const moved = moveShape(g.start, dx, dy);
      setShapes((prev) => prev.map((s) => (s.id === g.id ? moved : s)));
      return;
    }
    // Shape preview while dragging
    const narrowed = makeDraft(toolRef.current, g.origin, p, { color, strokeWidth });
    setDraft(narrowed);
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    const p = toImage(e.clientX, e.clientY);
    if (g.kind === "shape") {
      const s = makeDraft(toolRef.current, g.origin, p, { color, strokeWidth });
      setDraft(null);
      if (s && shapeBigEnough(s)) push(s);
    } else if (g.kind === "crop") {
      setCropRect((r) => {
        if (!r || r.width < 8 || r.height < 8) return crop;
        const next = clampRect(roundRectTo(r), image.width, image.height);
        // Drop annotations fully outside the crop region.
        setShapes((prev) => prev.filter((s) => !isOutside(s, next)));
        setRedoStack([]);
        return next;
      });
    }
  };

  const updateWatermark = (patch: { text?: string; opacity?: number; fontSize?: number }) => {
    setShapes((prev) => prev.map((s) => (s.kind === "watermark" ? { ...s, ...patch } : s)));
  };

  const commitText = () => {
    if (!textDraft) return;
    const text = textDraft.value.trim();
    if (text) {
      push({
        id: uid(),
        kind: "text",
        x: textDraft.x,
        y: textDraft.y,
        text,
        color,
        strokeWidth: 1,
        fontSize,
      });
    }
    setTextDraft(null);
  };

  // Cursor-centered wheel zoom
  const onWheel = (e: React.WheelEvent) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * 0.0022);
    const next = Math.min(6, Math.max(0.05, zoom * factor));
    setZoom(next);
  };

  const textStyle = textDraft
    ? (() => {
        const originX = stageSize.width / 2 + offset.x - (natural.width * zoom) / 2;
        const originY = stageSize.height / 2 + offset.y - (natural.height * zoom) / 2;
        return {
          left: originX + (textDraft.x - (crop?.x ?? 0)) * zoom,
          top: originY + (textDraft.y - (crop?.y ?? 0)) * zoom,
          fontSize: fontSize * zoom,
          color,
        };
      })()
    : null;

  return (
    <div className="editor">
      <div
        ref={stageRef}
        className="ed-stage"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onWheel={onWheel}
        style={{
          cursor:
            gesture.current?.kind === "pan"
              ? "grabbing"
              : tool === "select"
                ? "default"
                : tool === "text"
                  ? "text"
                  : "crosshair",
        }}
      >
        <div
          className="ed-paper"
          style={{ transform: `translate(${offset.x}px, ${offset.y}px)` }}
          data-crop-tool={tool === "crop"}
        >
          <canvas ref={baseCanvasRef} className="ed-layer ed-base" />
          <canvas ref={artCanvasRef} className="ed-layer ed-art" />
          {textDraft && (
            <textarea
              autoFocus
              className="ed-text-input"
              style={textStyle!}
              value={textDraft.value}
              onChange={(e) => setTextDraft({ ...textDraft, value: e.target.value })}
              onBlur={commitText}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setTextDraft(null);
                } else if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  commitText();
                }
              }}
              placeholder="输入文字"
            />
          )}
        </div>

        {cursor && (tool === "rect" || tool === "ellipse" || tool === "arrow" || tool === "pen" || tool === "blur" || tool === "crop") && (
          <div className="ed-coord tnum">
            {Math.round(cursor.x - (crop?.x ?? 0))}, {Math.round(cursor.y - (crop?.y ?? 0))}
          </div>
        )}
      </div>

      {/* Top status bar */}
      <div className="ed-top">
        <div className="ed-dims tnum">
          {Math.round(natural.width)} × {Math.round(natural.height)}
          <span className="ed-dims-sep">·</span>
          {Math.round(zoom * 100)}%
        </div>
        {!inline && (
          <div className="ed-top-actions">
            <button className="ed-ghost" onClick={onClose ?? onDone} title="关闭编辑器 (Esc)">
              关闭
            </button>
          </div>
        )}
      </div>

      <Toolbar
        tool={tool}
        onTool={(t) => {
          setTool(t);
          if (t !== "select") setSelectedId(null);
          if (t !== "crop") setCropRect(null);
          // Watermarks cover the full crop and are configured in the side panel.
          if (t === "watermark") {
            setShapes((prev) => {
              const rest = prev.filter((s) => s.kind !== "watermark");
              return [
                ...rest,
                {
                  id: uid(),
                  kind: "watermark",
                  text: watermarkText,
                  fontSize: watermarkFontSize,
                  opacity: watermarkOpacity,
                  angle: watermarkAngle,
                  color,
                  strokeWidth: 1,
                },
              ];
            });
          }
        }}
        canUndo={shapes.length > 0}
        canRedo={redoStack.length > 0}
        onUndo={undo}
        onRedo={redo}
        onCopy={() => void copy()}
        onSave={() => void save()}
        onClose={() => (onClose ? onClose() : onDone())}
        onDone={() => void copyAndClose()}
        busy={busy}
      />

      {tool === "watermark" && (
        <WatermarkPanel
          text={watermarkText}
          opacity={watermarkOpacity}
          fontSize={watermarkFontSize}
          onText={(v) => {
            setWatermarkText(v);
            updateWatermark({ text: v });
          }}
          onOpacity={(v) => {
            setWatermarkOpacity(v);
            updateWatermark({ opacity: v });
          }}
          onFontSize={(v) => {
            setWatermarkFontSize(v);
            updateWatermark({ fontSize: v });
          }}
        />
      )}

      <Inspector
        tool={tool}
        color={color}
        strokeWidth={strokeWidth}
        fontSize={fontSize}
        hasSelection={selectedId !== null}
        onColor={setColor}
        onStroke={setStrokeWidth}
        onFontSize={setFontSize}
        onDelete={removeSelected}
      />

      {toast && (
        <div className="ed-toast">
          <span>{toast.text}</span>
          {toast.action && (
            <button
              className="ed-toast-action"
              onClick={() => {
                toast.action!.run();
                setToast(null);
              }}
            >
              {toast.action.label}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// Helpers

function makeDraft(tool: ToolId, a: Point, b: Point, style: { color: string; strokeWidth: number }): Shape | null {
  const id = "draft";
  const base = { id, color: style.color, strokeWidth: style.strokeWidth };
  switch (tool) {
    case "rect":
    case "ellipse":
      return { ...base, kind: tool, ...normalizeRect(a, b) };
    case "blur":
      return { ...base, kind: "blur", ...normalizeRect(a, b) };
    case "arrow":
      return { ...base, kind: "arrow", from: a, to: b };
    case "pen":
      return { ...base, kind: "pen", points: [a, b] };
    default:
      return null;
  }
}

function shapeBigEnough(s: Shape) {
  const b = shapeBounds(s);
  if (s.kind === "arrow") return Math.hypot(s.to.x - s.from.x, s.to.y - s.from.y) > 8;
  return b.width > 4 || b.height > 4;
}

function moveShape(s: Shape, dx: number, dy: number): Shape {
  switch (s.kind) {
    case "rect":
    case "ellipse":
    case "blur":
      return { ...s, x: s.x + dx, y: s.y + dy };
    case "arrow":
      return { ...s, from: { x: s.from.x + dx, y: s.from.y + dy }, to: { x: s.to.x + dx, y: s.to.y + dy } };
    case "pen":
      return { ...s, points: s.points.map((p) => ({ x: p.x + dx, y: p.y + dy })) };
    case "text":
      return { ...s, x: s.x + dx, y: s.y + dy };
    case "watermark":
      return s;
  }
}

function hitShape(reversed: Shape[], p: Point): Shape | null {
  for (const s of reversed) {
    const b = shapeBounds(s);
    const pad = Math.max(8, s.strokeWidth * 2);
    if (p.x >= b.x - pad && p.y >= b.y - pad && p.x <= b.x + b.width + pad && p.y <= b.y + b.height + pad) {
      return s;
    }
  }
  return null;
}

function isOutside(s: Shape, crop: Rect | null) {
  if (!crop) return false;
  const b = shapeBounds(s);
  return b.x + b.width < crop.x || b.y + b.height < crop.y || b.x > crop.x + crop.width || b.y > crop.y + crop.height;
}

function drawSelectionFrame(ctx: CanvasRenderingContext2D, s: Shape | undefined, zoom: number) {
  if (!s) return;
  const b = shapeBounds(s);
  const pad = 6 / zoom;
  ctx.save();
  ctx.strokeStyle = "rgba(59,130,246,0.95)";
  ctx.lineWidth = 1 / zoom;
  ctx.setLineDash([4 / zoom, 4 / zoom]);
  ctx.strokeRect(b.x - pad, b.y - pad, b.width + pad * 2, b.height + pad * 2);
  ctx.restore();
}

function clampTo(r: Rect, within: Rect): Rect {
  const x = Math.max(within.x, r.x);
  const y = Math.max(within.y, r.y);
  return {
    x,
    y,
    width: Math.min(r.width, within.x + within.width - x),
    height: Math.min(r.height, within.y + within.height - y),
  };
}

function roundRectTo(r: Rect): Rect {
  return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) };
}

/** Keep the current directory and generate a new timestamped file name. */
function pickSavePath(current: string) {
  const sep = current.includes("\\") ? "\\" : "/";
  const dir = current.slice(0, current.lastIndexOf(sep));
  return `${dir}${sep}${defaultFileName()}`;
}


/** Watermark controls for text, opacity, and size. */
function WatermarkPanel(props: {
  text: string;
  opacity: number;
  fontSize: number;
  onText: (v: string) => void;
  onOpacity: (v: number) => void;
  onFontSize: (v: number) => void;
}) {
  return (
    <div className="ed-inspector ed-watermark">
      <div className="ed-insp-section">
        <div className="ed-insp-label">水印文字</div>
        <input
          className="ed-input"
          value={props.text}
          onChange={(e) => props.onText(e.target.value)}
          placeholder="例如：内部资料"
          spellCheck={false}
        />
      </div>
      <div className="ed-insp-section">
        <div className="ed-insp-label">
          透明度
          <span className="ed-insp-value tnum">{Math.round(props.opacity * 100)}%</span>
        </div>
        <input
          className="ed-range"
          type="range"
          min={4}
          max={80}
          value={Math.round(props.opacity * 100)}
          onChange={(e) => props.onOpacity(Number(e.target.value) / 100)}
          style={{ ["--fill" as string]: `${((props.opacity * 100 - 4) / 76) * 100}%` }}
        />
      </div>
      <div className="ed-insp-section">
        <div className="ed-insp-label">
          字号
          <span className="ed-insp-value tnum">{props.fontSize}px</span>
        </div>
        <input
          className="ed-range"
          type="range"
          min={12}
          max={120}
          step={2}
          value={props.fontSize}
          onChange={(e) => props.onFontSize(Number(e.target.value))}
          style={{ ["--fill" as string]: `${((props.fontSize - 12) / 108) * 100}%` }}
        />
      </div>
    </div>
  );
}
