import type { AppInfo, UpdateInfo } from "../types";
import type { SettingsCopy } from "./i18n";
import { formatMessage } from "./i18n";
import {
  DownloadIcon,
  ExternalIcon,
  GitHubIcon,
  RefreshIcon,
  SnaplyMark,
} from "./SettingsIcons";

export type UpdateState = "idle" | "checking" | "ready" | "error";

type Props = {
  copy: SettingsCopy;
  info: AppInfo | null;
  update: UpdateInfo | null;
  updateState: UpdateState;
  onCheckUpdates: () => void;
  onOpenProfile: () => void;
  onOpenRepository: () => void;
  onDownloadUpdate: () => void;
};

export function AboutSection(props: Props) {
  const { copy, info, update, updateState } = props;
  const version = info?.version ?? update?.currentVersion ?? "0.1.0";
  const checking = updateState === "checking";
  const hasUpdate = updateState === "ready" && update?.hasUpdate === true;
  const readyWithoutRelease = updateState === "ready" && update != null && update.latestVersion == null;
  const upToDate = updateState === "ready" && update != null && update.latestVersion != null && !update.hasUpdate;

  return (
    <section className="set-section set-about">
      <div className="set-about-hero">
        <div className="set-about-mark"><SnaplyMark /></div>
        <h1>Snaply</h1>
        <p>{copy.aboutDescription}</p>
        <span className="set-version-pill">{copy.versionLabel} {version}</span>
      </div>

      <div className={`set-update-card ${hasUpdate ? "has-update" : ""}`}>
        <div className="set-update-copy">
          <strong>
            {checking
              ? copy.checkingUpdates
              : hasUpdate
                ? copy.updateAvailable
                : upToDate
                  ? copy.upToDate
                  : readyWithoutRelease
                    ? copy.releaseUnavailable
                    : updateState === "error"
                      ? copy.updateCheckFailed
                      : copy.checkUpdates}
          </strong>
          {hasUpdate && update?.latestVersion && (
            <p>{formatMessage(copy.updateAvailableHint, { version: update.latestVersion })}</p>
          )}
        </div>

        {hasUpdate ? (
          <button className="set-button-primary" onClick={props.onDownloadUpdate}>
            <DownloadIcon />
            {copy.downloadUpdate}
          </button>
        ) : (
          <button className="set-button-secondary" onClick={props.onCheckUpdates} disabled={checking}>
            <RefreshIcon />
            {checking ? copy.checkingUpdates : copy.checkUpdates}
          </button>
        )}
      </div>

      {hasUpdate && update?.notes && (
        <details className="set-release-notes">
          <summary>{copy.releaseNotes}</summary>
          <p>{update.notes}</p>
        </details>
      )}

      <div className="set-about-links">
        <button className="set-about-link" onClick={props.onOpenProfile}>
          <GitHubIcon />
          <span>KalebCheng</span>
          <ExternalIcon />
        </button>
        <button className="set-about-link" onClick={props.onOpenRepository}>
          <GitHubIcon />
          <span>{copy.repository}</span>
          <ExternalIcon />
        </button>
      </div>

      <p className="set-copyright">{copy.copyright}</p>
    </section>
  );
}
