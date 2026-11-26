pub mod blacklist;
pub mod executor;
pub mod folder_picker;
pub mod process;
pub mod project_scanner;
pub mod resolver;
pub mod schema;

use std::collections::HashSet;

use crate::elevation::is_elevated;

use process::{CommandRunner, SystemCommandRunner};
use project_scanner::scan_project_artifacts;
use resolver::{build_docker_wsl_info, resolve_non_project_rules};
use schema::{DevCleanerConfig, DevRulesCatalog, DevScanReport, SafetyLevel};

pub fn scan_dev_cleanup_with_runner(
    config: &DevCleanerConfig,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
    now_ts: i64,
) -> DevScanReport {
    let running_procs = runner.running_processes();
    let disabled_ids: HashSet<String> = config.disabled_rule_ids.iter().cloned().collect();
    let mut warnings = Vec::new();

    let docker_wsl_info = build_docker_wsl_info(runner);

    let mut candidates = resolve_non_project_rules(
        &catalog.rules,
        &catalog.blacklist_never,
        &disabled_ids,
        &running_procs,
        runner,
        &docker_wsl_info,
        &mut warnings,
    );

    let project_candidates = scan_project_artifacts(
        &config.project_roots,
        config.min_project_age_days,
        now_ts,
        &catalog.rules,
        &catalog.blacklist_never,
        &disabled_ids,
        &running_procs,
        runner,
        &mut warnings,
    );

    candidates.extend(project_candidates);

    let safe_recoverable_bytes: u64 = candidates
        .iter()
        .filter(|c| c.nivel == SafetyLevel::Seguro)
        .map(|c| c.size_bytes)
        .sum();
    let review_recoverable_bytes: u64 = candidates
        .iter()
        .filter(|c| c.nivel == SafetyLevel::Revisar)
        .map(|c| c.size_bytes)
        .sum();

    // Collect notable running dev processes for top-level banner
    let tracked_dev_procs = [
        "studio64.exe",
        "code.exe",
        "docker desktop.exe",
        "node.exe",
        "emulator.exe",
        "idea64.exe",
        "devenv.exe",
    ];
    let mut running_dev_processes: Vec<String> = tracked_dev_procs
        .iter()
        .filter(|&&p| running_procs.contains(p))
        .map(|&p| p.to_string())
        .collect();
    running_dev_processes.sort();

    DevScanReport {
        is_elevated: is_elevated(),
        total_recoverable_bytes: safe_recoverable_bytes.saturating_add(review_recoverable_bytes),
        safe_recoverable_bytes,
        review_recoverable_bytes,
        running_dev_processes,
        configured_roots: config.project_roots.clone(),
        min_project_age_days: config.min_project_age_days,
        docker_wsl_info,
        candidates,
        warnings,
    }
}

pub fn scan_dev_cleanup(config: &DevCleanerConfig) -> Result<DevScanReport, String> {
    let catalog = DevRulesCatalog::load_embedded()?;
    let runner = SystemCommandRunner;
    let now_ts = chrono::Utc::now().timestamp();
    Ok(scan_dev_cleanup_with_runner(
        config, &catalog, &runner, now_ts,
    ))
}
