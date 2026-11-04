import { useEffect, useRef, useState } from "react";
import { Editor } from "./editor/Editor";
import { localizeError } from "./i18n/errors";
import { mainTranslations } from "./i18n/main";
import { api, isTauri } from "./lib/ipc";
import { bitmapFromRgba } from "./lib/decode";
import { composeExport } from "./lib/export";
import type { Language, ShotPayload } from "./types";
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

/** Main window entry. */
export function App() {
  return <MainWindow />;
}

/** The hidden main window that presents the editor after a capture. */
function MainWindow() {
  const [editing, setEditing] = useState<Editing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [language, setLanguage] = useState<Language>("zh-CN");
  const lastShotRef = useRef(0);
  const languageRef = useRef<Language>("zh-CN");
  const copy = mainTranslations[language];

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  useEffect(() => {
    if (!isTauri()) return;
    void api
      .getSettings()
      .then(({ settings }) => {
        languageRef.current = settings.language;
        setLanguage(settings.language);
      })
      .catch(() => undefined);
  }, []);

  // Handle overlay confirmation for edit, copy, or save.
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const handle = async (payload: ShotPayload) => {
      // `shot-ready` is retried by the overlay until this window exists; ignore
      // duplicate deliveries of the same capture session.
      if (payload.shotId && payload.shotId === lastShotRef.current) return;
      if (payload.shotId) lastShotRef.current = payload.shotId;
      setError(null);
      const settings = await api.getSettings().catch(() => null);
      if (settings) {
        languageRef.current = settings.settings.language;
        setLanguage(settings.settings.language);
      }
      const text = mainTranslations[languageRef.current];

      const info = await api.imageInfo();
      if (!info) {
        throw new Error(text.captureExpired);
      }
      const image = await bitmapFromRgba(await api.imageRgba(), info[0], info[1]);

      if (payload.action === "copy" || payload.action === "save") {
        // Re-render at physical resolution: the file on disk holds the raw crop.
        const { bytes } = await composeExport(image, payload.shapes);
        if (payload.action === "save") {
          await api.saveImage(bytes, payload.absolutePath);
        } else {
          await api.copyImage(bytes);
        }
        await api.hideMain();
        image.close();
        return;
      }

      if (disposed) {
        image.close();
        return;
      }
      setEditing({
        image,
        path: payload.absolutePath,
        key: Date.now(),
        shapes: payload.shapes,
        color: payload.color,
        strokeWidth: payload.strokeWidth,
        fontSize: payload.fontSize,
      });
    };

    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<ShotPayload>("shot-ready", ({ payload }) => {
          void handle(payload).catch((cause) => {
            // Surface the failure instead of silently dropping a capture.
            console.error("shot-ready failed", cause);
            setError(localizeError(cause, languageRef.current));
            void api.hideMain().catch(() => undefined);
          });
        }),
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((cause) => console.error("failed to subscribe to shot-ready", cause));

    return () => {
      disposed = true;
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
        language={language}
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
        {error ?? copy.idle}
      </div>
    </div>
  );
}
