use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::cleaner::detectors::query_recycle_bin;
use crate::cleaner::guardrails::is_reparse_point;
use crate::elevation::is_elevated;

use super::blacklist::validate_dev_target_path;
use super::process::{matching_blocking_processes, CommandRunner};
use super::schema::{
    BlacklistNeverSpec, CleanMethodKind, DevCandidate, DevRule, DockerWslInfo, RuleKind,
    SafetyLevel,
};

const CLI_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Expands `%VAR%` environment placeholders using the current process environment.
/// Returns `None` if any referenced variable is missing or empty.
pub fn expand_env_vars(template: &str) -> Option<PathBuf> {
    let mut result = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(rel_end) = template[i + 1..].find('%') {
                let var_name = &template[i + 1..i + 1 + rel_end];
                if !var_name.is_empty() {
                    let val = std::env::var(var_name).ok()?;
                    if val.trim().is_empty() {
                        return None;
                    }
                    result.push_str(val.trim_end_matches('\\'));
                    i += rel_end + 2;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    if result.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(result))
    }
}

/// Extracts the last non-empty line from CLI stdout that looks like a Windows filesystem path
/// (filtering out warnings like Corepack download notices).
pub fn parse_cli_path_stdout(stdout: &str) -> Option<PathBuf> {
    for line in stdout.lines().rev() {
        let trimmed = line.trim().trim_matches('"');
        if trimmed.is_empty() {
            continue;
        }
        let looks_like_win_path = (trimmed.len() >= 3
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/'))
            || trimmed.starts_with(r"\\");
        if looks_like_win_path {
            return Some(PathBuf::from(trimmed));
        }
    }
    None
}

/// Computes total bytes, file count, and newest modification timestamp without following
/// symlinks or junctions.
pub fn measure_dir_no_reparse(path: &Path) -> (u64, u64, i64) {
    if !path.exists() || is_reparse_point(path) {
        return (0, 0, 0);
    }
    let Ok(root_meta) = std::fs::symlink_metadata(path) else {
        return (0, 0, 0);
    };
    if root_meta.is_file() {
        let ts = root_meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        return (root_meta.len(), 1, ts);
    }

    let mut stack = vec![path.to_path_buf()];
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut newest_ts = 0i64;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if is_reparse_point(&p) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(p);
            } else {
                bytes = bytes.saturating_add(meta.len());
                count += 1;
                if let Ok(m) = meta.modified() {
                    if let Ok(dur) = m.duration_since(UNIX_EPOCH) {
                        newest_ts = newest_ts.max(dur.as_secs() as i64);
                    }
                }
            }
        }
    }
    (bytes, count, newest_ts)
}

pub fn build_docker_wsl_info(runner: &dyn CommandRunner) -> DockerWslInfo {
    let ver = runner.run("docker", &["--version"], None, CLI_QUERY_TIMEOUT);
    let docker_available = ver.success && !ver.not_found;
    let daemon_running = if docker_available {
        let info = runner.run(
            "docker",
            &["info", "--format", "{{.ID}}"],
            None,
            CLI_QUERY_TIMEOUT,
        );
        info.success
    } else {
        false
    };

    DockerWslInfo {
        docker_available,
        daemon_running,
        vhdx_notice: "El disco virtual de Docker/WSL (ext4.vhdx) NO reduce su tamaño automáticamente tras ejecutar `docker prune`. Para recuperar el espacio físico en Windows, compacta el VHDX aparte."
            .to_string(),
        compact_instructions: vec![
            "1. Cierra Docker Desktop completamente.".to_string(),
            "2. En PowerShell (Administrador), detén WSL: `wsl --shutdown`".to_string(),
            "3. Localiza tu disco virtual (usualmente `%LOCALAPPDATA%\\Docker\\wsl\\disk\\docker_data.vhdx` o `...\\wsl\\data\\ext4.vhdx`).".to_string(),
            "4. Si tienes Hyper-V: `Optimize-VHD -Path \"<ruta.vhdx>\" -Mode Full`".to_string(),
            "5. Sin Hyper-V (usando diskpart): `diskpart` → `select vdisk file=\"<ruta.vhdx>\"` → `attach vdisk readonly` → `compact vdisk` → `detach vdisk` → `exit`.".to_string(),
        ],
    }
}

pub fn resolve_non_project_rules(
    rules: &[DevRule],
    blacklist: &BlacklistNeverSpec,
    disabled_ids: &HashSet<String>,
    running_procs: &HashSet<String>,
    runner: &dyn CommandRunner,
    docker_info: &DockerWslInfo,
    warnings: &mut Vec<String>,
) -> Vec<DevCandidate> {
    let elevated = is_elevated();
    let mut out = Vec::new();

    for rule in rules {
        if rule.tipo == RuleKind::ArtefactoProyecto
            || rule.nivel == SafetyLevel::Nunca
            || disabled_ids.contains(&rule.id)
        {
            continue;
        }

        let special = rule.ubicacion.special_detector.as_deref();
        match special {
            Some("recycle_bin") => {
                resolve_recycle_bin(rule, &mut out);
            }
            Some("dism_cleanup") => {
                resolve_dism(rule, elevated, &mut out);
            }
            Some("dir_contents") => {
                resolve_dir_contents(rule, blacklist, running_procs, elevated, &mut out);
            }
            Some("gradle_daemon_logs") => {
                resolve_gradle_daemon_logs(rule, blacklist, running_procs, &mut out);
            }
            Some("gradle_old_dists") => {
                resolve_gradle_old_dists(rule, blacklist, running_procs, &mut out);
            }
            Some("vscode_orphan_ws") => {
                resolve_vscode_orphan_workspaces(rule, blacklist, running_procs, &mut out);
            }
            Some("vscode_old_exts") => {
                resolve_vscode_old_extensions(rule, blacklist, running_procs, &mut out);
            }
            Some("jetbrains_caches") => {
                resolve_jetbrains_caches(rule, blacklist, running_procs, &mut out);
            }
            Some("android_studio_old") => {
                resolve_android_studio_old_versions(rule, blacklist, running_procs, &mut out);
            }
            Some("android_sdk_unused") => {
                resolve_android_sdk_unused(rule, running_procs, runner, warnings, &mut out);
            }
            Some("android_avds") => {
                resolve_android_avds(rule, blacklist, running_procs, &mut out);
            }
            Some("docker_builder") => {
                resolve_docker_rule(rule, docker_info, runner, "Build Cache", &mut out);
            }
            Some("docker_dangling_images") => {
                resolve_docker_rule(rule, docker_info, runner, "Images", &mut out);
            }
            Some("docker_stopped_containers") => {
                resolve_docker_rule(rule, docker_info, runner, "Containers", &mut out);
            }
            _ => {
                resolve_standard_global_cache(
                    rule,
                    blacklist,
                    running_procs,
                    runner,
                    elevated,
                    &mut out,
                );
            }
        }
    }

    out
}

#[allow(clippy::too_many_arguments)]
fn make_base_candidate(
    rule: &DevRule,
    id: String,
    nombre: String,
    path: String,
    size_bytes: u64,
    item_count: u64,
    last_modified_ts: i64,
    running_procs: &HashSet<String>,
) -> DevCandidate {
    let blocking_running =
        matching_blocking_processes(&rule.precondiciones.blocking_processes, running_procs);
    let elevated = is_elevated();
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

fn resolve_standard_global_cache(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    runner: &dyn CommandRunner,
    _elevated: bool,
    out: &mut Vec<DevCandidate>,
) {
    let mut resolved_paths: Vec<PathBuf> = Vec::new();
    let mut cli_missing = false;

    if let Some(cli_query) = &rule.ubicacion.cli_query {
        if let Some((prog, args)) = cli_query.split_first() {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let res = runner.run(prog, &arg_refs, None, CLI_QUERY_TIMEOUT);
            if res.not_found {
                cli_missing = true;
            } else if res.success {
                if let Some(p) = parse_cli_path_stdout(&res.stdout) {
                    if p.exists() {
                        resolved_paths.push(p);
                    }
                }
            }
        }
    }

    // Fallback to environment variable paths (also checks additional paths if multiple are defined, e.g. VS Code)
    if resolved_paths.is_empty() || rule.ubicacion.env_fallback.len() > 1 {
        for tmpl in &rule.ubicacion.env_fallback {
            if let Some(p) = expand_env_vars(tmpl) {
                if p.exists()
                    && !resolved_paths.iter().any(|x| {
                        x.to_string_lossy()
                            .eq_ignore_ascii_case(&p.to_string_lossy())
                    })
                {
                    resolved_paths.push(p);
                }
            }
        }
    }

    // Also check if official command binary exists when cli_query wasn't specified
    if !cli_missing
        && rule.metodo.tipo == CleanMethodKind::ComandoOficial
        && rule.ubicacion.cli_query.is_none()
    {
        if let Some(cmd) = &rule.metodo.comando {
            if let Some(bin) = cmd.first() {
                let probe = runner.run(bin, &["--version"], None, CLI_QUERY_TIMEOUT);
                if probe.not_found {
                    cli_missing = true;
                }
            }
        }
    }

    for (idx, path) in resolved_paths.into_iter().enumerate() {
        if validate_dev_target_path(&path, blacklist).is_err() {
            continue;
        }
        let (bytes, count, ts) = measure_dir_no_reparse(&path);
        if bytes == 0 && count == 0 {
            continue;
        }

        let display = if idx == 0 {
            rule.nombre.clone()
        } else {
            let leaf = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{} ({leaf})", rule.nombre)
        };

        let mut cand = make_base_candidate(
            rule,
            format!("{}:{idx}", rule.id),
            display,
            path.to_string_lossy().into_owned(),
            bytes,
            count,
            ts,
            running_procs,
        );

        // If official CLI is missing, degrade to direct deletion ONLY when explicitly permitted by the rule
        if cli_missing && rule.metodo.tipo == CleanMethodKind::ComandoOficial {
            if rule.metodo.fallback_borrado_si_sin_cli {
                cand.method = CleanMethodKind::BorradoPermanente;
                cand.command = None;
                cand.allow_recycle_bin_option = true;
                let note =
                    "CLI oficial no encontrado en PATH; se usará borrado directo de la caché.";
                cand.warning = Some(match cand.warning {
                    Some(w) => format!("{w} {note}"),
                    None => note.to_string(),
                });
            } else {
                continue;
            }
        }

        out.push(cand);
    }
}

fn resolve_dir_contents(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    _elevated: bool,
    out: &mut Vec<DevCandidate>,
) {
    for (idx, tmpl) in rule.ubicacion.env_fallback.iter().enumerate() {
        let Some(dir) = expand_env_vars(tmpl) else {
            continue;
        };
        if !dir.is_dir() || is_reparse_point(&dir) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut total_bytes = 0u64;
        let mut total_items = 0u64;
        let mut newest_ts = 0i64;
        for entry in rd.flatten() {
            let child = entry.path();
            if validate_dev_target_path(&child, blacklist).is_err() {
                continue;
            }
            let (b, c, ts) = measure_dir_no_reparse(&child);
            total_bytes = total_bytes.saturating_add(b);
            total_items = total_items.saturating_add(c.max(1));
            newest_ts = newest_ts.max(ts);
        }
        if total_items == 0 {
            continue;
        }
        let label = if rule.ubicacion.env_fallback.len() > 1 {
            let leaf = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{} ({leaf})", rule.nombre)
        } else {
            rule.nombre.clone()
        };
        let mut cand = make_base_candidate(
            rule,
            format!("{}:{idx}", rule.id),
            label,
            dir.to_string_lossy().into_owned(),
            total_bytes,
            total_items,
            newest_ts,
            running_procs,
        );
        cand.delete_dir_contents_only = true;
        out.push(cand);
    }
}

fn resolve_recycle_bin(rule: &DevRule, out: &mut Vec<DevCandidate>) {
    let (bytes, items) = query_recycle_bin();
    if items == 0 && bytes == 0 {
        return;
    }
    let empty_procs = HashSet::new();
    let cand = make_base_candidate(
        rule,
        rule.id.clone(),
        format!("{} ({} elementos)", rule.nombre, items),
        "$Recycle.Bin".to_string(),
        bytes,
        items,
        0,
        &empty_procs,
    );
    out.push(cand);
}

fn resolve_dism(rule: &DevRule, elevated: bool, out: &mut Vec<DevCandidate>) {
    let empty_procs = HashSet::new();
    let mut cand = make_base_candidate(
        rule,
        rule.id.clone(),
        rule.nombre.clone(),
        "WinSxS (vía DISM oficial)".to_string(),
        0,
        1,
        0,
        &empty_procs,
    );
    cand.preselected = false;
    if !elevated {
        cand.warning =
            Some("Requiere reiniciar como administrador para ejecutar DISM.".to_string());
    }
    out.push(cand);
}

fn resolve_gradle_daemon_logs(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(base) = rule
        .ubicacion
        .env_fallback
        .first()
        .and_then(|t| expand_env_vars(t))
    else {
        return;
    };
    if !base.is_dir() || is_reparse_point(&base) {
        return;
    }

    let mut log_files = Vec::new();
    let mut total_bytes = 0u64;
    let mut newest_ts = 0i64;

    if let Ok(rd) = std::fs::read_dir(&base) {
        for ver_dir in rd.flatten() {
            let vp = ver_dir.path();
            if !vp.is_dir() || is_reparse_point(&vp) {
                continue;
            }
            if let Ok(sub_rd) = std::fs::read_dir(&vp) {
                for entry in sub_rd.flatten() {
                    let fp = entry.path();
                    if fp
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("log"))
                        && validate_dev_target_path(&fp, blacklist).is_ok()
                    {
                        let (b, _, ts) = measure_dir_no_reparse(&fp);
                        total_bytes = total_bytes.saturating_add(b);
                        newest_ts = newest_ts.max(ts);
                        log_files.push(fp.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }

    if log_files.is_empty() {
        return;
    }

    let first = log_files[0].clone();
    let rest = log_files[1..].to_vec();
    let mut cand = make_base_candidate(
        rule,
        rule.id.clone(),
        format!("{} ({} archivos .log)", rule.nombre, log_files.len()),
        first,
        total_bytes,
        log_files.len() as u64,
        newest_ts,
        running_procs,
    );
    cand.extra_paths = rest;
    out.push(cand);
}

fn resolve_gradle_old_dists(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(base) = rule
        .ubicacion
        .env_fallback
        .first()
        .and_then(|t| expand_env_vars(t))
    else {
        return;
    };
    if !base.is_dir() || is_reparse_point(&base) {
        return;
    }

    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };
    let mut dists: Vec<(PathBuf, u64, u64, i64)> = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_dir() || is_reparse_point(&p) || validate_dev_target_path(&p, blacklist).is_err() {
            continue;
        }
        let (b, c, ts) = measure_dir_no_reparse(&p);
        dists.push((p, b, c, ts));
    }

    if dists.len() <= 1 {
        return;
    }
    // Keep the most recently modified distribution; mark older ones as cleanable
    dists.sort_by_key(|d| std::cmp::Reverse(d.3));
    for (idx, (path, bytes, count, ts)) in dists.into_iter().skip(1).enumerate() {
        if bytes == 0 {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(make_base_candidate(
            rule,
            format!("{}:{idx}", rule.id),
            format!("Gradle Wrapper antiguo — {name}"),
            path.to_string_lossy().into_owned(),
            bytes,
            count,
            ts,
            running_procs,
        ));
    }
}

fn decode_vscode_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:///")?;
    let mut decoded = String::with_capacity(rest.len());
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                decoded.push(hex as char);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i] as char);
        i += 1;
    }
    let normalized = decoded.replace('/', "\\");
    Some(PathBuf::from(normalized))
}

fn resolve_vscode_orphan_workspaces(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(base) = rule
        .ubicacion
        .env_fallback
        .first()
        .and_then(|t| expand_env_vars(t))
    else {
        return;
    };
    if !base.is_dir() || is_reparse_point(&base) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };

    let mut orphan_dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_items = 0u64;
    let mut newest_ts = 0i64;

    for entry in rd.flatten() {
        let ws_dir = entry.path();
        if !ws_dir.is_dir() || is_reparse_point(&ws_dir) {
            continue;
        }
        let ws_json = ws_dir.join("workspace.json");
        let Ok(raw) = std::fs::read_to_string(&ws_json) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let uri_opt = val
            .get("folder")
            .or_else(|| val.get("workspace"))
            .and_then(|v| v.as_str());
        let Some(uri) = uri_opt else {
            continue;
        };
        // Only check local file:/// workspaces (never remote ssh/wsl/dev-container URIs)
        let Some(target_path) = decode_vscode_file_uri(uri) else {
            continue;
        };
        if !target_path.exists() && validate_dev_target_path(&ws_dir, blacklist).is_ok() {
            let (b, c, ts) = measure_dir_no_reparse(&ws_dir);
            total_bytes = total_bytes.saturating_add(b);
            total_items = total_items.saturating_add(c.max(1));
            newest_ts = newest_ts.max(ts);
            orphan_dirs.push(ws_dir.to_string_lossy().into_owned());
        }
    }

    if orphan_dirs.is_empty() {
        return;
    }
    let first = orphan_dirs[0].clone();
    let rest = orphan_dirs[1..].to_vec();
    let mut cand = make_base_candidate(
        rule,
        rule.id.clone(),
        format!("{} ({} carpetas huérfanas)", rule.nombre, orphan_dirs.len()),
        first,
        total_bytes,
        total_items,
        newest_ts,
        running_procs,
    );
    cand.extra_paths = rest;
    out.push(cand);
}

fn split_vscode_ext_dir(name: &str) -> Option<(String, Vec<u64>)> {
    // Format: publisher.ext-name-1.2.3 or publisher.ext-name-1.2.3-win32-x64
    let mut parts: Vec<&str> = name.split('-').collect();
    if parts.len() < 2 {
        return None;
    }
    // Strip trailing platform tags like win32-x64, universal
    while parts.len() > 2
        && matches!(
            parts.last().copied().unwrap_or(""),
            "win32" | "x64" | "arm64" | "ia32" | "universal" | "alpine" | "linux" | "darwin"
        )
    {
        parts.pop();
    }
    let ver_str = parts.pop()?;
    let ver_nums: Option<Vec<u64>> = ver_str.split('.').map(|s| s.parse::<u64>().ok()).collect();
    let nums = ver_nums?;
    if nums.is_empty() {
        return None;
    }
    let base_id = parts.join("-").to_ascii_lowercase();
    if !base_id.contains('.') {
        return None;
    }
    Some((base_id, nums))
}

fn resolve_vscode_old_extensions(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(base) = rule
        .ubicacion
        .env_fallback
        .first()
        .and_then(|t| expand_env_vars(t))
    else {
        return;
    };
    if !base.is_dir() || is_reparse_point(&base) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(&base) else {
        return;
    };

    let mut grouped: HashMap<String, Vec<(Vec<u64>, PathBuf)>> = HashMap::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if !p.is_dir() || is_reparse_point(&p) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((ext_id, ver)) = split_vscode_ext_dir(&name) {
            grouped.entry(ext_id).or_default().push((ver, p));
        }
    }

    let mut stale_paths = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_items = 0u64;
    let mut newest_ts = 0i64;

    for (_ext_id, mut versions) in grouped {
        if versions.len() <= 1 {
            continue;
        }
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        for (_ver, p) in versions.into_iter().skip(1) {
            if validate_dev_target_path(&p, blacklist).is_ok() {
                let (b, c, ts) = measure_dir_no_reparse(&p);
                total_bytes = total_bytes.saturating_add(b);
                total_items = total_items.saturating_add(c.max(1));
                newest_ts = newest_ts.max(ts);
                stale_paths.push(p.to_string_lossy().into_owned());
            }
        }
    }

    if stale_paths.is_empty() {
        return;
    }
    let first = stale_paths[0].clone();
    let rest = stale_paths[1..].to_vec();
    let mut cand = make_base_candidate(
        rule,
        rule.id.clone(),
        format!("{} ({} versiones antiguas)", rule.nombre, stale_paths.len()),
        first,
        total_bytes,
        total_items,
        newest_ts,
        running_procs,
    );
    cand.extra_paths = rest;
    out.push(cand);
}

fn is_jetbrains_or_as_dir(parent_is_google: bool, name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    if parent_is_google {
        low.starts_with("androidstudio")
    } else {
        low.starts_with("intellijidea")
            || low.starts_with("webstorm")
            || low.starts_with("phpstorm")
            || low.starts_with("pycharm")
            || low.starts_with("rider")
            || low.starts_with("goland")
            || low.starts_with("clion")
            || low.starts_with("datagrip")
            || low.starts_with("androidstudio")
    }
}

fn resolve_jetbrains_caches(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    for tmpl in &rule.ubicacion.env_fallback {
        let Some(base) = expand_env_vars(tmpl) else {
            continue;
        };
        if !base.is_dir() || is_reparse_point(&base) {
            continue;
        }
        let parent_is_google = base
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("google"));
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in rd.flatten() {
            let ide_dir = entry.path();
            let ide_name = entry.file_name().to_string_lossy().into_owned();
            if !ide_dir.is_dir()
                || is_reparse_point(&ide_dir)
                || !is_jetbrains_or_as_dir(parent_is_google, &ide_name)
            {
                continue;
            }
            for sub in ["caches", "log", "logs"] {
                let target = ide_dir.join(sub);
                if target.is_dir()
                    && !is_reparse_point(&target)
                    && validate_dev_target_path(&target, blacklist).is_ok()
                {
                    let (bytes, count, ts) = measure_dir_no_reparse(&target);
                    if bytes > 0 {
                        out.push(make_base_candidate(
                            rule,
                            format!("{}:{ide_name}:{sub}", rule.id),
                            format!("{ide_name} — {sub}"),
                            target.to_string_lossy().into_owned(),
                            bytes,
                            count,
                            ts,
                            running_procs,
                        ));
                    }
                }
            }
        }
    }
}

fn split_ide_family(name: &str) -> Option<(String, String)> {
    let idx = name.find(|c: char| c.is_ascii_digit())?;
    if idx == 0 {
        return None;
    }
    Some((name[..idx].to_ascii_lowercase(), name[idx..].to_string()))
}

fn resolve_android_studio_old_versions(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let mut families: HashMap<String, Vec<(String, PathBuf)>> = HashMap::new();
    for tmpl in &rule.ubicacion.env_fallback {
        let Some(base) = expand_env_vars(tmpl) else {
            continue;
        };
        if !base.is_dir() || is_reparse_point(&base) {
            continue;
        }
        let parent_is_google = base
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("google"));
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !p.is_dir()
                || is_reparse_point(&p)
                || !is_jetbrains_or_as_dir(parent_is_google, &name)
            {
                continue;
            }
            if let Some((fam, ver)) = split_ide_family(&name) {
                families.entry(fam).or_default().push((ver, p));
            }
        }
    }

    for (_fam, mut vers) in families {
        if vers.len() <= 1 {
            continue;
        }
        vers.sort_by(|a, b| b.0.cmp(&a.0));
        for (_ver, p) in vers.into_iter().skip(1) {
            if validate_dev_target_path(&p, blacklist).is_err() {
                continue;
            }
            let (bytes, count, ts) = measure_dir_no_reparse(&p);
            if bytes == 0 {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(make_base_candidate(
                rule,
                format!("{}:{name}", rule.id),
                format!("Versión antigua de IDE — {name}"),
                p.to_string_lossy().into_owned(),
                bytes,
                count,
                ts,
                running_procs,
            ));
        }
    }
}

fn find_android_sdk_root(rule: &DevRule) -> Option<PathBuf> {
    for tmpl in &rule.ubicacion.env_fallback {
        if let Some(p) = expand_env_vars(tmpl) {
            if p.is_dir() && !is_reparse_point(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn find_sdkmanager(sdk_root: &Path, runner: &dyn CommandRunner) -> Option<String> {
    let cmdline = sdk_root.join("cmdline-tools");
    if let Ok(rd) = std::fs::read_dir(&cmdline) {
        let mut candidates: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path().join("bin").join("sdkmanager.bat"))
            .filter(|p| p.is_file())
            .collect();
        candidates.sort();
        if let Some(best) = candidates.pop() {
            return Some(best.to_string_lossy().into_owned());
        }
    }
    let probe = runner.run("sdkmanager", &["--version"], None, CLI_QUERY_TIMEOUT);
    if probe.success && !probe.not_found {
        return Some("sdkmanager".to_string());
    }
    None
}

fn resolve_android_sdk_unused(
    rule: &DevRule,
    running_procs: &HashSet<String>,
    runner: &dyn CommandRunner,
    warnings: &mut Vec<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(sdk_root) = find_android_sdk_root(rule) else {
        return;
    };
    let sdkmanager_bin = find_sdkmanager(&sdk_root, runner);

    for component in ["build-tools", "platforms", "ndk", "system-images"] {
        let comp_dir = sdk_root.join(component);
        if !comp_dir.is_dir() || is_reparse_point(&comp_dir) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&comp_dir) else {
            continue;
        };
        let mut versions: Vec<(String, PathBuf)> = rd
            .flatten()
            .filter(|e| e.path().is_dir() && !is_reparse_point(&e.path()))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();

        if versions.len() <= 1 {
            continue;
        }
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        // Keep newest version; surface older versions for explicit sdkmanager --uninstall
        for (ver, ver_path) in versions.into_iter().skip(1) {
            let (bytes, count, ts) = measure_dir_no_reparse(&ver_path);
            if bytes == 0 {
                continue;
            }
            let pkg_id = format!("{component};{ver}");
            let mut cand = make_base_candidate(
                rule,
                format!("{}:{component}:{ver}", rule.id),
                format!("Android SDK — {pkg_id}"),
                ver_path.to_string_lossy().into_owned(),
                bytes,
                count,
                ts,
                running_procs,
            );
            cand.preselected = false;

            if let Some(ref bin) = sdkmanager_bin {
                cand.method = CleanMethodKind::ComandoOficial;
                cand.command = Some(vec![bin.clone(), "--uninstall".to_string(), pkg_id]);
            } else {
                cand.command = None;
                cand.blocked_reason = Some(
                    "Requiere Android SDK Command-line Tools (`sdkmanager`). Nunca se borra manualmente."
                        .to_string(),
                );
                cand.warning = cand.blocked_reason.clone();
            }
            out.push(cand);
        }
    }

    if sdkmanager_bin.is_none() {
        warnings.push(
            "Android SDK detectado sin `cmdline-tools` (`sdkmanager`). Los componentes antiguos del SDK solo pueden desinstalarse mediante `sdkmanager --uninstall`."
                .to_string(),
        );
    }
}

fn resolve_android_avds(
    rule: &DevRule,
    blacklist: &BlacklistNeverSpec,
    running_procs: &HashSet<String>,
    out: &mut Vec<DevCandidate>,
) {
    let Some(avd_root) = rule
        .ubicacion
        .env_fallback
        .first()
        .and_then(|t| expand_env_vars(t))
    else {
        return;
    };
    if !avd_root.is_dir() || is_reparse_point(&avd_root) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(&avd_root) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !p.is_dir()
            || !name.ends_with(".avd")
            || is_reparse_point(&p)
            || validate_dev_target_path(&p, blacklist).is_err()
        {
            continue;
        }
        let (bytes, count, ts) = measure_dir_no_reparse(&p);
        let avd_name = name.trim_end_matches(".avd");
        let ini_path = avd_root.join(format!("{avd_name}.ini"));
        let mut cand = make_base_candidate(
            rule,
            format!("{}:{avd_name}", rule.id),
            format!("Emulador Android (AVD) — {avd_name}"),
            p.to_string_lossy().into_owned(),
            bytes,
            count,
            ts,
            running_procs,
        );
        if ini_path.is_file() && validate_dev_target_path(&ini_path, blacklist).is_ok() {
            cand.extra_paths
                .push(ini_path.to_string_lossy().into_owned());
        }
        cand.preselected = false;
        out.push(cand);
    }
}

pub fn parse_docker_human_size(raw: &str) -> u64 {
    let token = raw.split_whitespace().next().unwrap_or("").trim();
    let num_end = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(token.len());
    let num: f64 = token[..num_end].parse().unwrap_or(0.0);
    let unit = token[num_end..].trim().to_ascii_uppercase();
    let mult: f64 = match unit.as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1024.0,
        "MB" | "MIB" => 1024.0 * 1024.0,
        "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        "TB" | "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (num * mult).max(0.0) as u64
}

fn resolve_docker_rule(
    rule: &DevRule,
    docker_info: &DockerWslInfo,
    runner: &dyn CommandRunner,
    df_type: &str,
    out: &mut Vec<DevCandidate>,
) {
    if !docker_info.docker_available {
        return;
    }
    let empty_procs = HashSet::new();
    if !docker_info.daemon_running {
        let mut cand = make_base_candidate(
            rule,
            rule.id.clone(),
            rule.nombre.clone(),
            "Docker Daemon".to_string(),
            0,
            0,
            0,
            &empty_procs,
        );
        cand.preselected = false;
        cand.blocked_reason = Some(
            "Docker Desktop / daemon no está iniciado; ábrelo para analizar y limpiar.".to_string(),
        );
        cand.warning = cand.blocked_reason.clone();
        out.push(cand);
        return;
    }

    let df = runner.run(
        "docker",
        &["system", "df", "--format", "{{json .}}"],
        None,
        CLI_QUERY_TIMEOUT,
    );
    let mut reclaimable_bytes = 0u64;
    if df.success {
        for line in df.stdout.lines() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                let kind = val.get("Type").and_then(|v| v.as_str()).unwrap_or("");
                if kind.eq_ignore_ascii_case(df_type) {
                    if let Some(rec) = val.get("Reclaimable").and_then(|v| v.as_str()) {
                        reclaimable_bytes = parse_docker_human_size(rec);
                    }
                }
            }
        }
    }

    let mut cand = make_base_candidate(
        rule,
        rule.id.clone(),
        rule.nombre.clone(),
        format!("Docker ({df_type})"),
        reclaimable_bytes,
        1,
        0,
        &empty_procs,
    );
    cand.preselected = false;
    out.push(cand);
}
