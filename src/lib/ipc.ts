import { Channel, invoke } from "@tauri-apps/api/core";

export interface ElevationStatus {
  isElevated: boolean;
  username: string;
}

export interface StartupContext {
  elevation: ElevationStatus;
  startupDrive: string | null;
  rulesPath: string;
}

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
  scannedFiles: number;
  scannedDirs: number;
  totalAllocatedBytes: number;
  currentPath: string;
  elapsedMs: number;
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
  accessDeniedCount: number;
  cancelled: boolean;
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

export type DevRuleCategory =
  | "Node"
  | "PHP"
  | "Python"
  | "Android"
  | "Docker"
  | "IDE"
  | "Sistema"
  | "Testing";

export type DevRuleKind = "cache-global" | "artefacto-proyecto" | "sistema";
export type DevSafetyLevel = "seguro" | "revisar" | "nunca";
export type DevCleanMethod = "comando-oficial" | "borrado-permanente" | "papelera";

export interface DevCleanerConfig {
  projectRoots: string[];
  minProjectAgeDays: number;
  disabledRuleIds: string[];
}

export interface DevCandidate {
  id: string;
  ruleId: string;
  nombre: string;
  categoria: DevRuleCategory;
  tipo: DevRuleKind;
  nivel: DevSafetyLevel;
  path: string;
  extraPaths: string[];
  deleteDirContentsOnly: boolean;
  projectName: string | null;
  projectRoot: string | null;
  projectAgeDays: number | null;
  noGitRepo: boolean;
  sizeBytes: number;
  itemCount: number;
  lastModifiedTs: number;
  method: DevCleanMethod;
  command: string[] | null;
  preCommands: string[][];
  allowRecycleBinOption: boolean;
  requiresAdmin: boolean;
  blockingProcessesRunning: string[];
  blockingProcessesSpec: string[];
  warning: string | null;
  blockedReason: string | null;
  comoRegenerar: string;
  preselected: boolean;
}

export interface DockerWslInfo {
  dockerAvailable: boolean;
  daemonRunning: boolean;
  vhdxNotice: string;
  compactInstructions: string[];
}

export interface DevScanReport {
  isElevated: boolean;
  totalRecoverableBytes: number;
  safeRecoverableBytes: number;
  reviewRecoverableBytes: number;
  runningDevProcesses: string[];
  configuredRoots: string[];
  minProjectAgeDays: number;
  dockerWslInfo: DockerWslInfo;
  candidates: DevCandidate[];
  warnings: string[];
}

export interface DevPreviewItemInput {
  candidate: DevCandidate;
  useRecycleBin: boolean;
}

export interface DevPlannedItem {
  id: string;
  ruleId: string;
  nombre: string;
  categoria: DevRuleCategory;
  nivel: DevSafetyLevel;
  path: string;
  extraPaths: string[];
  deleteDirContentsOnly: boolean;
  estimatedBytes: number;
  effectiveMethod: DevCleanMethod;
  command: string[] | null;
  preCommands: string[][];
  blockingProcessesSpec: string[];
  requiresAdmin: boolean;
  comoRegenerar: string;
}

export interface DevBlockedItem {
  id: string;
  nombre: string;
  path: string;
  reason: string;
}

export interface DevCleanupPlan {
  planId: string;
  items: DevPlannedItem[];
  blockedItems: DevBlockedItem[];
  totalEstimatedBytes: number;
  warnings: string[];
}

export interface DevExecutionOutcome {
  id: string;
  ruleId: string;
  nombre: string;
  path: string;
  methodLabel: string;
  estimatedBytes: number;
  freedBytes: number;
  status: string;
  detail: string | null;
}

export interface DevCleanupResult {
  planId: string;
  totalEstimatedBytes: number;
  totalFreedBytes: number;
  succeededCount: number;
  failedCount: number;
  skippedCount: number;
  outcomes: DevExecutionOutcome[];
}

export type ProtectionType = "archivo" | "carpeta-recursiva" | "glob";
export type ProtectionOrigin = "crítica" | "usuario";

export interface ProtectedEntry {
  id: string;
  patron: string;
  tipo: ProtectionType;
  origen: ProtectionOrigin;
  motivo: string;
  todasLasUnidades: boolean;
}

export interface ProtectionResult {
  protected: boolean;
  entry: ProtectedEntry | null;
  reason: string | null;
}

export interface SystemFileEntry {
  id: string;
  name: string;
  path: string;
  sizeBytes: number;
  explanation: string;
  canAdjust: boolean;
  adjustKind: "pagefile" | "hiberfil" | "none";
}

export interface HibernationStatus {
  available: boolean;
  label: string;
  detail: string | null;
}

export interface SystemFilesReport {
  files: SystemFileEntry[];
  totalBytes: number;
  hibernation: HibernationStatus;
  virtualMemoryInstruction: string;
}

export type PowercfgAction = "reduced" | "off" | "on";

export const api = {
  getStartupContext: () => invoke<StartupContext>("get_startup_context"),
  listDrives: (includeNetwork: boolean) =>
    invoke<DriveInfo[]>("cmd_list_drives", { includeNetwork }),
  startScan: (rootPath: string, onProgress: (p: ScanProgress) => void) => {
    const channel = new Channel<ScanProgress>();
    channel.onmessage = onProgress;
    return invoke<ScanSummary>("cmd_start_scan", { rootPath, onProgress: channel });
  },
  cancelScan: () => invoke<void>("cmd_cancel_scan"),
  getTreemapChildren: (nodeId: number, maxItems = 140) =>
    invoke<TreemapViewDto>("cmd_get_treemap_children", { nodeId, maxItems }),
  getTopEntries: (scopeNodeId: number | null, kind: "files" | "dirs", limit = 100) =>
    invoke<TopEntryDto[]>("cmd_get_top_entries", { scopeNodeId, kind, limit }),
  getCategoryBreakdown: () => invoke<CategoryBreakdown>("cmd_get_category_breakdown"),
  reloadRules: () => invoke<CategoryBreakdown>("cmd_reload_rules"),
  getAccessDenied: (limit = 200) => invoke<string[]>("cmd_get_access_denied", { limit }),
  openInExplorer: (path: string) => invoke<void>("cmd_open_in_explorer", { path }),
  listInstalledApps: () => invoke<InstalledApp[]>("cmd_list_installed_apps"),
  uninstallApp: (source: "win32" | "store_msix", uninstallString: string | null) =>
    invoke<string>("cmd_uninstall_app", { source, uninstallString }),
  openWindowsAppsSettings: () => invoke<void>("cmd_open_windows_apps_settings"),
  discoverCleanupCandidates: (minLargeMb: number, minAgeDays: number) =>
    invoke<CleanupCandidatesReport>("cmd_discover_cleanup_candidates", {
      minLargeMb,
      minAgeDays,
    }),
  previewCleanup: (targets: CleanupTargetInput[], mode: CleanupMode) =>
    invoke<CleanupPlan>("cmd_preview_cleanup", { request: { targets, mode } }),
  executeCleanup: (planId: string, confirmedPermanent: boolean) =>
    invoke<CleanupResult>("cmd_execute_cleanup", { planId, confirmedPermanent }),
  relaunchAsAdmin: (activeDrive: string | null) =>
    invoke<void>("cmd_relaunch_as_admin", { activeDrive }),
  listSnapshots: (driveRoot: string | null) =>
    invoke<ScanSnapshot[]>("cmd_list_snapshots", { driveRoot }),
  diffSnapshots: (olderId: number, newerId: number) =>
    invoke<SnapshotDiff>("cmd_diff_snapshots", { olderId, newerId }),
  listCleanupLogs: (limit = 200) => invoke<CleanupLogEntry[]>("cmd_list_cleanup_logs", { limit }),
  devGetConfig: () => invoke<DevCleanerConfig>("cmd_dev_get_config"),
  devSaveConfig: (config: DevCleanerConfig) =>
    invoke<DevCleanerConfig>("cmd_dev_save_config", { config }),
  devPickProjectRoot: () => invoke<string | null>("cmd_dev_pick_project_root"),
  devScan: (config?: DevCleanerConfig) =>
    invoke<DevScanReport>("cmd_dev_scan", { config: config ?? null }),
  devPreview: (items: DevPreviewItemInput[]) =>
    invoke<DevCleanupPlan>("cmd_dev_preview", { request: { items } }),
  devExecute: (planId: string) => invoke<DevCleanupResult>("cmd_dev_execute", { planId }),
  protectionList: () => invoke<ProtectedEntry[]>("cmd_protection_list"),
  protectionAdd: (
    patron: string,
    tipo: ProtectionType,
    motivo: string,
    todasLasUnidades: boolean,
  ) =>
    invoke<ProtectedEntry[]>("cmd_protection_add", {
      patron,
      tipo,
      motivo,
      todasLasUnidades,
    }),
  protectionRemove: (id: string) => invoke<ProtectedEntry[]>("cmd_protection_remove", { id }),
  protectionCheck: (path: string) => invoke<ProtectionResult>("cmd_protection_check", { path }),
  protectionPreviewMatches: (patron: string, tipo: ProtectionType, todasLasUnidades: boolean) =>
    invoke<number>("cmd_protection_preview_matches", {
      patron,
      tipo,
      todasLasUnidades,
    }),
  pickFile: () => invoke<string | null>("cmd_pick_file"),
  systemFilesReport: (driveRoot: string | null) =>
    invoke<SystemFilesReport>("cmd_system_files_report", { driveRoot }),
  openVirtualMemory: () => invoke<string>("cmd_open_virtual_memory"),
  openPowerOptions: () => invoke<void>("cmd_open_power_options"),
  runPowercfgElevated: (action: PowercfgAction) =>
    invoke<string>("cmd_run_powercfg_elevated", { action }),
  listDiagnosticLogs: (limit = 300) => invoke<string[]>("cmd_list_diagnostic_logs", { limit }),
  openLogsFolder: () => invoke<void>("cmd_open_logs_folder"),
};
