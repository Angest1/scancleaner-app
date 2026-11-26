use chrono::Utc;
use serde::{Deserialize, Serialize};
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

use super::guardrails::{is_reparse_point, validate_target_path, SandboxGuard};
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
            // Expand direct children inside Temp directory rather than deleting Temp directory itself
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

fn delete_to_recycle_bin_sta(paths: &[PathBuf]) -> Vec<Result<(), String>> {
    let paths_owned = paths.to_vec();
    std::thread::spawn(move || {
        let init_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let com_initialized = init_hr.is_ok();

        let mut results = Vec::with_capacity(paths_owned.len());
        let file_op_res: windows::core::Result<IFileOperation> =
            unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) };

        match file_op_res {
            Ok(file_op) => {
                let flags = FOF_ALLOWUNDO
                    | FOFX_RECYCLEONDELETE
                    | FOF_SILENT
                    | FOF_NOCONFIRMATION
                    | FOF_NOERRORUI
                    | FOFX_EARLYFAILURE;
                let _ = unsafe { file_op.SetOperationFlags(flags) };

                for p in &paths_owned {
                    let res = recycle_single_item(&file_op, p);
                    results.push(res);
                }
            }
            Err(e) => {
                for _ in &paths_owned {
                    results.push(Err(format!("Error inicializando IFileOperation: {e}")));
                }
            }
        }

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

fn recycle_single_item(file_op: &IFileOperation, path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("El archivo ya no existe.".to_string());
    }
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
            .map_err(|e| format!("La Papelera rechazó el elemento (no se borró): {e}"))?;

        let aborted = file_op.GetAnyOperationsAborted().unwrap_or(true.into());
        if aborted.as_bool() {
            return Err("Operación abortada por Windows; el archivo no fue eliminado.".to_string());
        }
    }

    // Verify it actually moved/disappeared rather than silently failing
    if path.exists() {
        return Err("El elemento sigue en disco tras solicitar Papelera.".to_string());
    }
    Ok(())
}

fn remove_permanently_safe(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if is_reparse_point(path) {
        return Err("Reparse point bloqueado durante borrado permanente.".to_string());
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_file() {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    } else if meta.is_dir() {
        // Recursive traversal checking every subdirectory for reparse points
        let rd = std::fs::read_dir(path).map_err(|e| e.to_string())?;
        for entry in rd.flatten() {
            let child = entry.path();
            if is_reparse_point(&child) {
                continue;
            }
            remove_permanently_safe(&child)?;
        }
        std::fs::remove_dir(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

pub fn execute_cleanup_plan(
    plan: &CleanupPlan,
    confirmed_permanent: bool,
    log_dir_override: Option<&Path>,
    sandbox_guard: Option<&SandboxGuard>,
) -> Result<CleanupResult, String> {
    if plan.mode == CleanupMode::Permanent && !confirmed_permanent {
        return Err("El borrado permanente requiere confirmación explícita.".to_string());
    }

    let log_dir = log_dir_override.map(PathBuf::from).unwrap_or_else(logs_dir);

    if plan.mode == CleanupMode::EmptyRecycleBin {
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
            },
        );
        return Ok(CleanupResult {
            plan_id: plan.plan_id.clone(),
            mode: plan.mode,
            freed_bytes: freed,
            succeeded_count: usize::from(status == "deleted"),
            failed_count: usize::from(status == "failed"),
            skipped_count: 0,
            outcomes: vec![CleanupItemOutcome {
                path: "$Recycle.Bin".to_string(),
                bytes: freed,
                status: status.to_string(),
                detail,
            }],
        });
    }

    // Re-validate every item immediately prior to deletion
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

    if plan.mode == CleanupMode::RecycleBin {
        let paths: Vec<PathBuf> = valid_items.iter().map(|i| PathBuf::from(&i.path)).collect();
        let results = delete_to_recycle_bin_sta(&paths);

        for (item, res) in valid_items.into_iter().zip(results) {
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
            let _ = append_cleanup_log(
                &log_dir,
                &CleanupLogEntry {
                    timestamp: Utc::now().to_rfc3339(),
                    plan_id: plan.plan_id.clone(),
                    path: item.path.clone(),
                    category: item.category.clone(),
                    bytes,
                    mode: "recycle_bin".to_string(),
                    status: status.to_string(),
                    detail: detail.clone(),
                },
            );
            outcomes.push(CleanupItemOutcome {
                path: item.path.clone(),
                bytes,
                status: status.to_string(),
                detail,
            });
        }
    } else {
        for item in valid_items {
            let p = PathBuf::from(&item.path);
            let (status, detail, bytes) = match remove_permanently_safe(&p) {
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
            let _ = append_cleanup_log(
                &log_dir,
                &CleanupLogEntry {
                    timestamp: Utc::now().to_rfc3339(),
                    plan_id: plan.plan_id.clone(),
                    path: item.path.clone(),
                    category: item.category.clone(),
                    bytes,
                    mode: "permanent".to_string(),
                    status: status.to_string(),
                    detail: detail.clone(),
                },
            );
            outcomes.push(CleanupItemOutcome {
                path: item.path.clone(),
                bytes,
                status: status.to_string(),
                detail,
            });
        }
    }

    Ok(CleanupResult {
        plan_id: plan.plan_id.clone(),
        mode: plan.mode,
        freed_bytes,
        succeeded_count,
        failed_count,
        skipped_count,
        outcomes,
    })
}
