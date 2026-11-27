import type { ScanEngineMode } from "./ipcTypes";

export interface DriveInfo {
  rootPath: string;
  label: string;
  kind: "fixed" | "removable" | "network" | "optical" | "ram_disk" | "unknown";
  fsType: "ntfs" | "fat32" | "ex_fat" | "refs" | "network" | "unknown";
  fsName: string;
  totalBytes: number;
  freeBytes: number;
  usedBytes: number;
  volumeSerial: number;
  available: boolean;
}

export interface ScanProgress {
  driveRoot?: string;
  phase?: string;
  engineMode?: ScanEngineMode;
  scannedFiles: number;
  scannedDirs: number;
  totalAllocatedBytes: number;
  currentPath: string;
  elapsedMs: number;
  mftProcessed?: number | null;
  mftTotal?: number | null;
  volumeUsedBytes?: number | null;
  usnProcessed?: number | null;
  usnTotal?: number | null;
  progressRatio?: number | null;
}

export interface ScanStats {
  durationMs: number;
  totalFiles: number;
  totalDirs: number;
  totalLogicalBytes: number;
  totalAllocatedBytes: number;
  hardlinksDeduplicated: number;
  symlinksSkipped: number;
  junctionsSkipped: number;
  cloudPlaceholders: number;
  adsStreamsCounted?: number;
  accessDeniedCount: number;
  cancelled: boolean;
  engineMode?: ScanEngineMode;
  stale?: boolean;
}

export interface ScanSummary {
  rootPath: string;
  fsType: string;
  stats: ScanStats;
  snapshotId: number | null;
}

export interface BreadcrumbItem {
  nodeId: number;
  name: string;
}

export interface TreemapNodeDto {
  nodeId: number | null;
  name: string;
  fullPath: string;
  kind: string;
  allocatedBytes: number;
  logicalBytes: number;
  fileCount: number;
  typeCat: string;
  originCat: string;
  percentage: number;
  isOtherBucket: boolean;
  isProtected: boolean;
  protectionReason: string | null;
}

export interface TreemapViewDto {
  nodeId: number;
  fullPath: string;
  totalAllocatedBytes: number;
  fileCount: number;
  breadcrumbs: BreadcrumbItem[];
  children: TreemapNodeDto[];
}

export interface TopEntryDto {
  nodeId: number;
  name: string;
  fullPath: string;
  allocatedBytes: number;
  logicalBytes: number;
  fileCount: number;
  lastWriteTs: number;
  typeCat: string;
  originCat: string;
  isProtected: boolean;
  protectionReason: string | null;
}

export interface CategoryBucket {
  key: string;
  label: string;
  allocatedBytes: number;
  logicalBytes: number;
  fileCount: number;
  percentage: number;
}

export interface CategoryBreakdown {
  totalAllocatedBytes: number;
  byType: CategoryBucket[];
  byOrigin: CategoryBucket[];
}

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string;
  version: string;
  source: "win32" | "store_msix";
  installLocation: string | null;
  sizeBytes: number;
  sizeSource: "measured" | "estimated" | "unknown";
  uninstallString: string | null;
  packageFamilyName: string | null;
  installDate: string | null;
}

export type CleanCategory =
  | "user_temp"
  | "system_temp"
  | "recycle_bin"
  | "browser_cache"
  | "dev_artifact"
  | "large_old_file";

export interface CleanCandidate {
  id: string;
  path: string;
  displayName: string;
  category: CleanCategory;
  sizeBytes: number;
  itemCount: number;
  lastModifiedTs: number;
  requiresAdmin: boolean;
  warning: string | null;
}

export interface CleanupCandidatesReport {
  isElevated: boolean;
  runningBrowsers: string[];
  recycleBinBytes: number;
  recycleBinItems: number;
  candidates: CleanCandidate[];
}

export type CleanupMode = "recycle_bin" | "permanent" | "empty_recycle_bin";

export interface CleanupTargetInput {
  path: string;
  category: string;
  isTempDirContents: boolean;
}

export interface CleanupPlan {
  planId: string;
  mode: CleanupMode;
  items: { path: string; category: string; bytes: number }[];
  blockedItems: { path: string; reason: string }[];
  totalBytes: number;
  warnings: string[];
}

export interface CleanupResult {
  planId: string;
  mode: CleanupMode;
  freedBytes: number;
  estimatedBytes?: number;
  realFreedBytes?: number | null;
  discrepancyExplanation?: string | null;
  backgroundJobId?: string | null;
  succeededCount: number;
  failedCount: number;
  skippedCount: number;
  outcomes: { path: string; bytes: number; status: string; detail: string | null }[];
}

export interface ScanSnapshot {
  id: number;
  driveRoot: string;
  scannedAt: string;
  totalDiskBytes: number;
  freeDiskBytes: number;
  scannedAllocatedBytes: number;
  fileCount: number;
  dirCount: number;
  durationMs: number;
  categories: CategoryBucket[];
  topDirs: { name: string; allocatedBytes: number }[];
}

export interface SnapshotDiff {
  olderId: number;
  newerId: number;
  driveRoot: string;
  allocatedDeltaBytes: number;
  fileCountDelta: number;
  topDirDeltas: { name: string; olderBytes: number; newerBytes: number; deltaBytes: number }[];
}

export interface CleanupLogEntry {
  timestamp: string;
  planId: string;
  path: string;
  category: string;
  bytes: number;
  mode: string;
  status: string;
  detail: string | null;
  ruleId?: string | null;
  estimatedBytes?: number | null;
  command?: string | null;
}
