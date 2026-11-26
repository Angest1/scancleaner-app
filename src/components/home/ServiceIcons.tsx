export function ExplorerIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-exp-a" x1="4" y1="6" x2="40" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#6366f1" />
          <stop offset="100%" stopColor="#38bdf8" />
        </linearGradient>
        <linearGradient
          id="ico-exp-b"
          x1="6"
          y1="14"
          x2="38"
          y2="38"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stopColor="#818cf8" stopOpacity="0.95" />
          <stop offset="100%" stopColor="#0ea5e9" />
        </linearGradient>
      </defs>
      <path
        d="M5 12.5C5 9.46 7.46 7 10.5 7H17.3C18.6 7 19.8 7.56 20.65 8.54L22.5 10.7H33.5C36.54 10.7 39 13.16 39 16.2V31.5C39 34.54 36.54 37 33.5 37H10.5C7.46 37 5 34.54 5 31.5V12.5Z"
        fill="url(#ico-exp-a)"
      />
      <rect x="9" y="16" width="14" height="16" rx="3.5" fill="rgba(255,255,255,0.28)" />
      <rect x="25" y="16" width="10" height="7" rx="2.5" fill="rgba(255,255,255,0.42)" />
      <rect
        x="25"
        y="25"
        width="10"
        height="7"
        rx="2.5"
        fill="url(#ico-exp-b)"
        stroke="rgba(255,255,255,0.45)"
        strokeWidth="1.2"
      />
    </svg>
  );
}

export function CleanerIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-cln-a" x1="6" y1="4" x2="38" y2="40" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#06b6d4" />
          <stop offset="100%" stopColor="#10b981" />
        </linearGradient>
      </defs>
      <path
        d="M22 4.5L36.5 10.5V21.5C36.5 30.6 30.3 37.3 22 40C13.7 37.3 7.5 30.6 7.5 21.5V10.5L22 4.5Z"
        fill="url(#ico-cln-a)"
      />
      <path
        d="M22 12.5L24.1 18.4L30 20.5L24.1 22.6L22 28.5L19.9 22.6L14 20.5L19.9 18.4L22 12.5Z"
        fill="#ffffff"
      />
      <circle cx="29.5" cy="14" r="2" fill="rgba(255,255,255,0.72)" />
    </svg>
  );
}

export function DevCleanerIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-dev-a" x1="5" y1="6" x2="39" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#8b5cf6" />
          <stop offset="55%" stopColor="#a855f7" />
          <stop offset="100%" stopColor="#ec4899" />
        </linearGradient>
      </defs>
      <rect x="5" y="7" width="34" height="30" rx="8.5" fill="url(#ico-dev-a)" />
      <path
        d="M15.5 17.5L20.5 22L15.5 26.5"
        stroke="#ffffff"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M23.5 26.5H29.5"
        stroke="rgba(255,255,255,0.85)"
        strokeWidth="2.8"
        strokeLinecap="round"
      />
      <circle cx="11.5" cy="12" r="1.5" fill="rgba(255,255,255,0.65)" />
      <circle cx="16" cy="12" r="1.5" fill="rgba(255,255,255,0.45)" />
    </svg>
  );
}

export function AppsIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-app-a" x1="6" y1="6" x2="38" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#3b82f6" />
          <stop offset="100%" stopColor="#6366f1" />
        </linearGradient>
      </defs>
      <rect x="6" y="6" width="14.5" height="14.5" rx="4.5" fill="url(#ico-app-a)" />
      <rect x="23.5" y="6" width="14.5" height="14.5" rx="4.5" fill="#38bdf8" fillOpacity="0.88" />
      <rect x="6" y="23.5" width="14.5" height="14.5" rx="4.5" fill="#818cf8" fillOpacity="0.85" />
      <rect x="23.5" y="23.5" width="14.5" height="14.5" rx="4.5" fill="url(#ico-app-a)" />
      <rect x="27.5" y="27.5" width="6.5" height="6.5" rx="2" fill="rgba(255,255,255,0.65)" />
    </svg>
  );
}

export function ProtectedIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-pro-a" x1="8" y1="8" x2="36" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#10b981" />
          <stop offset="100%" stopColor="#22c55e" />
        </linearGradient>
      </defs>
      <path
        d="M14.5 18V14.5C14.5 10.36 17.86 7 22 7C26.14 7 29.5 10.36 29.5 14.5V18"
        stroke="#10b981"
        strokeWidth="3.2"
        strokeLinecap="round"
      />
      <rect x="8.5" y="17.5" width="27" height="20.5" rx="6.5" fill="url(#ico-pro-a)" />
      <circle cx="22" cy="26.5" r="3" fill="#ffffff" />
      <path d="M22 29.2V32.5" stroke="#ffffff" strokeWidth="2.4" strokeLinecap="round" />
    </svg>
  );
}

export function HistoryIcon() {
  return (
    <svg width="42" height="42" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="ico-his-a" x1="6" y1="6" x2="38" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#f59e0b" />
          <stop offset="100%" stopColor="#f43f5e" />
        </linearGradient>
      </defs>
      <circle cx="22" cy="22" r="16.5" fill="url(#ico-his-a)" />
      <circle cx="22" cy="22" r="11" stroke="rgba(255,255,255,0.28)" strokeWidth="2" />
      <path
        d="M22 14.5V22L27.5 25"
        stroke="#ffffff"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
