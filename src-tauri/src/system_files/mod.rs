use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD,
    REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NO_CONSOLE, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::scanner::drives::list_drives;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFileEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub explanation: String,
    pub can_adjust: bool,
    pub adjust_kind: String, // "pagefile" | "hiberfil" | "none"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HibernationStatus {
    pub available: bool,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFilesReport {
    pub files: Vec<SystemFileEntry>,
    pub total_bytes: u64,
    pub hibernation: HibernationStatus,
    pub virtual_memory_instruction: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowercfgAction {
    Reduced,
    Off,
    On,
}

pub trait SystemProcessLauncher: Send + Sync {
    fn spawn(&self, program: &str, args: &[&str]) -> Result<(), String>;
    fn spawn_elevated(&self, program: &str, params: &str) -> Result<(), String>;
}

#[derive(Debug, Default, Clone)]
pub struct WinSystemProcessLauncher;

impl SystemProcessLauncher for WinSystemProcessLauncher {
    fn spawn(&self, program: &str, args: &[&str]) -> Result<(), String> {
        Command::new(program)
            .args(args)
            .spawn()
            .map_err(|e| format!("No se pudo abrir '{program}': {e}"))?;
        Ok(())
    }

    fn spawn_elevated(&self, program: &str, params: &str) -> Result<(), String> {
        let verb_w: Vec<u16> = "runas\0".encode_utf16().collect();
        let file_w: Vec<u16> = program.encode_utf16().chain(std::iter::once(0)).collect();
        let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NO_CONSOLE,
            lpVerb: PCWSTR(verb_w.as_ptr()),
            lpFile: PCWSTR(file_w.as_ptr()),
            lpParameters: PCWSTR(params_w.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };

        unsafe { ShellExecuteExW(&mut sei) }
            .map_err(|e| format!("Elevación UAC cancelada o rechazada: {e}"))?;
        Ok(())
    }
}

/// Reads file size using `FindFirstFileW`, which works even on locked Windows system files
/// like `pagefile.sys`, `swapfile.sys`, and `hiberfil.sys` where `std::fs::metadata` may fail.
pub fn locked_file_size(path: &Path) -> Option<u64> {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut data = WIN32_FIND_DATAW::default();
    let handle = unsafe { FindFirstFileW(PCWSTR(wide.as_ptr()), &mut data) }.ok()?;
    unsafe {
        let _ = FindClose(handle);
    }
    let size = ((data.nFileSizeHigh as u64) << 32) | (data.nFileSizeLow as u64);
    Some(size)
}

fn query_reg_dword(hkey: HKEY, name: &str) -> Option<u32> {
    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut reg_type = REG_VALUE_TYPE(0);
    let mut val = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    let res = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut reg_type),
            Some((&mut val as *mut u32).cast::<u8>()),
            Some(&mut len),
        )
    };
    if res.is_ok() && reg_type == REG_DWORD {
        Some(val)
    } else {
        None
    }
}

/// Reads hibernation status without admin elevation from
/// `HKLM\SYSTEM\CurrentControlSet\Control\Power`.
/// TODO: verificar en distintas ediciones de Windows 10/11 si `HiberFileType` siempre está presente
/// cuando se configura `powercfg /h /type reduced` (en estado por defecto Windows puede omitir `HiberFileType`).
pub fn read_hibernation_status(hiberfil_present: bool) -> HibernationStatus {
    let subkey: Vec<u16> = r"SYSTEM\CurrentControlSet\Control\Power\0"
        .encode_utf16()
        .collect();
    let mut hkey = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
    }
    .is_err()
    {
        return HibernationStatus {
            available: false,
            label: "Estado no disponible sin elevación".to_string(),
            detail: None,
        };
    }

    let enabled = query_reg_dword(hkey, "HibernateEnabled");
    let enabled_default = query_reg_dword(hkey, "HibernateEnabledDefault");
    let file_type = query_reg_dword(hkey, "HiberFileType");
    unsafe {
        let _ = RegCloseKey(hkey);
    }

    let effective_enabled = enabled.or(enabled_default);
    match effective_enabled {
        Some(0) => HibernationStatus {
            available: true,
            label: "Desactivada".to_string(),
            detail: Some("Hibernación e inicio rápido deshabilitados.".to_string()),
        },
        Some(_) => {
            let (label, detail) = match file_type {
                Some(1) => (
                    "Reducida (solo inicio rápido)".to_string(),
                    Some(
                        "HiberFileType=1: archivo reducido; hibernación completa desactivada."
                            .to_string(),
                    ),
                ),
                Some(2) => (
                    "Completa (hibernación + inicio rápido)".to_string(),
                    Some("HiberFileType=2: hibernación completa habilitada.".to_string()),
                ),
                _ if hiberfil_present => (
                    "Activada (configuración por defecto)".to_string(),
                    Some(
                        "hiberfil.sys presente; usa `powercfg /a` para ver modos soportados."
                            .to_string(),
                    ),
                ),
                _ => (
                    "Habilitada por defecto (archivo no presente)".to_string(),
                    None,
                ),
            };
            HibernationStatus {
                available: true,
                label,
                detail,
            }
        }
        None => HibernationStatus {
            available: false,
            label: "No determinable desde el Registro".to_string(),
            detail: None,
        },
    }
}

pub fn inspect_system_files(drive_filter: Option<&str>) -> SystemFilesReport {
    let mut roots: Vec<PathBuf> = if let Some(d) = drive_filter {
        let norm = if d.len() == 2 && d.ends_with(':') {
            format!(r"{d}\")
        } else {
            d.to_string()
        };
        vec![PathBuf::from(norm)]
    } else {
        list_drives(false)
            .into_iter()
            .filter(|d| d.available)
            .map(|d| PathBuf::from(d.root_path))
            .collect()
    };

    if roots.is_empty() {
        let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        roots.push(PathBuf::from(format!(r"{sys_drive}\")));
    }

    let specs = [
        (
            "pagefile.sys",
            "Lo administra Windows y crece según el uso de memoria y la configuración de volcados de memoria. No se recomienda desactivarlo.",
            true,
            "pagefile",
        ),
        (
            "hiberfil.sys",
            "Archivo usado por Windows para hibernación e inicio rápido. Su tamaño depende de la RAM instalada y del modo de powercfg.",
            true,
            "hiberfil",
        ),
        (
            "swapfile.sys",
            "Archivo de intercambio usado por las aplicaciones de la Store/UWP y administrado automáticamente por Windows.",
            false,
            "none",
        ),
    ];

    let mut files = Vec::new();
    let mut hiberfil_present = false;

    for root in roots {
        for (fname, explanation, can_adjust, adjust_kind) in specs {
            let full = root.join(fname);
            if let Some(size) = locked_file_size(&full) {
                if fname == "hiberfil.sys" {
                    hiberfil_present = true;
                }
                files.push(SystemFileEntry {
                    id: format!("sysfile:{}", full.to_string_lossy().to_ascii_lowercase()),
                    name: fname.to_string(),
                    path: full.to_string_lossy().into_owned(),
                    size_bytes: size,
                    explanation: explanation.to_string(),
                    can_adjust,
                    adjust_kind: adjust_kind.to_string(),
                });
            }
        }
    }

    let total_bytes = files.iter().map(|f| f.size_bytes).sum();
    let hibernation = read_hibernation_status(hiberfil_present);

    SystemFilesReport {
        files,
        total_bytes,
        hibernation,
        virtual_memory_instruction: "Pestaña Opciones avanzadas → Memoria virtual → Cambiar"
            .to_string(),
    }
}

/// Launches `SystemPropertiesPerformance.exe` from `%SystemRoot%\System32`,
/// falling back to `SystemPropertiesAdvanced.exe` if the former does not exist.
pub fn open_virtual_memory_dialog(
    system_root: &Path,
    launcher: &dyn SystemProcessLauncher,
) -> Result<String, String> {
    let sys32 = system_root.join("System32");
    let perf = sys32.join("SystemPropertiesPerformance.exe");
    let adv = sys32.join("SystemPropertiesAdvanced.exe");

    let target = if perf.is_file() {
        perf
    } else if adv.is_file() {
        adv
    } else {
        return Err(
            "No se encontró SystemPropertiesPerformance.exe ni SystemPropertiesAdvanced.exe en System32."
                .to_string(),
        );
    };

    let target_str = target.to_string_lossy().into_owned();
    launcher.spawn(&target_str, &[])?;
    Ok(target_str)
}

pub fn open_power_options(launcher: &dyn SystemProcessLauncher) -> Result<(), String> {
    launcher.spawn("control.exe", &["powercfg.cpl"])
}

pub fn run_powercfg_elevated(
    action: PowercfgAction,
    launcher: &dyn SystemProcessLauncher,
) -> Result<String, String> {
    let params = match action {
        PowercfgAction::Reduced => "/h /type reduced",
        PowercfgAction::Off => "/h off",
        PowercfgAction::On => "/h on",
    };
    launcher.spawn_elevated("powercfg.exe", params)?;
    Ok(format!("powercfg {params}"))
}
