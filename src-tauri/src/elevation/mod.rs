use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevationStatus {
    pub is_elevated: bool,
    pub username: String,
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast::<c_void>()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned_len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

pub fn current_elevation_status() -> ElevationStatus {
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "Usuario".to_string());
    ElevationStatus {
        is_elevated: is_elevated(),
        username,
    }
}

pub fn elevated_webview_data_dir() -> Option<PathBuf> {
    if !is_elevated() {
        return None;
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    let dir = PathBuf::from(local)
        .join("ScanCleaner")
        .join("webview2-elevated");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

pub fn relaunch_as_admin(active_drive: Option<String>) -> Result<(), String> {
    if is_elevated() {
        return Ok(());
    }
    let exe = std::env::current_exe()
        .map_err(|e| format!("No se pudo resolver el ejecutable actual: {e}"))?;
    let exe_str = exe.to_string_lossy().into_owned();
    let params = match active_drive.filter(|d| !d.trim().is_empty()) {
        Some(d) => format!("--relaunch --drive \"{}\"", d.trim_end_matches('\\')),
        None => "--relaunch".to_string(),
    };

    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();
    let file_w: Vec<u16> = exe_str.encode_utf16().chain(std::iter::once(0)).collect();
    let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NO_CONSOLE,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: if params.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(params_w.as_ptr())
        },
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe { ShellExecuteExW(&mut sei) }
        .map_err(|e| format!("Elevación UAC cancelada o rechazada: {e}"))?;

    Ok(())
}
