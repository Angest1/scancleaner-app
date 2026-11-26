export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / 1024 ** exp;
  return `${val >= 100 || exp === 0 ? val.toFixed(0) : val.toFixed(2)} ${units[exp]}`;
}

export function formatSignedBytes(delta: number): string {
  if (delta === 0) return "0 B";
  const sign = delta > 0 ? "+" : "-";
  return `${sign}${formatBytes(Math.abs(delta))}`;
}

export function formatUnixTs(tsSeconds: number): string {
  if (!tsSeconds || tsSeconds <= 0) return "—";
  return new Date(tsSeconds * 1000).toLocaleDateString("es-ES", {
    year: "numeric",
    month: "short",
    day: "2-digit",
  });
}

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const secs = ms / 1000;
  if (secs < 60) return `${secs.toFixed(1)} s`;
  const mins = Math.floor(secs / 60);
  const rem = Math.round(secs % 60);
  return `${mins}m ${rem}s`;
}

export const CATEGORY_COLORS: Record<string, string> = {
  video: "#f43f5e",
  image: "#a855f7",
  audio: "#ec4899",
  documents: "#38bdf8",
  archives: "#f59e0b",
  executables: "#ef4444",
  code: "#10b981",
  other: "#64748b",
  installedapps: "#3b82f6",
  system: "#6366f1",
  temp: "#f59e0b",
  browsercache: "#f97316",
  devartifact: "#14b8a6",
  recyclebin: "#ef4444",
  downloads: "#0ea5e9",
  userother: "#64748b",
};

export function colorForCategory(key: string): string {
  return CATEGORY_COLORS[key.toLowerCase()] ?? "#475569";
}
