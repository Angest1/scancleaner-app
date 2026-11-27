use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::app_data_dir;

pub const EMBEDDED_RULES_JSON: &str = include_str!("rules.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleCategory {
    Node,
    PHP,
    Python,
    Android,
    Docker,
    IDE,
    Sistema,
    Testing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    CacheGlobal,
    ArtefactoProyecto,
    Sistema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyLevel {
    Seguro,
    Revisar,
    Nunca,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CleanMethodKind {
    ComandoOficial,
    BorradoPermanente,
    Papelera,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleLocation {
    #[serde(default)]
    pub cli_query: Option<Vec<String>>,
    #[serde(default)]
    pub env_fallback: Vec<String>,
    #[serde(default)]
    pub folder_patterns: Vec<String>,
    #[serde(default)]
    pub special_detector: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDetection {
    #[serde(default)]
    pub required_markers: Vec<String>,
    #[serde(default)]
    pub require_git_ignored_if_repo: bool,
    #[serde(default)]
    pub require_project_age_days: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMethod {
    pub tipo: CleanMethodKind,
    #[serde(default)]
    pub comando: Option<Vec<String>>,
    #[serde(default)]
    pub fallback_borrado_si_sin_cli: bool,
    #[serde(default)]
    pub allow_recycle_bin_option: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePreconditions {
    #[serde(default)]
    pub blocking_processes: Vec<String>,
    #[serde(default)]
    pub pre_commands: Vec<Vec<String>>,
    #[serde(default)]
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevRule {
    pub id: String,
    pub nombre: String,
    pub categoria: RuleCategory,
    pub tipo: RuleKind,
    pub ubicacion: RuleLocation,
    #[serde(default)]
    pub deteccion: RuleDetection,
    pub nivel: SafetyLevel,
    pub metodo: RuleMethod,
    #[serde(default)]
    pub precondiciones: RulePreconditions,
    /// Rutas afectadas por la regla (usadas por la cola de reconciliación post-operación).
    /// Opcional para no romper reglas existentes.
    #[serde(default)]
    pub rutas_afectadas: Vec<String>,
    pub como_regenerar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistNeverSpec {
    pub exact_names: Vec<String>,
    pub glob_patterns: Vec<String>,
    pub db_dir_names: Vec<String>,
    pub source_extensions: Vec<String>,
    pub forbid_docker_volumes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevRulesCatalog {
    pub schema_version: u32,
    pub blacklist_never: BlacklistNeverSpec,
    pub rules: Vec<DevRule>,
}

impl DevRulesCatalog {
    pub fn load_embedded() -> Result<Self, String> {
        let catalog: Self = serde_json::from_str(EMBEDDED_RULES_JSON)
            .map_err(|e| format!("Error deserializando rules.json embebido: {e}"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for rule in &self.rules {
            if rule.id.trim().is_empty() {
                return Err("Regla con id vacío.".to_string());
            }
            if !seen.insert(rule.id.clone()) {
                return Err(format!("ID de regla duplicado: {}", rule.id));
            }
            if rule.nombre.trim().is_empty() || rule.como_regenerar.trim().is_empty() {
                return Err(format!(
                    "Regla '{}' debe tener nombre y comoRegenerar.",
                    rule.id
                ));
            }
            if rule.metodo.tipo == CleanMethodKind::ComandoOficial
                && rule
                    .metodo
                    .comando
                    .as_ref()
                    .is_none_or(|cmd| cmd.is_empty())
            {
                return Err(format!(
                    "Regla '{}' usa comando-oficial pero no define comando.",
                    rule.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanerConfig {
    #[serde(default)]
    pub project_roots: Vec<String>,
    #[serde(default = "default_min_age_days")]
    pub min_project_age_days: u64,
    #[serde(default)]
    pub disabled_rule_ids: Vec<String>,
}

fn default_min_age_days() -> u64 {
    30
}

impl Default for DevCleanerConfig {
    fn default() -> Self {
        Self {
            project_roots: Vec::new(),
            min_project_age_days: default_min_age_days(),
            disabled_rule_ids: Vec::new(),
        }
    }
}

pub fn dev_config_path() -> PathBuf {
    app_data_dir().join("dev-cleaner-config.json")
}

impl DevCleanerConfig {
    pub fn load_from(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_from(&dev_config_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&dev_config_path())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCandidate {
    pub id: String,
    pub rule_id: String,
    pub nombre: String,
    pub categoria: RuleCategory,
    pub tipo: RuleKind,
    pub nivel: SafetyLevel,
    pub path: String,
    pub extra_paths: Vec<String>,
    pub delete_dir_contents_only: bool,
    pub project_name: Option<String>,
    pub project_root: Option<String>,
    pub project_age_days: Option<u64>,
    pub no_git_repo: bool,
    pub size_bytes: u64,
    pub item_count: u64,
    pub last_modified_ts: i64,
    pub method: CleanMethodKind,
    pub command: Option<Vec<String>>,
    pub pre_commands: Vec<Vec<String>>,
    pub allow_recycle_bin_option: bool,
    pub requires_admin: bool,
    pub blocking_processes_running: Vec<String>,
    pub blocking_processes_spec: Vec<String>,
    pub warning: Option<String>,
    pub blocked_reason: Option<String>,
    pub como_regenerar: String,
    pub preselected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerWslInfo {
    pub docker_available: bool,
    pub daemon_running: bool,
    pub vhdx_notice: String,
    pub compact_instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevScanReport {
    pub is_elevated: bool,
    pub total_recoverable_bytes: u64,
    pub safe_recoverable_bytes: u64,
    pub review_recoverable_bytes: u64,
    pub running_dev_processes: Vec<String>,
    pub configured_roots: Vec<String>,
    pub min_project_age_days: u64,
    pub docker_wsl_info: DockerWslInfo,
    pub candidates: Vec<DevCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevPreviewItemInput {
    pub candidate: DevCandidate,
    pub use_recycle_bin: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevPreviewRequest {
    pub items: Vec<DevPreviewItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevPlannedItem {
    pub id: String,
    pub rule_id: String,
    pub nombre: String,
    pub categoria: RuleCategory,
    pub nivel: SafetyLevel,
    pub path: String,
    pub extra_paths: Vec<String>,
    pub delete_dir_contents_only: bool,
    pub estimated_bytes: u64,
    pub effective_method: CleanMethodKind,
    pub command: Option<Vec<String>>,
    pub pre_commands: Vec<Vec<String>>,
    pub blocking_processes_spec: Vec<String>,
    pub requires_admin: bool,
    pub como_regenerar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevBlockedItem {
    pub id: String,
    pub nombre: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanupPlan {
    pub plan_id: String,
    pub items: Vec<DevPlannedItem>,
    pub blocked_items: Vec<DevBlockedItem>,
    pub total_estimated_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevExecutionOutcome {
    pub id: String,
    pub rule_id: String,
    pub nombre: String,
    pub path: String,
    pub method_label: String,
    pub estimated_bytes: u64,
    pub freed_bytes: u64,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanupResult {
    pub plan_id: String,
    pub total_estimated_bytes: u64,
    pub total_freed_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_freed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discrepancy_explanation: Option<String>,
    #[serde(default)]
    pub affected_paths_for_reconcile: Vec<String>,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub outcomes: Vec<DevExecutionOutcome>,
}
