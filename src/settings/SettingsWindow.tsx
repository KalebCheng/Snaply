import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api, isTauri } from "../lib/ipc";
import type { AppInfo, Language, Settings, SettingsPayload, UpdateInfo } from "../types";
import { version as appVersion } from "../../package.json";
import { AboutSection, type UpdateState } from "./AboutSection";
import { GeneralSection } from "./GeneralSection";
import { translations } from "./i18n";
import { SettingsSidebar, type SettingsTab } from "./SettingsSidebar";
import { toAccelerator } from "./shortcut";
import "./settings.css";

const defaultSettings: Settings = {
  shortcut: "CommandOrControl+Shift+1",
  saveDir: null,
  copyAfterCapture: false,
  language: "zh-CN",
  dockVisibility: "hide",
};

const fallbackPayload: SettingsPayload = {
  settings: defaultSettings,
  defaultSaveDir: "",
  effectiveShortcut: defaultSettings.shortcut,
};

function normalizeSettings(settings: Partial<Settings> | null | undefined): Settings {
  return {
    ...defaultSettings,
    ...settings,
    dockVisibility: settings?.dockVisibility ?? defaultSettings.dockVisibility,
  };
}

export function SettingsWindow() {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [payload, setPayload] = useState<SettingsPayload>(fallbackPayload);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateState, setUpdateState] = useState<UpdateState>("idle");
  const win = useMemo(() => (isTauri() ? getCurrentWebviewWindow() : null), []);
  const copy = translations[draft?.language ?? "zh-CN"];

  const load = useCallback(async () => {
    setError(null);
    if (!isTauri()) {
      setPayload({
        settings: defaultSettings,
        defaultSaveDir: "~/Pictures/Snaply",
        effectiveShortcut: defaultSettings.shortcut,
      });
      setDraft(defaultSettings);
      setInfo({
        version: appVersion,
        shortcut: defaultSettings.shortcut,
        screenPermission: true,
        platform: /Mac|iPhone|iPad/.test(navigator.userAgent) ? "darwin" : "browser",
      });
      return;
    }

    const next = await api.getSettings();
    const normalized = { ...next, settings: normalizeSettings(next.settings) };
    setPayload(normalized);
    setDraft(normalized.settings);
    void api.appInfo().then(setInfo).catch(() => undefined);
  }, []);

  useEffect(() => {
    load().catch((cause) => {
      setError(String(cause));
      setDraft(defaultSettings);
    });
  }, [load]);

  const checkUpdates = useCallback(async () => {
    setUpdateState("checking");
    if (!isTauri()) {
      setUpdate({ currentVersion: appVersion, latestVersion: null, hasUpdate: false, releaseUrl: null, downloadUrl: null, publishedAt: null, notes: null });
      setUpdateState("ready");
      return;
    }
    try {
      setUpdate(await api.checkForUpdates());
      setUpdateState("ready");
    } catch {
      setUpdateState("error");
    }
  }, []);

  useEffect(() => {
    void checkUpdates();
  }, [checkUpdates]);

  useEffect(() => {
    document.documentElement.lang = draft?.language ?? "zh-CN";
    document.title = `Snaply ${copy.settingsTitle}`;
  }, [copy.settingsTitle, draft?.language]);

  const dirty = useMemo(() => {
    if (!draft) return false;
    return (
      draft.shortcut !== payload.settings.shortcut ||
      draft.saveDir !== payload.settings.saveDir ||
      draft.copyAfterCapture !== payload.settings.copyAfterCapture ||
      draft.language !== payload.settings.language ||
      draft.dockVisibility !== payload.settings.dockVisibility
    );
  }, [draft, payload.settings]);

  const save = useCallback(async () => {
    if (!draft || !isTauri()) return;
    setSaving(true);
    setError(null);
    try {
      const next = await api.saveSettings(draft);
      setPayload(next);
      setDraft(next.settings);
      setSavedAt(Date.now());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  }, [draft]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (recording) {
        event.preventDefault();
        if (event.key === "Escape") {
          setRecording(false);
          return;
        }
        const accelerator = toAccelerator(event);
        if (accelerator) {
          setRecording(false);
          setDraft((current) => (current ? { ...current, shortcut: accelerator } : current));
        }
        return;
      }

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (dirty) void save();
      } else if (event.key === "Escape") {
        void win?.close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [dirty, recording, save, win]);

  const updateSettings = (patch: Partial<Settings>) => {
    setDraft((current) => (current ? { ...current, ...patch } : current));
  };

  const pickFolder = async () => {
    try {
      const directory = await api.pickSaveDir();
      if (directory) updateSettings({ saveDir: directory });
    } catch (cause) {
      setError(String(cause));
    }
  };

  const resetShortcut = async () => {
    try {
      const next = await api.resetShortcut();
      setPayload(next);
      setDraft(next.settings);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const openExternal = (url: string) => {
    if (!isTauri()) return;
    void api.openExternalUrl(url).catch((cause) => setError(String(cause)));
  };

  if (!draft) {
    return (
      <div className="set-root">
        <div className="set-window-drag" data-tauri-drag-region />
        <SettingsSidebar active={activeTab} copy={copy} hasUpdate={false} onChange={setActiveTab} />
        <main className="set-content">
          <div className="set-panel">
            <div className="set-loading">
              <span className="set-loading-mark">Snaply</span>
              <span>{error ? `${copy.settingsLoadFailed}: ${error}` : copy.loadingSettings}</span>
              {error && <button className="set-button-secondary" onClick={() => void load()}>{copy.retry}</button>}
            </div>
          </div>
        </main>
      </div>
    );
  }

  const defaultSaveDir = draft.saveDir ?? payload.defaultSaveDir;
  const hasUpdate = updateState === "ready" && update?.hasUpdate === true;

  return (
    <div className="set-root">
      <div className="set-window-drag" data-tauri-drag-region />
      <SettingsSidebar active={activeTab} copy={copy} hasUpdate={hasUpdate} onChange={setActiveTab} />

      <main className="set-content">
        <div className="set-panel">
          {activeTab === "general" ? (
            <GeneralSection
              copy={copy}
              draft={draft}
              defaultSaveDir={defaultSaveDir}
              info={info}
              recording={recording}
              onRecordingChange={setRecording}
              onLanguageChange={(language: Language) => updateSettings({ language })}
              onCopyAfterCaptureChange={(copyAfterCapture) => updateSettings({ copyAfterCapture })}
              onDockVisibilityChange={(dockVisibility) => updateSettings({ dockVisibility })}
              onResetShortcut={() => void resetShortcut()}
              onPickFolder={() => void pickFolder()}
              onUseDefaultFolder={() => updateSettings({ saveDir: null })}
              onOpenPermissionSettings={() => void api.openScreenPermissionSettings()}
            />
          ) : (
            <AboutSection
              copy={copy}
              info={info}
              update={update}
              updateState={updateState}
              onCheckUpdates={() => void checkUpdates()}
              onOpenProfile={() => openExternal("https://github.com/KalebCheng")}
              onOpenRepository={() => openExternal("https://github.com/KalebCheng/Snaply")}
              onDownloadUpdate={() => {
                const target = update?.downloadUrl ?? update?.releaseUrl;
                if (target) openExternal(target);
              }}
            />
          )}

          {(activeTab === "general" || error) && (
            <footer className="set-footer">
              <span className="set-footer-status">
                {error ? <span className="set-error-text">{error}</span> : savedAt && !dirty ? copy.savedText : ""}
              </span>
              {activeTab === "general" && (
                <button
                  className="set-button-primary"
                  disabled={!dirty || saving || !isTauri()}
                  onClick={() => void save()}
                >
                  {saving ? copy.savingButton : copy.saveButton}
                </button>
              )}
            </footer>
          )}
        </div>
      </main>
    </div>
  );
}
