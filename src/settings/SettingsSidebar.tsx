import type { SettingsCopy } from "./i18n";
import { InfoIcon, SlidersIcon } from "./SettingsIcons";

export type SettingsTab = "general" | "about";

type Props = {
  active: SettingsTab;
  copy: SettingsCopy;
  hasUpdate: boolean;
  onChange: (tab: SettingsTab) => void;
};

export function SettingsSidebar({ active, copy, hasUpdate, onChange }: Props) {
  return (
    <aside className="set-sidebar">
      <div className="set-brand">Snaply</div>
      <nav className="set-nav" aria-label={copy.settingsTitle}>
        <button
          className={`set-nav-item ${active === "general" ? "is-active" : ""}`}
          aria-current={active === "general" ? "page" : undefined}
          onClick={() => onChange("general")}
        >
          <SlidersIcon />
          <span>{copy.navGeneral}</span>
        </button>
        <button
          className={`set-nav-item ${active === "about" ? "is-active" : ""}`}
          aria-current={active === "about" ? "page" : undefined}
          onClick={() => onChange("about")}
        >
          <InfoIcon />
          <span>{copy.navAbout}</span>
          {hasUpdate && <span className="set-nav-dot" aria-hidden="true" />}
        </button>
      </nav>
    </aside>
  );
}
