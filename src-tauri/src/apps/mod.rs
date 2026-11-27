use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::ApplicationModel::PackageSignatureKind;
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::scanner::model::ScanTree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    Win32,
    StoreMsix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeSource {
    Measured,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub source: AppSource,
    pub install_location: Option<String>,
    pub size_bytes: u64,
    pub size_source: SizeSource,
    pub uninstall_string: Option<String>,
    pub package_family_name: Option<String>,
    pub install_date: Option<String>,
}

struct RawWin32App {
    key_id: String,
    name: String,
    publisher: String,
    version: String,
    install_location: Option<String>,
    estimated_bytes: u64,
    uninstall_string: Option<String>,
    install_date: Option<String>,
}

fn read_reg_string(hkey: HKEY, value_name: &str) -> Option<String> {
    let name_wide: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut val_type = REG_VALUE_TYPE::default();
    let mut size = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut val_type),
            None,
            Some(&mut size),
        )
    };
    if status.is_err() || size == 0 || (val_type != REG_SZ && val_type != REG_EXPAND_SZ) {
        return None;
    }

    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let status2 = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut val_type),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status2.is_err() {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = OsString::from_wide(&buf[..end])
        .to_string_lossy()
        .trim()
        .to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn read_reg_dword(hkey: HKEY, value_name: &str) -> Option<u32> {
    let name_wide: Vec<u16> = value_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut val_type = REG_VALUE_TYPE::default();
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_wide.as_ptr()),
            None,
            Some(&mut val_type),
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status.is_ok() && val_type == REG_DWORD {
        Some(data)
    } else {
        None
    }
}

fn scan_uninstall_hive(root: HKEY, subkey_path: &str, hive_tag: &str, out: &mut Vec<RawWin32App>) {
    let path_wide: Vec<u16> = subkey_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut parent_key = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            root,
            PCWSTR(path_wide.as_ptr()),
            0,
            KEY_READ,
            &mut parent_key,
        )
    }
    .is_err()
    {
        return;
    }

    let mut idx = 0u32;
    loop {
        let mut name_buf = [0u16; 512];
        let mut name_len = name_buf.len() as u32;
        let res = unsafe {
            RegEnumKeyExW(
                parent_key,
                idx,
                PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                PWSTR::null(),
                None,
                None,
            )
        };
        idx += 1;
        if res.is_err() {
            break;
        }
        let sub_name = OsString::from_wide(&name_buf[..name_len as usize])
            .to_string_lossy()
            .into_owned();
        let sub_wide: Vec<u16> = sub_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut app_key = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                parent_key,
                PCWSTR(sub_wide.as_ptr()),
                0,
                KEY_READ,
                &mut app_key,
            )
        }
        .is_err()
        {
            continue;
        }

        let sys_component = read_reg_dword(app_key, "SystemComponent").unwrap_or(0);
        let parent_key_name = read_reg_string(app_key, "ParentKeyName");
        let release_type = read_reg_string(app_key, "ReleaseType");

        if sys_component != 1 && parent_key_name.is_none() && release_type.is_none() {
            if let Some(name) = read_reg_string(app_key, "DisplayName") {
                let uninstall_string = read_reg_string(app_key, "UninstallString")
                    .or_else(|| read_reg_string(app_key, "QuietUninstallString"));
                if uninstall_string.is_some() {
                    let publisher = read_reg_string(app_key, "Publisher").unwrap_or_default();
                    let version = read_reg_string(app_key, "DisplayVersion").unwrap_or_default();
                    let install_location = read_reg_string(app_key, "InstallLocation");
                    let estimated_kb = read_reg_dword(app_key, "EstimatedSize").unwrap_or(0);
                    let install_date = read_reg_string(app_key, "InstallDate");
                    out.push(RawWin32App {
                        key_id: format!("win32:{hive_tag}:{sub_name}"),
                        name,
                        publisher,
                        version,
                        install_location,
                        estimated_bytes: (estimated_kb as u64) * 1024,
                        uninstall_string,
                        install_date,
                    });
                }
            }
        }

        unsafe {
            let _ = RegCloseKey(app_key);
        }
    }

    unsafe {
        let _ = RegCloseKey(parent_key);
    }
}

fn is_too_broad_install_dir(path: &Path) -> bool {
    let norm = path
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    if norm.len() <= 2 {
        return true; // Drive root like C:
    }
    let blocked = [
        r"c:\windows",
        r"c:\program files",
        r"c:\program files (x86)",
        r"c:\programdata",
        r"c:\users",
    ];
    if blocked.contains(&norm.as_str()) {
        return true;
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if norm == profile.trim_end_matches('\\').to_ascii_lowercase() {
            return true;
        }
    }
    false
}

fn bounded_dir_size(path: &Path, max_entries: usize) -> Option<u64> {
    if !path.is_dir() {
        return None;
    }
    let mut stack = vec![path.to_path_buf()];
    let mut total = 0u64;
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).ok()?;
        for entry in rd.flatten() {
            visited += 1;
            if visited > max_entries {
                return Some(total);
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Some(total)
}

fn measure_app_dir(path_str: &str, scan_tree: Option<&ScanTree>) -> Option<u64> {
    let clean = path_str.trim().trim_matches('"');
    if clean.is_empty() {
        return None;
    }
    let path = PathBuf::from(clean);
    if is_too_broad_install_dir(&path) {
        return None;
    }
    if let Some(tree) = scan_tree {
        if let Some(nid) = tree.find_node_by_path(&path) {
            return Some(tree.allocated_size[nid as usize]);
        }
    }
    bounded_dir_size(&path, 25_000)
}

pub fn list_installed_apps(scan_tree: Option<&ScanTree>) -> Vec<InstalledApp> {
    let mut raw_win32 = Vec::new();
    scan_uninstall_hive(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        "hklm64",
        &mut raw_win32,
    );
    scan_uninstall_hive(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        "hklm32",
        &mut raw_win32,
    );
    scan_uninstall_hive(
        HKEY_CURRENT_USER,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        "hkcu",
        &mut raw_win32,
    );

    // Count install location collisions so shared folders aren't attributed to every app
    let mut location_counts: HashMap<String, usize> = HashMap::new();
    for app in &raw_win32 {
        if let Some(ref loc) = app.install_location {
            let norm = loc
                .trim()
                .trim_matches('"')
                .trim_end_matches('\\')
                .to_ascii_lowercase();
            if !norm.is_empty() {
                *location_counts.entry(norm).or_insert(0) += 1;
            }
        }
    }

    let mut seen_win32: HashSet<String> = HashSet::new();
    let mut apps: Vec<InstalledApp> = Vec::new();

    for raw in raw_win32 {
        let dedup_key = format!(
            "{}|{}",
            raw.name.to_ascii_lowercase(),
            raw.version.to_ascii_lowercase()
        );
        if !seen_win32.insert(dedup_key) {
            continue;
        }

        let shared_loc = raw.install_location.as_ref().is_some_and(|loc| {
            let norm = loc
                .trim()
                .trim_matches('"')
                .trim_end_matches('\\')
                .to_ascii_lowercase();
            location_counts.get(&norm).copied().unwrap_or(0) > 1
        });

        let (size_bytes, size_source) = if !shared_loc {
            if let Some(ref loc) = raw.install_location {
                if let Some(measured) = measure_app_dir(loc, scan_tree) {
                    (measured, SizeSource::Measured)
                } else if raw.estimated_bytes > 0 {
                    (raw.estimated_bytes, SizeSource::Estimated)
                } else {
                    (0, SizeSource::Unknown)
                }
            } else if raw.estimated_bytes > 0 {
                (raw.estimated_bytes, SizeSource::Estimated)
            } else {
                (0, SizeSource::Unknown)
            }
        } else if raw.estimated_bytes > 0 {
            (raw.estimated_bytes, SizeSource::Estimated)
        } else {
            (0, SizeSource::Unknown)
        };

        apps.push(InstalledApp {
            id: raw.key_id,
            name: raw.name,
            publisher: raw.publisher,
            version: raw.version,
            source: AppSource::Win32,
            install_location: raw.install_location,
            size_bytes,
            size_source,
            uninstall_string: raw.uninstall_string,
            package_family_name: None,
            install_date: raw.install_date,
        });
    }

    // Store / MSIX packages via WinRT PackageManager
    collect_msix_packages(scan_tree, &mut apps);

    apps.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes).then(a.name.cmp(&b.name)));
    apps
}

fn collect_msix_packages(scan_tree: Option<&ScanTree>, out: &mut Vec<InstalledApp>) {
    let Ok(pm) = PackageManager::new() else {
        return;
    };
    let Ok(packages) = pm.FindPackagesByUserSecurityId(&HSTRING::new()) else {
        return;
    };

    for pkg in packages {
        if pkg.IsFramework().unwrap_or(false)
            || pkg.IsResourcePackage().unwrap_or(false)
            || pkg.IsBundle().unwrap_or(false)
        {
            continue;
        }
        if matches!(pkg.SignatureKind(), Ok(PackageSignatureKind::System)) {
            continue;
        }

        let Ok(id) = pkg.Id() else { continue };
        let family_name = id.FamilyName().map(|h| h.to_string()).unwrap_or_default();
        let display_name = pkg
            .DisplayName()
            .map(|h| h.to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| id.Name().map(|h| h.to_string()).ok())
            .unwrap_or_else(|| family_name.clone());

        if display_name.is_empty() {
            continue;
        }

        let publisher = pkg
            .PublisherDisplayName()
            .map(|h| h.to_string())
            .unwrap_or_default();
        let version = id
            .Version()
            .map(|v| format!("{}.{}.{}.{}", v.Major, v.Minor, v.Build, v.Revision))
            .unwrap_or_default();
        let install_location: Option<String> = pkg
            .InstalledPath()
            .ok()
            .map(|h| h.to_string())
            .filter(|s| !s.is_empty());

        let mut measured_total = 0u64;
        let mut any_measured = false;
        if let Some(ref loc) = install_location {
            if let Some(sz) = measure_app_dir(loc, scan_tree) {
                measured_total = measured_total.saturating_add(sz);
                any_measured = true;
            }
        }
        if !family_name.is_empty() {
            if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
                let pkg_data = PathBuf::from(local_app).join("Packages").join(&family_name);
                if let Some(sz) = bounded_dir_size(&pkg_data, 15_000) {
                    measured_total = measured_total.saturating_add(sz);
                    any_measured = true;
                }
            }
        }

        out.push(InstalledApp {
            id: format!("msix:{family_name}"),
            name: display_name,
            publisher,
            version,
            source: AppSource::StoreMsix,
            install_location,
            size_bytes: measured_total,
            size_source: if any_measured {
                SizeSource::Measured
            } else {
                SizeSource::Unknown
            },
            uninstall_string: None,
            package_family_name: Some(family_name),
            install_date: None,
        });
    }
}

pub fn parse_uninstall_command(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        if let Some(end_quote) = rest.find('"') {
            let exe = rest[..end_quote].to_string();
            let args = rest[end_quote + 1..].trim().to_string();
            return Some((exe, args));
        }
    }
    // Check if unquoted path ends in .exe before args
    let lower = trimmed.to_ascii_lowercase();
    if let Some(exe_idx) = lower.find(".exe") {
        let split_at = exe_idx + 4;
        let exe = trimmed[..split_at].trim().to_string();
        let args = trimmed[split_at..].trim().to_string();
        return Some((exe, args));
    }
    match trimmed.split_once(char::is_whitespace) {
        Some((exe, args)) => Some((exe.to_string(), args.trim().to_string())),
        None => Some((trimmed.to_string(), String::new())),
    }
}

pub fn trigger_native_uninstall(
    source: AppSource,
    uninstall_string: Option<&str>,
) -> Result<String, String> {
    if source == AppSource::StoreMsix {
        open_windows_apps_settings()?;
        return Ok("Se abrió Configuración de Windows (Aplicaciones instaladas).".to_string());
    }

    let raw = uninstall_string.ok_or_else(|| {
        "Esta aplicación no expone UninstallString; usa Configuración de Windows.".to_string()
    })?;
    let (exe, args) = parse_uninstall_command(raw)
        .ok_or_else(|| "No se pudo interpretar el comando de desinstalación.".to_string())?;

    let exe_w: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let verb_w: Vec<u16> = "open\0".encode_utf16().collect();

    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(exe_w.as_ptr()),
            if args.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(args_w.as_ptr())
            },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (res.0 as isize) <= 32 {
        return Err(format!(
            "No se pudo iniciar el desinstalador nativo (código {}).",
            res.0 as isize
        ));
    }
    Ok(format!("Desinstalador nativo iniciado: {exe}"))
}

pub fn open_windows_apps_settings() -> Result<(), String> {
    let target: Vec<u16> = "ms-settings:appsfeatures\0".encode_utf16().collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (res.0 as isize) <= 32 {
        return Err("No se pudo abrir ms-settings:appsfeatures.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_and_unquoted_uninstall_strings() {
        let (exe, args) =
            parse_uninstall_command(r#""C:\Program Files\App Name\unins000.exe" /SILENT"#).unwrap();
        assert_eq!(exe, r"C:\Program Files\App Name\unins000.exe");
        assert_eq!(args, "/SILENT");

        let (exe2, args2) = parse_uninstall_command(r"MsiExec.exe /X{1234-5678}").unwrap();
        assert_eq!(exe2, "MsiExec.exe");
        assert_eq!(args2, "/X{1234-5678}");

        let (exe3, args3) =
            parse_uninstall_command(r"C:\Program Files\My App\uninstall.exe /S").unwrap();
        assert_eq!(exe3, r"C:\Program Files\My App\uninstall.exe");
        assert_eq!(args3, "/S");
    }

    #[test]
    fn identifies_broad_install_directories() {
        assert!(is_too_broad_install_dir(Path::new(r"C:\")));
        assert!(is_too_broad_install_dir(Path::new(r"C:\Program Files")));
        assert!(!is_too_broad_install_dir(Path::new(
            r"C:\Program Files\MySpecificApp"
        )));
    }
}
