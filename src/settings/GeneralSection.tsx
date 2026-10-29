import type { AppInfo, DockVisibility, Language, Settings } from "../types";
import type { SettingsCopy } from "./i18n";
import { languageOptions } from "./i18n";
import { keycaps } from "./shortcut";

type Props = {
  copy: SettingsCopy;
  draft: Settings;
  defaultSaveDir: string;
  info: AppInfo | null;
  recording: boolean;
  onRecordingChange: (recording: boolean) => void;
  onLanguageChange: (language: Language) => void;
  onCopyAfterCaptureChange: (enabled: boolean) => void;
  onDockVisibilityChange: (value: DockVisibility) => void;
  onResetShortcut: () => void;
  onPickFolder: () => void;
  onUseDefaultFolder: () => void;
  onOpenPermissionSettings: () => void;
};

export function GeneralSection(props: Props) {
  const { copy, draft, info } = props;
  const saveDir = draft.saveDir ?? props.defaultSaveDir;
  const needsPermission = info?.platform === "darwin" && info.screenPermission === false;

  return (
    <section className="set-section">
      <header className="set-section-header">
        <h1>{copy.generalTitle}</h1>
        <p>{copy.generalDescription}</p>
      </header>

      {needsPermission && (
        <div className="set-callout">
          <div>
            <strong>{copy.permissionTitle}</strong>
            <p>{copy.permissionHint}</p>
          </div>
          <button className="set-button-secondary" onClick={props.onOpenPermissionSettings}>
            {copy.grantPermission}
          </button>
        </div>
      )}

      <div className="set-group">
        <div className="set-field">
          <div className="set-field-copy">
            <div className="set-field-label">{copy.languageLabel}</div>
            <div className="set-field-hint">{copy.languageHint}</div>
          </div>
          <label className="set-select-wrap">
            <span className="set-select-icon" aria-hidden="true">文A</span>
            <select
              className="set-select"
              value={draft.language}
              onChange={(event) => props.onLanguageChange(event.target.value as Language)}
            >
              {languageOptions.map((language) => (
                <option key={language.id} value={language.id}>
                  {language.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="set-field">
          <div className="set-field-copy">
            <div className="set-field-label">{copy.shortcutLabel}</div>
            <div className="set-field-hint">{copy.shortcutHint}</div>
          </div>
          <div className="set-field-control set-inline-control">
            <button
              className={`set-shortcut ${props.recording ? "is-recording" : ""}`}
              onClick={() => props.onRecordingChange(!props.recording)}
              title={copy.shortcutClick}
            >
              {props.recording ? (
                <span>{copy.shortcutRecording}</span>
              ) : (
                keycaps(draft.shortcut).map((key, index) => <kbd key={index}>{key}</kbd>)
              )}
            </button>
            <button className="set-link" onClick={props.onResetShortcut}>
              {copy.shortcutReset}
            </button>
          </div>
        </div>
      </div>

      <div className="set-group">
        <div className="set-group-title">{copy.captureSection}</div>
        <div className="set-field">
          <div className="set-field-copy">
            <div className="set-field-label">{copy.copyAfterCaptureLabel}</div>
            <div className="set-field-hint">{copy.copyAfterCaptureHint}</div>
          </div>
          <label className="set-switch">
            <input
              type="checkbox"
              checked={draft.copyAfterCapture}
              onChange={(event) => props.onCopyAfterCaptureChange(event.target.checked)}
            />
            <span />
          </label>
        </div>

        {info?.platform === "darwin" && (
          <div className="set-field">
            <div className="set-field-copy">
              <div className="set-field-label">{copy.dockVisibilityLabel}</div>
              <div className="set-field-hint">{copy.dockVisibilityHint}</div>
            </div>
            <div className="set-segmented" role="group" aria-label={copy.dockVisibilityLabel}>
              {(
                [
                  ["hide", copy.dockHide],
                  ["followWindow", copy.dockFollowWindow],
                  ["show", copy.dockShow],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  className={draft.dockVisibility === value ? "is-active" : ""}
                  aria-pressed={draft.dockVisibility === value}
                  onClick={() => props.onDockVisibilityChange(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="set-group">
        <div className="set-group-title">{copy.saveSection}</div>
        <div className="set-field">
          <div className="set-field-copy">
            <div className="set-field-label">{copy.saveLocationLabel}</div>
            <div className="set-field-hint">{copy.saveLocationHint}</div>
            <div className="set-path" title={saveDir}>{saveDir}</div>
          </div>
          <div className="set-field-control set-inline-control">
            {draft.saveDir && (
              <button className="set-link" onClick={props.onUseDefaultFolder}>
                {copy.useDefault}
              </button>
            )}
            <button className="set-button-secondary" onClick={props.onPickFolder}>
              {copy.chooseFolder}
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
