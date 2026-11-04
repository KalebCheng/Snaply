import type { ReactNode } from "react";
import type { ToolId } from "../types";
import type { MainCopy } from "../i18n/main";

type Props = {
  tool: ToolId;
  onTool: (t: ToolId) => void;
  /** A crop is committed, so it can be cleared again. */
  hasCrop?: boolean;
  onResetCrop?: () => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onCopy: () => void;
  onSave: () => void;
  onClose: () => void;
  onDone: () => void;
  labels: MainCopy;
  busy: boolean;
};

/**
 * Floating annotation toolbar.
 * Tools are grouped on the left, followed by history and output actions.
 */
const ANNOTATE_TOOLS: { id: ToolId; hint: string; icon: ReactNode }[] = [
  { id: "select", hint: "V", icon: <IconCursor /> },
  { id: "rect", hint: "R", icon: <IconRect /> },
  { id: "ellipse", hint: "O", icon: <IconCircle /> },
  { id: "arrow", hint: "A", icon: <IconArrow /> },
  { id: "pen", hint: "P", icon: <IconPen /> },
  { id: "blur", hint: "B", icon: <IconMosaic /> },
  { id: "text", hint: "T", icon: <IconText /> },
  { id: "watermark", hint: "W", icon: <IconWatermark /> },
  { id: "crop", hint: "X", icon: <IconCrop /> },
];

export function Toolbar(props: Props) {
  const { tool, onTool, canUndo, canRedo, busy, labels } = props;
  return (
    <div className="ed-toolbar" role="toolbar" aria-label={labels.toolbar}>
      <div className="ed-tools">
        {ANNOTATE_TOOLS.map((t) => (
          <button
            key={t.id}
            className={`ed-tool ${tool === t.id ? "is-active" : ""}`}
            onClick={() => onTool(t.id)}
            title={`${labels.tools[t.id]} (${t.hint})`}
            aria-pressed={tool === t.id}
          >
            {t.icon}
          </button>
        ))}
        {props.hasCrop && props.onResetCrop && (
          <button
            className="ed-tool ed-tool-wide"
            onClick={props.onResetCrop}
            title={labels.restoreCropTitle}
          >
            {labels.restoreCrop}
          </button>
        )}
      </div>

      <span className="ed-divider" />

      <div className="ed-tools">
        <button className="ed-tool" onClick={props.onUndo} disabled={!canUndo} title={`${labels.undo} (⌘Z)`}>
          <IconUndo />
        </button>
        <button className="ed-tool" onClick={props.onRedo} disabled={!canRedo} title={`${labels.redo} (⇧⌘Z)`}>
          <IconRedo />
        </button>
        <button className="ed-tool" onClick={props.onCopy} disabled={busy} title={`${labels.copy} (⌘C)`}>
          <IconCopy />
        </button>
        <button className="ed-tool" onClick={props.onSave} disabled={busy} title={`${labels.save} (⌘S)`}>
          <IconDownload />
        </button>
        <button className="ed-tool is-danger" onClick={props.onClose} disabled={busy} title={`${labels.close} (Esc)`}>
          <IconX />
        </button>
        <button className="ed-tool is-confirm" onClick={props.onDone} disabled={busy} title={`${labels.done} (Enter)`}>
          <IconCheck />
        </button>
      </div>
    </div>
  );
}

const s = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};
const svg = { width: 19, height: 19, viewBox: "0 0 24 24" };

function IconCursor() {
  return (
    <svg {...svg} {...s}>
      <path d="M4 3.5 19.5 10 12.5 12.5 10 19.5Z" />
    </svg>
  );
}
function IconRect() {
  return (
    <svg {...svg} {...s}>
      <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
    </svg>
  );
}
function IconCircle() {
  return (
    <svg {...svg} {...s}>
      <circle cx="12" cy="12" r="8.2" />
    </svg>
  );
}
function IconArrow() {
  return (
    <svg {...svg} {...s}>
      <path d="M5 19 18 6" />
      <path d="M10.5 6H18v7.5" />
    </svg>
  );
}
function IconPen() {
  return (
    <svg {...svg} {...s}>
      <path d="M3.5 20.5c4-1 5.5-3.5 7.5-5.5s4-2 6-4l2.5-2.5" />
      <path d="m15.5 4 4.5 4.5-1.6 1.6-4.5-4.5Z" />
    </svg>
  );
}
function IconMosaic() {
  return (
    <svg {...svg} {...s}>
      <rect x="3.5" y="3.5" width="17" height="17" rx="2.5" />
      <path d="M3.5 8.5h3v-5M9.5 12.5h3v3h-3zM14.5 3.5v4h4M3.5 16.5h4v4M14.5 14.5h4v4" />
    </svg>
  );
}
function IconText() {
  return (
    <svg {...svg} {...s}>
      <path d="M5 6V4.5h14V6" />
      <path d="M12 4.5v15" />
      <path d="M9 19.5h6" />
    </svg>
  );
}
function IconWatermark() {
  return (
    <svg {...svg} {...s}>
      <rect x="3.5" y="3.5" width="17" height="17" rx="2.5" />
      <path d="M8 15.5 15.5 8" />
      <path d="M11 8h4.5v4.5" />
    </svg>
  );
}
function IconCrop() {
  return (
    <svg {...svg} {...s}>
      <path d="M6.5 2.5v15h15" />
      <path d="M2.5 6.5h15v15" />
    </svg>
  );
}
function IconUndo() {
  return (
    <svg {...svg} {...s}>
      <path d="M9 7 4.5 11.5 9 16" />
      <path d="M4.5 11.5H15a4.5 4.5 0 0 1 0 9h-3" />
    </svg>
  );
}
function IconRedo() {
  return (
    <svg {...svg} {...s}>
      <path d="m15 7 4.5 4.5L15 16" />
      <path d="M19.5 11.5H9a4.5 4.5 0 0 0 0 9h3" />
    </svg>
  );
}
function IconCopy() {
  return (
    <svg {...svg} {...s}>
      <rect x="9" y="9" width="12" height="12" rx="2.5" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </svg>
  );
}
function IconDownload() {
  return (
    <svg {...svg} {...s}>
      <path d="M12 3v12" />
      <path d="m7 12 5 5 5-5" />
      <path d="M5 21h14" />
    </svg>
  );
}
function IconX() {
  return (
    <svg {...svg} {...s}>
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  );
}
function IconCheck() {
  return (
    <svg {...svg} {...s} strokeWidth={2.2}>
      <path d="m5 12.5 4.5 4.5L19 7" />
    </svg>
  );
}
