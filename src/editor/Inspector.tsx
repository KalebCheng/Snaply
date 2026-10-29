import { PALETTE, type ToolId } from "../types";

type Props = {
  tool: ToolId;
  color: string;
  strokeWidth: number;
  fontSize: number;
  hasSelection: boolean;
  onColor: (c: string) => void;
  onStroke: (n: number) => void;
  onFontSize: (n: number) => void;
  onDelete: () => void;
};

/** Floating property panel for color, stroke width, and font size. */
export function Inspector(props: Props) {
  const { tool, color, strokeWidth, fontSize, hasSelection } = props;
  // Keep style controls visible while the selection tool is active.
  const showStroke = tool !== "text" || hasSelection;
  const showFont = tool === "text" || tool === "select" || hasSelection;

  return (
    <div className="ed-inspector">
      <div className="ed-insp-section">
        <div className="ed-insp-label">颜色</div>
        <div className="ed-swatches">
          {PALETTE.map((c) => (
            <button
              key={c}
              className={`ed-swatch ${color.toLowerCase() === c.toLowerCase() ? "is-active" : ""}`}
              style={{ background: c }}
              onClick={() => props.onColor(c)}
              title={c}
            />
          ))}
        </div>
      </div>

      {showStroke && (
        <div className="ed-insp-section">
          <div className="ed-insp-label">
            粗细
            <span className="ed-insp-value tnum">{strokeWidth}px</span>
          </div>
          <input
            className="ed-range"
            type="range"
            min={1}
            max={24}
            step={1}
            value={strokeWidth}
            onChange={(e) => props.onStroke(Number(e.target.value))}
            style={{ ["--fill" as string]: `${((strokeWidth - 1) / 23) * 100}%` }}
          />
        </div>
      )}

      {showFont && (
        <div className="ed-insp-section">
          <div className="ed-insp-label">
            字号
            <span className="ed-insp-value tnum">{fontSize}px</span>
          </div>
          <div className="ed-stepper">
            <button onClick={() => props.onFontSize(Math.max(10, fontSize - 2))}>−</button>
            <input
              className="ed-range"
              type="range"
              min={10}
              max={96}
              step={2}
              value={fontSize}
              onChange={(e) => props.onFontSize(Number(e.target.value))}
              style={{ ["--fill" as string]: `${((fontSize - 10) / 86) * 100}%` }}
            />
            <button onClick={() => props.onFontSize(Math.min(96, fontSize + 2))}>+</button>
          </div>
        </div>
      )}

      {hasSelection && (
        <button className="ed-danger" onClick={props.onDelete} title="删除所选 (Backspace)">
          删除所选
        </button>
      )}
    </div>
  );
}
