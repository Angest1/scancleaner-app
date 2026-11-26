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

/**
 * Smart middle-ellipsis for Windows/POSIX file paths.
 * Keeps the drive/root prefix and the leaf folder/filename visible.
 */
export function formatMiddleEllipsis(path: string, maxChars = 54): string {
  if (!path || path.length <= maxChars) return path;
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(sep).filter(Boolean);
  if (parts.length <= 2) {
    const head = Math.max(10, Math.floor(maxChars * 0.42));
    const tail = Math.max(10, maxChars - head - 1);
    return `${path.slice(0, head)}…${path.slice(-tail)}`;
  }

  const root = path.startsWith("\\\\")
    ? `\\\\${parts[0]}${sep}${parts[1]}`
    : path.startsWith(sep)
      ? `${sep}${parts[0]}`
      : `${parts[0]}${sep}${parts[1]}`;
  const tailTwo = `${parts[parts.length - 2]}${sep}${parts[parts.length - 1]}`;
  const candidate = `${root}${sep}…${sep}${tailTwo}`;
  if (candidate.length <= maxChars + 8) return candidate;

  const leaf = parts[parts.length - 1];
  const shortRoot = parts[0];
  const compact = `${shortRoot}${sep}…${sep}${leaf}`;
  if (compact.length <= maxChars + 4) return compact;

  const keepTail = Math.max(14, maxChars - shortRoot.length - 4);
  return `${shortRoot}${sep}…${sep}${leaf.slice(-keepTail)}`;
}

export const CATEGORY_COLORS: Record<string, string> = {
  video: "#c94a63",
  image: "#8b63c9",
  audio: "#b8568f",
  documents: "#2e8eb8",
  archives: "#c78832",
  executables: "#c75050",
  code: "#279973",
  other: "#4f5b6e",
  installedapps: "#3b78c9",
  system: "#5a63bd",
  temp: "#c78832",
  browsercache: "#cc6f35",
  devartifact: "#249188",
  recyclebin: "#c75050",
  downloads: "#2b85b3",
  userother: "#4f5b6e",
};

export function colorForCategory(key: string): string {
  return CATEGORY_COLORS[key.toLowerCase()] ?? "#4f5b6e";
}
