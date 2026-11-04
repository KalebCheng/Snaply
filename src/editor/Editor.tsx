import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Language, Point, Rect, Shape, ToolId, WatermarkShape } from "../types";
import { localizeError } from "../i18n/errors";
import { mainTranslations, type MainCopy } from "../i18n/main";
import { clampRect, normalizeRect, shapeBounds, uid } from "../lib/geometry";
import { drawShapeList } from "../lib/render";
import { api } from "../lib/ipc";
import { composeExport } from "../lib/export";
import { Toolbar } from "./Toolbar";
import { Inspector } from "./Inspector";
import "./editor.css";

type Props = {
  image: ImageBitmap;
  /** Capture file this editor belongs to; saves overwrite it in place. */
  initialPath: string;
  /** Annotations created in the overlay before opening the editor. */
  initialShapes?: Shape[];
  initialColor?: string;
  initialStroke?: number;
  initialFontSize?: number;
  /** Inline editing mode: fill the viewport and hide the close button. */
  inline?: boolean;
  language: Language;
  onCommitted?: (shapes: Shape[]) => void;
  onDone: () => void;
  onClose?: () => void;
};

type Toast = { id: number; text: string; action?: { label: string; run: () => void } };

/** The part of the document that undo/redo snapshots. */
type EditorState = { shapes: Shape[]; crop: Rect | null };

type History = {
  entries: EditorState[];
  index: number;
  /** Tag and timestamp of the newest commit, used to collapse slider drags. */
  tag: string | null;
  at: number;
};

const DEFAULT_COLOR = "#FF453A";
const DEFAULT_STROKE = 4;
/** Undo depth; snapshots share shape arrays, so this stays cheap. */
const MAX_HISTORY = 120;
/** Commits carrying the same tag inside this window replace the previous entry. */
const COALESCE_MS = 700;
/** Crop drags smaller than this are treated as a click. */
const MIN_CROP_SIZE = 8;

export function Editor({
  image,
  initialPath,
  initialShapes,
  initialColor,
  initialStroke,
  initialFontSize,
  inline,
  language,
  onCommitted,
  onDone,
  onClose,
}: Props) {
  const t = mainTranslations[language];
  const initialWatermark = initialShapes?.find((s): s is WatermarkShape => s.kind === "watermark");
  // Single source of truth for the document. Visual state (tool, selection, zoom,
  // pan, drafts) deliberately stays out of the history.
  const [history, setHistory] = useState<History>(() => ({
    entries: [{ shapes: initialShapes ?? [], crop: null }],
    index: 0,
    tag: null,
    at: 0,
  }));
  const state = history.entries[history.index];
  const shapes = state.shapes;
  const crop = state.crop;

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tool, setTool] = useState<ToolId>("select");
  // Pointer callbacks can close over an older render, so keep the latest tool in a ref.
  const toolRef = useRef<ToolId>(tool);
  toolRef.current = tool;
  const [color, setColor] = useState(initialColor ?? DEFAULT_COLOR);
  const [strokeWidth, setStrokeWidth] = useState(initialStroke ?? DEFAULT_STROKE);
  const [fontSize, setFontSize] = useState(initialFontSize ?? (image.height > 1200 ? 40 : 24));
  // Watermark text, opacity, and size. The angle has no control, so it stays fixed.
  const watermarkAngle = initialWatermark?.angle ?? -30;
  const [watermarkText, setWatermarkText] = useState(initialWatermark?.text ?? "Snaply");
  const [watermarkFontSize, setWatermarkFontSize] = useState(initialWatermark?.fontSize ?? 48);
  const [watermarkOpacity, setWatermarkOpacity] = useState(initialWatermark?.opacity ?? 0.18);
  // Pending crop drag. It never touches the committed crop until pointerup.
  const [cropDraft, setCropDraft] = useState<Rect | null>(null);
  // Live preview of a shape being dragged; committed once on pointerup.
  const [moveDraft, setMoveDraft] = useState<Shape | null>(null);
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState<Point>({ x: 0, y: 0 });
  const [cursor, setCursor] = useState<Point | null>(null);
  const [draft, setDraft] = useState<Shape | null>(null);
  const [textDraft, setTextDraft] = useState<
    { x: number; y: number; value: string; editingId?: string } | null
  >(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [stageSize, setStageSize] = useState({ width: window.innerWidth, height: window.innerHeight });
  const stageRef = useRef<HTMLDivElement>(null);
  const baseCanvasRef = useRef<HTMLCanvasElement>(null);
  const artCanvasRef = useRef<HTMLCanvasElement>(null);
  const composingRef = useRef(false);
  const toastTimer = useRef<number | null>(null);
  const retryRef = useRef<() => void>(() => {});
  const gesture = useRef<
    | { kind: "shape"; origin: Point }
    | { kind: "move"; id: string; origin: Point; start: Shape }
    | { kind: "crop"; origin: Point }
    | { kind: "text"; origin: Point; editingId?: string }
    | { kind: "pan"; origin: Point; start: Point }
    | null
  >(null);

  /** Real image-space bounds of the whole screenshot. Never derived from the crop. */
  const imageBounds = useMemo(
    (): Rect => ({ x: 0, y: 0, width: image.width, height: image.height }),
    [image],
  );
  /** Size of the visible (committed) image; a crop drag must not affect it. */
  const natural = useMemo(() => {
    const width = crop ? crop.width : image.width;
    const height = crop ? crop.height : image.height;
    return { width: Math.max(1, width), height: Math.max(1, height) };
  }, [crop, image.height, image.width]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

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
    const w = Math.max(1, Math.round(natural.width * zoom * dpr));
    const h = Math.max(1, Math.round(natural.height * zoom * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.style.width = `${natural.width * zoom}px`;
    canvas.style.height = `${natural.height * zoom}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, 0, 0);
    ctx.clearRect(0, 0, natural.width, natural.height);
    const src = crop ?? imageBounds;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(image, src.x, src.y, src.width, src.height, 0, 0, natural.width, natural.height);
  }, [image, imageBounds, crop, natural, zoom]);

  // Annotation layer
  useEffect(() => {
    const canvas = artCanvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(natural.width * zoom * dpr));
    const h = Math.max(1, Math.round(natural.height * zoom * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    canvas.style.width = `${natural.width * zoom}px`;
    canvas.style.height = `${natural.height * zoom}px`;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, 0, 0);
    ctx.clearRect(0, 0, natural.width, natural.height);
    const ox = crop?.x ?? 0;
    const oy = crop?.y ?? 0;
    ctx.translate(-ox, -oy);
    const displayed = moveDraft
      ? shapes.map((s) => (s.id === moveDraft.id ? moveDraft : s))
      : shapes;
    const visible = displayed.filter((s) => s.id !== textDraft?.editingId && !isOutside(s, crop));
    drawShapeList(ctx, visible, image, draft, {
      x: ox,
      y: oy,
      width: natural.width,
      height: natural.height,
    });
    if (selectedId) drawSelectionFrame(ctx, visible.find((s) => s.id === selectedId), zoom);
    // Live crop drag: dim everything outside the pending rectangle.
    if (cropDraft) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(imageBounds.x, imageBounds.y, imageBounds.width, imageBounds.height);
      ctx.rect(cropDraft.x, cropDraft.y, cropDraft.width, cropDraft.height);
      ctx.fillStyle = "rgba(0, 0, 0, 0.45)";
      ctx.fill("evenodd");
      ctx.strokeStyle = "rgba(59, 130, 246, 0.95)";
      ctx.lineWidth = 1 / zoom;
      ctx.setLineDash([4 / zoom, 4 / zoom]);
      ctx.strokeRect(cropDraft.x, cropDraft.y, cropDraft.width, cropDraft.height);
      ctx.restore();
    }
  }, [
    image,
    imageBounds,
    shapes,
    moveDraft,
    draft,
    crop,
    cropDraft,
    natural,
    zoom,
    selectedId,
    textDraft?.editingId,
  ]);

  useEffect(() => {
    onCommitted?.(shapes);
  }, [shapes, onCommitted]);

  // History: every document mutation is a snapshot commit.
  const commitState = useCallback((next: EditorState, tag: string | null = null) => {
    const at = Date.now();
    setHistory((h) => {
      const coalesce = tag !== null && h.tag === tag && h.index > 0 && at - h.at <= COALESCE_MS;
      // Any new edit after an undo drops the redo branch.
      const kept = coalesce ? h.entries.slice(0, h.index) : h.entries.slice(0, h.index + 1);
      let entries = [...kept, next];
      if (entries.length > MAX_HISTORY) entries = entries.slice(entries.length - MAX_HISTORY);
      return { entries, index: entries.length - 1, tag, at };
    });
  }, []);

  const undo = useCallback(() => {
    setHistory((h) => (h.index > 0 ? { ...h, index: h.index - 1, tag: null } : h));
    setSelectedId(null);
    setCropDraft(null);
    setMoveDraft(null);
  }, []);

  const redo = useCallback(() => {
    setHistory((h) =>
      h.index < h.entries.length - 1 ? { ...h, index: h.index + 1, tag: null } : h,
    );
    setSelectedId(null);
    setCropDraft(null);
    setMoveDraft(null);
  }, []);

  /** Commit new shapes while keeping the current (committed) crop. */
  const commitShapes = (next: Shape[], tag: string | null = null) =>
    commitState({ shapes: next, crop }, tag);

  /**
   * Tool switching is visual state only: a committed crop is document state, so
   * it survives tool changes. Pressing X again (or the "还原" button) removes it.
   */
  const selectTool = (next: ToolId) => {
    const previous = toolRef.current;
    setTool(next);
    setCropDraft(null);
    setMoveDraft(null);
    setDraft(null);
    if (next !== "select") setSelectedId(null);
    if (next === "crop" && previous === "crop") {
      if (crop) commitState({ shapes, crop: null });
      return;
    }
    // Watermarks cover the full crop and are configured in the side panel.
    if (next === "watermark" && !shapes.some((s) => s.kind === "watermark")) {
      commitState({
        shapes: [
          ...shapes,
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
        ],
        crop,
      });
    }
  };

  const removeSelected = useCallback(() => {
    if (!selectedId) return;
    commitState({ shapes: shapes.filter((s) => s.id !== selectedId), crop });
    setSelectedId(null);
  }, [selectedId, shapes, crop, commitState]);

  const updateSelectedColor = (next: string) => {
    setColor(next);
    const target = shapes.find((s) => s.id === selectedId);
    if (!target || target.color === next) return;
    commitShapes(
      shapes.map((shape) => (shape.id === selectedId ? { ...shape, color: next } : shape)),
      `color:${selectedId}`,
    );
  };

  const updateSelectedStroke = (next: number) => {
    setStrokeWidth(next);
    const target = shapes.find((s) => s.id === selectedId);
    if (!target || target.strokeWidth === next) return;
    commitShapes(
      shapes.map((shape) => (shape.id === selectedId ? { ...shape, strokeWidth: next } : shape)),
      `stroke:${selectedId}`,
    );
  };

  const updateSelectedFontSize = (next: number) => {
    setFontSize(next);
    const target = shapes.find((s) => s.id === selectedId);
    if (!target || target.kind !== "text" || target.fontSize === next) return;
    commitShapes(
      shapes.map((shape) =>
        shape.id === selectedId && shape.kind === "text" ? { ...shape, fontSize: next } : shape,
      ),
      `font:${selectedId}`,
    );
  };

  // Export actions
  const showToast = useCallback((text: string, action?: Toast["action"]) => {
    const id = Date.now();
    setToast({ id, text, action });
    if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(
      () => {
        toastTimer.current = null;
        setToast((t) => (t?.id === id ? null : t));
      },
      action ? 5200 : 2600,
    );
  }, []);

  useEffect(
    () => () => {
      if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    },
    [],
  );

  const copy = useCallback(async (): Promise<boolean> => {
    if (busy) return false;
    setBusy(true);
    try {
      const { bytes } = await composeExport(image, shapes, crop);
      await api.copyImage(bytes);
      showToast(t.copied);
      return true;
    } catch (e) {
      // Keep the editor (and its annotations) open so the copy can be retried.
      showToast(`${t.copyFailed}: ${localizeError(e, language)}`, {
        label: t.retry,
        run: () => retryRef.current(),
      });
      return false;
    } finally {
      setBusy(false);
    }
  }, [busy, image, shapes, crop, showToast, t, language]);

  const save = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try {
      const { bytes } = await composeExport(image, shapes, crop);
      const path = await api.saveImage(bytes, initialPath);
      showToast(t.saved, { label: t.revealInFolder, run: () => void api.revealInFolder(path) });
    } catch (e) {
      showToast(`${t.saveFailed}: ${localizeError(e, language)}`);
    } finally {
      setBusy(false);
    }
  }, [busy, image, shapes, crop, initialPath, showToast, t, language]);

  const copyAndClose = useCallback(async () => {
    const ok = await copy();
    if (ok) onDone();
  }, [copy, onDone]);

  retryRef.current = () => void copyAndClose();

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName ?? "";
      // Never hijack keys typed into an input, slider, or contenteditable element.
      if (target?.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
        return;
      }
      // IME composition: the candidate window owns Enter/Escape.
      if (e.isComposing || e.keyCode === 229) return;
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
        e.preventDefault();
        if (cropDraft) {
          // Cancel only the in-flight crop drag; the committed crop is untouched.
          gesture.current = null;
          setCropDraft(null);
        } else if (selectedId) setSelectedId(null);
        else if (tool !== "select") setTool("select");
        else onDone();
      } else if ((e.key === "Backspace" || e.key === "Delete") && selectedId) {
        removeSelected();
      } else if (e.key === "Enter") {
        // A focused toolbar button already activates itself on Enter.
        if (tag !== "BUTTON") {
          e.preventDefault();
          void copyAndClose();
        }
      } else if (!mod && !e.altKey) {
        // Bare letter shortcuts must not fire for ⌘R / ⌘V and friends.
        const key = e.key.toLowerCase();
        if (key === "v") selectTool("select");
        else if (key === "r") selectTool("rect");
        else if (key === "o") selectTool("ellipse");
        else if (key === "a") selectTool("arrow");
        else if (key === "p") selectTool("pen");
        else if (key === "t") selectTool("text");
        else if (key === "b") selectTool("blur");
        else if (key === "w") selectTool("watermark");
        else if (key === "x") selectTool("crop");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    copy,
    copyAndClose,
    cropDraft,
    fitZoom,
    onDone,
    redo,
    removeSelected,
    save,
    selectedId,
    selectTool,
    textDraft,
    tool,
    undo,
  ]);

  // Coordinate conversion: one conversion pair shared by drawing, hit testing,
  // the coordinate readout, and the inline text editor.
  const stageOrigin = useCallback((): Point => {
    const rect = stageRef.current?.getBoundingClientRect();
    const left = rect ? rect.left : 0;
    const top = rect ? rect.top : 0;
    const width = rect ? rect.width : stageSize.width;
    const height = rect ? rect.height : stageSize.height;
    return {
      x: left + width / 2 + offset.x - (natural.width * zoom) / 2,
      y: top + height / 2 + offset.y - (natural.height * zoom) / 2,
    };
  }, [natural, offset, stageSize, zoom]);

  /** Client (viewport) coordinates to image coordinates. */
  const stageToImage = useCallback(
    (clientX: number, clientY: number): Point => {
      const origin = stageOrigin();
      return {
        x: (clientX - origin.x) / zoom + (crop?.x ?? 0),
        y: (clientY - origin.y) / zoom + (crop?.y ?? 0),
      };
    },
    [crop, stageOrigin, zoom],
  );

  /** Image coordinates to client (viewport) coordinates. */
  const imageToStage = useCallback(
    (x: number, y: number): Point => {
      const origin = stageOrigin();
      return {
        x: origin.x + (x - (crop?.x ?? 0)) * zoom,
        y: origin.y + (y - (crop?.y ?? 0)) * zoom,
      };
    },
    [crop, stageOrigin, zoom],
  );

  // Pointer interaction
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // Let the active textarea blur and commit instead of starting a new annotation.
    if (textDraft) return;
    if (e.button === 0 || e.button === 1) e.currentTarget.setPointerCapture(e.pointerId);
    if (e.button === 1 || e.altKey) {
      gesture.current = { kind: "pan", origin: { x: e.clientX, y: e.clientY }, start: offset };
      return;
    }
    if (e.button !== 0) return;
    const p = stageToImage(e.clientX, e.clientY);

    const active = toolRef.current;
    if (active === "select") {
      const hit = hitShape([...shapes].reverse(), p);
      setSelectedId(hit?.id ?? null);
      if (hit) {
        setColor(hit.color);
        setStrokeWidth(hit.strokeWidth);
        if (hit.kind === "text") {
          setFontSize(hit.fontSize);
          gesture.current = { kind: "text", origin: p, editingId: hit.id };
        } else {
          gesture.current = { kind: "move", id: hit.id, origin: p, start: hit };
        }
      }
      return;
    }
    if (active === "crop") {
      // Only a draft: the committed crop is updated on pointerup.
      gesture.current = { kind: "crop", origin: p };
      setCropDraft({ x: p.x, y: p.y, width: 0, height: 0 });
      return;
    }
    if (active === "text") {
      gesture.current = { kind: "text", origin: p };
      return;
    }
    gesture.current = { kind: "shape", origin: p };
    setSelectedId(null);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    const p = stageToImage(e.clientX, e.clientY);
    setCursor(p);
    if (!g) return;
    if (g.kind === "pan") {
      setOffset({ x: g.start.x + (e.clientX - g.origin.x), y: g.start.y + (e.clientY - g.origin.y) });
      return;
    }
    if (g.kind === "crop") {
      // The drag rectangle is clamped to the whole screenshot, never to itself.
      setCropDraft(clampTo(normalizeRect(g.origin, p), imageBounds));
      return;
    }
    if (g.kind === "move") {
      setMoveDraft(moveShape(g.start, p.x - g.origin.x, p.y - g.origin.y));
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
    const p = stageToImage(e.clientX, e.clientY);
    if (g.kind === "shape") {
      const s = makeDraft(toolRef.current, g.origin, p, { color, strokeWidth });
      setDraft(null);
      if (s && shapeBigEnough(s)) commitShapes([...shapes, s]);
    } else if (g.kind === "text") {
      const shape = g.editingId ? shapes.find((item) => item.id === g.editingId) : null;
      setTextDraft(
        shape?.kind === "text"
          ? { x: shape.x, y: shape.y, value: shape.text, editingId: shape.id }
          : { x: g.origin.x, y: g.origin.y, value: "" },
      );
    } else if (g.kind === "move") {
      setMoveDraft(null);
      const dx = p.x - g.origin.x;
      const dy = p.y - g.origin.y;
      if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
        const moved = moveShape(g.start, dx, dy);
        commitShapes(shapes.map((s) => (s.id === g.id ? moved : s)));
      }
    } else if (g.kind === "crop") {
      setCropDraft(null);
      const r = clampTo(normalizeRect(g.origin, p), imageBounds);
      if (r.width < MIN_CROP_SIZE || r.height < MIN_CROP_SIZE) return;
      const next = clampRect(roundRectTo(r), image.width, image.height);
      // Annotations fully outside the new crop leave the document; snapshotting
      // crop and shapes together makes the whole step undoable.
      commitState({ shapes: shapes.filter((s) => !isOutside(s, next)), crop: next });
    }
  };

  const onPointerCancel = () => {
    gesture.current = null;
    setDraft(null);
    setCropDraft(null);
    setMoveDraft(null);
  };

  const updateWatermark = (patch: { text?: string; opacity?: number; fontSize?: number }) => {
    if (!shapes.some((s) => s.kind === "watermark")) return;
    commitShapes(
      shapes.map((s) => (s.kind === "watermark" ? { ...s, ...patch } : s)),
      "watermark",
    );
  };

  const commitText = () => {
    if (!textDraft) return;
    const text = textDraft.value.trim();
    if (textDraft.editingId) {
      const editingId = textDraft.editingId;
      if (!text) {
        commitState({ shapes: shapes.filter((shape) => shape.id !== editingId), crop });
        setSelectedId(null);
      } else {
        commitShapes(
          shapes.map((shape) =>
            shape.id === editingId && shape.kind === "text"
              ? { ...shape, text, color, strokeWidth: 1, fontSize }
              : shape,
          ),
        );
        setSelectedId(editingId);
      }
    } else if (text) {
      const id = uid();
      commitShapes([
        ...shapes,
        { id, kind: "text", x: textDraft.x, y: textDraft.y, text, color, strokeWidth: 1, fontSize },
      ]);
      setSelectedId(id);
    }
    setTextDraft(null);
    setTool("select");
  };

  // Cursor-centered wheel zoom: keep the image point under the cursor fixed.
  const onWheel = (e: React.WheelEvent) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const factor = Math.exp(-e.deltaY * 0.0022);
    const next = Math.min(6, Math.max(0.05, zoom * factor));
    if (next === zoom) return;
    const rect = stageRef.current?.getBoundingClientRect();
    const centerX = (rect ? rect.left + rect.width / 2 : stageSize.width / 2);
    const centerY = (rect ? rect.top + rect.height / 2 : stageSize.height / 2);
    const k = next / zoom;
    const origin = stageOrigin();
    setOffset({
      x: e.clientX - (e.clientX - origin.x) * k - centerX + (natural.width * next) / 2,
      y: e.clientY - (e.clientY - origin.y) * k - centerY + (natural.height * next) / 2,
    });
    setZoom(next);
  };

  // The inline editor is positioned inside `.ed-paper`, so it needs paper-local
  // pixels: the same conversion, minus the paper's client origin.
  const textStyle = (() => {
    if (!textDraft) return null;
    const origin = stageOrigin();
    const position = imageToStage(textDraft.x, textDraft.y);
    return {
      left: position.x - origin.x,
      top: position.y - origin.y,
      fontSize: fontSize * zoom,
      color,
    };
  })();

  return (
    <div className="editor">
      <div
        ref={stageRef}
        className="ed-stage"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerCancel}
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
              onChange={(e) => {
                const value = e.target.value;
                setTextDraft((current) => (current ? { ...current, value } : current));
              }}
              onCompositionStart={() => {
                composingRef.current = true;
              }}
              onCompositionEnd={() => {
                composingRef.current = false;
              }}
              onBlur={commitText}
              onKeyDown={(e) => {
                const native = e.nativeEvent;
                // IME candidate confirmation also dispatches Enter/Escape.
                if (native.isComposing || composingRef.current || native.keyCode === 229) return;
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setTextDraft(null);
                } else if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  commitText();
                }
              }}
              placeholder={t.textPlaceholder}
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
            <button className="ed-ghost" onClick={onClose ?? onDone} title={t.closeEditor}>
              {t.close}
            </button>
          </div>
        )}
      </div>

      <Toolbar
        tool={tool}
        onTool={selectTool}
        hasCrop={crop !== null}
        onResetCrop={() => {
          if (crop) commitState({ shapes, crop: null });
        }}
        canUndo={history.index > 0}
        canRedo={history.index < history.entries.length - 1}
        onUndo={undo}
        onRedo={redo}
        onCopy={() => void copy()}
        onSave={() => void save()}
        onClose={() => (onClose ? onClose() : onDone())}
        onDone={() => void copyAndClose()}
        labels={t}
        busy={busy}
      />

      {/* Right-hand column: property inspector plus the watermark controls. */}
      <div className="ed-side">
        <Inspector
          tool={tool}
          color={color}
          strokeWidth={strokeWidth}
          fontSize={fontSize}
          hasSelection={selectedId !== null}
          onColor={updateSelectedColor}
          onStroke={updateSelectedStroke}
          onFontSize={updateSelectedFontSize}
          onDelete={removeSelected}
          labels={t}
        />

        {tool === "watermark" && (
          <WatermarkPanel
            copy={t}
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
      </div>

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
  // Whole-image overlays are drawn inside the crop scope, so they are never
  // outside it (and dropping them would desync the preview from the export).
  if (s.kind === "watermark") return false;
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

/** Clamp a drag rectangle to `within`; the caller always passes the full image. */
function clampTo(r: Rect, within: Rect): Rect {
  const x = Math.max(within.x, r.x);
  const y = Math.max(within.y, r.y);
  return {
    x,
    y,
    width: Math.max(0, Math.min(r.width, within.x + within.width - x)),
    height: Math.max(0, Math.min(r.height, within.y + within.height - y)),
  };
}

function roundRectTo(r: Rect): Rect {
  return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) };
}

/** Watermark controls for text, opacity, and size. */
function WatermarkPanel(props: {
  copy: MainCopy;
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
        <div className="ed-insp-label">{props.copy.watermarkText}</div>
        <input
          className="ed-input"
          value={props.text}
          onChange={(e) => props.onText(e.target.value)}
          placeholder={props.copy.watermarkPlaceholder}
          spellCheck={false}
        />
      </div>
      <div className="ed-insp-section">
        <div className="ed-insp-label">
          {props.copy.opacity}
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
          {props.copy.fontSize}
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
