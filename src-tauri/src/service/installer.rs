use serde::Serialize;
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, CreateServiceW, DeleteService, OpenSCManagerW,
    OpenServiceW, QueryServiceStatus, StartServiceW, SC_MANAGER_ALL_ACCESS, SC_MANAGER_CONNECT,
    SERVICE_ALL_ACCESS, SERVICE_AUTO_START, SERVICE_CONTROL_STOP, SERVICE_ERROR_NORMAL,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS, SERVICE_WIN32_OWN_PROCESS,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::client::ping_service;
use super::protocol::{SERVICE_DISPLAY_NAME, SERVICE_NAME};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub pipe_reachable: bool,
    pub binary_path: String,
}

pub fn installed_service_binary_path() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("ScanCleaner").join("ScanCleanerIndexService.exe")
}

/// Returns the current user's Windows SID string (e.g. `S-1-5-21-...`).
pub fn current_user_sid_string() -> Result<String, String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| format!("OpenProcessToken falló: {e}"))?;

        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        if needed == 0 {
            let _ = CloseHandle(token);
            return Err("No se pudo obtener tamaño de TokenUser.".to_string());
        }
        let mut buf = vec![0u8; needed as usize];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        ok.map_err(|e| format!("GetTokenInformation(TokenUser) falló: {e}"))?;

        let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
        let mut str_sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut str_sid)
            .map_err(|e| format!("ConvertSidToStringSidW falló: {e}"))?;
        let sid_str = str_sid.to_string().map_err(|e| e.to_string())?;
        let _ = LocalFree(HLOCAL(str_sid.0.cast()));
        Ok(sid_str)
    }
}

pub fn query_service_status() -> IndexingServiceStatus {
    let bin = installed_service_binary_path()
        .to_string_lossy()
        .into_owned();
    let name_w: Vec<u16> = SERVICE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut installed = false;
    let mut running = false;

    if let Ok(scm) = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) } {
        if let Ok(svc) = unsafe { OpenServiceW(scm, PCWSTR(name_w.as_ptr()), SERVICE_QUERY_STATUS) }
        {
            installed = true;
            let mut st = SERVICE_STATUS::default();
            if unsafe { QueryServiceStatus(svc, &mut st) }.is_ok() {
                running = st.dwCurrentState == SERVICE_RUNNING;
            }
            unsafe {
                let _ = CloseServiceHandle(svc);
            }
        }
        unsafe {
            let _ = CloseServiceHandle(scm);
        }
    }

    let pipe_reachable = running && ping_service();
    IndexingServiceStatus {
        installed,
        running,
        pipe_reachable,
        binary_path: bin,
    }
}

/// Launches a single UAC prompt to install or uninstall the read-only indexing service.
pub fn request_service_action_uac(install: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let sid = current_user_sid_string()?;
    let params = if install {
        format!("--install-service --allowed-sid {sid}")
    } else {
        "--uninstall-service".to_string()
    };

    if crate::elevation::is_elevated() {
        return if install {
            install_service_elevated(&sid)
        } else {
            uninstall_service_elevated()
        };
    }

    let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();
    let file_w: Vec<u16> = exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NO_CONSOLE,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe { ShellExecuteExW(&mut sei) }
        .map_err(|e| format!("Elevación UAC cancelada o rechazada: {e}"))?;
    Ok(())
}

/// Copies the current executable to `%ProgramData%\ScanCleaner\ScanCleanerIndexService.exe`
/// (so `release-portable\ScanCleaner.exe` is never locked) and installs/starts the Windows Service.
pub fn install_service_elevated(allowed_sid: &str) -> Result<(), String> {
    let src_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dst_exe = installed_service_binary_path();
    if let Some(parent) = dst_exe.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Stop previous instance if already installed before replacing binary
    let _ = uninstall_service_elevated();
    fs::copy(&src_exe, &dst_exe)
        .map_err(|e| format!("No se pudo copiar binario de servicio a ProgramData: {e}"))?;

    let bin_cmd = format!(
        "\"{}\" --service --allowed-sid {}",
        dst_exe.display(),
        allowed_sid.trim()
    );
    let scm = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ALL_ACCESS) }
        .map_err(|e| format!("OpenSCManagerW falló: {e}"))?;

    let name_w: Vec<u16> = SERVICE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let disp_w: Vec<u16> = SERVICE_DISPLAY_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let bin_w: Vec<u16> = bin_cmd.encode_utf16().chain(std::iter::once(0)).collect();

    let svc = unsafe {
        CreateServiceW(
            scm,
            PCWSTR(name_w.as_ptr()),
            PCWSTR(disp_w.as_ptr()),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            PCWSTR(bin_w.as_ptr()),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    };

    let svc = match svc {
        Ok(s) => s,
        Err(e) => {
            unsafe {
                let _ = CloseServiceHandle(scm);
            }
            return Err(format!("CreateServiceW falló: {e}"));
        }
    };

    let start_res = unsafe { StartServiceW(svc, None) };
    unsafe {
        let _ = CloseServiceHandle(svc);
        let _ = CloseServiceHandle(scm);
    }
    start_res.map_err(|e| format!("StartServiceW falló: {e}"))
}

pub fn uninstall_service_elevated() -> Result<(), String> {
    let name_w: Vec<u16> = SERVICE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    if let Ok(scm) =
        unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ALL_ACCESS) }
    {
        if let Ok(svc) = unsafe { OpenServiceW(scm, PCWSTR(name_w.as_ptr()), SERVICE_ALL_ACCESS) } {
            let mut st = SERVICE_STATUS::default();
            let _ = unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut st) };
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = unsafe { DeleteService(svc) };
            unsafe {
                let _ = CloseServiceHandle(svc);
            }
        }
        unsafe {
            let _ = CloseServiceHandle(scm);
        }
    }

    let dst = installed_service_binary_path();
    if dst.exists() {
        let _ = fs::remove_file(&dst);
    }
    Ok(())
}

/// Sets or removes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\ScanCleaner`
/// so the app starts minimized to the system tray with Windows (disabled by default).
pub fn set_start_with_windows(enabled: bool) -> Result<(), String> {
    let subkey: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Run\0"
        .encode_utf16()
        .collect();
    let val_name: Vec<u16> = "ScanCleaner\0".encode_utf16().collect();

    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
    }
    .ok()
    .map_err(|e| format!("No se pudo abrir clave Run de HKCU: {e}"))?;

    let res = if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd = format!("\"{}\" --minimized-tray", exe.display());
        let data: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        unsafe { RegSetValueExW(hkey, PCWSTR(val_name.as_ptr()), 0, REG_SZ, Some(bytes)) }
            .ok()
            .map_err(|e| format!("RegSetValueExW falló: {e}"))
    } else {
        let _ = unsafe { RegDeleteValueW(hkey, PCWSTR(val_name.as_ptr())) };
        Ok(())
    };

    unsafe {
        let _ = RegCloseKey(hkey);
    }
    res
}
