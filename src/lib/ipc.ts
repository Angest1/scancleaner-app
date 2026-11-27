import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  DevCleanerConfig,
  DevCleanupPlan,
  DevCleanupResult,
  DevPreviewItemInput,
  DevScanReport,
  PowercfgAction,
  ProtectedEntry,
  ProtectionResult,
  ProtectionType,
  SystemFilesReport,
} from "./ipcDevTypes";
import type {
  AppSettings,
  DeletionJobStatus,
  DriveProgressItem,
  FocusSyncResult,
  IndexingServiceStatus,
  SearchHitDto,
  SyncStatusDto,
  TreemapLayoutDto,
  UninstallLeftoverSuggestion,
} from "./ipcTypes";

export type {
  DevBlockedItem,
  DevCandidate,
  DevCleanMethod,
  DevCleanerConfig,
  DevCleanupPlan,
  DevCleanupResult,
  DevExecutionOutcome,
  DevPlannedItem,
  DevPreviewItemInput,
  DevRuleCategory,
  DevRuleKind,
  DevSafetyLevel,
  DevScanReport,
  DockerWslInfo,
  HibernationStatus,
  PowercfgAction,
  ProtectedEntry,
  ProtectionOrigin,
  ProtectionResult,
  ProtectionType,
  SystemFileEntry,
  SystemFilesReport,
} from "./ipcDevTypes";
export type {
  AppSettings,
  DeletionJobStatus,
  DriveProgressItem,
  FocusSyncResult,
  FreedSpaceMeasurement,
  IndexingServiceStatus,
  ScanEngineMode,
  SearchHitDto,
  SyncStatusDto,
  TreemapLayoutDto,
  TreemapLayoutRectDto,
  UninstallLeftoverSuggestion,
} from "./ipcTypes";

import type {
  CategoryBreakdown,
  CleanupCandidatesReport,
  CleanupLogEntry,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  CleanupTargetInput,
  DriveInfo,
  InstalledApp,
  ScanProgress,
  ScanSnapshot,
  ScanSummary,
  SnapshotDiff,
  TopEntryDto,
  TreemapViewDto,
} from "./ipcScanTypes";

export type {
  BreadcrumbItem,
  CategoryBreakdown,
  CategoryBucket,
  CleanCandidate,
  CleanCategory,
  CleanupCandidatesReport,
  CleanupLogEntry,
  CleanupMode,
  CleanupPlan,
  CleanupResult,
  CleanupTargetInput,
  DriveInfo,
  InstalledApp,
  ScanProgress,
  ScanSnapshot,
  ScanStats,
  ScanSummary,
  SnapshotDiff,
  TopEntryDto,
  TreemapNodeDto,
  TreemapViewDto,
} from "./ipcScanTypes";

export interface ElevationStatus {
  isElevated: boolean;
  username: string;
}

export interface StartupContext {
  elevation: ElevationStatus;
  startupDrive: string | null;
  rulesPath: string;
  settings: AppSettings;
  serviceStatus: IndexingServiceStatus;
  windowsAnimationsEnabled: boolean;
}

export type StartupBootstrapEvent =
  | { kind: "drives_ready"; drives: DriveInfo[] }
  | {
      kind: "progress";
      globalRatio: number | null;
      phase: string;
      totalFiles: number;
      totalAllocatedBytes: number;
      drives: DriveProgressItem[];
    }
  | {
      kind: "ready";
      warm: boolean;
      partial: boolean;
      hasPreviousIndex: boolean;
      observations: string[];
      summaries: ScanSummary[];
      elapsedMs: number;
    }
  | { kind: "error"; message: string; hasPreviousIndex: boolean };

export const api = {
  getStartupContext: () => invoke<StartupContext>("get_startup_context"),
  showMainWindow: () => invoke<void>("cmd_show_main_window"),
  bootstrapStartup: (onEvent: (e: StartupBootstrapEvent) => void) => {
    const channel = new Channel<StartupBootstrapEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("cmd_bootstrap_startup", { onEvent: channel });
  },
  listDrives: (includeNetwork: boolean) =>
    invoke<DriveInfo[]>("cmd_list_drives", { includeNetwork }),
  setActiveDrive: (driveRoot: string) => invoke<void>("cmd_set_active_drive", { driveRoot }),
  startScan: (rootPath: string, onProgress: (p: ScanProgress) => void) => {
    const channel = new Channel<ScanProgress>();
    channel.onmessage = onProgress;
    return invoke<ScanSummary>("cmd_start_scan", { rootPath, onProgress: channel });
  },
  cancelScan: () => invoke<void>("cmd_cancel_scan"),
  getTreemapLayout: (
    nodeId: number,
    width: number,
    height: number,
    driveRoot?: string | null,
    minPx = 3,
    maxItems = 140,
  ) =>
    invoke<TreemapLayoutDto>("cmd_get_treemap_layout", {
      driveRoot: driveRoot ?? null,
      nodeId,
      width,
      height,
      minPx,
      maxItems,
    }),
  getTreemapChildren: (nodeId: number, maxItems = 140, driveRoot?: string | null) =>
    invoke<TreemapViewDto>("cmd_get_treemap_children", {
      nodeId,
      maxItems,
      driveRoot: driveRoot ?? null,
    }),
  getTopEntries: (
    scopeNodeId: number | null,
    kind: "files" | "dirs",
    limit = 100,
    driveRoot?: string | null,
  ) =>
    invoke<TopEntryDto[]>("cmd_get_top_entries", {
      scopeNodeId,
      kind,
      limit,
      driveRoot: driveRoot ?? null,
    }),
  searchVolume: (query: string, limit = 100, driveRoot?: string | null) =>
    invoke<SearchHitDto[]>("cmd_search_volume", {
      query,
      limit,
      driveRoot: driveRoot ?? null,
    }),
  getCategoryBreakdown: (driveRoot?: string | null) =>
    invoke<CategoryBreakdown>("cmd_get_category_breakdown", { driveRoot: driveRoot ?? null }),
  reloadRules: (driveRoot?: string | null) =>
    invoke<CategoryBreakdown>("cmd_reload_rules", { driveRoot: driveRoot ?? null }),
  getAccessDenied: (limit = 200, driveRoot?: string | null) =>
    invoke<string[]>("cmd_get_access_denied", { limit, driveRoot: driveRoot ?? null }),
  openInExplorer: (path: string) => invoke<void>("cmd_open_in_explorer", { path }),
  listInstalledApps: () => invoke<InstalledApp[]>("cmd_list_installed_apps"),
  uninstallApp: (
    source: "win32" | "store_msix",
    uninstallString: string | null,
    meta?: { appId?: string; appName?: string; installLocation?: string | null },
  ) =>
    invoke<string>("cmd_uninstall_app", {
      source,
      uninstallString,
      appId: meta?.appId ?? null,
      appName: meta?.appName ?? null,
      installLocation: meta?.installLocation ?? null,
    }),
  getUninstallLeftovers: () => invoke<UninstallLeftoverSuggestion[]>("cmd_get_uninstall_leftovers"),
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
  deletionJobsStatus: () => invoke<DeletionJobStatus[]>("cmd_deletion_jobs_status"),
  cancelDeletionJob: (jobId: string) => invoke<boolean>("cmd_cancel_deletion_job", { jobId }),
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
  getAppSettings: () => invoke<AppSettings>("cmd_get_app_settings"),
  saveAppSettings: (settings: AppSettings) =>
    invoke<AppSettings>("cmd_save_app_settings", { settings }),
  serviceStatus: () => invoke<IndexingServiceStatus>("cmd_service_status"),
  serviceInstall: () => invoke<IndexingServiceStatus>("cmd_service_install"),
  serviceUninstall: () => invoke<IndexingServiceStatus>("cmd_service_uninstall"),
  syncStatus: () => invoke<SyncStatusDto>("cmd_sync_status"),
  reconcilePaths: (volume: string, paths: string[]) =>
    invoke<void>("cmd_reconcile_paths", { volume, paths }),
  triggerFocusSync: () => invoke<FocusSyncResult>("cmd_trigger_focus_sync"),
  resolveValidNode: (fullPath: string, driveRoot?: string | null) =>
    invoke<[number, boolean, string]>("cmd_resolve_valid_node", {
      fullPath,
      driveRoot: driveRoot ?? null,
    }),
  listDiagnosticLogs: (limit = 300) => invoke<string[]>("cmd_list_diagnostic_logs", { limit }),
  openLogsFolder: () => invoke<void>("cmd_open_logs_folder"),
};
