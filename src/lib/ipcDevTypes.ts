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
  realFreedBytes?: number | null;
  discrepancyExplanation?: string | null;
  affectedPathsForReconcile?: string[];
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
