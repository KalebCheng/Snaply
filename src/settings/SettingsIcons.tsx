const iconProps = {
  width: 20,
  height: 20,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export function SlidersIcon() {
  return (
    <svg {...iconProps}>
      <path d="M4 7h16M4 17h16" />
      <circle cx="8" cy="7" r="2" />
      <circle cx="16" cy="17" r="2" />
    </svg>
  );
}

export function InfoIcon() {
  return (
    <svg {...iconProps}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6" />
      <path d="M12 7.5h.01" />
    </svg>
  );
}

export function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
      <path d="M12 .7a11.5 11.5 0 0 0-3.64 22.4c.58.1.8-.25.8-.56v-2.16c-3.25.7-3.94-1.38-3.94-1.38-.53-1.35-1.3-1.7-1.3-1.7-1.06-.73.08-.72.08-.72 1.17.08 1.78 1.2 1.78 1.2 1.05 1.78 2.75 1.27 3.42.97.1-.75.4-1.27.74-1.56-2.6-.3-5.33-1.3-5.33-5.8 0-1.28.46-2.33 1.2-3.15-.12-.3-.52-1.5.12-3.1 0 0 .98-.32 3.2 1.2a11.1 11.1 0 0 1 5.82 0c2.22-1.52 3.2-1.2 3.2-1.2.64 1.6.24 2.8.12 3.1.75.82 1.2 1.87 1.2 3.15 0 4.5-2.74 5.5-5.35 5.8.42.36.8 1.07.8 2.17v3.22c0 .31.21.67.81.56A11.5 11.5 0 0 0 12 .7Z" />
    </svg>
  );
}

export function ExternalIcon() {
  return (
    <svg {...iconProps} width={16} height={16}>
      <path d="M14 4h6v6" />
      <path d="m20 4-9 9" />
      <path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
    </svg>
  );
}

export function DownloadIcon() {
  return (
    <svg {...iconProps}>
      <path d="M12 3v12" />
      <path d="m7 11 5 5 5-5" />
      <path d="M5 21h14" />
    </svg>
  );
}

export function RefreshIcon() {
  return (
    <svg {...iconProps} width={16} height={16}>
      <path d="M20 7v5h-5" />
      <path d="M4 17v-5h5" />
      <path d="M6.1 8.5A7 7 0 0 1 18.6 7L20 12" />
      <path d="M17.9 15.5A7 7 0 0 1 5.4 17L4 12" />
    </svg>
  );
}

export function SnaplyMark() {
  return (
    <svg viewBox="0 0 64 64" width="64" height="64" fill="none" aria-hidden="true">
      <rect x="4" y="4" width="56" height="56" rx="16" fill="currentColor" opacity=".1" />
      {/* Frame corners match the application icon geometry. */}
      <path
        d="M13.5 21.3V13.5H21.3M42.7 13.5H50.5V21.3M50.5 42.7V50.5H42.7M21.3 50.5H13.5V42.7"
        stroke="currentColor"
        strokeWidth="4.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M23.4 40.6L36.4 27.6"
        stroke="currentColor"
        strokeWidth="3.9"
        strokeLinecap="round"
      />
      <path d="M42.8 21.2L40.8 32L32 23.2Z" fill="currentColor" />
    </svg>
  );
}
