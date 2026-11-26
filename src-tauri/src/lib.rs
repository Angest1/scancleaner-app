pub mod apps;
pub mod categorizer;
pub mod cleaner;
pub mod commands;
pub mod dev_cleaner;
pub mod diagnostics;
pub mod elevation;
pub mod protection;
pub mod scanner;
pub mod state;
pub mod storage;
pub mod system_files;

#[cfg(all(test, target_os = "windows", target_env = "gnu"))]
#[link(name = "testmanifest", kind = "static", modifiers = "+whole-archive")]
extern "C" {}

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{Manager, RunEvent, WebviewWindowBuilder, WindowEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use elevation::is_elevated;
use state::AppState;
use storage::{default_db_path, Storage};

const WINDOW_TITLE: &str = "ScanCleaner — Analizador y Limpiador de Discos";

struct StartupArgs {
    drive: Option<String>,
    relaunch: bool,
}

fn parse_startup_args() -> StartupArgs {
    let mut drive = None;
    let mut relaunch = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--relaunch" {
            relaunch = true;
        } else if arg == "--drive" {
            if let Some(val) = args.next() {
                let normalized = if val.len() == 2 && val.ends_with(':') {
                    format!("{val}\\")
                } else {
                    val
                };
                drive = Some(normalized);
            }
        }
    }
    StartupArgs { drive, relaunch }
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
        // Previous instance had no visible window (likely shutting down); proceed cleanly
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
    WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title(WINDOW_TITLE)
        .inner_size(1240.0, 820.0)
        .min_inner_size(980.0, 660.0)
        .center()
        .visible(true)
        .focused(true)
        .data_directory(data_dir)
        .build()?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::install_panic_hook();
    diagnostics::log_info(&format!(
        "ScanCleaner starting (pid={}, elevated={})",
        std::process::id(),
        is_elevated()
    ));

    let startup = parse_startup_args();
    let Some(_instance_guard) = acquire_single_instance_or_focus(startup.relaunch) else {
        return;
    };

    let storage = Storage::open(default_db_path()).expect("No se pudo inicializar SQLite");
    let app_state = AppState::new(storage, startup.drive);

    let app = tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            let primary_dir = webview_data_dir(false);
            if build_main_window(app, primary_dir).is_err() {
                // Retry once with an isolated per-PID WebView2 profile in case the primary
                // profile lockfile is still being released by a closing instance.
                let fallback_dir = webview_data_dir(true);
                build_main_window(app, fallback_dir)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && matches!(
                    event,
                    WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
                )
            {
                let state = window.state::<AppState>();
                if let Ok(guard) = state.cancel_flag.lock() {
                    if let Some(flag) = guard.as_ref() {
                        flag.store(true, Ordering::Relaxed);
                    }
                }
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_startup_context,
            commands::cmd_list_drives,
            commands::cmd_start_scan,
            commands::cmd_cancel_scan,
            commands::cmd_get_treemap_children,
            commands::cmd_get_top_entries,
            commands::cmd_get_category_breakdown,
            commands::cmd_reload_rules,
            commands::cmd_get_access_denied,
            commands::cmd_open_in_explorer,
            commands::cmd_list_installed_apps,
            commands::cmd_uninstall_app,
            commands::cmd_open_windows_apps_settings,
            commands::cmd_discover_cleanup_candidates,
            commands::cmd_preview_cleanup,
            commands::cmd_execute_cleanup,
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
