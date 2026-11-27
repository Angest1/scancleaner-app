use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use windows::core::PCWSTR;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName, SHEmptyRecycleBinW,
    FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
    FOF_SILENT, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};

use super::guardrails::{validate_target_path, SandboxGuard};
use crate::deletion::fast_delete::{
    delete_tree_parallel_fast, has_pending_trash_on_volume, stage_to_same_volume_trash,
};
use crate::reconcile::triggers::{
    measure_real_freed_space, DiskSpaceProvider, WinDiskSpaceProvider,
};
use crate::scanner::derived::volume_root_of_path;
use crate::storage::{append_cleanup_log, logs_dir, CleanupLogEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupMode {
    RecycleBin,
    Permanent,
    EmptyRecycleBin,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupTargetInput {
    pub path: String,
    pub category: String,
    pub is_temp_dir_contents: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCleanupRequest {
    pub targets: Vec<CleanupTargetInput>,
    pub mode: CleanupMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedItem {
    pub path: String,
    pub category: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedItem {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub plan_id: String,
    pub mode: CleanupMode,
    pub items: Vec<PlannedItem>,
    pub blocked_items: Vec<BlockedItem>,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItemOutcome {
    pub path: String,
    pub bytes: u64,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub plan_id: String,
    pub mode: CleanupMode,
    pub freed_bytes: u64,
    #[serde(default)]
    pub estimated_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_freed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discrepancy_explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_job_id: Option<String>,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub outcomes: Vec<CleanupItemOutcome>,
}

fn measure_path_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    if path.is_file() {
        return std::fs::symlink_metadata(path).map_or(0, |m| m.len());
    }
    let mut stack = vec![path.to_path_buf()];
    let mut sum = 0u64;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                sum = sum.saturating_add(meta.len());
            }
        }
    }
    sum
}

pub fn build_cleanup_plan(req: PreviewCleanupRequest) -> CleanupPlan {
    let plan_id = Uuid::new_v4().to_string();
    let mut items = Vec::new();
    let mut blocked_items = Vec::new();
    let mut warnings = Vec::new();

    if req.mode == CleanupMode::EmptyRecycleBin {
        let (rb_bytes, rb_items) = super::detectors::query_recycle_bin();
        if rb_items > 0 {
            items.push(PlannedItem {
                path: "$Recycle.Bin (Papelera del sistema)".to_string(),
                category: "recycle_bin".to_string(),
                bytes: rb_bytes,
            });
        }
        return CleanupPlan {
            plan_id,
            mode: req.mode,
            total_bytes: rb_bytes,
            items,
            blocked_items,
            warnings,
        };
    }

    if req.mode == CleanupMode::Permanent {
        warnings.push(
            "Modo de borrado PERMANENTE seleccionado: los archivos no podrán recuperarse desde la Papelera."
                .to_string(),
        );
    }

    for target in req.targets {
        let p = PathBuf::from(&target.path);
        if target.is_temp_dir_contents {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for entry in rd.flatten() {
                    let child = entry.path();
                    match validate_target_path(&child) {
                        Ok(()) => {
                            let bytes = measure_path_bytes(&child);
                            items.push(PlannedItem {
                                path: child.to_string_lossy().into_owned(),
                                category: target.category.clone(),
                                bytes,
                            });
                        }
                        Err(v) => blocked_items.push(BlockedItem {
                            path: v.path.to_string_lossy().into_owned(),
                            reason: v.reason,
                        }),
                    }
                }
            }
            continue;
        }

        match validate_target_path(&p) {
            Ok(()) => {
                if p.exists() {
                    let bytes = measure_path_bytes(&p);
                    items.push(PlannedItem {
                        path: p.to_string_lossy().into_owned(),
                        category: target.category,
                        bytes,
                    });
                }
            }
            Err(v) => blocked_items.push(BlockedItem {
                path: v.path.to_string_lossy().into_owned(),
                reason: v.reason,
            }),
        }
    }

    let total_bytes = items.iter().map(|i| i.bytes).sum();
    CleanupPlan {
        plan_id,
        mode: req.mode,
        items,
        blocked_items,
        total_bytes,
        warnings,
    }
}

/// Sends a batch of items to the Windows Recycle Bin using a SINGLE batched `IFileOperation`
/// (`DeleteItem` for every valid item followed by a single `PerformOperations()` call).
/// Falls back to per-item recycling only if the batch fails (e.g., one locked file).
pub(crate) fn delete_to_recycle_bin_sta(paths: &[PathBuf]) -> Vec<Result<(), String>> {
    if paths.is_empty() {
        return Vec::new();
    }
    let paths_owned = paths.to_vec();
    std::thread::spawn(move || {
        let init_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let com_initialized = init_hr.is_ok();

        let results = recycle_batch_com(&paths_owned);

        if com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
        results
    })
    .join()
    .unwrap_or_else(|_| {
        vec![Err(
            "Hilo STA de limpieza falló inesperadamente.".to_string()
        )]
    })
}

fn recycle_batch_com(paths: &[PathBuf]) -> Vec<Result<(), String>> {
    let flags = FOF_ALLOWUNDO
        | FOFX_RECYCLEONDELETE
        | FOF_SILENT
        | FOF_NOCONFIRMATION
        | FOF_NOERRORUI
        | FOFX_EARLYFAILURE;

    let file_op: IFileOperation =
        match unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) } {
            Ok(op) => op,
            Err(e) => {
                return paths
                    .iter()
                    .map(|_| Err(format!("Error inicializando IFileOperation: {e}")))
                    .collect();
            }
        };
    let _ = unsafe { file_op.SetOperationFlags(flags) };

    let mut queued_indices = Vec::with_capacity(paths.len());
    let mut results: Vec<Result<(), String>> = vec![Ok(()); paths.len()];

    for (idx, p) in paths.iter().enumerate() {
        if !p.exists() {
            results[idx] = Err("El archivo ya no existe.".to_string());
            continue;
        }
        let wide: Vec<u16> = p
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        match unsafe {
            SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(wide.as_ptr()), None)
        } {
            Ok(shell_item) => {
                if let Err(e) = unsafe { file_op.DeleteItem(&shell_item, None) } {
                    results[idx] = Err(format!("DeleteItem falló: {e}"));
                } else {
                    queued_indices.push(idx);
                }
            }
            Err(e) => {
                results[idx] = Err(format!("No se pudo crear IShellItem: {e}"));
            }
        }
    }

    if queued_indices.is_empty() {
        return results;
    }

    // Single PerformOperations() call for the entire batch!
    let batch_ok = unsafe { file_op.PerformOperations() }.is_ok()
        && !unsafe { file_op.GetAnyOperationsAborted() }
            .unwrap_or(true.into())
            .as_bool();

    if batch_ok {
        let mut any_leftover = false;
        for &idx in &queued_indices {
            if paths[idx].exists() {
                results[idx] =
                    Err("El elemento sigue en disco tras solicitar Papelera.".to_string());
                any_leftover = true;
            }
        }
        if !any_leftover {
            return results;
        }
    }

    // Fallback: isolate failing items one-by-one so valid siblings still succeed
    for &idx in &queued_indices {
        if !paths[idx].exists() {
            results[idx] = Ok(());
            continue;
        }
        results[idx] = recycle_single_item_fresh(&paths[idx], flags);
    }

    results
}

fn recycle_single_item_fresh(
    path: &Path,
    flags: windows::Win32::UI::Shell::FILEOPERATION_FLAGS,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let file_op: IFileOperation = unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }
        .map_err(|e| format!("IFileOperation fallback error: {e}"))?;
    let _ = unsafe { file_op.SetOperationFlags(flags) };
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let shell_item: IShellItem =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
            .map_err(|e| format!("No se pudo crear IShellItem: {e}"))?;
    unsafe {
        file_op
            .DeleteItem(&shell_item, None)
            .map_err(|e| format!("DeleteItem falló: {e}"))?;
        file_op
            .PerformOperations()
            .map_err(|e| format!("La Papelera rechazó el elemento: {e}"))?;
        let aborted = file_op.GetAnyOperationsAborted().unwrap_or(true.into());
        if aborted.as_bool() {
            return Err("Operación abortada por Windows.".to_string());
        }
    }
    if path.exists() {
        return Err("El elemento sigue en disco tras solicitar Papelera.".to_string());
    }
    Ok(())
}

/// Fast permanent deletion using parallel workers + POSIX `FileDispositionInfoEx` with fallback,
/// never following reparse points and always enforcing `is_protected()`.
pub(crate) fn remove_permanently_safe(path: &Path) -> Result<(), String> {
    delete_tree_parallel_fast(path, None, None, true).map(|_| ())
}

/// Executes a cleanup plan with optional fast staging (`stage_only_for_background = true`)
/// where permanent targets are instantly renamed to `<vol>:\.scancleaner-trash\<job>` and returned
/// for parallel background draining!
pub fn execute_cleanup_plan(
    plan: &CleanupPlan,
    confirmed_permanent: bool,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
) -> Result<CleanupResult, String> {
    execute_cleanup_plan_with_options(
        plan,
        confirmed_permanent,
        log_dir_override,
        sandbox_guard,
        None,
        false,
    )
    .map(|(res, _)| res)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProgressEvent {
    pub plan_id: String,
    pub phase: String,
    pub current_label: String,
    pub current_path: String,
    pub completed_items: usize,
    pub total_items: usize,
    pub freed_bytes_so_far: u64,
    pub failed_count: usize,
}

pub fn execute_cleanup_plan_with_options(
    plan: &CleanupPlan,
    confirmed_permanent: bool,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
    sandbox_root: Option<&Path>,
    defer_trash_drain: bool,
) -> Result<(CleanupResult, Vec<PathBuf>), String> {
    execute_cleanup_plan_with_progress(
        plan,
        confirmed_permanent,
        log_dir_override,
        sandbox_guard,
        sandbox_root,
        defer_trash_drain,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn execute_cleanup_plan_with_progress(
    plan: &CleanupPlan,
    confirmed_permanent: bool,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
    sandbox_root: Option<&Path>,
    defer_trash_drain: bool,
    on_progress: Option<&(dyn Fn(CleanupProgressEvent) + Sync)>,
) -> Result<(CleanupResult, Vec<PathBuf>), String> {
    if plan.mode == CleanupMode::Permanent && !confirmed_permanent {
        return Err("El borrado permanente requiere confirmación explícita.".to_string());
    }

    let total_items = plan.items.len().max(1);
    let emit = |phase: &str, label: &str, path: &str, done: usize, freed: u64, failed: usize| {
        if let Some(cb) = on_progress {
            cb(CleanupProgressEvent {
                plan_id: plan.plan_id.clone(),
                phase: phase.to_string(),
                current_label: label.to_string(),
                current_path: path.to_string(),
                completed_items: done,
                total_items,
                freed_bytes_so_far: freed,
                failed_count: failed,
            });
        }
    };
    emit(
        "preparing",
        "Validando guardrails de limpieza…",
        "",
        0,
        0,
        0,
    );

    let log_dir = log_dir_override.map(PathBuf::from).unwrap_or_else(logs_dir);
    let space_provider = WinDiskSpaceProvider;

    // Capture free space BEFORE cleanup per involved volume
    let mut free_before_by_vol: HashMap<String, u64> = HashMap::new();
    for item in &plan.items {
        if let Some(vol) = volume_root_of_path(Path::new(&item.path)) {
            free_before_by_vol
                .entry(vol.clone())
                .or_insert_with(|| space_provider.query_space(&vol).map_or(0, |(_, f)| f));
        }
    }

    if plan.mode == CleanupMode::EmptyRecycleBin {
        emit(
            "deleting",
            "Vaciando Papelera de reciclaje…",
            "$Recycle.Bin",
            0,
            0,
            0,
        );
        let sys_drive = format!(
            "{}\\",
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string())
        );
        let free_before = space_provider.query_space(&sys_drive).map_or(0, |(_, f)| f);

        let hr = unsafe {
            SHEmptyRecycleBinW(
                None,
                PCWSTR::null(),
                SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
            )
        };
        let (status, detail, freed) = if hr.is_ok() {
            ("deleted", None, plan.total_bytes)
        } else {
            (
                "failed",
                Some(format!("SHEmptyRecycleBinW error: {hr:?}")),
                0,
            )
        };
        let meas = measure_real_freed_space(
            &sys_drive,
            free_before,
            plan.total_bytes,
            &space_provider,
            false,
            false,
            status == "failed",
        );

        let _ = append_cleanup_log(
            &log_dir,
            &CleanupLogEntry {
                timestamp: Utc::now().to_rfc3339(),
                plan_id: plan.plan_id.clone(),
                path: "$Recycle.Bin".to_string(),
                category: "recycle_bin".to_string(),
                bytes: freed,
                mode: "empty_recycle_bin".to_string(),
                status: status.to_string(),
                detail: detail.clone(),
                rule_id: None,
                estimated_bytes: Some(plan.total_bytes),
                real_freed_bytes: Some(meas.real_freed_bytes),
                command: None,
            },
        );
        return Ok((
            CleanupResult {
                plan_id: plan.plan_id.clone(),
                mode: plan.mode,
                freed_bytes: freed,
                estimated_bytes: plan.total_bytes,
                real_freed_bytes: Some(meas.real_freed_bytes),
                discrepancy_explanation: meas.discrepancy_explanation,
                background_job_id: None,
                succeeded_count: usize::from(status == "deleted"),
                failed_count: usize::from(status == "failed"),
                skipped_count: 0,
                outcomes: vec![CleanupItemOutcome {
                    path: "$Recycle.Bin".to_string(),
                    bytes: freed,
                    status: status.to_string(),
                    detail,
                }],
            },
            Vec::new(),
        ));
    }

    // Re-validate every item immediately prior to rename/deletion
    let mut valid_items: Vec<&PlannedItem> = Vec::new();
    let mut outcomes: Vec<CleanupItemOutcome> = Vec::new();
    let mut skipped_count = 0usize;

    for item in &plan.items {
        let p = PathBuf::from(&item.path);
        let check = if let Some(guard) = sandbox_guard {
            guard.verify_inside_sandbox(&p)
        } else {
            validate_target_path(&p)
        };
        if let Err(v) = check {
            skipped_count += 1;
            let _ = append_cleanup_log(
                &log_dir,
                &CleanupLogEntry {
                    timestamp: Utc::now().to_rfc3339(),
                    plan_id: plan.plan_id.clone(),
                    path: item.path.clone(),
                    category: item.category.clone(),
                    bytes: 0,
                    mode: format!("{:?}", plan.mode).to_ascii_lowercase(),
                    status: "blocked".to_string(),
                    detail: Some(v.reason.clone()),
                    rule_id: None,
                    estimated_bytes: Some(item.bytes),
                    real_freed_bytes: Some(0),
                    command: None,
                },
            );
            outcomes.push(CleanupItemOutcome {
                path: item.path.clone(),
                bytes: 0,
                status: "blocked".to_string(),
                detail: Some(v.reason),
            });
        } else {
            valid_items.push(item);
        }
    }

    let mut freed_bytes = 0u64;
    let mut succeeded_count = 0usize;
    let mut failed_count = 0usize;
    let mut staged_paths_to_drain: Vec<PathBuf> = Vec::new();

    if plan.mode == CleanupMode::RecycleBin {
        emit(
            "deleting",
            &format!("Enviando {} elementos a la Papelera…", valid_items.len()),
            valid_items.first().map(|i| i.path.as_str()).unwrap_or(""),
            skipped_count,
            0,
            0,
        );
        let paths: Vec<PathBuf> = valid_items.iter().map(|i| PathBuf::from(&i.path)).collect();
        let results = delete_to_recycle_bin_sta(&paths);

        for (idx, (item, res)) in valid_items.into_iter().zip(results).enumerate() {
            let (status, detail, bytes) = match res {
                Ok(()) => {
                    succeeded_count += 1;
                    freed_bytes = freed_bytes.saturating_add(item.bytes);
                    ("recycled", None, item.bytes)
                }
                Err(msg) => {
                    failed_count += 1;
                    ("failed", Some(msg), 0)
                }
            };
            outcomes.push(CleanupItemOutcome {
                path: item.path.clone(),
                bytes,
                status: status.to_string(),
                detail,
            });
            if idx % 10 == 0 || idx + 1 == paths.len() {
                emit(
                    "deleting",
                    "Registrando elementos en Papelera…",
                    &item.path,
                    skipped_count + idx + 1,
                    freed_bytes,
                    failed_count,
                );
            }
        }
    } else {
        for (idx, item) in valid_items.into_iter().enumerate() {
            emit(
                "deleting",
                "Eliminando de forma segura…",
                &item.path,
                skipped_count + idx,
                freed_bytes,
                failed_count,
            );
            let p = PathBuf::from(&item.path);
            let res = if let Ok(staged) =
                stage_to_same_volume_trash(&p, &plan.plan_id, sandbox_guard, sandbox_root)
            {
                if defer_trash_drain {
                    staged_paths_to_drain.push(staged);
                    Ok(())
                } else {
                    delete_tree_parallel_fast(&staged, None, None, false).map(|_| ())
                }
            } else {
                // Direct parallel delete fallback (e.g. if staging dir couldn't be created)
                delete_tree_parallel_fast(&p, None, None, true).map(|_| ())
            };

            let (status, detail, bytes) = match res {
                Ok(()) => {
                    succeeded_count += 1;
                    freed_bytes = freed_bytes.saturating_add(item.bytes);
                    ("deleted_permanent", None, item.bytes)
                }
                Err(msg) => {
                    failed_count += 1;
                    ("failed", Some(msg), 0)
                }
            };
            outcomes.push(CleanupItemOutcome {
                path: item.path.clone(),
                bytes,
                status: status.to_string(),
                detail,
            });
            emit(
                "deleting",
                "Procesando lote de limpieza…",
                &item.path,
                skipped_count + idx + 1,
                freed_bytes,
                failed_count,
            );
        }
    }
    emit(
        "finalizing",
        "Midiendo espacio liberado y guardando auditoría…",
        "",
        total_items,
        freed_bytes,
        failed_count,
    );

    // Measure real freed space delta across affected volumes
    let mut total_real_freed = 0u64;
    let mut discrepancy: Option<String> = None;
    for (vol, free_before) in &free_before_by_vol {
        let pending = defer_trash_drain || has_pending_trash_on_volume(vol);
        let m = measure_real_freed_space(
            vol,
            *free_before,
            freed_bytes,
            &space_provider,
            pending,
            false,
            failed_count > 0,
        );
        total_real_freed = total_real_freed.saturating_add(m.real_freed_bytes);
        if discrepancy.is_none() {
            discrepancy = m.discrepancy_explanation;
        }
    }

    // Persist audit logs including BOTH estimated and real freed bytes
    for outcome in &outcomes {
        if outcome.status == "blocked" {
            continue;
        }
        let _ = append_cleanup_log(
            &log_dir,
            &CleanupLogEntry {
                timestamp: Utc::now().to_rfc3339(),
                plan_id: plan.plan_id.clone(),
                path: outcome.path.clone(),
                category: "cleanup".to_string(),
                bytes: outcome.bytes,
                mode: format!("{:?}", plan.mode).to_ascii_lowercase(),
                status: outcome.status.clone(),
                detail: outcome.detail.clone(),
                rule_id: None,
                estimated_bytes: Some(outcome.bytes),
                real_freed_bytes: Some(total_real_freed),
                command: None,
            },
        );
    }

    Ok((
        CleanupResult {
            plan_id: plan.plan_id.clone(),
            mode: plan.mode,
            freed_bytes,
            estimated_bytes: plan.total_bytes,
            real_freed_bytes: Some(total_real_freed),
            discrepancy_explanation: discrepancy,
            background_job_id: if staged_paths_to_drain.is_empty() {
                None
            } else {
                Some(plan.plan_id.clone())
            },
            succeeded_count,
            failed_count,
            skipped_count,
            outcomes,
        },
        staged_paths_to_drain,
    ))
}
