use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};

use crate::cleaner::executor::{delete_to_recycle_bin_sta, remove_permanently_safe};
use crate::cleaner::guardrails::{is_reparse_point, SandboxGuard};
use crate::deletion::fast_delete::{delete_tree_parallel_fast, stage_to_same_volume_trash};
use crate::elevation::is_elevated;
use crate::reconcile::triggers::{
    measure_real_freed_space, DiskSpaceProvider, WinDiskSpaceProvider,
};
use crate::scanner::derived::volume_root_of_path;
use crate::storage::{append_cleanup_log, logs_dir, CleanupLogEntry};

use super::blacklist::{validate_dev_rule_and_command, validate_dev_target_path};
use super::process::{matching_blocking_processes, CommandRunner};
use super::resolver::{expand_env_vars, measure_dir_no_reparse, parse_docker_human_size};
use super::schema::{
    CleanMethodKind, DevBlockedItem, DevCleanupPlan, DevCleanupResult, DevExecutionOutcome,
    DevPlannedItem, DevPreviewRequest, DevRulesCatalog, SafetyLevel,
};

const OFFICIAL_CMD_TIMEOUT: Duration = Duration::from_secs(120);
const DISM_TIMEOUT: Duration = Duration::from_secs(600);
const PRE_CMD_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_PARALLEL_OFFICIAL_CMDS: usize = 3;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevCleanupProgressEvent {
    pub plan_id: String,
    pub phase: String,
    pub current_label: String,
    pub current_path: String,
    pub completed_items: usize,
    pub total_items: usize,
    pub freed_bytes_so_far: u64,
    pub failed_count: usize,
}

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

        let is_virtual_target = cand.path.starts_with("$Recycle.Bin")
            || cand.path.starts_with("Docker")
            || cand.path.starts_with("WinSxS");
        if !is_virtual_target && !cand.delete_dir_contents_only {
            // Defense-in-depth: a rule with a specialized sub-detector (e.g. jetbrains_caches)
            // must NEVER delete its raw parent env_fallback directory (e.g. %LOCALAPPDATA%\Google).
            if let Some(ref sd) = rule.ubicacion.special_detector {
                if sd != "dir_contents" {
                    let cand_norm = cand
                        .path
                        .trim()
                        .trim_end_matches('\\')
                        .replace('/', "\\")
                        .to_ascii_lowercase();
                    let matches_raw_parent = rule.ubicacion.env_fallback.iter().any(|tmpl| {
                        expand_env_vars(tmpl).is_some_and(|p| {
                            p.to_string_lossy()
                                .trim_end_matches('\\')
                                .replace('/', "\\")
                                .to_ascii_lowercase()
                                == cand_norm
                        })
                    });
                    if matches_raw_parent {
                        blocked_items.push(DevBlockedItem {
                            id: cand.id,
                            nombre: cand.nombre,
                            path: cand.path,
                            reason: "Bloqueado por seguridad: ruta raíz de detector especializado."
                                .to_string(),
                        });
                        continue;
                    }
                }
            }
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

/// Executes a DevCleanupPlan:
/// - Official commands run concurrently with bounded parallelism (max 3 workers).
/// - Permanent path deletions use instant same-volume staging + parallel POSIX deletion.
/// - Recycle Bin deletions use batched `IFileOperation`.
/// - Measures real volume free-space delta before/after and collects `rutasAfectadas` for reconciliation.
pub fn execute_dev_cleanup_plan(
    plan: &DevCleanupPlan,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
) -> DevCleanupResult {
    let (res, _) = execute_dev_cleanup_plan_with_progress(
        plan,
        catalog,
        runner,
        log_dir_override,
        sandbox_guard,
        false,
        None,
    );
    res
}

pub fn execute_dev_cleanup_plan_with_progress(
    plan: &DevCleanupPlan,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
    defer_trash_drain: bool,
    on_progress: Option<&(dyn Fn(DevCleanupProgressEvent) + Sync)>,
) -> (DevCleanupResult, Vec<PathBuf>) {
    let log_dir = log_dir_override.map(PathBuf::from).unwrap_or_else(logs_dir);
    let running = runner.running_processes();
    let space_provider = WinDiskSpaceProvider;
    let total_items = plan.items.len();

    let emit_progress =
        |phase: &str, label: &str, path: &str, completed: usize, freed: u64, failed: usize| {
            if let Some(cb) = on_progress {
                cb(DevCleanupProgressEvent {
                    plan_id: plan.plan_id.clone(),
                    phase: phase.to_string(),
                    current_label: label.to_string(),
                    current_path: path.to_string(),
                    completed_items: completed,
                    total_items,
                    freed_bytes_so_far: freed,
                    failed_count: failed,
                });
            }
        };

    emit_progress("preparing", "Preparando plan de limpieza Dev…", "", 0, 0, 0);

    // Collect affected volumes & capture free space BEFORE
    let mut affected_paths: Vec<String> = Vec::new();
    let mut free_before_by_vol: HashMap<String, u64> = HashMap::new();
    let mut touched_docker = false;

    for item in &plan.items {
        if item.rule_id.starts_with("docker-") {
            touched_docker = true;
        }
        if let Some(rule) = catalog.rules.iter().find(|r| r.id == item.rule_id) {
            for tmpl in &rule.rutas_afectadas {
                if let Some(exp) = expand_env_vars(tmpl) {
                    let s = exp.to_string_lossy().into_owned();
                    if let Some(vol) = volume_root_of_path(&exp) {
                        free_before_by_vol.entry(vol.clone()).or_insert_with(|| {
                            space_provider.query_space(&vol).map_or(0, |(_, f)| f)
                        });
                    }
                    affected_paths.push(s);
                }
            }
        }
        let p = Path::new(&item.path);
        if let Some(vol) = volume_root_of_path(p) {
            free_before_by_vol
                .entry(vol.clone())
                .or_insert_with(|| space_provider.query_space(&vol).map_or(0, |(_, f)| f));
            affected_paths.push(item.path.clone());
        }
    }

    // Separate official commands (executed concurrently up to MAX_PARALLEL_OFFICIAL_CMDS)
    // and direct file deletions
    let outcomes_slots: Arc<Mutex<Vec<Option<DevExecutionOutcome>>>> =
        Arc::new(Mutex::new(vec![None; plan.items.len()]));
    let progress_counters = Arc::new(Mutex::new((0usize, 0u64, 0usize))); // (completed, freed, failed)
    let staged_paths = Arc::new(Mutex::new(Vec::<PathBuf>::new()));

    let mut cmd_indices: Vec<usize> = Vec::new();
    let mut path_indices: Vec<usize> = Vec::new();
    for (idx, item) in plan.items.iter().enumerate() {
        if item.effective_method == CleanMethodKind::ComandoOficial {
            cmd_indices.push(idx);
        } else {
            path_indices.push(idx);
        }
    }

    // Execute direct path deletions FIRST (fast staging / recycle bin) so user sees immediate momentum
    for idx in path_indices {
        let item = &plan.items[idx];
        let (done_now, freed_now, fail_now) =
            progress_counters.lock().map(|g| *g).unwrap_or((0, 0, 0));
        emit_progress(
            "deleting",
            &format!("Limpiando: {}", item.nombre),
            &item.path,
            done_now,
            freed_now,
            fail_now,
        );
        let defer_ref = if defer_trash_drain {
            Some(staged_paths.as_ref())
        } else {
            None
        };
        let outcome = execute_single_dev_item(
            item,
            &plan.plan_id,
            catalog,
            runner,
            &running,
            sandbox_guard,
            defer_ref,
        );
        let is_ok = matches!(
            outcome.status.as_str(),
            "deleted_permanent" | "recycled" | "command_ok"
        );
        let is_fail = !is_ok && outcome.status != "skipped" && outcome.status != "blocked";
        let (done_after, freed_after, fail_after) = if let Ok(mut c) = progress_counters.lock() {
            c.0 += 1;
            if is_ok {
                c.1 = c.1.saturating_add(outcome.freed_bytes);
            }
            if is_fail {
                c.2 += 1;
            }
            *c
        } else {
            (done_now + 1, freed_now, fail_now)
        };
        if let Ok(mut g) = outcomes_slots.lock() {
            g[idx] = Some(outcome);
        }
        emit_progress(
            "deleting",
            &format!("Completado: {}", item.nombre),
            &item.path,
            done_after,
            freed_after,
            fail_after,
        );
    }

    // Execute official commands in parallel chunks of MAX_PARALLEL_OFFICIAL_CMDS
    for chunk in cmd_indices.chunks(MAX_PARALLEL_OFFICIAL_CMDS) {
        std::thread::scope(|scope| {
            for &idx in chunk {
                let item = &plan.items[idx];
                let slots = outcomes_slots.clone();
                let counters = progress_counters.clone();
                let running_ref = &running;
                let emit_ref = &emit_progress;
                scope.spawn(move || {
                    let cmd_label = format_method_label(item);
                    let (done_now, freed_now, fail_now) =
                        counters.lock().map(|g| *g).unwrap_or((0, 0, 0));
                    emit_ref(
                        "command",
                        &format!("Ejecutando comando: {cmd_label}"),
                        &item.path,
                        done_now,
                        freed_now,
                        fail_now,
                    );
                    let outcome = execute_single_dev_item(
                        item,
                        &plan.plan_id,
                        catalog,
                        runner,
                        running_ref,
                        sandbox_guard,
                        None,
                    );
                    let is_ok = matches!(
                        outcome.status.as_str(),
                        "deleted_permanent" | "recycled" | "command_ok"
                    );
                    let is_fail =
                        !is_ok && outcome.status != "skipped" && outcome.status != "blocked";
                    let (done_after, freed_after, fail_after) = if let Ok(mut c) = counters.lock() {
                        c.0 += 1;
                        if is_ok {
                            c.1 = c.1.saturating_add(outcome.freed_bytes);
                        }
                        if is_fail {
                            c.2 += 1;
                        }
                        *c
                    } else {
                        (done_now + 1, freed_now, fail_now)
                    };
                    if let Ok(mut g) = slots.lock() {
                        g[idx] = Some(outcome);
                    }
                    emit_ref(
                        "command",
                        &format!("Finalizado: {}", item.nombre),
                        &item.path,
                        done_after,
                        freed_after,
                        fail_after,
                    );
                });
            }
        });
    }

    emit_progress(
        "finalizing",
        "Midiendo espacio liberado y registrando auditoría…",
        "",
        total_items,
        progress_counters.lock().map(|g| g.1).unwrap_or(0),
        progress_counters.lock().map(|g| g.2).unwrap_or(0),
    );

    let outcomes: Vec<DevExecutionOutcome> = outcomes_slots
        .lock()
        .map(|g| g.iter().flatten().cloned().collect())
        .unwrap_or_default();

    let mut total_freed_bytes = 0u64;
    let mut succeeded_count = 0usize;
    let mut failed_count = 0usize;
    let mut skipped_count = 0usize;

    for o in &outcomes {
        match o.status.as_str() {
            "deleted_permanent" | "recycled" | "command_ok" => {
                succeeded_count += 1;
                total_freed_bytes = total_freed_bytes.saturating_add(o.freed_bytes);
            }
            "skipped" | "blocked" => skipped_count += 1,
            _ => failed_count += 1,
        }
    }

    let drained_vec = staged_paths.lock().map(|g| g.clone()).unwrap_or_default();
    let has_pending = !drained_vec.is_empty();

    // Real freed space delta across affected volumes
    let mut total_real_freed = 0u64;
    let mut discrepancy = None;
    for (vol, free_before) in &free_before_by_vol {
        let m = measure_real_freed_space(
            vol,
            *free_before,
            total_freed_bytes,
            &space_provider,
            has_pending,
            touched_docker,
            failed_count > 0,
        );
        total_real_freed = total_real_freed.saturating_add(m.real_freed_bytes);
        if discrepancy.is_none() {
            discrepancy = m.discrepancy_explanation;
        }
    }

    for (item, outcome) in plan.items.iter().zip(outcomes.iter()) {
        log_dev_outcome(
            &log_dir,
            &plan.plan_id,
            item,
            &outcome.method_label,
            outcome.freed_bytes,
            Some(total_real_freed),
            &outcome.status,
            outcome.detail.as_deref(),
        );
    }

    affected_paths.sort();
    affected_paths.dedup();

    emit_progress(
        "completed",
        "Limpieza completada",
        "",
        total_items,
        total_freed_bytes,
        failed_count,
    );

    (
        DevCleanupResult {
            plan_id: plan.plan_id.clone(),
            total_estimated_bytes: plan.total_estimated_bytes,
            total_freed_bytes,
            real_freed_bytes: Some(total_real_freed),
            discrepancy_explanation: discrepancy,
            affected_paths_for_reconcile: affected_paths,
            succeeded_count,
            failed_count,
            skipped_count,
            outcomes,
        },
        drained_vec,
    )
}

fn execute_single_dev_item(
    item: &DevPlannedItem,
    plan_id: &str,
    catalog: &DevRulesCatalog,
    runner: &dyn CommandRunner,
    running: &std::collections::HashSet<String>,
    sandbox_guard: Option<&SandboxGuard>,
    defer_drain_paths: Option<&Mutex<Vec<PathBuf>>>,
) -> DevExecutionOutcome {
    let method_label = format_method_label(item);

    // 1. Re-check blocking processes immediately before executing
    let active_procs = matching_blocking_processes(&item.blocking_processes_spec, running);
    if !active_procs.is_empty() {
        let detail = format!(
            "Omitido: proceso bloqueante en ejecución ({}).",
            active_procs.join(", ")
        );
        return DevExecutionOutcome {
            id: item.id.clone(),
            rule_id: item.rule_id.clone(),
            nombre: item.nombre.clone(),
            path: item.path.clone(),
            method_label,
            estimated_bytes: item.estimated_bytes,
            freed_bytes: 0,
            status: "skipped".to_string(),
            detail: Some(detail),
        };
    }

    // 2. Re-validate rule & command against blacklist
    if let Some(rule) = catalog.rules.iter().find(|r| r.id == item.rule_id) {
        if let Err(reason) =
            validate_dev_rule_and_command(rule, item.command.as_deref(), &catalog.blacklist_never)
        {
            return DevExecutionOutcome {
                id: item.id.clone(),
                rule_id: item.rule_id.clone(),
                nombre: item.nombre.clone(),
                path: item.path.clone(),
                method_label,
                estimated_bytes: item.estimated_bytes,
                freed_bytes: 0,
                status: "blocked".to_string(),
                detail: Some(reason),
            };
        }
    }

    // 3. Pre-commands
    for pre in &item.pre_commands {
        if let Some((prog, args)) = pre.split_first() {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = runner.run(prog, &arg_refs, None, PRE_CMD_TIMEOUT);
        }
    }

    // 4. Execute
    let (status, freed, detail) = match item.effective_method {
        CleanMethodKind::ComandoOficial => execute_official_command(item, runner),
        CleanMethodKind::BorradoPermanente => execute_path_deletion(
            item,
            plan_id,
            false,
            &catalog.blacklist_never,
            sandbox_guard,
            defer_drain_paths,
        ),
        CleanMethodKind::Papelera => execute_path_deletion(
            item,
            plan_id,
            true,
            &catalog.blacklist_never,
            sandbox_guard,
            None,
        ),
    };

    DevExecutionOutcome {
        id: item.id.clone(),
        rule_id: item.rule_id.clone(),
        nombre: item.nombre.clone(),
        path: item.path.clone(),
        method_label,
        estimated_bytes: item.estimated_bytes,
        freed_bytes: freed,
        status,
        detail,
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

    let detail = if prog.eq_ignore_ascii_case("docker") {
        Some("Nota: si el archivo .vhdx de Docker/WSL no redujo su tamaño físico, compacta el VHDX con las instrucciones de la tarjeta Docker.".to_string())
    } else {
        None
    };

    ("command_ok".to_string(), freed, detail)
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
    plan_id: &str,
    use_recycle_bin: bool,
    blacklist: &super::schema::BlacklistNeverSpec,
    sandbox_guard: Option<&SandboxGuard>,
    defer_drain_paths: Option<&Mutex<Vec<PathBuf>>>,
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
            let del_res =
                if let Ok(staged) = stage_to_same_volume_trash(&p, plan_id, sandbox_guard, None) {
                    if let Some(deferred) = defer_drain_paths {
                        if let Ok(mut g) = deferred.lock() {
                            g.push(staged);
                        }
                        Ok(())
                    } else {
                        delete_tree_parallel_fast(&staged, None, None, false).map(|_| ())
                    }
                } else {
                    remove_permanently_safe(&p)
                };

            match del_res {
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

#[allow(clippy::too_many_arguments)]
fn log_dev_outcome(
    log_dir: &Path,
    plan_id: &str,
    item: &DevPlannedItem,
    method_label: &str,
    freed_bytes: u64,
    real_freed_bytes: Option<u64>,
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
            real_freed_bytes,
            command: item.command.as_ref().map(|c| c.join(" ")),
        },
    );
}
