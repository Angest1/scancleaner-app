use chrono::Utc;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};

use crate::cleaner::executor::{delete_to_recycle_bin_sta, remove_permanently_safe};
use crate::cleaner::guardrails::{is_reparse_point, SandboxGuard};
use crate::elevation::is_elevated;
use crate::storage::{append_cleanup_log, logs_dir, CleanupLogEntry};

use super::blacklist::{validate_dev_rule_and_command, validate_dev_target_path};
use super::process::{matching_blocking_processes, CommandRunner};
use super::resolver::{measure_dir_no_reparse, parse_docker_human_size};
use super::schema::{
    CleanMethodKind, DevBlockedItem, DevCleanupPlan, DevCleanupResult, DevExecutionOutcome,
    DevPlannedItem, DevPreviewRequest, DevRulesCatalog, SafetyLevel,
};

const OFFICIAL_CMD_TIMEOUT: Duration = Duration::from_secs(120);
const DISM_TIMEOUT: Duration = Duration::from_secs(600);
const PRE_CMD_TIMEOUT: Duration = Duration::from_secs(20);

pub fn build_dev_cleanup_plan(
    req: DevPreviewRequest,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
) -> DevCleanupPlan {
    let plan_id = Uuid::new_v4().to_string();
    let running = runner.running_processes();
    let elevated = is_elevated();

    let mut items = Vec::new();
    let mut blocked_items = Vec::new();
    let mut warnings = Vec::new();

    for input in req.items {
        let cand = input.candidate;

        let Some(rule) = catalog.rules.iter().find(|r| r.id == cand.rule_id) else {
            blocked_items.push(DevBlockedItem {
                id: cand.id,
                nombre: cand.nombre,
                path: cand.path,
                reason: "Regla no registrada en el catálogo oficial.".to_string(),
            });
            continue;
        };

        if rule.nivel == SafetyLevel::Nunca {
            blocked_items.push(DevBlockedItem {
                id: cand.id,
                nombre: cand.nombre,
                path: cand.path,
                reason: "Nivel 'nunca': prohibido limpiar.".to_string(),
            });
            continue;
        }

        if let Some(ref reason) = cand.blocked_reason {
            blocked_items.push(DevBlockedItem {
                id: cand.id,
                nombre: cand.nombre,
                path: cand.path,
                reason: reason.clone(),
            });
            continue;
        }

        if cand.requires_admin && !elevated {
            blocked_items.push(DevBlockedItem {
                id: cand.id,
                nombre: cand.nombre,
                path: cand.path,
                reason: "Requiere permisos de administrador.".to_string(),
            });
            continue;
        }

        let active_procs = matching_blocking_processes(&cand.blocking_processes_spec, &running);
        if !active_procs.is_empty() {
            warnings.push(format!(
                "{}: proceso activo ({}) — se omitirá si sigue corriendo al ejecutar.",
                cand.nombre,
                active_procs.join(", ")
            ));
        }

        let effective_method = if input.use_recycle_bin && cand.allow_recycle_bin_option {
            CleanMethodKind::Papelera
        } else {
            cand.method
        };

        if let Err(reason) =
            validate_dev_rule_and_command(rule, cand.command.as_deref(), &catalog.blacklist_never)
        {
            blocked_items.push(DevBlockedItem {
                id: cand.id,
                nombre: cand.nombre,
                path: cand.path,
                reason,
            });
            continue;
        }

        // Validate filesystem paths for direct deletion or cache paths
        let is_virtual_target = cand.path.starts_with("$Recycle.Bin")
            || cand.path.starts_with("Docker")
            || cand.path.starts_with("WinSxS");
        if !is_virtual_target && !cand.delete_dir_contents_only {
            let p = Path::new(&cand.path);
            if let Err(v) = validate_dev_target_path(p, &catalog.blacklist_never) {
                blocked_items.push(DevBlockedItem {
                    id: cand.id,
                    nombre: cand.nombre,
                    path: cand.path,
                    reason: v.reason,
                });
                continue;
            }
            let mut extra_err = None;
            for ep in &cand.extra_paths {
                if let Err(v) = validate_dev_target_path(Path::new(ep), &catalog.blacklist_never) {
                    extra_err = Some(v.reason);
                    break;
                }
            }
            if let Some(reason) = extra_err {
                blocked_items.push(DevBlockedItem {
                    id: cand.id,
                    nombre: cand.nombre,
                    path: cand.path,
                    reason,
                });
                continue;
            }
        }

        items.push(DevPlannedItem {
            id: cand.id,
            rule_id: cand.rule_id,
            nombre: cand.nombre,
            categoria: cand.categoria,
            nivel: cand.nivel,
            path: cand.path,
            extra_paths: cand.extra_paths,
            delete_dir_contents_only: cand.delete_dir_contents_only,
            estimated_bytes: cand.size_bytes,
            effective_method,
            command: cand.command,
            pre_commands: cand.pre_commands,
            blocking_processes_spec: cand.blocking_processes_spec,
            requires_admin: cand.requires_admin,
            como_regenerar: cand.como_regenerar,
        });
    }

    let total_estimated_bytes = items.iter().map(|i| i.estimated_bytes).sum();

    DevCleanupPlan {
        plan_id,
        items,
        blocked_items,
        total_estimated_bytes,
        warnings,
    }
}

pub fn execute_dev_cleanup_plan(
    plan: &DevCleanupPlan,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
) -> DevCleanupResult {
    let log_dir = log_dir_override.map(PathBuf::from).unwrap_or_else(logs_dir);
    let running = runner.running_processes();

    let mut total_freed_bytes = 0u64;
    let mut succeeded_count = 0usize;
    let mut failed_count = 0usize;
    let mut skipped_count = 0usize;
    let mut outcomes = Vec::with_capacity(plan.items.len());

    for item in &plan.items {
        let method_label = format_method_label(item);

        // 1. Re-check blocking processes immediately before executing — never force-kill
        let active_procs = matching_blocking_processes(&item.blocking_processes_spec, &running);
        if !active_procs.is_empty() {
            skipped_count += 1;
            let detail = format!(
                "Omitido: proceso bloqueante en ejecución ({}).",
                active_procs.join(", ")
            );
            log_dev_outcome(
                &log_dir,
                &plan.plan_id,
                item,
                &method_label,
                0,
                "skipped",
                Some(&detail),
            );
            outcomes.push(DevExecutionOutcome {
                id: item.id.clone(),
                rule_id: item.rule_id.clone(),
                nombre: item.nombre.clone(),
                path: item.path.clone(),
                method_label,
                estimated_bytes: item.estimated_bytes,
                freed_bytes: 0,
                status: "skipped".to_string(),
                detail: Some(detail),
            });
            continue;
        }

        // 2. Re-validate rule & command against blacklist
        if let Some(rule) = catalog.rules.iter().find(|r| r.id == item.rule_id) {
            if let Err(reason) = validate_dev_rule_and_command(
                rule,
                item.command.as_deref(),
                &catalog.blacklist_never,
            ) {
                skipped_count += 1;
                log_dev_outcome(
                    &log_dir,
                    &plan.plan_id,
                    item,
                    &method_label,
                    0,
                    "blocked",
                    Some(&reason),
                );
                outcomes.push(DevExecutionOutcome {
                    id: item.id.clone(),
                    rule_id: item.rule_id.clone(),
                    nombre: item.nombre.clone(),
                    path: item.path.clone(),
                    method_label,
                    estimated_bytes: item.estimated_bytes,
                    freed_bytes: 0,
                    status: "blocked".to_string(),
                    detail: Some(reason),
                });
                continue;
            }
        }

        // 3. Execute pre-commands (e.g. `gradle --stop`), best-effort
        for pre in &item.pre_commands {
            if let Some((prog, args)) = pre.split_first() {
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let _ = runner.run(prog, &arg_refs, None, PRE_CMD_TIMEOUT);
            }
        }

        // 4. Execute according to effective_method
        let (status, freed, detail) = match item.effective_method {
            CleanMethodKind::ComandoOficial => execute_official_command(item, runner),
            CleanMethodKind::BorradoPermanente => {
                execute_path_deletion(item, false, &catalog.blacklist_never, sandbox_guard)
            }
            CleanMethodKind::Papelera => {
                execute_path_deletion(item, true, &catalog.blacklist_never, sandbox_guard)
            }
        };

        match status.as_str() {
            "deleted_permanent" | "recycled" | "command_ok" => {
                succeeded_count += 1;
                total_freed_bytes = total_freed_bytes.saturating_add(freed);
            }
            "skipped" | "blocked" => {
                skipped_count += 1;
            }
            _ => {
                failed_count += 1;
            }
        }

        log_dev_outcome(
            &log_dir,
            &plan.plan_id,
            item,
            &method_label,
            freed,
            &status,
            detail.as_deref(),
        );

        outcomes.push(DevExecutionOutcome {
            id: item.id.clone(),
            rule_id: item.rule_id.clone(),
            nombre: item.nombre.clone(),
            path: item.path.clone(),
            method_label,
            estimated_bytes: item.estimated_bytes,
            freed_bytes: freed,
            status,
            detail,
        });
    }

    DevCleanupResult {
        plan_id: plan.plan_id.clone(),
        total_estimated_bytes: plan.total_estimated_bytes,
        total_freed_bytes,
        succeeded_count,
        failed_count,
        skipped_count,
        outcomes,
    }
}

fn format_method_label(item: &DevPlannedItem) -> String {
    match item.effective_method {
        CleanMethodKind::ComandoOficial => item
            .command
            .as_ref()
            .map(|c| c.join(" "))
            .unwrap_or_else(|| "comando-oficial".to_string()),
        CleanMethodKind::BorradoPermanente => "borrado-permanente".to_string(),
        CleanMethodKind::Papelera => "papelera".to_string(),
    }
}

fn execute_official_command(
    item: &DevPlannedItem,
    runner: &dyn CommandRunner,
) -> (String, u64, Option<String>) {
    let Some(cmd) = &item.command else {
        return (
            "failed".to_string(),
            0,
            Some("Sin comando configurado.".to_string()),
        );
    };
    if cmd.first().is_some_and(|c| c == "SHEmptyRecycleBinW") {
        let hr = unsafe {
            SHEmptyRecycleBinW(
                None,
                PCWSTR::null(),
                SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
            )
        };
        return if hr.is_ok() {
            ("command_ok".to_string(), item.estimated_bytes, None)
        } else {
            (
                "failed".to_string(),
                0,
                Some(format!("SHEmptyRecycleBinW error: {hr:?}")),
            )
        };
    }

    let Some((prog, args)) = cmd.split_first() else {
        return ("failed".to_string(), 0, Some("Comando vacío.".to_string()));
    };

    let target_path = Path::new(&item.path);
    let has_fs_path = target_path.exists()
        && !item.path.starts_with("Docker")
        && !item.path.starts_with("WinSxS");
    let before_bytes = if has_fs_path {
        measure_dir_no_reparse(target_path).0
    } else {
        item.estimated_bytes
    };

    let timeout = if prog.eq_ignore_ascii_case("DISM.exe") || prog.eq_ignore_ascii_case("dism") {
        DISM_TIMEOUT
    } else {
        OFFICIAL_CMD_TIMEOUT
    };

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let res = runner.run(prog, &arg_refs, None, timeout);

    if res.not_found {
        return (
            "skipped".to_string(),
            0,
            Some(format!("Herramienta '{prog}' no encontrada en PATH.")),
        );
    }
    if res.timed_out {
        return ("failed".to_string(), 0, Some(res.stderr));
    }
    if !res.success {
        let err_snippet = if !res.stderr.trim().is_empty() {
            res.stderr
                .trim()
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            format!("Código de salida {:?}", res.exit_code)
        };
        return ("failed".to_string(), 0, Some(err_snippet));
    }

    let freed = if has_fs_path {
        let after_bytes = if target_path.exists() {
            measure_dir_no_reparse(target_path).0
        } else {
            0
        };
        before_bytes.saturating_sub(after_bytes)
    } else if prog.eq_ignore_ascii_case("docker") {
        parse_docker_reclaimed_bytes(&res.stdout).unwrap_or(item.estimated_bytes)
    } else {
        item.estimated_bytes
    };

    ("command_ok".to_string(), freed, None)
}

fn parse_docker_reclaimed_bytes(stdout: &str) -> Option<u64> {
    for line in stdout.lines().rev() {
        let lower = line.to_ascii_lowercase();
        if let Some(idx) = lower.find("reclaimed space:") {
            let val_str = line[idx + "reclaimed space:".len()..].trim();
            return Some(parse_docker_human_size(val_str));
        }
    }
    None
}

fn execute_path_deletion(
    item: &DevPlannedItem,
    use_recycle_bin: bool,
    blacklist: &super::schema::BlacklistNeverSpec,
    sandbox_guard: Option<&SandboxGuard>,
) -> (String, u64, Option<String>) {
    let mut targets: Vec<PathBuf> = Vec::new();

    if item.delete_dir_contents_only {
        let root = Path::new(&item.path);
        if let Ok(rd) = std::fs::read_dir(root) {
            for entry in rd.flatten() {
                targets.push(entry.path());
            }
        }
    } else {
        targets.push(PathBuf::from(&item.path));
        for ep in &item.extra_paths {
            targets.push(PathBuf::from(ep));
        }
    }

    let mut valid_targets: Vec<(PathBuf, u64)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for p in targets {
        if is_reparse_point(&p) {
            errors.push(format!("Reparse point omitido: {}", p.display()));
            continue;
        }
        let check = if let Some(guard) = sandbox_guard {
            guard
                .verify_inside_sandbox(&p)
                .and_then(|()| validate_dev_target_path(&p, blacklist))
        } else {
            validate_dev_target_path(&p, blacklist)
        };
        match check {
            Ok(()) => {
                if p.exists() {
                    let b = measure_dir_no_reparse(&p).0;
                    valid_targets.push((p, b));
                }
            }
            Err(v) => {
                errors.push(v.reason);
            }
        }
    }

    if valid_targets.is_empty() {
        if errors.is_empty() {
            return ("deleted_permanent".to_string(), 0, None);
        }
        return ("blocked".to_string(), 0, Some(errors.join(" | ")));
    }

    let mut freed = 0u64;
    let mut any_ok = false;

    if use_recycle_bin {
        let paths: Vec<PathBuf> = valid_targets.iter().map(|(p, _)| p.clone()).collect();
        let results = delete_to_recycle_bin_sta(&paths);
        for ((p, before), res) in valid_targets.into_iter().zip(results) {
            match res {
                Ok(()) => {
                    any_ok = true;
                    let after = if p.exists() {
                        measure_dir_no_reparse(&p).0
                    } else {
                        0
                    };
                    freed = freed.saturating_add(before.saturating_sub(after));
                }
                Err(msg) => errors.push(msg),
            }
        }
        let status = if any_ok { "recycled" } else { "failed" };
        let detail = if errors.is_empty() {
            None
        } else {
            Some(errors.into_iter().take(3).collect::<Vec<_>>().join(" | "))
        };
        (status.to_string(), freed, detail)
    } else {
        for (p, before) in valid_targets {
            match remove_permanently_safe(&p) {
                Ok(()) => {
                    any_ok = true;
                    freed = freed.saturating_add(before);
                }
                Err(msg) => {
                    let after = if p.exists() {
                        measure_dir_no_reparse(&p).0
                    } else {
                        0
                    };
                    let partial = before.saturating_sub(after);
                    if partial > 0 {
                        any_ok = true;
                        freed = freed.saturating_add(partial);
                    }
                    errors.push(msg);
                }
            }
        }
        let status = if any_ok {
            "deleted_permanent"
        } else {
            "failed"
        };
        let detail = if errors.is_empty() {
            None
        } else {
            Some(errors.into_iter().take(3).collect::<Vec<_>>().join(" | "))
        };
        (status.to_string(), freed, detail)
    }
}

fn log_dev_outcome(
    log_dir: &Path,
    plan_id: &str,
    item: &DevPlannedItem,
    method_label: &str,
    freed_bytes: u64,
    status: &str,
    detail: Option<&str>,
) {
    let _ = append_cleanup_log(
        log_dir,
        &CleanupLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            plan_id: plan_id.to_string(),
            path: item.path.clone(),
            category: format!("dev:{:?}", item.categoria).to_ascii_lowercase(),
            bytes: freed_bytes,
            mode: method_label.to_string(),
            status: status.to_string(),
            detail: detail.map(ToString::to_string),
            rule_id: Some(item.rule_id.clone()),
            estimated_bytes: Some(item.estimated_bytes),
            command: item.command.as_ref().map(|c| c.join(" ")),
        },
    );
}
