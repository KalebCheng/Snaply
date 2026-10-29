export type ToolId = "select" | "rect" | "ellipse" | "arrow" | "pen" | "text" | "blur" | "watermark" | "crop";

export type Point = { x: number; y: number };

export type Rect = { x: number; y: number; width: number; height: number };

type Base = {
  id: string;
  /** Annotation color as a CSS color string. */
  color: string;
  /** Stroke width in image logical pixels. */
  strokeWidth: number;
};

export type RectShape = Base & { kind: "rect" | "ellipse" } & Rect;

export type ArrowShape = Base & {
  kind: "arrow";
  from: Point;
  to: Point;
};

export type PenShape = Base & {
  kind: "pen";
  points: Point[];
};

export type TextShape = Base & {
  kind: "text";
  x: number;
  y: number;
  text: string;
  fontSize: number;
};

export type BlurShape = Base & {
  kind: "blur";
} & Rect;

/** Tiled watermark repeated diagonally across the active canvas. */
export type WatermarkShape = Base & {
  kind: "watermark";
  text: string;
  fontSize: number;
  opacity: number;
  angle: number;
};

export type Shape = RectShape | ArrowShape | PenShape | TextShape | BlurShape | WatermarkShape;

export type MonitorInfo = {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  pixelWidth: number;
  pixelHeight: number;
  primary: boolean;
};

export type OverlaySnapshot = {
  monitor: MonitorInfo;
  startedAtMs: number;
};

export type WindowHit = {
  title: string;
  appName: string;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Zero marks non-standard surfaces that cannot be snapped to. */
  layer: number;
};

export type CursorLocation = {
  monitorId: number;
  x: number;
  y: number;
};

/** Payload sent to the editor through the `shot-ready` event. */
export type ShotPayload = {
  absolutePath: string;
  fileName: string;
  png: number[];
  mode: "edit" | "copy";
  action: "edit" | "copy" | "save";
  shapes: Shape[];
  color: string;
  strokeWidth: number;
  fontSize: number;
};

export type CreateResult = {
  absolutePath: string;
  fileName: string;
  png: number[];
  mode: "edit" | "copy";
};

export type Settings = {
  shortcut: string;
  saveDir: string | null;
  copyAfterCapture: boolean;
  language: Language;
  dockVisibility: DockVisibility;
};

export const SUPPORTED_LANGUAGES = ["zh-CN", "en", "ru"] as const;

export type Language = (typeof SUPPORTED_LANGUAGES)[number];

export type DockVisibility = "hide" | "followWindow" | "show";

export type SettingsPayload = {
  settings: Settings;
  defaultSaveDir: string;
  effectiveShortcut: string;
};

export type AppInfo = {
  version: string;
  shortcut: string;
  screenPermission: boolean;
  platform: string;
};

export type UpdateInfo = {
  currentVersion: string;
  latestVersion: string | null;
  hasUpdate: boolean;
  releaseUrl: string | null;
  downloadUrl: string | null;
  publishedAt: string | null;
  notes: string | null;
};

export const PALETTE = ["#FF453A", "#FF9F0A", "#30D158", "#3B82F6", "#BF5AF2", "#FFFFFF", "#000000"] as const;
