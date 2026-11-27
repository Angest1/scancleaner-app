use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_NAME,
};

use crate::protection::ProtectionEngine;
use crate::scanner::derived::volume_root_of_path;

use super::queue::{ReconcileOrigin, ReconcilePriority, ReconcileRequest};

/// Mockable disk free-space provider (`GetDiskFreeSpaceExW`) for drift detection & real freed-space measurement.
pub trait DiskSpaceProvider: Send + Sync {
    /// Returns `(total_bytes, free_bytes)` for `drive_root`.
    fn query_space(&self, drive_root: &str) -> Option<(u64, u64)>;
}

pub struct WinDiskSpaceProvider;

impl DiskSpaceProvider for WinDiskSpaceProvider {
    fn query_space(&self, drive_root: &str) -> Option<(u64, u64)> {
        let wide: Vec<u16> = drive_root
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut avail = 0u64;
        let mut total = 0u64;
        let mut total_free = 0u64;
        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                Some(&mut avail),
                Some(&mut total),
                Some(&mut total_free),
            )
        }
        .ok()?;
        Some((total, total_free))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftCheckResult {
    pub volume: String,
    pub real_used_bytes: u64,
    pub index_used_bytes: u64,
    pub diff_bytes: u64,
    pub threshold_bytes: u64,
    pub drifted: bool,
}

/// Evaluates whether real volume used space (`total - free` from `GetDiskFreeSpaceExW`)
/// has drifted beyond `max(total * pct/100, min_gb * 1 GiB)` from the indexed allocated total.
pub fn evaluate_free_space_drift(
    volume: &str,
    index_used_bytes: u64,
    provider: &dyn DiskSpaceProvider,
    threshold_percent: f64,
    threshold_min_gb: u64,
) -> Option<DriftCheckResult> {
    let (total, free) = provider.query_space(volume)?;
    let real_used_bytes = total.saturating_sub(free);
    let diff_bytes = real_used_bytes.abs_diff(index_used_bytes);
    let pct_bytes = ((total as f64) * (threshold_percent / 100.0)) as u64;
    let abs_bytes = threshold_min_gb.saturating_mul(1024 * 1024 * 1024);
    let threshold_bytes = pct_bytes.max(abs_bytes);

    Some(DriftCheckResult {
        volume: volume.to_string(),
        real_used_bytes,
        index_used_bytes,
        diff_bytes,
        threshold_bytes,
        drifted: diff_bytes > threshold_bytes,
    })
}

/// Real vs estimated space freed analysis (Section 9).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreedSpaceMeasurement {
    pub volume: String,
    pub free_before_bytes: u64,
    pub free_after_bytes: u64,
    pub real_freed_bytes: u64,
    pub estimated_bytes: u64,
    pub discrepancy_explanation: Option<String>,
}

pub fn measure_real_freed_space(
    volume: &str,
    free_before_bytes: u64,
    estimated_bytes: u64,
    provider: &dyn DiskSpaceProvider,
    pending_bg_trash: bool,
    touched_docker_vhdx: bool,
    had_locked_failures: bool,
) -> FreedSpaceMeasurement {
    let free_after_bytes = provider
        .query_space(volume)
        .map(|(_, f)| f)
        .unwrap_or(free_before_bytes);
    let real_freed_bytes = free_after_bytes.saturating_sub(free_before_bytes);

    let discrepancy_explanation =
        if estimated_bytes > 4 * 1024 * 1024 && real_freed_bytes < (estimated_bytes * 70 / 100) {
            let mut reasons = Vec::new();
            if pending_bg_trash {
                reasons.push("borrado en segundo plano aún vaciando .scancleaner-trash");
            }
            if touched_docker_vhdx {
                reasons.push("disco virtual .vhdx de Docker/WSL sin compactar");
            }
            if had_locked_failures {
                reasons.push("archivos bloqueados en uso por otro proceso");
            }
            if reasons.is_empty() {
                reasons.push("hardlinks compartidos, compresión NTFS o copias de sombra");
            }
            Some(format!(
                "Diferencia entre espacio estimado y liberado real: probable causa — {}.",
                reasons.join("; ")
            ))
        } else {
            None
        };

    FreedSpaceMeasurement {
        volume: volume.to_string(),
        free_before_bytes,
        free_after_bytes,
        real_freed_bytes,
        estimated_bytes,
        discrepancy_explanation,
    }
}

/// Mockable Registry/MSIX uninstall watcher backend.
pub trait UninstallPresenceProbe: Send + Sync {
    /// Returns true if the Win32 Uninstall registry key or MSIX package is still registered.
    fn is_app_still_registered(&self, app_id: &str) -> bool;
    /// Blocks briefly until a change occurs in Uninstall hives (or timeout slice elapses).
    fn wait_for_uninstall_change(&self, max_wait: Duration);
}

pub struct WinUninstallPresenceProbe;

impl UninstallPresenceProbe for WinUninstallPresenceProbe {
    fn is_app_still_registered(&self, app_id: &str) -> bool {
        if let Some(family) = app_id.strip_prefix("msix:") {
            if let Ok(pm) = windows::Management::Deployment::PackageManager::new() {
                if let Ok(pkgs) = pm.FindPackagesByUserSecurityId(&windows::core::HSTRING::new()) {
                    for pkg in pkgs {
                        if let Ok(id) = pkg.Id() {
                            if id
                                .FamilyName()
                                .map(|h| h.to_string().eq_ignore_ascii_case(family))
                                .unwrap_or(false)
                            {
                                return true;
                            }
                        }
                    }
                }
            }
            return false;
        }

        // Format: `win32:<hive_tag>:<subkey_name>`
        let parts: Vec<&str> = app_id.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "win32" {
            return false;
        }
        let (root_hive, base) = match parts[1] {
            "hklm64" => (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            "hklm32" => (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            "hkcu" => (
                HKEY_CURRENT_USER,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            _ => return false,
        };
        let full = format!(r"{base}\{}", parts[2]);
        let wide: Vec<u16> = full.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = HKEY::default();
        let ok = unsafe { RegOpenKeyExW(root_hive, PCWSTR(wide.as_ptr()), 0, KEY_READ, &mut hkey) };
        if ok.is_ok() {
            unsafe {
                let _ = RegCloseKey(hkey);
            }
            true
        } else {
            false
        }
    }

    fn wait_for_uninstall_change(&self, max_wait: Duration) {
        // Watch HKCU Uninstall key with `RegNotifyChangeKeyValue` on a short poll slice
        // so we wake up immediately on registry uninstall key removal while honoring timeout.
        // TODO: verificar si usar 3 eventos con WaitForMultipleObjects (HKLM64, HKLM32, HKCU)
        // simultáneos reduce aún más la latencia frente al polling ligero cada 1s para claves HKLM.
        let subkey: Vec<u16> = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\0"
            .encode_utf16()
            .collect();
        let mut hkey = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                KEY_NOTIFY,
                &mut hkey,
            )
        }
        .is_ok()
        {
            // Async notification requires event handle; keep bounded sleep for safe cancellation
            let _ =
                unsafe { RegNotifyChangeKeyValue(hkey, false, REG_NOTIFY_CHANGE_NAME, None, true) };
            unsafe {
                let _ = RegCloseKey(hkey);
            }
        }
        std::thread::sleep(max_wait.min(Duration::from_secs(1)));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallLeftoverSuggestion {
    pub app_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub nivel: String, // always "revisar"
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallWatchResult {
    pub app_id: String,
    pub app_name: String,
    pub key_removed: bool,
    pub timed_out: bool,
    pub reconcile_requests: Vec<ReconcileRequest>,
    pub leftovers: Vec<UninstallLeftoverSuggestion>,
}

/// Watches an uninstall until its Uninstall registry key / MSIX registration disappears
/// (NOT just when the initial process exits, since many uninstallers relaunch from `%TEMP%`),
/// or until `timeout` elapses. Returns reconcile requests & non-protected leftover suggestions in level `revisar`.
#[allow(clippy::too_many_arguments)]
pub fn watch_native_uninstall_completion(
    app_id: &str,
    app_name: &str,
    install_location: Option<&str>,
    known_appdata_paths: &[PathBuf],
    timeout: Duration,
    poll_slice: Duration,
    probe: &dyn UninstallPresenceProbe,
    protection: &ProtectionEngine,
) -> UninstallWatchResult {
    let start = Instant::now();
    let mut key_removed = false;

    while start.elapsed() < timeout {
        if !probe.is_app_still_registered(app_id) {
            key_removed = true;
            break;
        }
        probe.wait_for_uninstall_change(poll_slice);
    }

    let mut reconcile_paths: Vec<PathBuf> = Vec::new();
    let mut leftovers: Vec<UninstallLeftoverSuggestion> = Vec::new();

    if let Some(loc) = install_location {
        let p = PathBuf::from(loc.trim().trim_matches('"'));
        if !p.as_os_str().is_empty() {
            reconcile_paths.push(p.clone());
            if key_removed && p.exists() && !protection.is_protected(&p).protected {
                let sz = crate::dev_cleaner::resolver::measure_dir_no_reparse(&p).0;
                if sz > 0 {
                    leftovers.push(UninstallLeftoverSuggestion {
                        app_name: app_name.to_string(),
                        path: p.to_string_lossy().into_owned(),
                        size_bytes: sz,
                        nivel: "revisar".to_string(),
                        reason: "Restos de carpeta de instalación tras desinstalar (revisar antes de borrar)."
                            .to_string(),
                    });
                }
            }
        }
    }

    for ad in known_appdata_paths {
        reconcile_paths.push(ad.clone());
        if key_removed && ad.exists() && !protection.is_protected(ad).protected {
            let sz = crate::dev_cleaner::resolver::measure_dir_no_reparse(ad).0;
            if sz > 0 {
                leftovers.push(UninstallLeftoverSuggestion {
                    app_name: app_name.to_string(),
                    path: ad.to_string_lossy().into_owned(),
                    size_bytes: sz,
                    nivel: "revisar".to_string(),
                    reason: "Restos en AppData tras desinstalar (revisar antes de borrar)."
                        .to_string(),
                });
            }
        }
    }

    // Group reconcile paths by volume
    let mut by_vol: std::collections::HashMap<String, Vec<PathBuf>> =
        std::collections::HashMap::new();
    for p in reconcile_paths {
        if let Some(vol) = volume_root_of_path(&p) {
            by_vol.entry(vol).or_default().push(p);
        }
    }
    let reconcile_requests: Vec<ReconcileRequest> = by_vol
        .into_iter()
        .map(|(volume, paths)| ReconcileRequest {
            volume,
            paths,
            origin: ReconcileOrigin::NativeUninstall,
            priority: ReconcilePriority::High,
        })
        .collect();

    UninstallWatchResult {
        app_id: app_id.to_string(),
        app_name: app_name.to_string(),
        key_removed,
        timed_out: !key_removed,
        reconcile_requests,
        leftovers,
    }
}

/// Device arrival / removal event (from `WM_DEVICECHANGE` or mocked in tests).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceChangeEvent {
    Arrived { drive_root: String },
    Removed { drive_root: String },
}

/// Parses a `WM_DEVICECHANGE` `dbcv_unitmask` bitmask into drive roots (e.g. `0b1000` -> `["D:\\"]`).
pub fn decode_device_unitmask(unitmask: u32) -> Vec<String> {
    let mut out = Vec::new();
    for bit in 0..26u32 {
        if (unitmask & (1u32 << bit)) != 0 {
            let ch = (b'A' + bit as u8) as char;
            out.push(format!("{ch}:\\"));
        }
    }
    out
}

/// Builds High-priority reconcile requests after deleting a batch of paths.
pub fn build_post_delete_reconcile_requests(
    affected_paths: &[PathBuf],
    used_recycle_bin: bool,
) -> Vec<ReconcileRequest> {
    let mut by_vol: std::collections::HashMap<String, Vec<PathBuf>> =
        std::collections::HashMap::new();
    for p in affected_paths {
        if let Some(vol) = volume_root_of_path(p) {
            let parent = p
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(&vol));
            by_vol.entry(vol.clone()).or_default().push(parent);
            if used_recycle_bin {
                by_vol
                    .entry(vol.clone())
                    .or_default()
                    .push(PathBuf::from(format!(r"{vol}$Recycle.Bin")));
            }
        }
    }

    by_vol
        .into_iter()
        .map(|(volume, paths)| ReconcileRequest {
            volume,
            paths,
            origin: ReconcileOrigin::UserDeletion,
            priority: ReconcilePriority::High,
        })
        .collect()
}
