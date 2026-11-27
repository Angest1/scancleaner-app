use chrono::Utc;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{Emitter, Manager, State};

use crate::apps::{
    list_installed_apps, open_windows_apps_settings, trigger_native_uninstall, AppSource,
    InstalledApp,
};
use crate::categorizer::{classify_tree, user_rules_path, CategoryBreakdown, RulesConfig};
use crate::cleaner::detectors::{discover_cleanup_candidates_from_trees, CleanupCandidatesReport};
use crate::cleaner::executor::{
    build_cleanup_plan, execute_cleanup_plan_with_options, CleanupMode, CleanupPlan, CleanupResult,
    PreviewCleanupRequest,
};
use crate::deletion::fast_delete::{delete_tree_parallel_fast, resume_pending_trash_drains};
use crate::deletion::queue::{DeletionJobPhase, DeletionJobStatus};
use crate::dev_cleaner::executor::{build_dev_cleanup_plan, execute_dev_cleanup_plan};
use crate::dev_cleaner::folder_picker::{pick_file_native, pick_folder_native};
use crate::dev_cleaner::process::SystemCommandRunner;
use crate::dev_cleaner::scan_dev_cleanup_from_trees;
use crate::dev_cleaner::schema::{
    DevCleanerConfig, DevCleanupPlan, DevCleanupResult, DevPreviewRequest, DevRulesCatalog,
    DevScanReport,
};
use crate::diagnostics::{log_info, log_timing, log_warn, read_diagnostic_logs};
use crate::elevation::{current_elevation_status, is_elevated, relaunch_as_admin, ElevationStatus};
use crate::index::default_index_dir;
use crate::index::snapshot::{default_snapshot_path, load_volume_snapshot, save_volume_snapshot};
use crate::index::usn::{
    sync_tree_with_usn, RealFileMetaLookup, UsnJournalReader, UsnSyncOutcome, WinUsnJournalReader,
};
use crate::index::watcher::VolumeDirWatcher;
use crate::protection::{
    is_protected_default, user_protection_config_path, ProtectedEntry, ProtectionEngine,
    ProtectionOrigin, ProtectionResult, ProtectionType,
};
use crate::reconcile::queue::{
    ReconcileOrigin, ReconcilePriority, ReconcileRequest, SyncStatusDto,
};
use crate::reconcile::triggers::{
    build_post_delete_reconcile_requests, watch_native_uninstall_completion,
    UninstallLeftoverSuggestion, WinUninstallPresenceProbe,
};
use crate::scanner::derived::{normalize_volume_key, volume_root_of_path, DerivedBuildContext};
use crate::scanner::drives::{list_drives, DriveInfo, DriveKind};
use crate::scanner::model::{
    FileSystemType, NodeFlags, NodeId, NodeKind, ScanEngineMode, ScanProgress, ScanSummary,
};
use crate::scanner::treemap_layout::{compute_treemap_layout, TreemapLayoutDto};
use crate::scanner::{scan_directory_with_options, ScanOptions};
use crate::service::client::{ping_service, ServiceUsnJournalReader};
use crate::service::installer::{
    query_service_status, request_service_action_uac, set_start_with_windows, IndexingServiceStatus,
};
use crate::settings::AppSettings;
use crate::state::AppState;
use crate::storage::{logs_dir, read_cleanup_logs, CleanupLogEntry, ScanSnapshot, SnapshotDiff};
use crate::system_files::{
    inspect_system_files, locked_file_size, open_power_options, open_virtual_memory_dialog,
    run_powercfg_elevated, PowercfgAction, SystemFilesReport, WinSystemProcessLauncher,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupContext {
    pub elevation: ElevationStatus,
    pub startup_drive: Option<String>,
    pub rules_path: String,
    pub settings: AppSettings,
    pub service_status: IndexingServiceStatus,
    pub windows_animations_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveProgressItem {
    pub drive_root: String,
    pub label: String,
    pub used_bytes: u64,
    pub progress_ratio: Option<f64>,
    pub phase: String,
    pub files: u64,
    pub allocated_bytes: u64,
    pub done: bool,
    pub stale: bool,
    pub engine_mode: ScanEngineMode,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartupBootstrapEvent {
    DrivesReady {
        drives: Vec<DriveInfo>,
    },
    Progress {
        global_ratio: Option<f64>,
        phase: String,
        total_files: u64,
        total_allocated_bytes: u64,
        drives: Vec<DriveProgressItem>,
    },
    Ready {
        warm: bool,
        partial: bool,
        has_previous_index: bool,
        observations: Vec<String>,
        summaries: Vec<ScanSummary>,
        elapsed_ms: u64,
    },
    Error {
        message: String,
        has_previous_index: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbItem {
    pub node_id: NodeId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapNodeDto {
    pub node_id: Option<NodeId>,
    pub name: String,
    pub full_path: String,
    pub kind: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u32,
    pub type_cat: String,
    pub origin_cat: String,
    pub percentage: f64,
    pub is_other_bucket: bool,
    pub is_protected: bool,
    pub is_cloud_only: bool,
    pub is_pending_trash: bool,
    pub protection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapViewDto {
    pub node_id: NodeId,
    pub full_path: String,
    pub total_allocated_bytes: u64,
    pub file_count: u32,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub children: Vec<TreemapNodeDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopEntryDto {
    pub node_id: NodeId,
    pub name: String,
    pub full_path: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u32,
    pub last_write_ts: i64,
    pub type_cat: String,
    pub origin_cat: String,
    pub is_protected: bool,
    pub is_cloud_only: bool,
    pub protection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHitDto {
    pub node_id: NodeId,
    pub name: String,
    pub full_path: String,
    pub kind: String,
    pub allocated_bytes: u64,
    pub is_protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSyncResult {
    pub flushed_batches: usize,
    pub system_file_notice: Option<String>,
}

/// Checks Windows "Show animations in Windows" setting (`SPI_GETCLIENTAREAANIMATION`).
pub fn is_windows_animations_enabled() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut enabled: i32 = 1;
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut i32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_err() || enabled != 0
}

#[tauri::command]
pub fn get_startup_context(state: State<'_, AppState>) -> StartupContext {
    let settings = state.settings.read().map(|g| g.clone()).unwrap_or_default();
    StartupContext {
        elevation: current_elevation_status(),
        startup_drive: state.startup_drive.clone(),
        rules_path: user_rules_path().to_string_lossy().into_owned(),
        settings,
        service_status: query_service_status(),
        windows_animations_enabled: is_windows_animations_enabled(),
    }
}

#[tauri::command]
pub fn cmd_show_main_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn cmd_list_drives(include_network: bool) -> Vec<DriveInfo> {
    list_drives(include_network)
}

#[tauri::command]
pub fn cmd_set_active_drive(drive_root: String, state: State<'_, AppState>) -> Result<(), String> {
    let key = normalize_volume_key(&drive_root);
    if let Ok(mut ad) = state.active_drive.write() {
        *ad = Some(key.clone());
    }
    if let Some(slot) = state.get_volume_slot(Some(&key)) {
        if let Ok(g) = slot.read() {
            let bd = g
                .tree
                .derived
                .to_category_breakdown(g.tree.stats.total_allocated_bytes);
            if let Ok(mut bdg) = state.active_breakdown.lock() {
                *bdg = Some(bd);
            }
        }
    }
    Ok(())
}

/// Instant startup bootstrap across all local drives (fixed + removable USB, excluding network):
/// 1. Emits `DrivesReady` immediately (`GetDiskFreeSpaceExW`).
/// 2. Loads any valid binary snapshots (`SCIDX v1`, mmap + LZ4) in parallel (< 1s warm).
/// 3. If snapshots exist, immediately emits `Ready { warm: true }` and starts background USN delta sync
///    (or fallback rescan if non-admin / stale) + live watchers + pending trash drain.
/// 4. On cold start (no valid snapshots), scans local drives emitting weighted global + per-drive
///    determinate progress at ~10 Hz, persists snapshots atomically, and emits `Ready { warm: false }`.
#[tauri::command]
pub async fn cmd_bootstrap_startup(
    on_event: Channel<StartupBootstrapEvent>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let t0 = Instant::now();
    let drives: Vec<DriveInfo> = tauri::async_runtime::spawn_blocking(|| {
        list_drives(false)
            .into_iter()
            .filter(|d| {
                d.available
                    && matches!(
                        d.kind,
                        DriveKind::Fixed | DriveKind::Removable | DriveKind::RamDisk
                    )
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?;

    let _ = on_event.send(StartupBootstrapEvent::DrivesReady {
        drives: drives.clone(),
    });

    if drives.is_empty() {
        let _ = on_event.send(StartupBootstrapEvent::Error {
            message: "No se detectaron unidades locales disponibles.".to_string(),
            has_previous_index: false,
        });
        return Ok(());
    }

    // Resume draining any leftover `.scancleaner-trash` folders from previous sessions in background
    resume_pending_trash_drains(drives.iter().map(|d| d.root_path.clone()).collect());

    let include_ads = state
        .settings
        .read()
        .map(|s| s.include_ads)
        .unwrap_or(false);
    let idx_dir = default_index_dir();

    // Try loading persisted binary snapshots in parallel
    let _ = on_event.send(StartupBootstrapEvent::Progress {
        global_ratio: Some(0.08),
        phase: "Leyendo índice…".to_string(),
        total_files: 0,
        total_allocated_bytes: 0,
        drives: drives
            .iter()
            .map(|d| DriveProgressItem {
                drive_root: d.root_path.clone(),
                label: d.label.clone(),
                used_bytes: d.used_bytes,
                progress_ratio: Some(0.08),
                phase: "Leyendo índice…".to_string(),
                files: 0,
                allocated_bytes: 0,
                done: false,
                stale: false,
                engine_mode: ScanEngineMode::SnapshotCached,
            })
            .collect(),
    });

    let drives_for_load = drives.clone();
    let loaded_snaps: Vec<(DriveInfo, Option<crate::scanner::model::ScanTree>)> =
        tauri::async_runtime::spawn_blocking(move || {
            drives_for_load
                .into_par_iter()
                .map(|d| {
                    let path = default_snapshot_path(&d.root_path, d.volume_serial);
                    let loaded = load_volume_snapshot(&path, Some(d.volume_serial))
                        .ok()
                        .filter(|(t, _)| t.include_ads == include_ads)
                        .map(|(t, _)| t);
                    (d, loaded)
                })
                .collect()
        })
        .await
        .map_err(|e| e.to_string())?;

    let any_loaded = loaded_snaps.iter().any(|(_, t)| t.is_some());
    let has_fast_usn = is_elevated() || ping_service();

    if any_loaded {
        let mut summaries = Vec::new();
        let mut missing_drives = Vec::new();
        let mut loaded_roots = Vec::new();
        let mut observations = Vec::new();

        for (d, tree_opt) in loaded_snaps {
            if let Some(mut tree) = tree_opt {
                let stale = !has_fast_usn || tree.fs_type != FileSystemType::Ntfs;
                tree.stats.stale = stale;
                let summary = ScanSummary {
                    root_path: tree.root_path.to_string_lossy().into_owned(),
                    fs_type: tree.fs_type,
                    stats: tree.stats.clone(),
                    snapshot_id: None,
                };
                loaded_roots.push(d.clone());
                state.insert_volume(tree, stale);
                summaries.push(summary);
            } else {
                missing_drives.push(d);
            }
        }

        if !has_fast_usn {
            observations.push(
                "Sin permisos de administrador ni servicio de indexación: los snapshots se muestran de inmediato y se actualizan en segundo plano con la ruta de respaldo."
                    .to_string(),
            );
        }

        let elapsed = t0.elapsed().as_millis() as u64;
        log_timing(
            "cmd_bootstrap_startup_warm",
            elapsed as u128,
            &format!(
                "loaded={} missing={}",
                summaries.len(),
                missing_drives.len()
            ),
        );

        let _ = on_event.send(StartupBootstrapEvent::Ready {
            warm: true,
            partial: !has_fast_usn || !missing_drives.is_empty(),
            has_previous_index: true,
            observations,
            summaries,
            elapsed_ms: elapsed,
        });

        // Spawn background incremental USN sync (or fallback rescan) for loaded drives & missing drives
        let app_bg = app.clone();
        std::thread::Builder::new()
            .name("startup-bg-sync".to_string())
            .spawn(move || {
                let st = app_bg.state::<AppState>();
                let ctx = DerivedBuildContext::load_default();
                let meta = RealFileMetaLookup;
                let use_service_usn = !is_elevated() && ping_service();

                for d in &loaded_roots {
                    start_watcher_for_drive(&app_bg, &d.root_path);
                    if let Some(slot) = st.get_volume_slot(Some(&d.root_path)) {
                        let needs_full = if d.fs_type == FileSystemType::Ntfs
                            && (is_elevated() || use_service_usn)
                        {
                            let reader: Box<dyn UsnJournalReader> = if is_elevated() {
                                Box::new(WinUsnJournalReader)
                            } else {
                                Box::new(ServiceUsnJournalReader)
                            };
                            if let Ok(mut g) = slot.write() {
                                match sync_tree_with_usn(&mut g.tree, reader.as_ref(), &meta, &ctx)
                                {
                                    UsnSyncOutcome::UpToDate { applied_events, .. } => {
                                        g.stale = false;
                                        if applied_events > 0 {
                                            let _ = save_volume_snapshot(&g.tree, &idx_dir);
                                        }
                                        false
                                    }
                                    UsnSyncOutcome::NeedsFullRescan { .. }
                                    | UsnSyncOutcome::Unavailable { .. } => true,
                                }
                            } else {
                                false
                            }
                        } else {
                            // No admin/journal: mark stale and rescan in background with fallback
                            true
                        };

                        if needs_full {
                            let cancel = Arc::new(AtomicBool::new(false));
                            let new_tree = scan_directory_with_options(
                                PathBuf::from(&d.root_path),
                                ScanOptions {
                                    include_ads,
                                    prefer_mft: true,
                                },
                                cancel,
                                |_| {},
                            );
                            if !new_tree.stats.cancelled {
                                let _ = save_volume_snapshot(&new_tree, &idx_dir);
                                st.insert_volume(new_tree, false);
                                let _ = app_bg.emit("sync-updated", &d.root_path);
                            }
                        }
                    }
                }

                for d in missing_drives {
                    start_watcher_for_drive(&app_bg, &d.root_path);
                    let cancel = Arc::new(AtomicBool::new(false));
                    let tree = scan_directory_with_options(
                        PathBuf::from(&d.root_path),
                        ScanOptions {
                            include_ads,
                            prefer_mft: true,
                        },
                        cancel,
                        |_| {},
                    );
                    if !tree.stats.cancelled {
                        let _ = save_volume_snapshot(&tree, &idx_dir);
                        st.insert_volume(tree, false);
                        let _ = app_bg.emit("sync-updated", &d.root_path);
                    }
                }
            })
            .ok();

        return Ok(());
    }

    // Cold start: scan all local drives with weighted global + per-drive progress at ~10 Hz
    let cancel_flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut cg) = state.cancel_flag.lock() {
        *cg = Some(cancel_flag.clone());
    }

    let total_used_weight: u64 = drives
        .iter()
        .map(|d| d.used_bytes.max(1024 * 1024))
        .sum::<u64>()
        .max(1);

    let drive_states: Arc<Mutex<Vec<DriveProgressItem>>> = Arc::new(Mutex::new(
        drives
            .iter()
            .map(|d| DriveProgressItem {
                drive_root: d.root_path.clone(),
                label: d.label.clone(),
                used_bytes: d.used_bytes,
                progress_ratio: Some(0.0),
                phase: format!("Pendiente {}", d.root_path),
                files: 0,
                allocated_bytes: 0,
                done: false,
                stale: false,
                engine_mode: ScanEngineMode::FallbackWorkStealing,
            })
            .collect(),
    ));
    let last_global_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_millis(200)));

    let drives_for_cold = drives.clone();
    let ds_clone = drive_states.clone();
    let emit_clone = last_global_emit.clone();
    let on_event_clone = on_event.clone();
    let cancel_cold = cancel_flag.clone();

    let scanned_trees: Vec<crate::scanner::model::ScanTree> =
        tauri::async_runtime::spawn_blocking(move || {
            let mut trees = Vec::with_capacity(drives_for_cold.len());
            for (idx, d) in drives_for_cold.iter().enumerate() {
                if cancel_cold.load(Ordering::Relaxed) {
                    break;
                }
                let root_pb = PathBuf::from(&d.root_path);
                let ds_inner = ds_clone.clone();
                let emit_inner = emit_clone.clone();
                let ev_inner = on_event_clone.clone();
                let tree = scan_directory_with_options(
                    root_pb,
                    ScanOptions {
                        include_ads,
                        prefer_mft: true,
                    },
                    cancel_cold.clone(),
                    move |prog| {
                        if let Ok(mut list) = ds_inner.lock() {
                            if let Some(slot) = list.get_mut(idx) {
                                slot.progress_ratio = prog.progress_ratio;
                                slot.phase = prog.phase.clone();
                                slot.files = prog.scanned_files;
                                slot.allocated_bytes = prog.total_allocated_bytes;
                                slot.engine_mode = prog.engine_mode;
                            }
                            if let Ok(mut last) = emit_inner.lock() {
                                if last.elapsed() >= Duration::from_millis(100) {
                                    *last = Instant::now();
                                    let weighted: f64 = list
                                        .iter()
                                        .map(|item| {
                                            let w = item.used_bytes.max(1024 * 1024) as f64
                                                / total_used_weight as f64;
                                            let r = if item.done {
                                                1.0
                                            } else {
                                                item.progress_ratio.unwrap_or(0.0)
                                            };
                                            w * r
                                        })
                                        .sum();
                                    let tf: u64 = list.iter().map(|i| i.files).sum();
                                    let tb: u64 = list.iter().map(|i| i.allocated_bytes).sum();
                                    let _ = ev_inner.send(StartupBootstrapEvent::Progress {
                                        global_ratio: Some(weighted.clamp(0.0, 0.99)),
                                        phase: prog.phase,
                                        total_files: tf,
                                        total_allocated_bytes: tb,
                                        drives: list.clone(),
                                    });
                                }
                            }
                        }
                    },
                );

                if !tree.stats.cancelled {
                    let _ = save_volume_snapshot(&tree, &idx_dir);
                }
                if let Ok(mut list) = ds_clone.lock() {
                    if let Some(slot) = list.get_mut(idx) {
                        slot.done = true;
                        slot.progress_ratio = Some(1.0);
                        slot.files = tree.stats.total_files;
                        slot.allocated_bytes = tree.stats.total_allocated_bytes;
                        slot.phase = "Completado".to_string();
                    }
                }
                trees.push(tree);
            }
            trees
        })
        .await
        .map_err(|e| e.to_string())?;

    let mut summaries = Vec::new();
    let mut observations = Vec::new();
    let mut partial = false;

    for tree in scanned_trees {
        let root_s = tree.root_path.to_string_lossy().into_owned();
        if tree.stats.access_denied_count > 0 {
            partial = true;
            observations.push(format!(
                "{root_s}: {} carpetas sin permiso de lectura (instala el servicio de indexación en Ajustes para lectura MFT completa).",
                tree.stats.access_denied_count
            ));
        }
        if tree.stats.engine_mode == ScanEngineMode::FallbackWorkStealing
            && tree.fs_type == FileSystemType::Ntfs
            && !is_elevated()
        {
            partial = true;
        }
        let summary = ScanSummary {
            root_path: root_s.clone(),
            fs_type: tree.fs_type,
            stats: tree.stats.clone(),
            snapshot_id: None,
        };
        state.insert_volume(tree, false);
        start_watcher_for_drive(&app, &root_s);
        summaries.push(summary);
    }

    if partial && observations.is_empty() {
        observations.push(
            "Escaneo completado con la ruta de respaldo (sin privilegios MFT). Puedes activar el servicio opcional en Ajustes para escaneos instantáneos."
                .to_string(),
        );
    }

    let elapsed = t0.elapsed().as_millis() as u64;
    log_timing(
        "cmd_bootstrap_startup_cold",
        elapsed as u128,
        &format!("drives={}", summaries.len()),
    );

    let _ = on_event.send(StartupBootstrapEvent::Ready {
        warm: false,
        partial,
        has_previous_index: false,
        observations,
        summaries,
        elapsed_ms: elapsed,
    });

    Ok(())
}

pub fn start_watcher_for_drive(app: &tauri::AppHandle, drive_root: &str) {
    let key = normalize_volume_key(drive_root);
    let state = app.state::<AppState>();
    if let Ok(guard) = state.watchers.lock() {
        if guard.contains_key(&key) {
            return;
        }
    }
    let queue_arc = state.reconcile_queue.clone();
    let vol_for_cb = key.clone();
    if let Ok(watcher) = VolumeDirWatcher::start(PathBuf::from(&key), move |paths| {
        if let Ok(mut q) = queue_arc.lock() {
            q.enqueue(
                ReconcileRequest {
                    volume: vol_for_cb.clone(),
                    paths,
                    origin: ReconcileOrigin::DirWatcher,
                    priority: ReconcilePriority::Medium,
                },
                Instant::now(),
            );
        }
    }) {
        if let Ok(mut guard) = state.watchers.lock() {
            guard.insert(key, watcher);
        }
    }
}

#[tauri::command]
pub async fn cmd_start_scan(
    root_path: String,
    on_progress: Channel<ScanProgress>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanSummary, String> {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.cancel_flag.lock().map_err(|e| e.to_string())?;
        if let Some(prev) = guard.as_ref() {
            prev.store(true, Ordering::Relaxed);
        }
        *guard = Some(cancel_flag.clone());
    }

    let include_ads = state
        .settings
        .read()
        .map(|s| s.include_ads)
        .unwrap_or(false);
    let t0 = Instant::now();
    log_info(&format!("cmd_start_scan begin: {root_path}"));
    let root = PathBuf::from(&root_path);
    let tree = tauri::async_runtime::spawn_blocking(move || {
        scan_directory_with_options(
            root,
            ScanOptions {
                include_ads,
                prefer_mft: true,
            },
            cancel_flag,
            |prog| {
                let _ = on_progress.send(prog);
            },
        )
    })
    .await
    .map_err(|e| format!("Error ejecutando el escaneo: {e}"))?;

    let breakdown = tree
        .derived
        .to_category_breakdown(tree.stats.total_allocated_bytes);

    log_timing(
        "cmd_start_scan",
        t0.elapsed().as_millis(),
        &format!(
            "root={root_path} files={} dirs={} mode={:?} cancelled={}",
            tree.stats.total_files,
            tree.stats.total_dirs,
            tree.stats.engine_mode,
            tree.stats.cancelled
        ),
    );

    let mut snapshot_id = None;
    if !tree.stats.cancelled {
        let _ = save_volume_snapshot(&tree, &default_index_dir());
        let drives = list_drives(false);
        let (total_disk, free_disk) = drives
            .iter()
            .find(|d| d.root_path.eq_ignore_ascii_case(&root_path))
            .map_or((0, 0), |d| (d.total_bytes, d.free_bytes));

        if let Ok(id) =
            state
                .storage
                .save_snapshot(&tree, total_disk, free_disk, &breakdown.by_origin)
        {
            snapshot_id = Some(id);
        }
    }

    let summary = ScanSummary {
        root_path: tree.root_path.to_string_lossy().into_owned(),
        fs_type: tree.fs_type,
        stats: tree.stats.clone(),
        snapshot_id,
    };

    let key = normalize_volume_key(&summary.root_path);
    state.insert_volume(tree, false);
    if let Ok(mut ad) = state.active_drive.write() {
        *ad = Some(key.clone());
    }
    start_watcher_for_drive(&app, &key);

    Ok(summary)
}

#[tauri::command]
pub fn cmd_cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.cancel_flag.lock().map_err(|e| e.to_string())?;
    if let Some(flag) = guard.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_get_treemap_layout(
    drive_root: Option<String>,
    node_id: NodeId,
    width: f64,
    height: f64,
    min_px: Option<f64>,
    max_items: Option<usize>,
    state: State<'_, AppState>,
) -> Result<TreemapLayoutDto, String> {
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    let limit = max_items.unwrap_or(140).clamp(10, 400);
    compute_treemap_layout(
        &guard.tree,
        node_id,
        width,
        height,
        min_px.unwrap_or(3.0),
        limit,
    )
}

#[tauri::command]
pub fn cmd_get_treemap_children(
    node_id: NodeId,
    max_items: Option<usize>,
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<TreemapViewDto, String> {
    let limit = max_items.unwrap_or(140).clamp(10, 400);
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    let tree = &guard.tree;

    let idx = node_id as usize;
    if idx >= tree.len() {
        return Err("Nodo fuera de rango.".to_string());
    }

    let total_alloc = tree.allocated_size[idx].max(1);

    let mut breadcrumbs = Vec::new();
    let mut cur = node_id;
    let mut depth_guard = 0usize;
    while cur != crate::scanner::model::NONE_NODE && depth_guard < 256 {
        breadcrumbs.push(BreadcrumbItem {
            node_id: cur,
            name: tree.node_name(cur).to_string(),
        });
        cur = tree.parent_of(cur);
        depth_guard += 1;
    }
    breadcrumbs.reverse();

    let mut raw_children: Vec<NodeId> = tree.children_iter(node_id).collect();
    raw_children
        .sort_by(|&a, &b| tree.allocated_size[b as usize].cmp(&tree.allocated_size[a as usize]));

    let mut children = Vec::new();
    let prot_engine = ProtectionEngine::load_default();
    let mut other_bytes = 0u64;
    let mut other_logical = 0u64;
    let mut other_files = 0u32;
    let mut other_count = 0usize;

    for (pos, cid) in raw_children.into_iter().enumerate() {
        let cidx = cid as usize;
        let alloc = tree.allocated_size[cidx];
        if alloc == 0 && pos >= 30 {
            continue;
        }
        if pos < limit {
            let full_pb = tree.reconstruct_path(cid);
            let flags = tree.flags_of(cid);
            let is_prot = flags.contains(NodeFlags::PROTECTED)
                || flags.contains(NodeFlags::PROTECTED_DESCENDANT);
            let reason = if is_prot {
                prot_engine.is_protected_for_ui(&full_pb).reason
            } else {
                None
            };
            children.push(TreemapNodeDto {
                node_id: Some(cid),
                name: tree.node_name(cid).to_string(),
                full_path: full_pb.to_string_lossy().into_owned(),
                kind: format!("{:?}", tree.kind_of(cid)).to_ascii_lowercase(),
                allocated_bytes: alloc,
                logical_bytes: tree.logical_size[cidx],
                file_count: tree.file_count[cidx],
                type_cat: format!("{:?}", tree.type_cat_of(cid)).to_ascii_lowercase(),
                origin_cat: format!("{:?}", tree.origin_cat_of(cid)).to_ascii_lowercase(),
                percentage: (alloc as f64 / total_alloc as f64) * 100.0,
                is_other_bucket: false,
                is_protected: is_prot,
                is_cloud_only: flags.contains(NodeFlags::CLOUD_ONLY),
                is_pending_trash: flags.contains(NodeFlags::PENDING_TRASH),
                protection_reason: reason,
            });
        } else {
            other_bytes = other_bytes.saturating_add(alloc);
            other_logical = other_logical.saturating_add(tree.logical_size[cidx]);
            other_files = other_files.saturating_add(tree.file_count[cidx]);
            other_count += 1;
        }
    }

    if other_count > 0 && other_bytes > 0 {
        children.push(TreemapNodeDto {
            node_id: None,
            name: format!("Otros ({other_count} elementos)"),
            full_path: tree
                .reconstruct_path(node_id)
                .to_string_lossy()
                .into_owned(),
            kind: "other".to_string(),
            allocated_bytes: other_bytes,
            logical_bytes: other_logical,
            file_count: other_files,
            type_cat: "other".to_string(),
            origin_cat: "userother".to_string(),
            percentage: (other_bytes as f64 / total_alloc as f64) * 100.0,
            is_other_bucket: true,
            is_protected: false,
            is_cloud_only: false,
            is_pending_trash: false,
            protection_reason: None,
        });
    }

    Ok(TreemapViewDto {
        node_id,
        full_path: tree
            .reconstruct_path(node_id)
            .to_string_lossy()
            .into_owned(),
        total_allocated_bytes: tree.allocated_size[idx],
        file_count: tree.file_count[idx],
        breadcrumbs,
        children,
    })
}

#[tauri::command]
pub fn cmd_get_top_entries(
    scope_node_id: Option<NodeId>,
    kind: String,
    limit: Option<usize>,
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<TopEntryDto>, String> {
    let max_rows = limit.unwrap_or(100).clamp(1, 500);
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    let tree = &guard.tree;

    let scope = scope_node_id.unwrap_or(0);
    let want_dirs = kind == "dirs";

    // Fast path: root scope uses precomputed `derived.top_dirs` / `derived.top_files` in O(limit)!
    let matches: Vec<NodeId> = if scope == 0 {
        let src = if want_dirs {
            &tree.derived.top_dirs
        } else {
            &tree.derived.top_files
        };
        src.iter()
            .copied()
            .filter(|&nid| {
                (nid as usize) < tree.len()
                    && !tree.flags_of(nid).contains(NodeFlags::TOMBSTONE)
                    && !tree.flags_of(nid).contains(NodeFlags::OPTIMISTIC_HIDDEN)
            })
            .take(max_rows)
            .collect()
    } else {
        let mut collected: Vec<NodeId> = Vec::new();
        let mut stack = vec![scope];
        while let Some(nid) = stack.pop() {
            for cid in tree.children_iter(nid) {
                let ckind = tree.kind_of(cid);
                let alloc = tree.allocated_size[cid as usize];
                if ckind == NodeKind::Dir {
                    stack.push(cid);
                    if want_dirs && alloc > 0 {
                        collected.push(cid);
                    }
                } else if !want_dirs
                    && matches!(ckind, NodeKind::File | NodeKind::CloudPlaceholder)
                    && alloc > 0
                {
                    collected.push(cid);
                }
            }
        }
        collected.sort_by(|&a, &b| {
            tree.allocated_size[b as usize].cmp(&tree.allocated_size[a as usize])
        });
        collected.truncate(max_rows);
        collected
    };

    let prot_engine = ProtectionEngine::load_default();
    Ok(matches
        .into_iter()
        .map(|nid| {
            let idx = nid as usize;
            let full_pb = tree.reconstruct_path(nid);
            let flags = tree.flags_of(nid);
            let is_prot = flags.contains(NodeFlags::PROTECTED)
                || flags.contains(NodeFlags::PROTECTED_DESCENDANT);
            let reason = if is_prot {
                prot_engine.is_protected_for_ui(&full_pb).reason
            } else {
                None
            };
            TopEntryDto {
                node_id: nid,
                name: tree.node_name(nid).to_string(),
                full_path: full_pb.to_string_lossy().into_owned(),
                allocated_bytes: tree.allocated_size[idx],
                logical_bytes: tree.logical_size[idx],
                file_count: tree.file_count[idx],
                last_write_ts: tree.last_write_ts_of(nid),
                type_cat: format!("{:?}", tree.type_cat_of(nid)).to_ascii_lowercase(),
                origin_cat: format!("{:?}", tree.origin_cat_of(nid)).to_ascii_lowercase(),
                is_protected: is_prot,
                is_cloud_only: flags.contains(NodeFlags::CLOUD_ONLY),
                protection_reason: reason,
            }
        })
        .collect())
}

#[tauri::command]
pub fn cmd_search_volume(
    query: String,
    limit: Option<usize>,
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchHitDto>, String> {
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    let tree = &guard.tree;
    let hits = tree.search_by_name(&query, limit.unwrap_or(100).clamp(1, 300));
    Ok(hits
        .into_iter()
        .map(|nid| SearchHitDto {
            node_id: nid,
            name: tree.node_name(nid).to_string(),
            full_path: tree.reconstruct_path(nid).to_string_lossy().into_owned(),
            kind: format!("{:?}", tree.kind_of(nid)).to_ascii_lowercase(),
            allocated_bytes: tree.allocated_size[nid as usize],
            is_protected: tree.flags_of(nid).contains(NodeFlags::PROTECTED),
        })
        .collect())
}

#[tauri::command]
pub fn cmd_get_category_breakdown(
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<CategoryBreakdown, String> {
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    Ok(guard
        .tree
        .derived
        .to_category_breakdown(guard.tree.stats.total_allocated_bytes))
}

#[tauri::command]
pub fn cmd_reload_rules(
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<CategoryBreakdown, String> {
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let mut guard = slot.write().map_err(|e| e.to_string())?;
    let rules = RulesConfig::load();
    let breakdown = classify_tree(&mut guard.tree, &rules);
    if let Ok(mut bdg) = state.active_breakdown.lock() {
        *bdg = Some(breakdown.clone());
    }
    Ok(breakdown)
}

#[tauri::command]
pub fn cmd_get_access_denied(
    limit: Option<usize>,
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let max_items = limit.unwrap_or(200);
    let Some(slot) = state.get_volume_slot(drive_root.as_deref()) else {
        return Ok(Vec::new());
    };
    let guard = slot.read().map_err(|e| e.to_string())?;
    Ok(guard
        .tree
        .access_denied
        .iter()
        .take(max_items)
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub fn cmd_open_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("La ruta ya no existe en disco.".to_string());
    }
    Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map_err(|e| format!("No se pudo abrir el Explorador: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_list_installed_apps(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledApp>, String> {
    let tree_opt = state
        .get_volume_slot(Some(r"C:\"))
        .and_then(|s| s.read().ok().map(|g| g.tree.clone()));
    tauri::async_runtime::spawn_blocking(move || list_installed_apps(tree_opt.as_ref()))
        .await
        .map_err(|e| format!("Error listando aplicaciones: {e}"))
}

#[tauri::command]
pub fn cmd_uninstall_app(
    source: AppSource,
    uninstall_string: Option<String>,
    app_id: Option<String>,
    app_name: Option<String>,
    install_location: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let msg = trigger_native_uninstall(source, uninstall_string.as_deref())?;

    // Spawn Registry/PackageManager uninstall completion watcher (handles %TEMP% relaunchers!)
    if let Some(id) = app_id {
        let name = app_name.unwrap_or_else(|| id.clone());
        let timeout_mins = state
            .settings
            .read()
            .map(|s| s.uninstall_watch_timeout_mins)
            .unwrap_or(10);
        let queue_arc = state.reconcile_queue.clone();
        let leftovers_arc = state.uninstall_leftovers.clone();
        let app_handle = app.clone();

        std::thread::Builder::new()
            .name(format!("uninstall-watch-{id}"))
            .spawn(move || {
                let mut known_appdata = Vec::new();
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    let cand = PathBuf::from(local).join(&name);
                    if cand.exists() {
                        known_appdata.push(cand);
                    }
                }
                if let Ok(roam) = std::env::var("APPDATA") {
                    let cand = PathBuf::from(roam).join(&name);
                    if cand.exists() {
                        known_appdata.push(cand);
                    }
                }
                let probe = WinUninstallPresenceProbe;
                let prot = ProtectionEngine::load_default();
                let res = watch_native_uninstall_completion(
                    &id,
                    &name,
                    install_location.as_deref(),
                    &known_appdata,
                    Duration::from_secs(timeout_mins.saturating_mul(60)),
                    Duration::from_secs(2),
                    &probe,
                    &prot,
                );
                if let Ok(mut q) = queue_arc.lock() {
                    for req in res.reconcile_requests {
                        q.enqueue(req, Instant::now());
                    }
                }
                if !res.leftovers.is_empty() {
                    if let Ok(mut lv) = leftovers_arc.lock() {
                        lv.extend(res.leftovers);
                    }
                }
                let _ = app_handle.emit("apps-updated", &id);
            })
            .ok();
    }

    Ok(msg)
}

#[tauri::command]
pub fn cmd_get_uninstall_leftovers(state: State<'_, AppState>) -> Vec<UninstallLeftoverSuggestion> {
    state
        .uninstall_leftovers
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn cmd_open_windows_apps_settings() -> Result<(), String> {
    open_windows_apps_settings()
}

#[tauri::command]
pub async fn cmd_discover_cleanup_candidates(
    min_large_mb: Option<u64>,
    min_age_days: Option<u64>,
    state: State<'_, AppState>,
) -> Result<CleanupCandidatesReport, String> {
    let mb = min_large_mb.unwrap_or(100);
    let days = min_age_days.unwrap_or(30);
    let slots = state.all_available_volume_slots();

    tauri::async_runtime::spawn_blocking(move || {
        let guards: Vec<_> = slots.iter().filter_map(|s| s.read().ok()).collect();
        let refs: Vec<&crate::scanner::model::ScanTree> = guards.iter().map(|g| &g.tree).collect();
        discover_cleanup_candidates_from_trees(&refs, mb, days)
    })
    .await
    .map_err(|e| format!("Error analizando candidatos de limpieza: {e}"))
}

#[tauri::command]
pub async fn cmd_preview_cleanup(
    request: PreviewCleanupRequest,
    state: State<'_, AppState>,
) -> Result<CleanupPlan, String> {
    let plan = tauri::async_runtime::spawn_blocking(move || build_cleanup_plan(request))
        .await
        .map_err(|e| format!("Error generando vista previa: {e}"))?;

    let mut guard = state.pending_plans.lock().map_err(|e| e.to_string())?;
    guard.insert(plan.plan_id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
pub async fn cmd_execute_cleanup(
    plan_id: String,
    confirmed_permanent: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CleanupResult, String> {
    let plan = {
        let mut guard = state.pending_plans.lock().map_err(|e| e.to_string())?;
        guard
            .remove(&plan_id)
            .ok_or_else(|| "El plan de limpieza expiró o ya fue ejecutado.".to_string())?
    };

    // 1. Optimistic UI tree update: hide targeted nodes immediately and subtract from parents!
    let mut hidden_nodes: Vec<(String, NodeId, PathBuf)> = Vec::new();
    for item in &plan.items {
        let pb = PathBuf::from(&item.path);
        if let Some(vol) = volume_root_of_path(&pb) {
            if let Some(slot) = state.get_volume_slot(Some(&vol)) {
                if let Ok(mut g) = slot.write() {
                    if let Some(nid) = g.tree.find_node_by_path(&pb) {
                        if g.tree.optimistic_hide(nid) {
                            hidden_nodes.push((vol, nid, pb));
                        }
                    }
                }
            }
        }
    }
    let _ = app.emit("sync-updated", "optimistic-delete");

    let defer_drain = plan.mode == CleanupMode::Permanent;
    let plan_clone = plan.clone();
    let (result, staged_paths) = tauri::async_runtime::spawn_blocking(move || {
        execute_cleanup_plan_with_options(
            &plan_clone,
            confirmed_permanent,
            None,
            None,
            None,
            defer_drain,
        )
    })
    .await
    .map_err(|e| format!("Error ejecutando limpieza: {e}"))??;

    // 2. Revert optimistic hide for any item that failed or was blocked!
    let failed_paths: std::collections::HashSet<String> = result
        .outcomes
        .iter()
        .filter(|o| o.status == "failed" || o.status == "blocked")
        .map(|o| o.path.to_ascii_lowercase())
        .collect();

    for (vol, nid, pb) in hidden_nodes {
        let p_low = pb.to_string_lossy().to_ascii_lowercase();
        if let Some(slot) = state.get_volume_slot(Some(&vol)) {
            if let Ok(mut g) = slot.write() {
                if failed_paths.contains(&p_low) {
                    g.tree.optimistic_revert(nid);
                } else {
                    g.tree.remove_subtree(nid);
                }
            }
        }
    }

    // 3. Enqueue High-priority reconciliation for affected parents + $Recycle.Bin
    let affected_pbs: Vec<PathBuf> = plan.items.iter().map(|i| PathBuf::from(&i.path)).collect();
    let reqs = if plan.mode == CleanupMode::EmptyRecycleBin {
        list_drives(false)
            .into_iter()
            .filter(|d| d.available)
            .map(|d| ReconcileRequest {
                volume: d.root_path.clone(),
                paths: vec![PathBuf::from(format!(r"{}$Recycle.Bin", d.root_path))],
                origin: ReconcileOrigin::EmptyRecycleBin,
                priority: ReconcilePriority::High,
            })
            .collect()
    } else {
        build_post_delete_reconcile_requests(&affected_pbs, plan.mode == CleanupMode::RecycleBin)
    };
    if let Ok(mut q) = state.reconcile_queue.lock() {
        for r in reqs {
            q.enqueue(r, Instant::now());
        }
    }

    // 4. Drain staged `.scancleaner-trash` folders in background with progress + cancel support
    if !staged_paths.is_empty() {
        let jobs = state.deletion_jobs.clone();
        let job_id = plan.plan_id.clone();
        let cancel = jobs.register_job(
            job_id.clone(),
            "permanent".to_string(),
            staged_paths.len(),
            result.estimated_bytes,
        );
        let app_bg = app.clone();
        let queue_arc = state.reconcile_queue.clone();

        std::thread::Builder::new()
            .name(format!("bg-trash-drain-{job_id}"))
            .spawn(move || {
                jobs.update_job(&job_id, |s| s.phase = DeletionJobPhase::DeletingBackground);
                let _ = app_bg.emit("deletion-job-updated", jobs.get_job(&job_id));
                let mut completed = 0usize;
                let mut failed = 0usize;
                let mut touched_vols = Vec::new();

                for sp in staged_paths {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Some(vol) = volume_root_of_path(&sp) {
                        touched_vols.push(vol);
                    }
                    match delete_tree_parallel_fast(&sp, Some(&cancel), None, false) {
                        Ok(_) => completed += 1,
                        Err(e) => {
                            failed += 1;
                            jobs.update_job(&job_id, |s| s.errors.push(e));
                        }
                    }
                    jobs.update_job(&job_id, |s| {
                        s.completed_items = completed;
                        s.failed_items = failed;
                    });
                    let _ = app_bg.emit("deletion-job-updated", jobs.get_job(&job_id));
                }

                let final_phase = if cancel.load(Ordering::Relaxed) {
                    DeletionJobPhase::Cancelled
                } else if failed > 0 && completed == 0 {
                    DeletionJobPhase::Failed
                } else {
                    DeletionJobPhase::Completed
                };
                jobs.update_job(&job_id, |s| s.phase = final_phase);
                let _ = app_bg.emit("deletion-job-updated", jobs.get_job(&job_id));

                // Reconcile `.scancleaner-trash` on affected volumes so pending-trash clears automatically
                if let Ok(mut q) = queue_arc.lock() {
                    for vol in touched_vols {
                        q.enqueue(
                            ReconcileRequest {
                                volume: vol.clone(),
                                paths: vec![PathBuf::from(format!(r"{vol}.scancleaner-trash"))],
                                origin: ReconcileOrigin::UserDeletion,
                                priority: ReconcilePriority::High,
                            },
                            Instant::now(),
                        );
                    }
                }
            })
            .ok();
    }

    Ok(result)
}

#[tauri::command]
pub fn cmd_deletion_jobs_status(state: State<'_, AppState>) -> Vec<DeletionJobStatus> {
    state.deletion_jobs.list_jobs()
}

#[tauri::command]
pub fn cmd_cancel_deletion_job(job_id: String, state: State<'_, AppState>) -> bool {
    state.deletion_jobs.cancel_job(&job_id)
}

#[tauri::command]
pub fn cmd_relaunch_as_admin(
    active_drive: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    relaunch_as_admin(active_drive)?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn cmd_list_snapshots(
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ScanSnapshot>, String> {
    state.storage.list_snapshots(drive_root.as_deref())
}

#[tauri::command]
pub fn cmd_diff_snapshots(
    older_id: i64,
    newer_id: i64,
    state: State<'_, AppState>,
) -> Result<SnapshotDiff, String> {
    state.storage.diff_snapshots(older_id, newer_id)
}

#[tauri::command]
pub fn cmd_list_cleanup_logs(limit: Option<usize>) -> Vec<CleanupLogEntry> {
    read_cleanup_logs(&logs_dir(), limit.unwrap_or(200))
}

#[tauri::command]
pub fn cmd_dev_get_config() -> DevCleanerConfig {
    DevCleanerConfig::load()
}

#[tauri::command]
pub fn cmd_dev_save_config(config: DevCleanerConfig) -> Result<DevCleanerConfig, String> {
    config.save()?;
    Ok(config)
}

#[tauri::command]
pub async fn cmd_dev_pick_project_root() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(pick_folder_native)
        .await
        .map_err(|e| format!("Error abriendo selector de carpetas: {e}"))?
}

#[tauri::command]
pub async fn cmd_dev_scan(
    config: Option<DevCleanerConfig>,
    state: State<'_, AppState>,
) -> Result<DevScanReport, String> {
    let cfg = config.unwrap_or_else(DevCleanerConfig::load);
    let slots = state.all_available_volume_slots();
    tauri::async_runtime::spawn_blocking(move || {
        let guards: Vec<_> = slots.iter().filter_map(|s| s.read().ok()).collect();
        let refs: Vec<&crate::scanner::model::ScanTree> = guards.iter().map(|g| &g.tree).collect();
        scan_dev_cleanup_from_trees(&cfg, &refs)
    })
    .await
    .map_err(|e| format!("Error ejecutando escaneo Dev: {e}"))?
}

#[tauri::command]
pub async fn cmd_dev_preview(
    request: DevPreviewRequest,
    state: State<'_, AppState>,
) -> Result<DevCleanupPlan, String> {
    let plan = tauri::async_runtime::spawn_blocking(move || -> Result<DevCleanupPlan, String> {
        let catalog = DevRulesCatalog::load_embedded()?;
        let runner = SystemCommandRunner;
        Ok(build_dev_cleanup_plan(request, &catalog, &runner))
    })
    .await
    .map_err(|e| format!("Error generando dry-run Dev: {e}"))??;

    let mut guard = state.pending_dev_plans.lock().map_err(|e| e.to_string())?;
    guard.insert(plan.plan_id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
pub async fn cmd_dev_execute(
    plan_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DevCleanupResult, String> {
    let plan = {
        let mut guard = state.pending_dev_plans.lock().map_err(|e| e.to_string())?;
        guard
            .remove(&plan_id)
            .ok_or_else(|| "El plan de Limpieza Dev expiró o ya fue ejecutado.".to_string())?
    };

    // Optimistically hide targeted filesystem paths immediately
    let mut hidden: Vec<(String, NodeId, String)> = Vec::new();
    for item in &plan.items {
        let pb = Path::new(&item.path);
        if let Some(vol) = volume_root_of_path(pb) {
            if let Some(slot) = state.get_volume_slot(Some(&vol)) {
                if let Ok(mut g) = slot.write() {
                    if let Some(nid) = g.tree.find_node_by_path(pb) {
                        if g.tree.optimistic_hide(nid) {
                            hidden.push((vol, nid, item.path.to_ascii_lowercase()));
                        }
                    }
                }
            }
        }
    }
    let _ = app.emit("sync-updated", "optimistic-dev-delete");

    let result =
        tauri::async_runtime::spawn_blocking(move || -> Result<DevCleanupResult, String> {
            let catalog = DevRulesCatalog::load_embedded()?;
            let runner = SystemCommandRunner;
            Ok(execute_dev_cleanup_plan(
                &plan, &catalog, &runner, None, None,
            ))
        })
        .await
        .map_err(|e| format!("Error ejecutando Limpieza Dev: {e}"))??;

    // Revert failed/skipped items
    let failed: std::collections::HashSet<String> = result
        .outcomes
        .iter()
        .filter(|o| o.status == "failed" || o.status == "skipped" || o.status == "blocked")
        .map(|o| o.path.to_ascii_lowercase())
        .collect();
    for (vol, nid, path_low) in hidden {
        if let Some(slot) = state.get_volume_slot(Some(&vol)) {
            if let Ok(mut g) = slot.write() {
                if failed.contains(&path_low) {
                    g.tree.optimistic_revert(nid);
                } else {
                    g.tree.remove_subtree(nid);
                }
            }
        }
    }

    // Enqueue High-priority reconciliation for all `rutasAfectadas` & cleaned paths!
    let mut by_vol: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for p_str in &result.affected_paths_for_reconcile {
        let pb = PathBuf::from(p_str);
        if let Some(vol) = volume_root_of_path(&pb) {
            by_vol.entry(vol).or_default().push(pb);
        }
    }
    if let Ok(mut q) = state.reconcile_queue.lock() {
        for (volume, paths) in by_vol {
            q.enqueue(
                ReconcileRequest {
                    volume,
                    paths,
                    origin: ReconcileOrigin::OfficialCommand,
                    priority: ReconcilePriority::High,
                },
                Instant::now(),
            );
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn cmd_protection_list() -> Vec<ProtectedEntry> {
    ProtectionEngine::load_default().all_entries()
}

#[tauri::command]
pub fn cmd_protection_add(
    patron: String,
    tipo: ProtectionType,
    motivo: String,
    todas_las_unidades: bool,
) -> Result<Vec<ProtectedEntry>, String> {
    let trimmed = patron.trim();
    if trimmed.is_empty() {
        return Err("El patrón o ruta no puede estar vacío.".to_string());
    }
    let mut engine = ProtectionEngine::load_default();
    let entry = ProtectedEntry {
        id: format!("user-{}", uuid::Uuid::new_v4()),
        patron: trimmed.to_string(),
        tipo,
        origen: ProtectionOrigin::Usuario,
        motivo: if motivo.trim().is_empty() {
            "Protegida manualmente por el usuario.".to_string()
        } else {
            motivo.trim().to_string()
        },
        todas_las_unidades,
    };
    engine.user_entries.push(entry);
    engine.save_user_entries(&user_protection_config_path())?;
    Ok(engine.all_entries())
}

#[tauri::command]
pub fn cmd_protection_remove(id: String) -> Result<Vec<ProtectedEntry>, String> {
    let mut engine = ProtectionEngine::load_default();
    if engine.critical_entries.iter().any(|e| e.id == id) {
        return Err("Las entradas críticas de fábrica no se pueden eliminar.".to_string());
    }
    let before = engine.user_entries.len();
    engine.user_entries.retain(|e| e.id != id);
    if engine.user_entries.len() == before {
        return Err("Entrada de usuario no encontrada.".to_string());
    }
    engine.save_user_entries(&user_protection_config_path())?;
    Ok(engine.all_entries())
}

#[tauri::command]
pub fn cmd_protection_check(path: String) -> ProtectionResult {
    is_protected_default(Path::new(&path))
}

#[tauri::command]
pub fn cmd_protection_preview_matches(
    patron: String,
    tipo: ProtectionType,
    todas_las_unidades: bool,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let trimmed = patron.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let Some(slot) = state.get_volume_slot(None) else {
        return Ok(0);
    };
    let guard = slot.read().map_err(|e| e.to_string())?;
    let candidate = ProtectedEntry {
        id: "preview".to_string(),
        patron: trimmed.to_string(),
        tipo,
        origen: ProtectionOrigin::Usuario,
        motivo: String::new(),
        todas_las_unidades,
    };
    Ok(ProtectionEngine::preview_matches_in_tree(
        &guard.tree,
        &candidate,
    ))
}

#[tauri::command]
pub async fn cmd_pick_file() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(pick_file_native)
        .await
        .map_err(|e| format!("Error abriendo selector de archivos: {e}"))?
}

#[tauri::command]
pub fn cmd_system_files_report(drive_root: Option<String>) -> SystemFilesReport {
    inspect_system_files(drive_root.as_deref())
}

#[tauri::command]
pub fn cmd_open_virtual_memory(state: State<'_, AppState>) -> Result<String, String> {
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let pf = PathBuf::from(format!(r"{sys_drive}\pagefile.sys"));
    let before = locked_file_size(&pf).unwrap_or(0);
    if let Ok(mut w) = state.pending_system_file_watch.lock() {
        *w = Some((pf.to_string_lossy().into_owned(), before));
    }

    let sys_root = std::env::var("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows"));
    open_virtual_memory_dialog(&sys_root, &WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_open_power_options() -> Result<(), String> {
    open_power_options(&WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_run_powercfg_elevated(
    action: PowercfgAction,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let hf = PathBuf::from(format!(r"{sys_drive}\hiberfil.sys"));
    let before = locked_file_size(&hf).unwrap_or(0);
    if let Ok(mut w) = state.pending_system_file_watch.lock() {
        *w = Some((hf.to_string_lossy().into_owned(), before));
    }
    run_powercfg_elevated(action, &WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_get_app_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.read().map(|g| g.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn cmd_save_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<AppSettings, String> {
    let prev_tray = state
        .settings
        .read()
        .map(|g| g.start_with_windows_tray)
        .unwrap_or(false);
    if settings.start_with_windows_tray != prev_tray {
        set_start_with_windows(settings.start_with_windows_tray)?;
    }
    settings.save()?;
    if let Ok(mut g) = state.settings.write() {
        *g = settings.clone();
    }
    Ok(settings)
}

#[tauri::command]
pub fn cmd_service_status() -> IndexingServiceStatus {
    query_service_status()
}

#[tauri::command]
pub fn cmd_service_install() -> Result<IndexingServiceStatus, String> {
    request_service_action_uac(true)?;
    std::thread::sleep(Duration::from_millis(700));
    Ok(query_service_status())
}

#[tauri::command]
pub fn cmd_service_uninstall() -> Result<IndexingServiceStatus, String> {
    request_service_action_uac(false)?;
    std::thread::sleep(Duration::from_millis(700));
    Ok(query_service_status())
}

#[tauri::command]
pub fn cmd_sync_status(state: State<'_, AppState>) -> SyncStatusDto {
    let mut dto = state
        .sync_status
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    if let Ok(q) = state.reconcile_queue.lock() {
        dto.pending_batches = q.pending_count();
    }
    dto
}

#[tauri::command]
pub fn cmd_reconcile_paths(
    volume: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pbs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    if let Ok(mut q) = state.reconcile_queue.lock() {
        q.enqueue(
            ReconcileRequest {
                volume,
                paths: pbs,
                origin: ReconcileOrigin::ManualBackup,
                priority: ReconcilePriority::High,
            },
            Instant::now(),
        );
    }
    Ok(())
}

/// Triggered when the window regains focus:
/// 1. Reconciles pending `hiberfil.sys`/`pagefile.sys` changes (notifying if reboot needed).
/// 2. Immediately flushes pending USN/reconcile deltas for instant freshness.
#[tauri::command]
pub fn cmd_trigger_focus_sync(state: State<'_, AppState>) -> FocusSyncResult {
    let mut notice = None;
    let pending_sys = state
        .pending_system_file_watch
        .lock()
        .ok()
        .and_then(|mut g| g.take());

    if let Some((path_str, before_bytes)) = pending_sys {
        let pb = PathBuf::from(&path_str);
        let after_bytes = locked_file_size(&pb).unwrap_or(0);
        if path_str.to_ascii_lowercase().ends_with("pagefile.sys") && after_bytes == before_bytes {
            notice = Some(
                "El tamaño de pagefile.sys no cambió todavía: los ajustes de memoria virtual suelen requerir reiniciar Windows para aplicarse."
                    .to_string(),
            );
        }
        if let Some(vol) = volume_root_of_path(&pb) {
            if let Ok(mut q) = state.reconcile_queue.lock() {
                q.enqueue(
                    ReconcileRequest {
                        volume: vol,
                        paths: vec![pb],
                        origin: ReconcileOrigin::SystemFileAdjust,
                        priority: ReconcilePriority::High,
                    },
                    Instant::now(),
                );
            }
        }
    }

    // Also enqueue High-priority focus check for all loaded volumes
    let vols: Vec<String> = state
        .volumes
        .read()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let mut flushed = 0usize;
    if let Ok(mut q) = state.reconcile_queue.lock() {
        for v in vols {
            if q.enqueue(
                ReconcileRequest {
                    volume: v.clone(),
                    paths: vec![PathBuf::from(&v)],
                    origin: ReconcileOrigin::WindowFocus,
                    priority: ReconcilePriority::High,
                },
                Instant::now() - Duration::from_secs(1),
            ) {
                flushed += 1;
            }
        }
    }

    if let Ok(mut st) = state.sync_status.lock() {
        st.last_synced_ts = Some(Utc::now().timestamp());
    }

    FocusSyncResult {
        flushed_batches: flushed,
        system_file_notice: notice,
    }
}

#[tauri::command]
pub fn cmd_list_diagnostic_logs(limit: Option<usize>) -> Vec<String> {
    read_diagnostic_logs(limit.unwrap_or(300))
}

#[tauri::command]
pub fn cmd_open_logs_folder() -> Result<(), String> {
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("No se pudo abrir la carpeta de logs: {e}"))?;
    Ok(())
}

/// Checks if a given path still exists in the indexed volume; if it vanished, returns the nearest existing ancestor node ID.
#[tauri::command]
pub fn cmd_resolve_valid_node(
    drive_root: Option<String>,
    full_path: String,
    state: State<'_, AppState>,
) -> Result<(NodeId, bool, String), String> {
    let slot = state
        .get_volume_slot(drive_root.as_deref())
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let guard = slot.read().map_err(|e| e.to_string())?;
    let pb = Path::new(&full_path);
    if let Some(nid) = guard.tree.find_node_by_path(pb) {
        let f = guard.tree.flags_of(nid);
        if !f.contains(NodeFlags::TOMBSTONE) && !f.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
            return Ok((nid, true, full_path));
        }
    }
    let anc = guard.tree.find_nearest_existing_ancestor(pb);
    let anc_path = guard
        .tree
        .reconstruct_path(anc)
        .to_string_lossy()
        .into_owned();
    Ok((anc, false, anc_path))
}

// Keep unused import check clean
#[allow(dead_code)]
fn _keep_warn() {
    log_warn("");
}
