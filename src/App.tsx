import { useEffect, useState } from "react";
import { Overlay } from "./overlay/Overlay";
import { Editor } from "./editor/Editor";
import { api, isTauri } from "./lib/ipc";
import { bitmapFromRgba, decodePng } from "./lib/decode";
import { composeExport } from "./lib/export";
import type { ShotPayload } from "./types";
import "./styles/idle.css";

type Editing = {
  image: ImageBitmap;
  path: string;
  key: number;
  shapes?: ShotPayload["shapes"];
  color: string;
  strokeWidth: number;
  fontSize: number;
};

/** Main window entry. Overlay and settings use independent HTML entries. */
export function App() {
  if (location.pathname.endsWith("/overlay.html")) return <Overlay />;
  return <MainWindow />;
}

/** The hidden main window that presents the editor after a capture. */
function MainWindow() {
  const [editing, setEditing] = useState<Editing | null>(null);

  // Handle overlay confirmation for edit, copy, or save.
  useEffect(() => {
    if (!isTauri()) return;
    let stop = false;
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<ShotPayload>("shot-ready", async ({ payload }) => {
        try {
          // Use the cached cropped RGBA pixels when available.
          const info = await api.imageInfo();
          const img = info
            ? await bitmapFromRgba(await api.imageRgba(), info[0], info[1])
            : await decodePng(new Uint8Array(payload.png));

          if (payload.action === "copy" || payload.action === "save") {
            // Compose annotations when needed; otherwise reuse the backend PNG.
            const bytes =
              payload.shapes.length === 0 && payload.action === "copy"
                ? new Uint8Array(payload.png)
                : (await composeExport(img, payload.shapes)).bytes;
            if (payload.action === "save") {
              await api.saveImage(payload.absolutePath, bytes);
            } else {
              await api.copyImage(bytes);
            }
            await api.hideMain();
            return;
          }

          if (!stop)
            setEditing({
              image: img,
              path: payload.absolutePath,
              key: Date.now(),
              shapes: payload.shapes,
              color: payload.color,
              strokeWidth: payload.strokeWidth,
              fontSize: payload.fontSize,
            });
        } catch {
          /* The capture session may already be closed. */
        }
      }).then((f) => (unlisten = f));
    });
    return () => {
      stop = true;
      unlisten?.();
    };
  }, []);

  if (editing) {
    return (
      <Editor
        key={editing.key}
        image={editing.image}
        initialPath={editing.path}
        initialShapes={editing.shapes}
        initialColor={editing.color}
        initialStroke={editing.strokeWidth}
        initialFontSize={editing.fontSize}
        onDone={() => {
          setEditing(null);
          void api.hideMain();
        }}
      />
    );
  }

  // Idle state shown if the main window is opened explicitly.
  return (
    <div className="app-idle" data-tauri-drag-region>
      <div className="app-idle-card">
        <span className="app-idle-dot" />
        Snaply 正在托盘运行 · 按快捷键或点托盘图标截图
      </div>
    </div>
  );
}
