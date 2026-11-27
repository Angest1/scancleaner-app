pub mod apps;
pub mod categorizer;
pub mod cleaner;
pub mod commands;
pub mod deletion;
pub mod dev_cleaner;
pub mod diagnostics;
pub mod elevation;
pub mod index;
pub mod protection;
pub mod reconcile;
pub mod scanner;
pub mod service;
pub mod settings;
pub mod state;
pub mod storage;
pub mod system_files;

#[cfg(all(test, target_os = "windows", target_env = "gnu"))]
#[link(name = "testmanifest", kind = "static", modifiers = "+whole-archive")]
extern "C" {}

use chrono::Utc;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{Emitter, Manager, RunEvent, WebviewWindowBuilder, WindowEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    REG_VALUE_TYPE,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use elevation::is_elevated;
use index::default_index_dir;
use index::snapshot::{default_snapshot_path, load_volume_snapshot, save_volume_snapshot};
use index::usn::{
    sync_tree_with_usn, RealFileMetaLookup, UsnJournalReader, UsnSyncOutcome, WinUsnJournalReader,
};
use reconcile::queue::{BackgroundIoGuard, ReconcileOrigin, ReconcilePriority, ReconcileRequest};
use reconcile::triggers::{evaluate_free_space_drift, WinDiskSpaceProvider};
use scanner::derived::{normalize_volume_key, DerivedBuildContext};
use scanner::drives::{list_drives, DriveKind};
use scanner::fallback_walk::{reconcile_subtree_fallback, reconcile_watcher_path};
use scanner::model::FileSystemType;
use scanner::{scan_directory_with_options, ScanOptions};
use service::client::{ping_service, ServiceUsnJournalReader};
use state::AppState;
use storage::{default_db_path, Storage};

const WINDOW_TITLE: &str = "ScanCleaner — Analizador y Limpiador de Discos";

struct StartupArgs {
    drive: Option<String>,
    relaunch: bool,
    minimized_tray: bool,
    run_service: bool,
    install_service: bool,
    uninstall_service: bool,
    allowed_sid: Option<String>,
}

fn parse_startup_args() -> StartupArgs {
    let mut drive = None;
    let mut relaunch = false;
    let mut minimized_tray = false;
    let mut run_service = false;
    let mut install_service = false;
    let mut uninstall_service = false;
    let mut allowed_sid = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--relaunch" => relaunch = true,
            "--minimized-tray" => minimized_tray = true,
            "--service" => run_service = true,
            "--install-service" => install_service = true,
            "--uninstall-service" => uninstall_service = true,
            "--allowed-sid" => {
                allowed_sid = args.next();
            }
            "--drive" => {
                if let Some(val) = args.next() {
                    let normalized = if val.len() == 2 && val.ends_with(':') {
                        format!("{val}\\")
                    } else {
                        val
                    };
                    drive = Some(normalized);
                }
            }
            _ => {}
        }
    }
    StartupArgs {
        drive,
        relaunch,
        minimized_tray,
        run_service,
        install_service,
        uninstall_service,
        allowed_sid,
    }
}

/// Detects Windows system light/dark theme from registry so the hidden WebView2 window
/// initializes with the exact theme canvas color (`#f4f3fb` light / `#0a0c13` dark) — zero white flash.
fn initial_window_bg_color() -> Color {
    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\0"
        .encode_utf16()
        .collect();
    let val_name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut hkey = HKEY::default();
    let mut is_light = true;

    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
    }
    .is_ok()
    {
        let mut reg_type = REG_VALUE_TYPE(0);
        let mut val = 1u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        if unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(val_name.as_ptr()),
                None,
                Some(&mut reg_type),
                Some((&mut val as *mut u32).cast::<u8>()),
                Some(&mut len),
            )
        }
        .is_ok()
            && reg_type == REG_DWORD
        {
            is_light = val != 0;
        }
        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }

    if is_light {
        Color(0xf4, 0xf3, 0xfb, 0xff)
    } else {
        Color(0x0a, 0x0c, 0x13, 0xff)
    }
}

struct InstanceMutex(HANDLE);

impl Drop for InstanceMutex {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn acquire_single_instance_or_focus(relaunch: bool) -> Option<InstanceMutex> {
    let mutex_name: Vec<u16> = "Local\\ScanCleaner_SingleInstance_v1\0"
        .encode_utf16()
        .collect();
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) }.ok()?;
    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

    if already_exists && !relaunch {
        let title_w: Vec<u16> = WINDOW_TITLE
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title_w.as_ptr())) } {
            if !hwnd.0.is_null() {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                    let _ = CloseHandle(handle);
                }
                return None;
            }
        }
    }

    Some(InstanceMutex(handle))
}

fn webview_data_dir(fallback_per_pid: bool) -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ScanCleaner");

    let folder_name = if fallback_per_pid {
        format!("webview2-{}", std::process::id())
    } else if is_elevated() {
        "webview2-elevated".to_string()
    } else {
        "webview2".to_string()
    };

    let dir = base.join(folder_name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn build_main_window(app: &tauri::App, data_dir: PathBuf) -> tauri::Result<()> {
    // Created HIDDEN (`visible(false)`) with exact theme background color to prevent any white flash!
    // Shown via `cmd_show_main_window` once the first screen paints (or stays hidden if `--minimized-tray`).
    WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title(WINDOW_TITLE)
        .inner_size(1240.0, 820.0)
        .min_inner_size(980.0, 660.0)
        .center()
        .visible(false)
        .background_color(initial_window_bg_color())
        .data_directory(data_dir)
        .build()?;
    Ok(())
}

/// Spawns the central background reconciliation worker + 60s free-space drift & volume hotplug monitor.
/// Emits batched UI updates at most 4 Hz (`>= 250ms` between emits).
fn start_background_sync_workers(app_handle: tauri::AppHandle) {
    let handle_recon = app_handle.clone();
    std::thread::Builder::new()
        .name("reconcile-worker".to_string())
        .spawn(move || {
            let mut last_ui_emit = Instant::now() - Duration::from_secs(1);
            let mut last_snap_save: std::collections::HashMap<String, Instant> =
                std::collections::HashMap::new();
            let mut pending_emit = false;
            let meta_lookup = RealFileMetaLookup;

            loop {
                std::thread::sleep(Duration::from_millis(80));
                let state = handle_recon.state::<AppState>();
                let (batch_opt, remaining) = state
                    .reconcile_queue
                    .lock()
                    .ok()
                    .map(|mut q| {
                        let b = q.pop_ready(Instant::now());
                        let rem = q.pending_count();
                        (b, rem)
                    })
                    .unwrap_or((None, 0));

                if let Some(batch) = batch_opt {
                    let is_user_or_drift = batch.priority != ReconcilePriority::Low;
                    if is_user_or_drift {
                        if let Ok(mut st) = state.sync_status.lock() {
                            st.syncing = true;
                            st.pending_batches = remaining;
                            st.last_origin = batch
                                .origins
                                .first()
                                .map(|o| format!("{o:?}").to_ascii_lowercase());
                        }
                        let _ = handle_recon.emit(
                            "sync-status-changed",
                            state.sync_status.lock().map(|g| g.clone()).ok(),
                        );
                    }

                    // Enter low-priority I/O mode for Medium/Low background batches
                    let _io_guard = BackgroundIoGuard::enter_if_low_priority(batch.priority);
                    let ctx = DerivedBuildContext::load_default();
                    let use_service = !is_elevated() && ping_service();

                    let mut ok = true;
                    let mut needs_parallel_full_rescan = false;
                    let mut mutated = false;

                    if let Some(slot) = state.get_volume_slot(Some(&batch.volume)) {
                        let should_save = {
                            if let Ok(mut g) = slot.write() {
                                // Try USN first for NTFS WindowFocus / UsnJournal / Drift events
                                let usn_eligible = g.tree.fs_type == FileSystemType::Ntfs
                                    && (is_elevated() || use_service)
                                    && batch.origins.iter().all(|o| {
                                        matches!(
                                            o,
                                            ReconcileOrigin::UsnJournal
                                                | ReconcileOrigin::WindowFocus
                                                | ReconcileOrigin::FreeSpaceDrift
                                        )
                                    });

                                let mut run_subtrees = !usn_eligible;
                                if usn_eligible {
                                    let reader: Box<dyn UsnJournalReader> = if is_elevated() {
                                        Box::new(WinUsnJournalReader)
                                    } else {
                                        Box::new(ServiceUsnJournalReader)
                                    };
                                    match sync_tree_with_usn(
                                        &mut g.tree,
                                        reader.as_ref(),
                                        &meta_lookup,
                                        &ctx,
                                    ) {
                                        UsnSyncOutcome::UpToDate { applied_events, .. } => {
                                            g.stale = false;
                                            mutated = applied_events > 0;
                                        }
                                        UsnSyncOutcome::NeedsFullRescan { reason } => {
                                            diagnostics::log_warn(&format!(
                                                "USN rescan needed on {}: {reason}",
                                                batch.volume
                                            ));
                                            needs_parallel_full_rescan = true;
                                            run_subtrees = false;
                                        }
                                        UsnSyncOutcome::Unavailable { .. } => {
                                            run_subtrees = false;
                                        }
                                    }
                                }

                                if run_subtrees {
                                    let watcher_only = batch
                                        .origins
                                        .iter()
                                        .all(|o| matches!(o, ReconcileOrigin::DirWatcher));
                                    for p in &batch.paths {
                                        // NEVER run single-threaded subtree fallback on a volume root (`C:\`)!
                                        if scanner::is_volume_root(p) {
                                            if batch
                                                .origins
                                                .contains(&ReconcileOrigin::FreeSpaceDrift)
                                                || batch
                                                    .origins
                                                    .contains(&ReconcileOrigin::SystemFileAdjust)
                                            {
                                                needs_parallel_full_rescan = true;
                                            }
                                            continue;
                                        }
                                        let changed = if watcher_only {
                                            reconcile_watcher_path(&mut g.tree, p, &ctx)
                                        } else {
                                            reconcile_subtree_fallback(&mut g.tree, p, &ctx)
                                        };
                                        if changed {
                                            mutated = true;
                                        }
                                    }
                                }

                                // Persist snapshot immediately for High priority ops, or at most every 30s for Low background churn
                                mutated
                                    && !needs_parallel_full_rescan
                                    && (batch.priority == ReconcilePriority::High
                                        || last_snap_save
                                            .get(&batch.volume)
                                            .is_none_or(|t| t.elapsed() >= Duration::from_secs(30)))
                            } else {
                                ok = false;
                                false
                            }
                        };

                        // Save snapshot under READ lock (NOT write lock!) so UI queries never stall
                        if should_save {
                            if let Ok(g) = slot.read() {
                                let _ = save_volume_snapshot(&g.tree, &default_index_dir());
                                last_snap_save.insert(batch.volume.clone(), Instant::now());
                            }
                        }
                    }

                    // Run any required full-volume rescan in parallel OUTSIDE the volume write lock
                    if needs_parallel_full_rescan {
                        let include_ads = state
                            .settings
                            .read()
                            .map(|s| s.include_ads)
                            .unwrap_or(false);
                        let cancel = Arc::new(AtomicBool::new(false));
                        let new_tree = scan_directory_with_options(
                            PathBuf::from(&batch.volume),
                            ScanOptions {
                                include_ads,
                                prefer_mft: true,
                            },
                            cancel,
                            |_| {},
                        );
                        if !new_tree.stats.cancelled {
                            let _ = save_volume_snapshot(&new_tree, &default_index_dir());
                            last_snap_save.insert(batch.volume.clone(), Instant::now());
                            state.insert_volume(new_tree, false);
                            mutated = true;
                        }
                    }

                    if ok {
                        let rem_after = state
                            .reconcile_queue
                            .lock()
                            .map(|mut q| {
                                q.clear_retry(&batch.volume);
                                q.pending_count()
                            })
                            .unwrap_or(0);
                        if let Ok(mut st) = state.sync_status.lock() {
                            st.syncing = false;
                            st.pending_batches = rem_after;
                            st.last_synced_ts = Some(Utc::now().timestamp());
                            if mutated {
                                st.revision += 1;
                            }
                        }
                        // Always emit `syncing: false` immediately so the topbar pill never gets stuck!
                        if is_user_or_drift || mutated {
                            let dto = state.sync_status.lock().map(|g| g.clone()).ok();
                            let _ = handle_recon.emit("sync-status-changed", dto);
                        }
                        if mutated {
                            pending_emit = true;
                        }
                    } else {
                        let rem_after = state
                            .reconcile_queue
                            .lock()
                            .map(|mut q| {
                                q.schedule_retry(batch.clone(), Instant::now());
                                q.pending_count()
                            })
                            .unwrap_or(0);
                        if let Ok(mut st) = state.sync_status.lock() {
                            st.syncing = false;
                            st.pending_batches = rem_after;
                            st.observations.push(format!(
                                "Reconciliación diferida en {} (reintento automático).",
                                batch.volume
                            ));
                            if st.observations.len() > 8 {
                                st.observations.remove(0);
                            }
                        }
                        let dto = state.sync_status.lock().map(|g| g.clone()).ok();
                        let _ = handle_recon.emit("sync-status-changed", dto);
                    }
                }

                // Coalesce UI tree refresh emissions to <= 4 Hz (every >= 250ms)
                if pending_emit && last_ui_emit.elapsed() >= Duration::from_millis(250) {
                    last_ui_emit = Instant::now();
                    pending_emit = false;
                    let _ = handle_recon.emit("sync-updated", "reconciled");
                }
            }
        })
        .ok();

    // Periodic 60s Free-Space Drift & Volume Arrival/Removal monitor
    // TODO: verificar integración nativa de WM_DEVICECHANGE vía subclassing directo del HWND de WebView2;
    // el monitor de volúmenes detecta llegadas/desconexiones de forma segura sin riesgo de conflicto de WndProc con tao/WebView2.
    let handle_monitor = app_handle;
    std::thread::Builder::new()
        .name("drift-and-device-monitor".to_string())
        .spawn(move || {
            let provider = WinDiskSpaceProvider;
            let mut known_drives: HashSet<String> = list_drives(false)
                .into_iter()
                .filter(|d| d.available)
                .map(|d| normalize_volume_key(&d.root_path))
                .collect();
            let mut last_drift_check = Instant::now();

            loop {
                std::thread::sleep(Duration::from_secs(4));
                let state = handle_monitor.state::<AppState>();

                // 1. Check drive arrival / removal
                let current_drives = list_drives(false);
                let current_set: HashSet<String> = current_drives
                    .iter()
                    .filter(|d| {
                        d.available
                            && matches!(
                                d.kind,
                                DriveKind::Fixed | DriveKind::Removable | DriveKind::RamDisk
                            )
                    })
                    .map(|d| normalize_volume_key(&d.root_path))
                    .collect();

                let mut drives_changed = false;
                for removed in known_drives.difference(&current_set) {
                    state.mark_volume_unavailable(removed);
                    drives_changed = true;
                }
                for arrived in current_set.difference(&known_drives) {
                    drives_changed = true;
                    if let Some(dinfo) = current_drives
                        .iter()
                        .find(|d| normalize_volume_key(&d.root_path) == *arrived)
                    {
                        let snap_path =
                            default_snapshot_path(&dinfo.root_path, dinfo.volume_serial);
                        if let Ok((tree, _)) =
                            load_volume_snapshot(&snap_path, Some(dinfo.volume_serial))
                        {
                            state.insert_volume(tree, true);
                        } else {
                            let root = dinfo.root_path.clone();
                            let app_scan = handle_monitor.clone();
                            std::thread::spawn(move || {
                                let cancel = Arc::new(AtomicBool::new(false));
                                let tree = scan_directory_with_options(
                                    PathBuf::from(&root),
                                    ScanOptions::default(),
                                    cancel,
                                    |_| {},
                                );
                                if !tree.stats.cancelled {
                                    let _ = save_volume_snapshot(&tree, &default_index_dir());
                                    app_scan.state::<AppState>().insert_volume(tree, false);
                                    let _ = app_scan.emit("drives-changed", &root);
                                }
                            });
                        }
                    }
                }
                if drives_changed {
                    known_drives = current_set;
                    let _ = handle_monitor.emit("drives-changed", "device-change");
                }

                // 2. Every ~60s check free-space drift on every loaded volume
                if last_drift_check.elapsed() >= Duration::from_secs(60) {
                    last_drift_check = Instant::now();
                    let (pct, min_gb) = state
                        .settings
                        .read()
                        .map(|s| (s.drift_threshold_percent, s.drift_threshold_gb))
                        .unwrap_or((1.0, 1));
                    let slots = state.all_available_volume_slots();
                    for slot in slots {
                        if let Ok(g) = slot.read() {
                            let vol = normalize_volume_key(&g.tree.root_path.to_string_lossy());
                            // Compare against real volume used bytes recorded at scan time so Access Denied
                            // folders (without admin) don't cause permanent false-positive drift every 60s!
                            let baseline_used = g
                                .real_used_at_scan_bytes
                                .unwrap_or(g.tree.stats.total_allocated_bytes);
                            if let Some(drift) = evaluate_free_space_drift(
                                &vol,
                                baseline_used,
                                &provider,
                                pct,
                                min_gb,
                            ) {
                                if drift.drifted {
                                    if let Ok(mut q) = state.reconcile_queue.lock() {
                                        q.enqueue(
                                            ReconcileRequest {
                                                volume: vol.clone(),
                                                paths: vec![PathBuf::from(&vol)],
                                                origin: ReconcileOrigin::FreeSpaceDrift,
                                                priority: ReconcilePriority::Medium,
                                            },
                                            Instant::now(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
        .ok();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::install_panic_hook();
    let startup = parse_startup_args();

    // Multi-role binary dispatch for the optional read-only indexing service (Section 4)
    if startup.run_service {
        let sid = startup
            .allowed_sid
            .unwrap_or_else(|| "S-1-5-32-544".to_string());
        let _ = service::server::run_windows_service(sid);
        return;
    }
    if startup.install_service {
        if let Some(sid) = startup.allowed_sid {
            let _ = service::installer::install_service_elevated(&sid);
        }
        return;
    }
    if startup.uninstall_service {
        let _ = service::installer::uninstall_service_elevated();
        return;
    }

    diagnostics::log_info(&format!(
        "ScanCleaner starting (pid={}, elevated={}, minimized_tray={})",
        std::process::id(),
        is_elevated(),
        startup.minimized_tray
    ));

    let Some(_instance_guard) = acquire_single_instance_or_focus(startup.relaunch) else {
        return;
    };

    let storage = Storage::open(default_db_path()).expect("No se pudo inicializar SQLite");
    let app_state = AppState::new(storage, startup.drive);
    let start_minimized = startup.minimized_tray;

    let app = tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            let primary_dir = webview_data_dir(false);
            if build_main_window(app, primary_dir).is_err() {
                let fallback_dir = webview_data_dir(true);
                build_main_window(app, fallback_dir)?;
            }

            // Build system tray icon so background index stays fresh when minimized to tray
            if let Ok(show_item) =
                MenuItem::with_id(app, "tray_show", "Abrir ScanCleaner", true, None::<&str>)
            {
                if let Ok(quit_item) =
                    MenuItem::with_id(app, "tray_quit", "Salir", true, None::<&str>)
                {
                    if let Ok(menu) = Menu::with_items(app, &[&show_item, &quit_item]) {
                        let mut builder = TrayIconBuilder::new()
                            .tooltip("ScanCleaner — Índice de disco activo")
                            .menu(&menu)
                            .on_menu_event(|app_h, ev| match ev.id.as_ref() {
                                "tray_show" => {
                                    if let Some(w) = app_h.get_webview_window("main") {
                                        let _ = w.show();
                                        let _ = w.unminimize();
                                        let _ = w.set_focus();
                                    }
                                }
                                "tray_quit" => {
                                    app_h.exit(0);
                                }
                                _ => {}
                            })
                            .on_tray_icon_event(|tray, event| {
                                if let TrayIconEvent::Click {
                                    button: MouseButton::Left,
                                    button_state: MouseButtonState::Up,
                                    ..
                                } = event
                                {
                                    if let Some(w) = tray.app_handle().get_webview_window("main") {
                                        let _ = w.show();
                                        let _ = w.unminimize();
                                        let _ = w.set_focus();
                                    }
                                }
                            });
                        if let Some(icon) = app.default_window_icon() {
                            builder = builder.icon(icon.clone());
                        }
                        let _ = builder.build(app);
                    }
                }
            }

            let _ = start_minimized;
            start_background_sync_workers(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    WindowEvent::Focused(true) => {
                        let _ = window.emit("window-focused", true);
                    }
                    WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                        let state = window.state::<AppState>();
                        if let Ok(guard) = state.cancel_flag.lock() {
                            if let Some(flag) = guard.as_ref() {
                                flag.store(true, Ordering::Relaxed);
                            }
                        }
                        window.app_handle().exit(0);
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_startup_context,
            commands::cmd_show_main_window,
            commands::cmd_bootstrap_startup,
            commands::cmd_list_drives,
            commands::cmd_list_indexed_summaries,
            commands::cmd_set_active_drive,
            commands::cmd_start_scan,
            commands::cmd_cancel_scan,
            commands::cmd_get_treemap_layout,
            commands::cmd_get_treemap_children,
            commands::cmd_get_top_entries,
            commands::cmd_search_volume,
            commands::cmd_get_category_breakdown,
            commands::cmd_reload_rules,
            commands::cmd_get_access_denied,
            commands::cmd_open_in_explorer,
            commands::cmd_list_installed_apps,
            commands::cmd_uninstall_app,
            commands::cmd_get_uninstall_leftovers,
            commands::cmd_open_windows_apps_settings,
            commands::cmd_discover_cleanup_candidates,
            commands::cmd_preview_cleanup,
            commands::cmd_execute_cleanup,
            commands::cmd_deletion_jobs_status,
            commands::cmd_cancel_deletion_job,
            commands::cmd_relaunch_as_admin,
            commands::cmd_list_snapshots,
            commands::cmd_diff_snapshots,
            commands::cmd_list_cleanup_logs,
            commands::cmd_dev_get_config,
            commands::cmd_dev_save_config,
            commands::cmd_dev_pick_project_root,
            commands::cmd_dev_scan,
            commands::cmd_dev_preview,
            commands::cmd_dev_execute,
            commands::cmd_protection_list,
            commands::cmd_protection_add,
            commands::cmd_protection_remove,
            commands::cmd_protection_check,
            commands::cmd_protection_preview_matches,
            commands::cmd_pick_file,
            commands::cmd_system_files_report,
            commands::cmd_open_virtual_memory,
            commands::cmd_open_power_options,
            commands::cmd_run_powercfg_elevated,
            commands::cmd_get_app_settings,
            commands::cmd_save_app_settings,
            commands::cmd_service_status,
            commands::cmd_service_install,
            commands::cmd_service_uninstall,
            commands::cmd_sync_status,
            commands::cmd_reconcile_paths,
            commands::cmd_trigger_focus_sync,
            commands::cmd_resolve_valid_node,
            commands::cmd_list_diagnostic_logs,
            commands::cmd_open_logs_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ScanCleaner");

    app.run(|_app_handle, event| {
        if let RunEvent::Exit = event {
            std::process::exit(0);
        }
    });
}
