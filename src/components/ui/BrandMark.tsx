export function BrandMark({ size = 30 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      className="shrink-0"
    >
      <defs>
        <linearGradient
          id="sc-brand-bg"
          x1="2"
          y1="2"
          x2="30"
          y2="30"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stopColor="#6d5dfc" />
          <stop offset="55%" stopColor="#3b82f6" />
          <stop offset="100%" stopColor="#22d3ee" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="9" fill="url(#sc-brand-bg)" />
      <circle cx="16" cy="16" r="8" stroke="rgba(255,255,255,0.34)" strokeWidth="2.1" />
      <path d="M16 8A8 8 0 0 1 24 16" stroke="#ffffff" strokeWidth="2.5" strokeLinecap="round" />
      <path d="M16 16L21.5 10.5" stroke="#ffffff" strokeWidth="1.9" strokeLinecap="round" />
      <circle cx="16" cy="16" r="2.2" fill="#ffffff" />
    </svg>
  );
}
