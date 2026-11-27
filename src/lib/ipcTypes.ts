export type ScanEngineMode =
  | "mft_direct"
  | "mft_service"
  | "fallback_work_stealing"
  | "snapshot_cached"
  | "usn_incremental";

export interface AppSettings {
  skipStartupAnimation: boolean;
  includeAds: boolean;
  startWithWindowsTray: boolean;
  uninstallWatchTimeoutMins: number;
  driftThresholdPercent: number;
  driftThresholdGb: number;
}

export interface IndexingServiceStatus {
  installed: boolean;
  running: boolean;
  pipeReachable: boolean;
  binaryPath: string;
}

export interface DriveProgressItem {
  driveRoot: string;
  label: string;
  usedBytes: number;
  progressRatio: number | null;
  phase: string;
  files: number;
  allocatedBytes: number;
  done: boolean;
  stale: boolean;
  engineMode: ScanEngineMode;
}

export interface TreemapLayoutRectDto {
  id: string;
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
  isCloudOnly: boolean;
  isPendingTrash: boolean;
  protectionReason: string | null;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface TreemapLayoutDto {
  nodeId: number;
  fullPath: string;
  totalAllocatedBytes: number;
  fileCount: number;
  breadcrumbs: { nodeId: number; name: string }[];
  rects: TreemapLayoutRectDto[];
}

export interface SearchHitDto {
  nodeId: number;
  name: string;
  fullPath: string;
  kind: string;
  allocatedBytes: number;
  isProtected: boolean;
}

export interface SyncStatusDto {
  syncing: boolean;
  pendingBatches: number;
  lastSyncedTs: number | null;
  lastOrigin: string | null;
  observations: string[];
  revision: number;
}

export interface FocusSyncResult {
  flushedBatches: number;
  systemFileNotice: string | null;
}

export interface FreedSpaceMeasurement {
  volume: string;
  freeBeforeBytes: number;
  freeAfterBytes: number;
  realFreedBytes: number;
  estimatedBytes: number;
  discrepancyExplanation: string | null;
}

export interface DeletionJobStatus {
  jobId: string;
  mode: string;
  phase: "queued" | "staging" | "deleting_background" | "completed" | "cancelled" | "failed";
  totalItems: number;
  completedItems: number;
  failedItems: number;
  estimatedBytes: number;
  realFreedBytes: number | null;
  discrepancyExplanation: string | null;
  errors: string[];
  measurements: FreedSpaceMeasurement[];
}

export interface UninstallLeftoverSuggestion {
  appName: string;
  path: string;
  sizeBytes: number;
  nivel: string;
  reason: string;
}
