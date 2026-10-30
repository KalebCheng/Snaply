import { useEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api, isTauri } from "../lib/ipc";
import { decodePng } from "../lib/decode";
import { composeExport } from "../lib/export";
import { drawShapeList } from "../lib/render";
import { normalizeRect, uid } from "../lib/geometry";
import type { Point, Rect, Shape, ToolId, WindowHit } from "../types";
import { Magnifier } from "./Magnifier";
import {
  HANDLE_CURSOR,
  HANDLE_SIZE,
  handlePoints,
  hitHandle,
  moveRect,
  resizeRect,
  roundRect,
  type Handle,
} from "./selection";
import "./overlay.css";

type Drag =
  | { kind: "new"; origin: Point; current: Point }
  | { kind: "move"; origin: Point; start: Rect; current: Rect }
  | { kind: "resize"; handle: Handle; current: Rect }
  | { kind: "text"; origin: Point }
  | { kind: "draw"; tool: ToolId; origin: Point; points: Point[] }
  | { kind: "shapeMove"; id: string; origin: Point; start: Shape }
  | { kind: "crop"; origin: Point; current: Rect };

const TOOLS: { id: ToolId; label: string; hint: string }[] = [
  { id: "select", label: "选择", hint: "V" },
  { id: "rect", label: "矩形", hint: "R" },
  { id: "ellipse", label: "椭圆", hint: "O" },
  { id: "arrow", label: "箭头", hint: "A" },
  { id: "pen", label: "画笔", hint: "P" },
  { id: "blur", label: "马赛克", hint: "B" },
  { id: "text", label: "文字", hint: "T" },
  { id: "watermark", label: "水印", hint: "W" },
  { id: "crop", label: "裁剪", hint: "X" },
];

/** Read the monitor id from the overlay URL. */
function readMonitorId(): number | null {
  const m = new URLSearchParams(window.location.search).get("m");
  return m ? Number(m) : null;
}

/**
 * Full-screen capture overlay with window hit testing, selection, and
 * in-place annotations.
 */
export function Overlay() {
  const monitorId = useState(readMonitorId)[0];
  const win = useState(() => (isTauri() ? getCurrentWebviewWindow() : null))[0];
  const [image, setImage] = useState<ImageBitmap | null>(null);
  const [viewport, setViewport] = useState({ width: window.innerWidth, height: window.innerHeight });
  const [cursor, setCursor] = useState<Point | null>(null);
  const [sel, setSel] = useState<Rect | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [hover, setHover] = useState<WindowHit | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [altDown, setAltDown] = useState(false);
  const [magnifierOn] = useState(true);

  const [mode, setMode] = useState<"select" | "edit">("select");
  const [tool, setTool] = useState<ToolId>("select");
  const [shapes, setShapes] = useState<Shape[]>([]);
  const [draftShape, setDraftShape] = useState<Shape | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [color, setColor] = useState("#FF453A");
  const [strokeWidth, setStrokeWidth] = useState(4);
  const [fontSize, setFontSize] = useState(24);
  const [watermarkText, setWatermarkText] = useState("Snaply");
  const [watermarkFontSize, setWatermarkFontSize] = useState(48);
  const [watermarkOpacity, setWatermarkOpacity] = useState(0.25);
  const [textInput, setTextInput] = useState<{ x: number; y: number; value: string } | null>(null);
  const [copyAfterCapture, setCopyAfterCapture] = useState(false);

  const baseRef = useRef<HTMLCanvasElement>(null);
  const artRef = useRef<HTMLCanvasElement>(null);
  const textInputRef = useRef<HTMLTextAreaElement>(null);
  const presetAtDownRef = useRef<WindowHit | null>(null);
  const commitLockRef = useRef(false);
  const cursorRef = useRef<Point | null>(cursor);
  cursorRef.current = cursor;
  const toolRef = useRef(tool);
  toolRef.current = tool;
  const styleRef = useRef({ color, strokeWidth, fontSize });
  styleRef.current = { color, strokeWidth, fontSize };

  // Lifecycle
  useEffect(() => {
    if (monitorId == null) return setError("缺少显示器参数");
    if (!isTauri()) {
      // Draw a demo image when running outside Tauri.
      const demo = document.createElement("canvas");
      demo.width = 1600;
      demo.height = 1000;
      const g = demo.getContext("2d")!;
      const grad = g.createLinearGradient(0, 0, 1600, 1000);
      grad.addColorStop(0, "#e9eef5");
      grad.addColorStop(1, "#cfd9e6");
      g.fillStyle = grad;
      g.fillRect(0, 0, 1600, 1000);
      g.fillStyle = "#1f2937";
      g.font = "600 56px -apple-system, sans-serif";
      g.fillText("Snaply 选区预览", 80, 140);
      g.fillStyle = "#64748b";
      g.font = "30px -apple-system, sans-serif";
      g.fillText("拖出选区后，这里会出现就地标注工具条。", 80, 200);
      g.fillStyle = "#94a3b8";
      for (let i = 0; i < 5; i++) g.fillRect(80, 280 + i * 120, 1440, 88);
      void createImageBitmap(demo).then(setImage);
      return;
    }
    (async () => {
      try {
        // Settings failures must not block capture.
        void api
          .getSettings()
          .then((settings) => setCopyAfterCapture(settings.settings.copyAfterCapture))
          .catch(() => undefined);
        // Read settings independently while decoding the screenshot.
        setImage(await decodePng(await api.overlayImage(monitorId)));
      } catch (e) {
        setError(String(e));
      }
    })();
  }, [monitorId]);

  useEffect(() => {
    const onResize = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    const un = listen("overlay-refresh", async () => {
      if (monitorId == null) return;
      try {
        resetAll();
        setImage(await decodePng(await api.overlayImage(monitorId)));
      } catch {
        /* The capture session may already be closed. */
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [monitorId]);

  // 未获得焦点的覆盖层可能收不到鼠标事件。每个覆盖层主动轮询全局光标，
  // 这样从 A 屏移动到 B 屏时，两边都能及时更新本地坐标和窗口命中结果。
  useEffect(() => {
    if (!isTauri() || monitorId == null || !win) return;
    let cancelled = false;
    let inFlight = false;
    const sample = async () => {
      if (cancelled || inFlight) return;
      inFlight = true;
      try {
        const pointer = await api.cursorLocation();
        if (cancelled) return;
        if (!pointer || pointer.monitorId !== monitorId) {
          setCursor(null);
          setHover(null);
          return;
        }
        if (
          !Number.isFinite(pointer.x) ||
          !Number.isFinite(pointer.y) ||
          pointer.x < 0 ||
          pointer.y < 0 ||
          pointer.x > viewport.width ||
          pointer.y > viewport.height
        ) {
          return;
        }
        setCursor({ x: pointer.x, y: pointer.y });
      } catch {
        // 截图会话关闭期间忽略轮询失败。
      } finally {
        inFlight = false;
      }
    };
    void sample();
    const timer = window.setInterval(sample, 16);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [monitorId, viewport.height, viewport.width, win]);

  // Base image
  useEffect(() => {
    const canvas = baseRef.current;
    if (!canvas || !image) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(viewport.width * dpr);
    canvas.height = Math.round(viewport.height * dpr);
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, viewport.width, viewport.height);
    ctx.drawImage(image, 0, 0, viewport.width, viewport.height);
  }, [image, viewport]);

  // Annotation layer using physical pixel coordinates.
  useEffect(() => {
    const canvas = artRef.current;
    if (!canvas || !image) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(viewport.width * dpr);
    canvas.height = Math.round(viewport.height * dpr);
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const k = image.width / viewport.width;
    // Annotation points use image pixels, while the canvas uses CSS pixels times DPR.
    ctx.setTransform(dpr / k, 0, 0, dpr / k, 0, 0);
    ctx.clearRect(0, 0, image.width, image.height);
    ctx.save();
    // Clip all annotations to the selected region during editing.
    if (mode === "edit" && sel) {
      ctx.beginPath();
      ctx.rect(sel.x * k, sel.y * k, sel.width * k, sel.height * k);
      ctx.clip();
    }
    const scope =
      mode === "edit" && sel
        ? { x: sel.x * k, y: sel.y * k, width: sel.width * k, height: sel.height * k }
        : undefined;
    drawShapeList(ctx, shapes, image, draftShape, scope);
    const selected = shapes.find((s) => s.id === selectedId);
    if (selected) frameShape(ctx, selected, k);
    ctx.restore();
  }, [image, viewport, shapes, draftShape, selectedId, mode, sel]);

  // 窗口命中测试
  useEffect(() => {
    if (!isTauri() || mode === "edit" || drag || monitorId == null) {
      setHover(null);
      return;
    }
    let cancelled = false;
    let inFlight = false;
    const sample = async () => {
      const point = cursorRef.current;
      if (cancelled || inFlight || !point) return;
      inFlight = true;
      try {
        // 窗口枚举使用相对当前显示器的逻辑坐标。
        const hits = await api.windowsUnderCursor(monitorId, point.x, point.y);
        const next = hits.find((h) => h.layer > 0) ?? null;
        if (!cancelled) {
          setHover((previous) => (sameWindowHit(previous, next) ? previous : next));
        }
      } catch {
        if (!cancelled) setHover(null);
      } finally {
        inFlight = false;
      }
    };
    void sample();
    const timer = window.setInterval(sample, 30);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [drag, monitorId, mode]);

  // Modifier keys
  useEffect(() => {
    const down = (e: KeyboardEvent) => e.key === "Alt" && setAltDown(true);
    const up = (e: KeyboardEvent) => e.key === "Alt" && setAltDown(false);
    const blur = () => setAltDown(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
    };
  }, []);

  const textInputActive = textInput !== null;
  useEffect(() => {
    if (!textInputActive) return;
    const frame = window.requestAnimationFrame(() => {
      const input = textInputRef.current;
      if (!input) return;
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [textInputActive]);

  // Coordinate conversion
  const toImage = (p: Point): Point => {
    const k = image ? image.width / viewport.width : 1;
    return { x: p.x * k, y: p.y * k };
  };

  /** Clamp annotation points to the selected region. */
  const editPoint = (p: Point): Point => {
    if (mode !== "edit" || !sel) return toImage(p);
    return toImage({
      x: Math.min(Math.max(p.x, sel.x), sel.x + sel.width),
      y: Math.min(Math.max(p.y, sel.y), sel.y + sel.height),
    });
  };

  const resetAll = () => {
    setMode("select");
    setSel(null);
    setShapes([]);
    setDraftShape(null);
    setSelectedId(null);
    setHover(null);
    presetAtDownRef.current = null;
    setTool("select");
  };

  const cancel = () => (isTauri() ? void api.cancelCapture().catch(() => win?.close()) : undefined);

  const focusOverlay = () => {
    if (!win) return;
    void win.setFocus().catch(() => undefined);
  };

  const onRootPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    const target = event.target;
    if (target instanceof Element && target.closest(".ov-toolbar, .ov-text-input")) return;
    focusOverlay();
  };

  const leaveOverlay = () => {
    // Tauri 模式下 Rust 的全局光标轮询是唯一数据源。窗口切换焦点时
    // WebView 可能补发一次错误坐标的 leave/move 事件，不能让它覆盖光标。
    if (drag || isTauri()) return;
    setCursor(null);
    setHover(null);
  };

  const closeToIdle = () => {
    if (isTauri()) cancel();
    else resetAll();
  };

  /** Finish editing, copy directly, or save through the main window. */
  const commit = (requested: "edit" | "copy" | "save") => {
    // A highlighted window is a valid target for keyboard actions.
    const target = sel ?? hover;
    if (!target || !monitorId || busy || commitLockRef.current) return;
    const action = requested === "edit" && copyAfterCapture ? "copy" : requested;
    const r = roundRect(target);
    if (r.width < 2 || r.height < 2) return;
    commitLockRef.current = true;
    setBusy(true);

    // Compose and copy before the overlay window is destroyed.
    if (action === "copy") {
      if (!image) {
        commitLockRef.current = false;
        setBusy(false);
        return;
      }
      const k = image.width / viewport.width;
      const crop = {
        x: Math.round(r.x * k),
        y: Math.round(r.y * k),
        width: Math.max(1, Math.round(r.width * k)),
        height: Math.max(1, Math.round(r.height * k)),
      };
      composeExport(image, shapes, crop)
        .then(async ({ bytes }) => {
          await api.copyImage(bytes);
          // Persist the capture after a successful clipboard write.
          void api.overlayCommit(monitorId, r, "copy").catch(() => {
            void api.cancelCapture().catch(() => undefined);
          });
        })
        .catch((e) => {
          commitLockRef.current = false;
          setBusy(false);
          setError(`复制失败：${String(e)}`);
        });
      return;
    }

    const k = image ? image.width / viewport.width : 1;
    const offset = { x: Math.round(r.x * k), y: Math.round(r.y * k) };
    api
      .overlayCommit(monitorId, r, "edit")
      .then((result) => {
        emitTo("main", "shot-ready", {
          ...result,
          mode: "edit",
          action,
          // Shift annotation coordinates into the cropped image space.
          shapes: offsetShapes(shapes, -offset.x, -offset.y),
          color,
          strokeWidth,
          fontSize,
        });
      })
      .catch((e) => {
        commitLockRef.current = false;
        setBusy(false);
        setError(String(e));
      });
  };

  const addWatermark = () => {
    if (shapes.some((s) => s.kind === "watermark")) return;
    setShapes((prev) => [
      ...prev,
      {
        id: uid(),
        kind: "watermark",
        text: watermarkText,
        fontSize: watermarkFontSize,
        opacity: watermarkOpacity,
        angle: -30,
        color: "#6B7280",
        strokeWidth: 1,
      },
    ]);
  };

  const updateWatermark = (patch: { text?: string; fontSize?: number; opacity?: number }) => {
    setShapes((prev) => prev.map((s) => (s.kind === "watermark" ? { ...s, ...patch } : s)));
  };

  // Pointer input
  /** Validate pointer coordinates and clamp them to the viewport. */
  const pointOf = (e: { clientX: number; clientY: number }): Point | null => {
    const { clientX, clientY } = e;
    if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) return null;
    return {
      x: Math.min(Math.max(clientX, 0), viewport.width),
      y: Math.min(Math.max(clientY, 0), viewport.height),
    };
  };

  const enterEdit = (rect: Rect) => {
    setSel(roundRect(rect));
    setMode("edit");
    setTool("select");
    setHover(null);
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // Do not start interaction before the screenshot is decoded.
    if (!image || busy || e.button !== 0 || textInput) return;
    const p = pointOf(e);
    if (!p) return;
    e.currentTarget.setPointerCapture(e.pointerId);

    if (mode === "select") {
      // Use a hovered window for a click, or a free selection when dragging.
      if (sel) {
        const h = hitHandle(sel, p);
        if (h) return setDrag({ kind: "resize", handle: h, current: sel });
        if (p.x > sel.x && p.x < sel.x + sel.width && p.y > sel.y && p.y < sel.y + sel.height) {
          return setDrag({ kind: "move", origin: p, start: sel, current: sel });
        }
      }
      // Capture the hovered window before the pointer moves.
      presetAtDownRef.current = hover;
      setSel(null);
      setDrag({ kind: "new", origin: p, current: p });
      return;
    }

    if (!sel) return;
    if (p.x < sel.x || p.x > sel.x + sel.width || p.y < sel.y || p.y > sel.y + sel.height) return;
    const ip = editPoint(p);
    const active = toolRef.current;
    if (active === "select") {
      const hit = [...shapes].reverse().find((s) => shapeHitTest(s, ip, image));
      setSelectedId(hit?.id ?? null);
      if (hit) setDrag({ kind: "shapeMove", id: hit.id, origin: ip, start: hit });
      return;
    }
    if (active === "text") {
      return setDrag({ kind: "text", origin: p });
    }
    if (active === "crop") {
      setDrag({ kind: "crop", origin: p, current: { x: p.x, y: p.y, width: 0, height: 0 } });
      return;
    }
    setDrag({ kind: "draw", tool: active, origin: ip, points: [ip] });
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const p = pointOf(e);
    if (!p) return;
    if (!isTauri()) setCursor(p);
    if (!drag || !image) return;
    if (drag.kind === "new") {
      setDrag({ ...drag, current: p });
    } else if (drag.kind === "move") {
      setDrag({ ...drag, current: moveRect(drag.start, p.x - drag.origin.x, p.y - drag.origin.y, viewport) });
    } else if (drag.kind === "resize") {
      setDrag({ ...drag, current: resizeRect(drag.current, drag.handle, p, viewport) });
    } else if (drag.kind === "crop") {
      const r = normalizeRect(drag.origin, p);
      setDrag({ ...drag, current: r });
    } else if (drag.kind === "draw") {
      const ip = editPoint(p);
      const points = drag.tool === "pen" ? [...drag.points, ip] : drag.points;
      setDrag({ ...drag, points });
      const next = makeShape(drag.tool, drag.origin, ip, points, styleRef.current);
      setDraftShape(next);
    } else if (drag.kind === "shapeMove") {
      const ip = editPoint(p);
      const box = shapeBox(drag.start);
      let dx = ip.x - drag.origin.x;
      let dy = ip.y - drag.origin.y;
      if (box && sel) {
        const k = image.width / viewport.width;
        const minX = sel.x * k;
        const minY = sel.y * k;
        const maxX = (sel.x + sel.width) * k;
        const maxY = (sel.y + sel.height) * k;
        const dxMin = minX - box.x;
        const dxMax = maxX - box.x - box.width;
        const dyMin = minY - box.y;
        const dyMax = maxY - box.y - box.height;
        dx = dxMax >= dxMin ? Math.min(Math.max(dx, dxMin), dxMax) : 0;
        dy = dyMax >= dyMin ? Math.min(Math.max(dy, dyMin), dyMax) : 0;
      }
      const moved = moveShape(drag.start, dx, dy);
      setShapes((prev) => prev.map((s) => (s.id === drag.id ? moved : s)));
    }
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = drag;
    setDrag(null);
    if (!d) return;
    const p = pointOf(e);
    if (!p) return;
    if (d.kind === "text") {
      setTextInput({ x: d.origin.x, y: d.origin.y, value: "" });
    } else if (d.kind === "new") {
      const moved = Math.hypot(p.x - d.origin.x, p.y - d.origin.y) > 4;
      const preset = presetAtDownRef.current;
      const rect = moved
        ? roundRect(clampSelection(normalizeRect(d.origin, p), viewport))
        : preset
          ? roundRect({ x: preset.x, y: preset.y, width: preset.width, height: preset.height })
          : null;
      presetAtDownRef.current = null;
      if (rect && rect.width >= 4 && rect.height >= 4) {
        enterEdit(rect);
        return;
      }
      // Retry hit testing after mouse-up if the hover state was stale.
      if (!moved && isTauri() && monitorId != null) {
        void api
          .windowsUnderCursor(monitorId, p.x, p.y)
          .then((hits) => {
            const hit = hits.find((h) => h.layer > 0);
            if (hit) enterEdit(hit);
          })
          .catch(() => undefined);
      }
    } else if (d.kind === "move" || d.kind === "resize") {
      setSel(d.current);
    } else if (d.kind === "crop") {
      const r = d.current;
      if (r.width > 8 && r.height > 8) {
        setSel(roundRect(r));
        setShapes((prev) => prev.filter((s) => !outsideCrop(s, r, image)));
      }
      setTool("select");
    } else if (d.kind === "draw") {
      setDraftShape(null);
      const ip = editPoint(p);
      const shape = makeShape(d.tool, d.origin, ip, d.points, styleRef.current);
      if (shape && bigEnough(shape)) setShapes((prev) => [...prev, shape]);
    }
  };

  // Text input
  const commitText = () => {
    if (!textInput) return;
    const text = textInput.value.trim();
    if (text) {
      const ip = editPoint({ x: textInput.x, y: textInput.y });
      setShapes((prev) => [
        ...prev,
        { id: uid(), kind: "text", x: ip.x, y: ip.y, text, color, strokeWidth: 1, fontSize },
      ]);
    }
    setTextInput(null);
  };

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const editingText =
        target?.tagName === "TEXTAREA" ||
        (target?.tagName === "INPUT" && (target as HTMLInputElement).type === "text") ||
        target?.isContentEditable;
      if (textInput || editingText) return;
      const mod = e.metaKey || e.ctrlKey;
      const k = e.key.toLowerCase();
      if (mod && k === "z") {
        e.preventDefault();
        setShapes((prev) => prev.slice(0, -1));
        return;
      }
      if (mod && k === "c" && (mode === "edit" || hover)) {
        e.preventDefault();
        commit("copy");
        return;
      }
      if (mode === "edit" && mod && k === "s") {
        e.preventDefault();
        commit("save");
        return;
      }
      if (e.key === "Enter" && (sel || hover)) {
        e.preventDefault();
        if (mode === "select") {
          if (!sel && hover) setSel(roundRect(hover));
          setMode("edit");
          setTool("select");
          setHover(null);
        } else {
          commit("edit");
        }
        return;
      }
      if (e.key === "Escape") {
        if (mode === "edit" && tool !== "select") setTool("select");
        else if (mode === "edit" && selectedId) setSelectedId(null);
        else if (mode === "edit") resetAll();
        else if (sel) setSel(null);
        else closeToIdle();
        return;
      }
      if (mode === "edit" && (e.key === "Backspace" || e.key === "Delete") && selectedId) {
        setShapes((prev) => prev.filter((s) => s.id !== selectedId));
        setSelectedId(null);
        return;
      }
      if (mode === "edit") {
        const map: Record<string, ToolId> = {
          v: "select",
          r: "rect",
          o: "ellipse",
          a: "arrow",
          p: "pen",
          b: "blur",
          t: "text",
          w: "watermark",
          x: "crop",
        };
        const next = map[k];
        if (next) {
          setTool(next);
          if (next === "watermark") addWatermark();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    mode,
    tool,
    sel,
    selectedId,
    textInput,
    shapes,
    image,
    color,
    strokeWidth,
    fontSize,
    viewport,
    copyAfterCapture,
    hover,
    watermarkText,
    watermarkFontSize,
    watermarkOpacity,
  ]);

  // Some WebViews emit copy without a keydown event, so handle both.
  useEffect(() => {
    const onCopy = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement | null;
      const editingText =
        target?.tagName === "TEXTAREA" ||
        (target?.tagName === "INPUT" && (target as HTMLInputElement).type === "text") ||
        target?.isContentEditable;
      if (textInput || editingText || !(mode === "edit" || hover)) return;
      e.preventDefault();
      commit("copy");
    };
    document.addEventListener("copy", onCopy, true);
    return () => document.removeEventListener("copy", onCopy, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, hover, textInput, sel, shapes, image, copyAfterCapture, viewport, color, strokeWidth, fontSize, watermarkText, watermarkFontSize, watermarkOpacity]);

  useEffect(() => {
    const onContext = (e: MouseEvent) => {
      e.preventDefault();
      if (mode === "edit") resetAll();
      else if (sel) setSel(null);
      else closeToIdle();
    };
    window.addEventListener("contextmenu", onContext);
    return () => window.removeEventListener("contextmenu", onContext);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, sel]);

  if (error) {
    return (
      <div className="ov-error">
        <div className="ov-error-card">
          <div className="ov-error-title">无法截图</div>
          <p>{error}</p>
          <button className="ov-btn ov-btn-primary" onClick={cancel}>
            关闭
          </button>
        </div>
      </div>
    );
  }

  const preset = mode === "select" && !drag ? hover : null;
  const liveSel =
    drag?.kind === "new"
      ? roundRect(normalizeRect(drag.origin, drag.current))
      : drag?.kind === "resize" || drag?.kind === "move"
        ? drag.current
        : sel;
  const cropRect = drag?.kind === "crop" ? drag.current : null;
  const k = image ? image.width / viewport.width : 1;

  return (
    <div
      className="overlay-root"
      data-mode={mode}
      data-ready={!!image}
      onPointerDown={onRootPointerDown}
      onPointerLeave={leaveOverlay}
    >
      <canvas ref={baseRef} className="ov-canvas" />
      <canvas ref={artRef} className="ov-art" />

      {liveSel ? (
        <>
          <div className="ov-shade" style={{ left: 0, top: 0, width: viewport.width, height: Math.max(0, liveSel.y) }} />
          <div
            className="ov-shade"
            style={{
              left: 0,
              top: liveSel.y + liveSel.height,
              width: viewport.width,
              height: Math.max(0, viewport.height - liveSel.y - liveSel.height),
            }}
          />
          <div className="ov-shade" style={{ left: 0, top: liveSel.y, width: Math.max(0, liveSel.x), height: liveSel.height }} />
          <div
            className="ov-shade"
            style={{
              left: liveSel.x + liveSel.width,
              top: liveSel.y,
              width: Math.max(0, viewport.width - liveSel.x - liveSel.width),
              height: liveSel.height,
            }}
          />
        </>
      ) : (
        mode === "select" && <div className="ov-shade is-empty" />
      )}

      {preset && (
        <div
          className="ov-preset"
          style={{
            left: preset.x,
            top: preset.y,
            width: preset.width,
            height: preset.height,
            borderRadius: Math.min(8, Math.min(preset.width, preset.height) / 8),
          }}
        >
          <div
            className="ov-preset-label"
            style={{ top: preset.y < 32 ? 4 : -30 }}
          >
            <span className="ov-preset-app">{preset.appName || "窗口"}</span>
            {preset.title && preset.title !== preset.appName && <span className="ov-preset-title">{preset.title}</span>}
            <span className="tnum ov-preset-size">
              {Math.round(preset.width)} × {Math.round(preset.height)}
            </span>
          </div>
          <span className="ov-preset-corner tl" />
          <span className="ov-preset-corner tr" />
          <span className="ov-preset-corner bl" />
          <span className="ov-preset-corner br" />
        </div>
      )}

      {cropRect && (
        <div className="ov-crop" style={{ left: cropRect.x, top: cropRect.y, width: cropRect.width, height: cropRect.height }} />
      )}

      {liveSel && (
        <div
          className={`ov-selection ${mode === "edit" ? "is-editing" : ""}`}
          style={{ left: liveSel.x, top: liveSel.y, width: liveSel.width, height: liveSel.height }}
        >
          {mode === "select" && (
            <div className="ov-size tnum" style={{ left: 0, top: -30 }}>
              {Math.round(liveSel.width)} × {Math.round(liveSel.height)}
            </div>
          )}
        </div>
      )}

      {mode === "select" &&
        liveSel &&
        !drag &&
        (Object.entries(handlePoints(liveSel)) as [Handle, Point][]).map(([key, pt]) => (
          <div
            key={key}
            className="ov-handle"
            style={{
              left: pt.x - HANDLE_SIZE / 2,
              top: pt.y - HANDLE_SIZE / 2,
              width: HANDLE_SIZE,
              height: HANDLE_SIZE,
              cursor: HANDLE_CURSOR[key],
            }}
          />
        ))}

      <div
        className="ov-input"
        style={{ cursor: mode === "select" ? "none" : cursorFor(mode, tool, cursor, liveSel) }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onDragStart={(e) => e.preventDefault()}
      />

      {mode === "select" && cursor && (
        <div className="ov-cursor" style={{ left: cursor.x, top: cursor.y }}>
          <span className="ov-cursor-ring" />
        </div>
      )}

      {textInput && (
        <textarea
          ref={textInputRef}
          className="ov-text-input"
          style={{ left: textInput.x, top: textInput.y, color, fontSize: Math.max(12, fontSize / k) }}
          value={textInput.value}
          onChange={(e) =>
            setTextInput((current) => (current ? { ...current, value: e.target.value } : current))
          }
          onBlur={commitText}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              setTextInput(null);
            } else if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              commitText();
            }
          }}
          placeholder="输入文字，回车确认"
        />
      )}

      {mode === "select" && !liveSel && (
        <div className="ov-hint">
          <span>拖动选择区域；高亮窗口后可点击标注，或直接按 <kbd>⌘C</kbd> 复制</span>
          <span className="ov-hint-dot" />
          <span>
            <kbd>Esc</kbd> 退出
          </span>
        </div>
      )}

      {magnifierOn && mode === "select" && !busy && (drag || (!sel && cursor)) && image && (
        <Magnifier source={image} point={altDown && sel ? null : cursor} viewport={viewport} />
      )}

      {mode === "edit" && sel && (
        <EditToolbar
          selection={sel}
          viewport={viewport}
          tool={tool}
          onTool={(t) => {
            setTool(t);
            if (t === "watermark") addWatermark();
            if (t !== "select") setSelectedId(null);
          }}
          color={color}
          onColor={setColor}
          strokeWidth={strokeWidth}
          onStroke={setStrokeWidth}
          fontSize={fontSize}
          onFontSize={setFontSize}
          watermarkText={watermarkText}
          watermarkFontSize={watermarkFontSize}
          watermarkOpacity={watermarkOpacity}
          onWatermarkText={(text) => {
            setWatermarkText(text);
            updateWatermark({ text });
          }}
          onWatermarkFontSize={(size) => {
            setWatermarkFontSize(size);
            updateWatermark({ fontSize: size });
          }}
          onWatermarkOpacity={(opacity) => {
            setWatermarkOpacity(opacity);
            updateWatermark({ opacity });
          }}
          hasSelection={!!selectedId}
          busy={busy}
          onUndo={() => setShapes((prev) => prev.slice(0, -1))}
          onRedo={() => undefined}
          onCopy={() => commit("copy")}
          onSave={() => commit("save")}
          onClose={resetAll}
          onDone={() => commit("edit")}
        />
      )}
    </div>
  );
}

function sameWindowHit(a: WindowHit | null, b: WindowHit | null) {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.appName === b.appName &&
    a.title === b.title &&
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height &&
    a.layer === b.layer
  );
}

function cursorFor(mode: "select" | "edit", tool: ToolId, cursor: Point | null, sel: Rect | null) {
  if (mode === "select") {
    if (sel && cursor) {
      const h = hitHandle(sel, cursor);
      if (h) return HANDLE_CURSOR[h];
      if (cursor.x > sel.x && cursor.x < sel.x + sel.width && cursor.y > sel.y && cursor.y < sel.y + sel.height) return "move";
    }
    return "crosshair";
  }
  if (tool === "select") return "default";
  if (tool === "text") return "text";
  return "crosshair";
}

// Shape tools

function makeShape(
  tool: ToolId,
  a: Point,
  b: Point,
  points: Point[],
  style: { color: string; strokeWidth: number },
): Shape | null {
  const base = { id: uid(), color: style.color, strokeWidth: style.strokeWidth };
  switch (tool) {
    case "rect":
    case "ellipse":
    case "blur":
      return { ...base, kind: tool, ...normalizeRect(a, b) };
    case "arrow":
      return { ...base, kind: "arrow", from: a, to: b };
    case "pen":
      return { ...base, kind: "pen", points };
    default:
      return null;
  }
}

function bigEnough(s: Shape) {
  if (s.kind === "arrow") return Math.hypot(s.to.x - s.from.x, s.to.y - s.from.y) > 6;
  if (s.kind === "pen") return s.points.length > 2;
  if (s.kind === "rect" || s.kind === "ellipse" || s.kind === "blur") return s.width > 4 || s.height > 4;
  return true;
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
    default:
      return s;
  }
}

function clampSelection(r: Rect, bounds: { width: number; height: number }): Rect {
  const x = Math.min(Math.max(r.x, 0), bounds.width);
  const y = Math.min(Math.max(r.y, 0), bounds.height);
  return {
    x,
    y,
    width: Math.min(r.width, bounds.width - x),
    height: Math.min(r.height, bounds.height - y),
  };
}

/** Shift annotations from monitor coordinates into cropped-image coordinates. */
function offsetShapes(shapes: Shape[], dx: number, dy: number): Shape[] {
  return shapes.map((shape) => moveShape(shape, dx, dy));
}

function shapeHitTest(s: Shape, p: Point, image: ImageBitmap | null): boolean {
  const k = image ? image.width / window.innerWidth : 1;
  const pad = Math.max(10 * k, s.strokeWidth * 2);
  switch (s.kind) {
    case "rect":
    case "ellipse":
    case "blur":
      return p.x >= s.x - pad && p.y >= s.y - pad && p.x <= s.x + s.width + pad && p.y <= s.y + s.height + pad;
    case "arrow":
      return pointNearSegment(p, s.from, s.to, pad + 4 * k);
    case "pen":
      return s.points.some((q) => Math.hypot(q.x - p.x, q.y - p.y) < pad + 4 * k);
    case "text":
      return (
        p.x >= s.x - pad &&
        p.x <= s.x + s.fontSize * s.text.length &&
        p.y >= s.y - pad &&
        p.y <= s.y + s.fontSize * 1.4
      );
    default:
      return false;
  }
}

function pointNearSegment(p: Point, a: Point, b: Point, tol: number) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy || 1;
  const t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy)) < tol;
}

function outsideCrop(s: Shape, crop: Rect, image: ImageBitmap | null) {
  const k = image ? image.width / window.innerWidth : 1;
  const r = { x: crop.x * k, y: crop.y * k, width: crop.width * k, height: crop.height * k };
  switch (s.kind) {
    case "rect":
    case "ellipse":
    case "blur":
      return s.x + s.width < r.x || s.y + s.height < r.y || s.x > r.x + r.width || s.y > r.y + r.height;
    case "arrow": {
      const x1 = Math.min(s.from.x, s.to.x);
      const y1 = Math.min(s.from.y, s.to.y);
      const x2 = Math.max(s.from.x, s.to.x);
      const y2 = Math.max(s.from.y, s.to.y);
      return x2 < r.x || y2 < r.y || x1 > r.x + r.width || y1 > r.y + r.height;
    }
    case "pen":
      return s.points.every((p) => p.x < r.x || p.y < r.y || p.x > r.x + r.width || p.y > r.y + r.height);
    case "text":
      return (
        s.x + s.fontSize * s.text.length < r.x ||
        s.y + s.fontSize < r.y ||
        s.x > r.x + r.width ||
        s.y > r.y + r.height
      );
    default:
      return false;
  }
}

/** Draw the dashed selection frame around an annotation. */
function frameShape(ctx: CanvasRenderingContext2D, s: Shape, px: number) {
  const pad = 6 * px;
  ctx.save();
  ctx.strokeStyle = "rgba(59,130,246,0.95)";
  ctx.lineWidth = 1.2 * px;
  ctx.setLineDash([5 * px, 4 * px]);
  const box = shapeBox(s);
  if (box) ctx.strokeRect(box.x - pad, box.y - pad, box.width + pad * 2, box.height + pad * 2);
  ctx.restore();
}

function shapeBox(s: Shape): Rect | null {
  switch (s.kind) {
    case "rect":
    case "ellipse":
    case "blur":
      return { x: s.x, y: s.y, width: s.width, height: s.height };
    case "arrow":
      return {
        x: Math.min(s.from.x, s.to.x),
        y: Math.min(s.from.y, s.to.y),
        width: Math.abs(s.to.x - s.from.x),
        height: Math.abs(s.to.y - s.from.y),
      };
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
        y: s.y,
        width: s.fontSize * Math.max(...s.text.split("\n").map((l) => l.length)) * 0.62,
        height: s.fontSize * 1.4,
      };
    case "watermark":
      return { x: 0, y: 0, width: 0, height: 0 };
    default:
      return null;
  }
}

// Toolbar

type EditToolbarProps = {
  selection: Rect;
  viewport: { width: number; height: number };
  tool: ToolId;
  onTool: (t: ToolId) => void;
  color: string;
  onColor: (c: string) => void;
  strokeWidth: number;
  onStroke: (n: number) => void;
  fontSize: number;
  onFontSize: (n: number) => void;
  watermarkText: string;
  watermarkFontSize: number;
  watermarkOpacity: number;
  onWatermarkText: (text: string) => void;
  onWatermarkFontSize: (size: number) => void;
  onWatermarkOpacity: (opacity: number) => void;
  hasSelection: boolean;
  busy: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onCopy: () => void;
  onSave: () => void;
  onClose: () => void;
  onDone: () => void;
};

const PALETTE = ["#FF453A", "#FF9F0A", "#30D158", "#3B82F6", "#BF5AF2", "#FFFFFF", "#000000"];

/** Annotation toolbar with a dedicated style row. */
function EditToolbar(p: EditToolbarProps) {
  const { selection: s, viewport: v } = p;
  // Use the measured toolbar width so it stays inside the viewport.
  const barW = Math.min(548, Math.max(280, v.width - 20));
  const showStyle = p.tool === "rect" || p.tool === "ellipse" || p.tool === "arrow" || p.tool === "pen" || p.tool === "blur";
  const hasExtraRow = showStyle || p.tool === "text" || p.tool === "watermark";
  const barH = hasExtraRow ? 88 : 56;
  const margin = 10;
  const gap = 12;

  // Prefer below the selection, then above, then inside its lower edge.
  const maxLeft = Math.max(margin, v.width - barW - margin);
  const left = Math.round(Math.min(Math.max(s.x + (s.width - barW) / 2, margin), maxLeft));
  const below = s.y + s.height + gap;
  const above = s.y - barH - gap;
  const fitsBelow = below + barH <= v.height - margin;
  const fitsAbove = above >= margin;
  const inside = !fitsBelow && !fitsAbove;
  const top = fitsBelow
    ? below
    : fitsAbove
      ? above
      : Math.min(v.height - barH - margin, Math.max(margin, s.y + s.height - barH - gap));

  return (
    <div
      className="ov-toolbar"
      style={{ left, top, minWidth: barW }}
      data-placement={inside ? "inside" : fitsBelow ? "below" : "above"}
    >
      <div className="ov-toolbar-row">
        {TOOLS.map((t) => (
          <button
            key={t.id}
            className={`ov-tool ${p.tool === t.id ? "is-active" : ""}`}
            title={`${t.label} (${t.hint.toUpperCase()})`}
            onClick={() => p.onTool(t.id)}
          >
            <ToolIcon id={t.id} />
          </button>
        ))}
        <span className="ov-sep" />
        <button className="ov-tool" title="撤销 (⌘Z)" onClick={p.onUndo}>
          <IconUndo />
        </button>
        <button className="ov-tool" title="重做" onClick={p.onRedo} disabled>
          <IconRedo />
        </button>
        <span className="ov-sep" />
        <button className="ov-tool" title="复制 (⌘C)" onClick={p.onCopy} disabled={p.busy}>
          <IconCopy />
        </button>
        <button className="ov-tool" title="保存 (⌘S)" onClick={p.onSave} disabled={p.busy}>
          <IconDownload />
        </button>
        <button className="ov-tool is-close" title="关闭 (Esc)" onClick={p.onClose} disabled={p.busy}>
          <IconX />
        </button>
        <button className="ov-tool is-confirm" title="完成 (Enter)" onClick={p.onDone} disabled={p.busy}>
          <IconCheck />
        </button>
      </div>

      {showStyle && (
        <div className="ov-toolbar-row is-style">
          <div className="ov-swatches">
            {PALETTE.map((c) => (
              <button
                key={c}
                className={`ov-swatch ${p.color.toLowerCase() === c.toLowerCase() ? "is-active" : ""}`}
                style={{ background: c }}
                onClick={() => p.onColor(c)}
                title={c}
              />
            ))}
          </div>
          <input
            className="ov-range"
            type="range"
            min={1}
            max={20}
            value={p.strokeWidth}
            onChange={(e) => p.onStroke(Number(e.target.value))}
            title={`粗细 ${p.strokeWidth}px`}
            style={{ ["--fill" as string]: `${((p.strokeWidth - 1) / 19) * 100}%` }}
          />
        </div>
      )}

      {p.tool === "text" && (
        <div className="ov-toolbar-row is-style">
          <span className="ov-style-label">字号</span>
          <input
            className="ov-range"
            type="range"
            min={10}
            max={96}
            value={p.fontSize}
            onChange={(e) => p.onFontSize(Number(e.target.value))}
            title={`字号 ${p.fontSize}px`}
          />
          <span className="ov-style-value tnum">{p.fontSize}px</span>
        </div>
      )}

      {p.tool === "watermark" && (
        <div className="ov-toolbar-row is-style is-watermark">
          <input
            className="ov-watermark-input"
            value={p.watermarkText}
            onChange={(e) => p.onWatermarkText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                e.currentTarget.blur();
              }
            }}
            placeholder="水印文字"
            spellCheck={false}
          />
          <span className="ov-style-label">字号</span>
          <input
            className="ov-range"
            type="range"
            min={12}
            max={120}
            value={p.watermarkFontSize}
            onChange={(e) => p.onWatermarkFontSize(Number(e.target.value))}
            title={`水印字号 ${p.watermarkFontSize}px`}
          />
          <span className="ov-style-value tnum">{p.watermarkFontSize}px</span>
          <span className="ov-style-label">透明度</span>
          <input
            className="ov-range"
            type="range"
            min={4}
            max={80}
            value={Math.round(p.watermarkOpacity * 100)}
            onChange={(e) => p.onWatermarkOpacity(Number(e.target.value) / 100)}
            title={`水印透明度 ${Math.round(p.watermarkOpacity * 100)}%`}
          />
          <span className="ov-style-value tnum">{Math.round(p.watermarkOpacity * 100)}%</span>
        </div>
      )}
    </div>
  );
}

function ToolIcon({ id }: { id: ToolId }) {
  switch (id) {
    case "select":
      return (
        <svg {...icon}>
          <path d="M4 3.5 19.5 10 12.5 12.5 10 19.5Z" />
        </svg>
      );
    case "rect":
      return (
        <svg {...icon}>
          <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
        </svg>
      );
    case "ellipse":
      return (
        <svg {...icon}>
          <circle cx="12" cy="12" r="8.2" />
        </svg>
      );
    case "arrow":
      return (
        <svg {...icon}>
          <path d="M5 19 18 6" />
          <path d="M10.5 6H18v7.5" />
        </svg>
      );
    case "pen":
      return (
        <svg {...icon}>
          <path d="m14 4.5 1.7-1.7a2 2 0 0 1 2.8 0l2.7 2.7a2 2 0 0 1 0 2.8L19.5 10" />
          <path d="M14 4.5 4 14.5 3 21l6.5-1L19.5 10" />
        </svg>
      );
    case "blur":
      return (
        <svg {...icon}>
          <rect x="3.5" y="3.5" width="17" height="17" rx="2.5" />
          <path d="M3.5 9h3V4M9.5 13h3v3h-3zM14 4v4h4M4 16h4v4" />
        </svg>
      );
    case "text":
      return (
        <svg {...icon}>
          <path d="M5 6V4.5h14V6" />
          <path d="M12 4.5v15" />
          <path d="M9 19.5h6" />
        </svg>
      );
    case "watermark":
      return (
        <svg {...icon}>
          <rect x="3.5" y="3.5" width="17" height="17" rx="2.5" />
          <path d="M8 16 16 8" />
          <path d="M11 8h5v5" />
        </svg>
      );
    case "crop":
      return (
        <svg {...icon}>
          <path d="M6.5 2.5v15h15" />
          <path d="M2.5 6.5h15v15" />
        </svg>
      );
  }
}

const icon = {
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

function IconUndo() {
  return (
    <svg {...icon}>
      <path d="M9 7 4.5 11.5 9 16" />
      <path d="M4.5 11.5H15a4.5 4.5 0 0 1 0 9h-3" />
    </svg>
  );
}
function IconRedo() {
  return (
    <svg {...icon}>
      <path d="m15 7 4.5 4.5L15 16" />
      <path d="M19.5 11.5H9a4.5 4.5 0 0 0 0 9h3" />
    </svg>
  );
}
function IconCopy() {
  return (
    <svg {...icon}>
      <rect x="9" y="9" width="12" height="12" rx="2.5" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </svg>
  );
}
function IconDownload() {
  return (
    <svg {...icon}>
      <path d="M12 3v12" />
      <path d="m7 12 5 5 5-5" />
      <path d="M5 21h14" />
    </svg>
  );
}
function IconX() {
  return (
    <svg {...icon}>
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  );
}
function IconCheck() {
  return (
    <svg {...icon} strokeWidth={2.2}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </svg>
  );
}
