pub mod blacklist;
pub mod caches;
pub mod executor;
pub mod folder_picker;
pub mod process;
pub mod project_scanner;
pub mod resolver;
pub mod schema;

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cleaner::detectors::query_recycle_bin;
use crate::elevation::is_elevated;
use crate::scanner::model::{NodeFlags, ScanTree};

use caches::{batch_check_git_ignored, query_tool_cached};
use process::{matching_blocking_processes, CommandRunner, SystemCommandRunner};
use project_scanner::scan_project_artifacts;
use resolver::{build_docker_wsl_info, expand_env_vars, resolve_non_project_rules};
use schema::{
    CleanMethodKind, DevCandidate, DevCleanerConfig, DevRule, DevRulesCatalog, DevScanReport,
    DockerWslInfo, RuleKind, SafetyLevel,
};

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
    assemble_dev_report(config, candidates, running_procs, docker_wsl_info, warnings)
}

pub fn scan_dev_cleanup(config: &DevCleanerConfig) -> Result<DevScanReport, String> {
    let catalog = DevRulesCatalog::load_embedded()?;
    let runner = SystemCommandRunner;
    let now_ts = chrono::Utc::now().timestamp();
    Ok(scan_dev_cleanup_with_runner(
        config, &catalog, &runner, now_ts,
    ))
}

/// Instant (< 300 ms) Dev Cleaner view builder using precomputed `DerivedIndexes` across all indexed
/// volumes, mtime-invalidated CLI tool cache, and batched `git check-ignore --stdin` per repo.
/// Never walks disk directories upfront!
pub fn scan_dev_cleanup_from_trees(
    config: &DevCleanerConfig,
    trees: &[&ScanTree],
) -> Result<DevScanReport, String> {
    let catalog = DevRulesCatalog::load_embedded()?;
    if trees.is_empty() {
        return scan_dev_cleanup(config);
    }

    let runner = SystemCommandRunner;
    let running_procs = runner.running_processes();
    let disabled_ids: HashSet<String> = config.disabled_rule_ids.iter().cloned().collect();
    let elevated = is_elevated();
    let now_ts = chrono::Utc::now().timestamp();
    let warnings = Vec::new();

    // Cached Docker availability check (zero process spawn when cached & exe mtime unchanged!)
    let (docker_available, _) =
        query_tool_cached("docker", &["--version"], &runner, Duration::from_secs(2));
    let docker_wsl_info = DockerWslInfo {
        docker_available,
        daemon_running: docker_available && running_procs.contains("docker desktop.exe"),
        vhdx_notice: "El disco virtual de Docker/WSL (ext4.vhdx) NO reduce su tamaño automáticamente tras ejecutar `docker prune`. Para recuperar el espacio físico en Windows, compacta el VHDX aparte."
            .to_string(),
        compact_instructions: vec![
            "1. Cierra Docker Desktop completamente.".to_string(),
            "2. En PowerShell (Administrador), detén WSL: `wsl --shutdown`".to_string(),
            "3. Localiza tu disco virtual (usualmente `%LOCALAPPDATA%\\Docker\\wsl\\disk\\docker_data.vhdx` o `...\\wsl\\data\\ext4.vhdx`).".to_string(),
            "4. Si tienes Hyper-V: `Optimize-VHD -Path \"<ruta.vhdx>\" -Mode Full`".to_string(),
            "5. Sin Hyper-V (usando diskpart): `diskpart` → `select vdisk file=\"<ruta.vhdx>\"` → `attach vdisk readonly` → `compact vdisk` → `detach vdisk` → `exit`.".to_string(),
        ],
    };

    let rules_by_id: HashMap<&str, &DevRule> =
        catalog.rules.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut candidates: Vec<DevCandidate> = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 1. Precomputed global caches & system rules from DerivedIndexes (O(caches))
    for &tree in trees {
        for cache in &tree.derived.global_caches {
            let Some(ref rule_id) = cache.rule_id else {
                continue;
            };
            if disabled_ids.contains(rule_id) {
                continue;
            }
            let Some(&rule) = rules_by_id.get(rule_id.as_str()) else {
                continue;
            };
            if rule.nivel == SafetyLevel::Nunca {
                continue;
            }
            // Safety guard (protects even against older cached .idx snapshots):
            // Never treat a special-detector rule's parent env_fallback folder as a direct cache!
            let special = rule.ubicacion.special_detector.as_deref();
            if special.is_some() && special != Some("dir_contents") {
                continue;
            }
            let idx = cache.node_id as usize;
            if idx >= tree.len() {
                continue;
            }
            let flags = tree.flags_of(cache.node_id);
            if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
            {
                continue;
            }
            let size = tree.allocated_size[idx];
            let items = tree.file_count[idx] as u64;
            if size == 0 && items == 0 {
                continue;
            }
            let path_pb = tree.reconstruct_path(cache.node_id);
            let path_str = path_pb.to_string_lossy().into_owned();
            if !seen_paths.insert(path_str.to_ascii_lowercase()) {
                continue;
            }
            let is_dir_contents =
                rule.ubicacion.special_detector.as_deref() == Some("dir_contents");
            if !is_dir_contents
                && (flags.contains(NodeFlags::PROTECTED)
                    || flags.contains(NodeFlags::PROTECTED_DESCENDANT))
            {
                continue;
            }

            let mut cand = build_instant_candidate(
                rule,
                format!("{}:{}", rule.id, cache.node_id),
                cache.label.clone(),
                path_str,
                size,
                items.max(1),
                tree.last_write_ts_of(cache.node_id),
                &running_procs,
                elevated,
            );
            cand.delete_dir_contents_only = is_dir_contents;

            // Lazily verify official CLI via mtime-cached lookup (zero process spawn on cache hit!)
            if rule.metodo.tipo == CleanMethodKind::ComandoOficial {
                if let Some(cmd) = &rule.metodo.comando {
                    if let Some(bin) = cmd.first() {
                        if caches::resolve_executable_mtime(bin).is_none() {
                            if rule.metodo.fallback_borrado_si_sin_cli {
                                cand.method = CleanMethodKind::BorradoPermanente;
                                cand.command = None;
                                cand.allow_recycle_bin_option = true;
                            } else {
                                continue;
                            }
                        }
                    }
                }
            }
            candidates.push(cand);
        }
    }

    // 1b. Run specialized non-project detectors (Android Studio caches, old Gradle dists,
    // VS Code orphan workspaces, etc.) so parent roots like `%LOCALAPPDATA%\Google` are NEVER
    // targeted directly — only their specific safe sub-folders (`AndroidStudio*\caches`, etc.).
    let special_rules: Vec<DevRule> = catalog
        .rules
        .iter()
        .filter(|r| {
            r.tipo != RuleKind::ArtefactoProyecto
                && r.nivel != SafetyLevel::Nunca
                && !disabled_ids.contains(&r.id)
                && matches!(
                    r.ubicacion.special_detector.as_deref(),
                    Some(
                        "gradle_daemon_logs"
                            | "gradle_old_dists"
                            | "vscode_orphan_ws"
                            | "vscode_old_exts"
                            | "jetbrains_caches"
                            | "android_studio_old"
                            | "android_avds"
                    )
                )
        })
        .cloned()
        .collect();
    if !special_rules.is_empty() {
        let mut special_warnings = Vec::new();
        let special_cands = resolve_non_project_rules(
            &special_rules,
            &catalog.blacklist_never,
            &disabled_ids,
            &running_procs,
            &runner,
            &docker_wsl_info,
            &mut special_warnings,
        );
        for c in special_cands {
            if seen_paths.insert(c.path.to_ascii_lowercase()) {
                candidates.push(c);
            }
        }
    }

    // Recycle Bin & DISM virtual rules (instant Win32 metadata queries)
    if let Some(&rb_rule) = rules_by_id.get("sys-recycle-bin") {
        if !disabled_ids.contains(&rb_rule.id) {
            let (rb_bytes, rb_items) = query_recycle_bin();
            if rb_items > 0 || rb_bytes > 0 {
                candidates.push(build_instant_candidate(
                    rb_rule,
                    rb_rule.id.clone(),
                    format!("{} ({} elementos)", rb_rule.nombre, rb_items),
                    "$Recycle.Bin".to_string(),
                    rb_bytes,
                    rb_items,
                    0,
                    &running_procs,
                    elevated,
                ));
            }
        }
    }
    if let Some(&dism_rule) = rules_by_id.get("sys-dism-cleanup") {
        if !disabled_ids.contains(&dism_rule.id) {
            let mut c = build_instant_candidate(
                dism_rule,
                dism_rule.id.clone(),
                dism_rule.nombre.clone(),
                "WinSxS (vía DISM oficial)".to_string(),
                0,
                1,
                0,
                &running_procs,
                elevated,
            );
            c.preselected = false;
            candidates.push(c);
        }
    }

    // 2. Precomputed project artifacts filtered to configured `project_roots` (or all if empty? Respect existing rule: ONLY configured roots!)
    let roots_lower: Vec<String> = config
        .project_roots
        .iter()
        .map(|r| {
            r.trim()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        })
        .filter(|r| !r.is_empty())
        .collect();

    if !roots_lower.is_empty() {
        struct RawArt<'a> {
            rule: &'a DevRule,
            artifact_path: PathBuf,
            project_dir: PathBuf,
            git_root: Option<PathBuf>,
            size_bytes: u64,
            file_count: u64,
            last_write_ts: i64,
            pattern: String,
        }

        let mut raw_arts: Vec<RawArt<'_>> = Vec::new();
        for &tree in trees {
            for art in &tree.derived.dev_artifacts {
                if disabled_ids.contains(&art.rule_id) {
                    continue;
                }
                let Some(&rule) = rules_by_id.get(art.rule_id.as_str()) else {
                    continue;
                };
                let idx = art.node_id as usize;
                if idx >= tree.len() {
                    continue;
                }
                let flags = tree.flags_of(art.node_id);
                if flags.contains(NodeFlags::TOMBSTONE)
                    || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
                {
                    continue;
                }
                let size = tree.allocated_size[idx];
                if size == 0 {
                    continue;
                }
                let artifact_path = tree.reconstruct_path(art.node_id);
                let art_low = artifact_path.to_string_lossy().to_ascii_lowercase();
                if !roots_lower
                    .iter()
                    .any(|r| art_low == *r || art_low.starts_with(&format!(r"{r}\")))
                {
                    continue;
                }
                if !seen_paths.insert(art_low) {
                    continue;
                }
                if flags.contains(NodeFlags::PROTECTED)
                    || flags.contains(NodeFlags::PROTECTED_DESCENDANT)
                {
                    continue;
                }
                let project_dir = tree.reconstruct_path(art.project_dir_id);
                let git_root = find_git_root_fast(&project_dir);
                raw_arts.push(RawArt {
                    rule,
                    artifact_path,
                    project_dir,
                    git_root,
                    size_bytes: size,
                    file_count: tree.file_count[idx] as u64,
                    last_write_ts: tree
                        .last_write_ts_of(art.project_dir_id)
                        .max(tree.last_write_ts_of(art.node_id)),
                    pattern: art.pattern.clone(),
                });
            }
        }

        // Batch `git check-ignore --stdin` ONCE per unique git repository in parallel!
        let mut by_repo: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for a in &raw_arts {
            if let Some(ref gr) = a.git_root {
                by_repo
                    .entry(gr.clone())
                    .or_default()
                    .push(a.artifact_path.clone());
            }
        }

        let ignored_all: HashSet<String> = by_repo
            .into_par_iter()
            .flat_map(|(repo, paths)| {
                batch_check_git_ignored(&repo, &paths)
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect();

        for a in raw_arts {
            let no_git_repo = if a.git_root.is_some() {
                if a.rule.deteccion.require_git_ignored_if_repo
                    && !ignored_all
                        .contains(&a.artifact_path.to_string_lossy().to_ascii_lowercase())
                {
                    continue;
                }
                false
            } else {
                true
            };

            let age_days = if a.last_write_ts > 0 && now_ts > a.last_write_ts {
                ((now_ts - a.last_write_ts) / 86_400) as u64
            } else {
                0
            };
            if a.rule.deteccion.require_project_age_days && age_days < config.min_project_age_days {
                continue;
            }

            let project_name = a
                .project_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.project_dir.to_string_lossy().into_owned());

            let mut c = build_instant_candidate(
                a.rule,
                format!("proj:{}:{}", a.rule.id, a.artifact_path.to_string_lossy()),
                format!("{project_name} / {}", a.pattern),
                a.artifact_path.to_string_lossy().into_owned(),
                a.size_bytes,
                a.file_count,
                a.last_write_ts,
                &running_procs,
                elevated,
            );
            c.tipo = RuleKind::ArtefactoProyecto;
            c.nivel = SafetyLevel::Revisar;
            c.project_name = Some(project_name);
            c.project_root = Some(a.project_dir.to_string_lossy().into_owned());
            c.project_age_days = Some(age_days);
            c.no_git_repo = no_git_repo;
            c.preselected = false;
            if no_git_repo {
                c.warning = Some(match c.warning {
                    Some(w) => {
                        format!("Sin repo Git: verifica que no contenga archivos manuales. {w}")
                    }
                    None => "Sin repo Git: verifica que no contenga archivos manuales.".to_string(),
                });
            }
            candidates.push(c);
        }
    }

    // Also include any rules with `env_fallback` whose path wasn't indexed (e.g. Docker rules when daemon active)
    if docker_wsl_info.docker_available && docker_wsl_info.daemon_running {
        for rule_id in [
            "docker-builder-prune",
            "docker-dangling-images",
            "docker-stopped-containers",
        ] {
            if disabled_ids.contains(rule_id) {
                continue;
            }
            if let Some(&rule) = rules_by_id.get(rule_id) {
                let mut c = build_instant_candidate(
                    rule,
                    rule.id.clone(),
                    rule.nombre.clone(),
                    format!("Docker ({})", rule.id),
                    0,
                    1,
                    0,
                    &running_procs,
                    elevated,
                );
                c.preselected = false;
                candidates.push(c);
            }
        }
    }

    candidates.sort_by_key(|c| std::cmp::Reverse(c.size_bytes));
    let _ = expand_env_vars;
    Ok(assemble_dev_report(
        config,
        candidates,
        running_procs,
        docker_wsl_info,
        warnings,
    ))
}

fn find_git_root_fast(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    let mut depth = 0usize;
    while let Some(c) = cur {
        if c.join(".git").exists() {
            return Some(c.to_path_buf());
        }
        depth += 1;
        if depth >= 6 {
            break;
        }
        cur = c.parent();
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn build_instant_candidate(
    rule: &DevRule,
    id: String,
    nombre: String,
    path: String,
    size_bytes: u64,
    item_count: u64,
    last_modified_ts: i64,
    running_procs: &HashSet<String>,
    elevated: bool,
) -> DevCandidate {
    let blocking_running =
        matching_blocking_processes(&rule.precondiciones.blocking_processes, running_procs);
    let admin_locked = rule.precondiciones.requires_admin && !elevated;
    let mut warn_parts = Vec::new();
    if !blocking_running.is_empty() {
        warn_parts.push(format!(
            "Proceso bloqueante activo ({}); ciérralo o se omitirá al ejecutar.",
            blocking_running.join(", ")
        ));
    }
    if admin_locked {
        warn_parts.push("Requiere permisos de administrador.".to_string());
    }
    let warning = if warn_parts.is_empty() {
        None
    } else {
        Some(warn_parts.join(" "))
    };
    let preselected =
        rule.nivel == SafetyLevel::Seguro && blocking_running.is_empty() && !admin_locked;

    DevCandidate {
        id,
        rule_id: rule.id.clone(),
        nombre,
        categoria: rule.categoria,
        tipo: rule.tipo,
        nivel: rule.nivel,
        path,
        extra_paths: Vec::new(),
        delete_dir_contents_only: false,
        project_name: None,
        project_root: None,
        project_age_days: None,
        no_git_repo: false,
        size_bytes,
        item_count,
        last_modified_ts,
        method: rule.metodo.tipo,
        command: rule.metodo.comando.clone(),
        pre_commands: rule.precondiciones.pre_commands.clone(),
        allow_recycle_bin_option: rule.metodo.allow_recycle_bin_option,
        requires_admin: rule.precondiciones.requires_admin,
        blocking_processes_running: blocking_running,
        blocking_processes_spec: rule.precondiciones.blocking_processes.clone(),
        warning,
        blocked_reason: None,
        como_regenerar: rule.como_regenerar.clone(),
        preselected,
    }
}

fn assemble_dev_report(
    config: &DevCleanerConfig,
    candidates: Vec<DevCandidate>,
    running_procs: HashSet<String>,
    docker_wsl_info: DockerWslInfo,
    warnings: Vec<String>,
) -> DevScanReport {
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
