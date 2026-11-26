use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::cleaner::guardrails::is_reparse_point;

use super::blacklist::{matches_simple_glob, validate_dev_target_path};
use super::process::{matching_blocking_processes, CommandRunner};
use super::resolver::measure_dir_no_reparse;
use super::schema::{BlacklistNeverSpec, DevCandidate, DevRule, RuleKind, SafetyLevel};

const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SCAN_DEPTH: usize = 8;

/// Scans ONLY the user-configured project roots for regenerable project artifacts.
/// - Never follows symlinks or junctions (`is_reparse_point`).
/// - Never descends inside detected artifacts.
/// - Enforces: (1) required manifest, (2) `git check-ignore` when inside a git repo,
///   (3) project age >= `min_age_days` based on last git commit or source file mtime.
#[allow(clippy::too_many_arguments)]
pub fn scan_project_artifacts(
    project_roots: &[String],
    min_age_days: u64,
    now_ts: i64,
    rules: &[DevRule],
    blacklist: &BlacklistNeverSpec,
    disabled_ids: &HashSet<String>,
    running_procs: &HashSet<String>,
    runner: &dyn CommandRunner,
    warnings: &mut Vec<String>,
) -> Vec<DevCandidate> {
    let artifact_rules: Vec<&DevRule> = rules
        .iter()
        .filter(|r| {
            r.tipo == RuleKind::ArtefactoProyecto
                && r.nivel != SafetyLevel::Nunca
                && !disabled_ids.contains(&r.id)
        })
        .collect();

    if artifact_rules.is_empty() {
        return Vec::new();
    }

    // Collect all single-segment artifact folder names so we never descend into them
    let mut stop_dir_names: HashSet<String> = HashSet::new();
    stop_dir_names.insert(".git".to_string());
    for rule in &artifact_rules {
        for pat in &rule.ubicacion.folder_patterns {
            if !pat.contains('\\') && !pat.contains('/') {
                stop_dir_names.insert(pat.to_ascii_lowercase());
            }
        }
    }

    let mut candidates = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for root_str in project_roots {
        let root = PathBuf::from(root_str.trim());
        if root_str.trim().is_empty() {
            continue;
        }
        if !root.is_dir() {
            warnings.push(format!(
                "Raíz de proyectos no encontrada o inaccesible: {}",
                root.display()
            ));
            continue;
        }
        if is_reparse_point(&root) {
            warnings.push(format!(
                "Raíz omitida por ser symlink/junction: {}",
                root.display()
            ));
            continue;
        }

        walk_root(
            &root,
            &root,
            0,
            min_age_days,
            now_ts,
            &artifact_rules,
            &stop_dir_names,
            blacklist,
            running_procs,
            runner,
            &mut seen_paths,
            &mut candidates,
        );
    }

    candidates.sort_by_key(|c| std::cmp::Reverse(c.size_bytes));
    candidates
}

#[allow(clippy::too_many_arguments)]
fn walk_root(
    configured_root: &Path,
    dir: &Path,
    depth: usize,
    min_age_days: u64,
    now_ts: i64,
    rules: &[&DevRule],
    stop_dir_names: &HashSet<String>,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    runner: &dyn CommandRunner,
    seen_paths: &mut HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    if depth > MAX_SCAN_DEPTH || is_reparse_point(dir) {
        return;
    }

    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut entry_names_lower: Vec<String> = Vec::new();

    for entry in rd.flatten() {
        let p = entry.path();
        if is_reparse_point(&p) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let name_low = name.to_ascii_lowercase();
        entry_names_lower.push(name_low.clone());
        if let Ok(meta) = std::fs::symlink_metadata(&p) {
            if meta.is_dir() && !meta.file_type().is_symlink() {
                subdirs.push(p);
            }
        }
    }

    // Track which subdirectories inside `dir` are matched artifacts so we NEVER descend into them
    let mut matched_artifact_dirs: HashSet<String> = HashSet::new();
    let git_root_opt = find_git_root_within(dir, configured_root);

    for rule in rules {
        if !has_required_manifest(&entry_names_lower, &rule.deteccion.required_markers) {
            continue;
        }

        for pattern in &rule.ubicacion.folder_patterns {
            let artifact_path = dir.join(pattern);
            if !artifact_path.is_dir() || is_reparse_point(&artifact_path) {
                continue;
            }

            // Record first segment so we do not recurse into this artifact
            if let Some(first_seg) = pattern.split(['\\', '/']).next() {
                matched_artifact_dirs.insert(first_seg.to_ascii_lowercase());
            }

            let key = artifact_path.to_string_lossy().to_ascii_lowercase();
            if seen_paths.contains(&key) {
                continue;
            }

            // Mandatory blacklist check (level `nunca` prevails over every rule)
            if validate_dev_target_path(&artifact_path, blacklist).is_err() {
                continue;
            }

            // Condition 2: If inside a git repo, `git check-ignore` MUST confirm ignored
            let no_git_repo = if let Some(ref git_root) = git_root_opt {
                if rule.deteccion.require_git_ignored_if_repo
                    && !is_git_ignored(git_root, &artifact_path, runner)
                {
                    continue;
                }
                false
            } else {
                true
            };

            // Condition 3: Project age based on last git commit or source mtime (NOT artifact mtime)
            let project_last_ts = compute_project_last_activity_ts(
                dir,
                git_root_opt.as_deref(),
                stop_dir_names,
                runner,
            );
            let age_days = if project_last_ts > 0 && now_ts > project_last_ts {
                ((now_ts - project_last_ts) / 86_400) as u64
            } else {
                0
            };

            if rule.deteccion.require_project_age_days && age_days < min_age_days {
                continue;
            }

            let (bytes, count, art_ts) = measure_dir_no_reparse(&artifact_path);
            if bytes == 0 && count == 0 {
                continue;
            }

            seen_paths.insert(key);
            let project_name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.to_string_lossy().into_owned());

            let blocking_running =
                matching_blocking_processes(&rule.precondiciones.blocking_processes, running_procs);

            let mut warn_parts = Vec::new();
            if no_git_repo {
                warn_parts
                    .push("Sin repo Git: verifica que no contenga archivos manuales.".to_string());
            }
            if !blocking_running.is_empty() {
                warn_parts.push(format!(
                    "Proceso bloqueante activo ({}); se omitirá si sigue corriendo.",
                    blocking_running.join(", ")
                ));
            }
            let warning = if warn_parts.is_empty() {
                None
            } else {
                Some(warn_parts.join(" "))
            };

            out.push(DevCandidate {
                id: format!("proj:{}:{}", rule.id, artifact_path.to_string_lossy()),
                rule_id: rule.id.clone(),
                nombre: format!("{project_name} / {pattern}"),
                categoria: rule.categoria,
                tipo: RuleKind::ArtefactoProyecto,
                nivel: SafetyLevel::Revisar,
                path: artifact_path.to_string_lossy().into_owned(),
                extra_paths: Vec::new(),
                delete_dir_contents_only: false,
                project_name: Some(project_name),
                project_root: Some(dir.to_string_lossy().into_owned()),
                project_age_days: Some(age_days),
                no_git_repo,
                size_bytes: bytes,
                item_count: count,
                last_modified_ts: if project_last_ts > 0 {
                    project_last_ts
                } else {
                    art_ts
                },
                method: rule.metodo.tipo,
                command: None,
                pre_commands: rule.precondiciones.pre_commands.clone(),
                allow_recycle_bin_option: rule.metodo.allow_recycle_bin_option,
                requires_admin: false,
                blocking_processes_running: blocking_running,
                blocking_processes_spec: rule.precondiciones.blocking_processes.clone(),
                warning,
                blocked_reason: None,
                como_regenerar: rule.como_regenerar.clone(),
                preselected: false,
            });
        }
    }

    // Recurse into non-artifact, non-`.git` subdirectories
    for sub in subdirs {
        let name_low = sub
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if name_low == ".git"
            || matched_artifact_dirs.contains(&name_low)
            || stop_dir_names.contains(&name_low)
        {
            continue;
        }
        walk_root(
            configured_root,
            &sub,
            depth + 1,
            min_age_days,
            now_ts,
            rules,
            stop_dir_names,
            blacklist,
            running_procs,
            runner,
            seen_paths,
            out,
        );
    }
}

fn has_required_manifest(entries_lower: &[String], markers: &[String]) -> bool {
    if markers.is_empty() {
        return true;
    }
    for marker in markers {
        let pat = marker.to_ascii_lowercase();
        if entries_lower.iter().any(|e| matches_simple_glob(&pat, e)) {
            return true;
        }
    }
    false
}

fn find_git_root_within(dir: &Path, configured_root: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    while let Some(c) = cur {
        if c.join(".git").exists() {
            return Some(c.to_path_buf());
        }
        if c == configured_root {
            break;
        }
        cur = c.parent();
    }
    None
}

fn is_git_ignored(git_root: &Path, artifact_path: &Path, runner: &dyn CommandRunner) -> bool {
    let rel = artifact_path
        .strip_prefix(git_root)
        .unwrap_or(artifact_path)
        .to_string_lossy()
        .replace('\\', "/");
    let res = runner.run(
        "git",
        &["check-ignore", "-q", &rel],
        Some(git_root),
        GIT_TIMEOUT,
    );
    // `git check-ignore -q` exits 0 iff the path is ignored
    res.exit_code == Some(0)
}

fn compute_project_last_activity_ts(
    project_dir: &Path,
    git_root: Option<&Path>,
    stop_dir_names: &HashSet<String>,
    runner: &dyn CommandRunner,
) -> i64 {
    if let Some(root) = git_root {
        let res = runner.run(
            "git",
            &["log", "-1", "--format=%ct"],
            Some(root),
            GIT_TIMEOUT,
        );
        if res.success {
            if let Ok(ts) = res.stdout.trim().parse::<i64>() {
                if ts > 0 {
                    return ts;
                }
            }
        }
    }

    // Fallback: newest mtime of source files inside `project_dir` (excluding artifacts and `.git`)
    let mut newest = 0i64;
    let mut stack = vec![(project_dir.to_path_buf(), 0usize)];
    let mut visited = 0usize;

    while let Some((d, depth)) = stack.pop() {
        if depth > 4 || visited > 2_000 || is_reparse_point(&d) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            visited += 1;
            if visited > 2_000 {
                break;
            }
            let p = entry.path();
            if is_reparse_point(&p) {
                continue;
            }
            let name_low = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name_low == ".git" || stop_dir_names.contains(&name_low) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push((p, depth + 1));
            } else if let Ok(m) = meta.modified() {
                if let Ok(dur) = m.duration_since(UNIX_EPOCH) {
                    newest = newest.max(dur.as_secs() as i64);
                }
            }
        }
    }

    newest
}
