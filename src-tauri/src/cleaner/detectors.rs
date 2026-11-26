use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

use crate::elevation::is_elevated;
use crate::scanner::model::{NodeKind, OriginCategory, ScanTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanCategory {
    UserTemp,
    SystemTemp,
    RecycleBin,
    BrowserCache,
    DevArtifact,
    LargeOldFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanCandidate {
    pub id: String,
    pub path: String,
    pub display_name: String,
    pub category: CleanCategory,
    pub size_bytes: u64,
    pub item_count: u64,
    pub last_modified_ts: i64,
    pub requires_admin: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidatesReport {
    pub is_elevated: bool,
    pub running_browsers: Vec<String>,
    pub recycle_bin_bytes: u64,
    pub recycle_bin_items: u64,
    pub candidates: Vec<CleanCandidate>,
}

pub fn detect_running_browsers() -> Vec<String> {
    let targets = [
        ("chrome.exe", "Google Chrome"),
        ("msedge.exe", "Microsoft Edge"),
        ("brave.exe", "Brave"),
        ("firefox.exe", "Mozilla Firefox"),
        ("opera.exe", "Opera"),
    ];
    let mut found = HashSet::new();
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snap, &mut entry) }.is_ok() {
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = OsString::from_wide(&entry.szExeFile[..end])
                .to_string_lossy()
                .to_ascii_lowercase();
            for (exe, label) in targets {
                if name == exe {
                    found.insert(label.to_string());
                }
            }
            if unsafe { Process32NextW(snap, &mut entry) }.is_err() {
                break;
            }
        }
    }
    unsafe {
        let _ = CloseHandle(snap);
    }
    let mut list: Vec<String> = found.into_iter().collect();
    list.sort();
    list
}

pub fn query_recycle_bin() -> (u64, u64) {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    let ok = unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) };
    if ok.is_ok() {
        (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64)
    } else {
        (0, 0)
    }
}

fn dir_stats_bounded(path: &Path, max_entries: usize) -> (u64, u64, i64) {
    if !path.exists() {
        return (0, 0, 0);
    }
    if path.is_file() {
        let meta = std::fs::symlink_metadata(path).ok();
        let len = meta.as_ref().map_or(0, |m| m.len());
        let ts = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        return (len, 1, ts);
    }

    let mut stack = vec![path.to_path_buf()];
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut newest_ts = 0i64;
    let mut visited = 0usize;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            visited += 1;
            if visited > max_entries {
                return (bytes, count, newest_ts);
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
                bytes = bytes.saturating_add(meta.len());
                count += 1;
                if let Ok(mod_t) = meta.modified() {
                    if let Ok(dur) = mod_t.duration_since(std::time::UNIX_EPOCH) {
                        newest_ts = newest_ts.max(dur.as_secs() as i64);
                    }
                }
            }
        }
    }
    (bytes, count, newest_ts)
}

fn collect_temp_children(
    temp_dir: &Path,
    category: CleanCategory,
    requires_admin: bool,
    out: &mut Vec<CleanCandidate>,
) {
    let Ok(rd) = std::fs::read_dir(temp_dir) else {
        return;
    };
    let mut total_bytes = 0u64;
    let mut total_items = 0u64;
    let mut newest_ts = 0i64;
    let mut child_paths = Vec::new();

    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let (b, c, ts) = dir_stats_bounded(&p, 5_000);
        total_bytes = total_bytes.saturating_add(b);
        total_items = total_items.saturating_add(c.max(1));
        newest_ts = newest_ts.max(ts);
        child_paths.push(p);
    }

    if total_items > 0 {
        let label = match category {
            CleanCategory::UserTemp => "Archivos temporales del usuario (%TEMP%)",
            CleanCategory::SystemTemp => "Archivos temporales del sistema (C:\\Windows\\Temp)",
            _ => "Temporales",
        };
        out.push(CleanCandidate {
            id: format!("temp:{category:?}"),
            path: temp_dir.to_string_lossy().into_owned(),
            display_name: label.to_string(),
            category,
            size_bytes: total_bytes,
            item_count: total_items,
            last_modified_ts: newest_ts,
            requires_admin,
            warning: if requires_admin && !is_elevated() {
                Some("Requiere permisos de administrador para limpiar.".to_string())
            } else {
                None
            },
        });
    }
}

fn collect_browser_caches(running: &[String], out: &mut Vec<CleanCandidate>) {
    let Ok(local_app) = std::env::var("LOCALAPPDATA") else {
        return;
    };
    let base = PathBuf::from(&local_app);

    let chromium_targets = [
        (
            "Google Chrome",
            base.join(r"Google\Chrome\User Data\Default"),
        ),
        (
            "Microsoft Edge",
            base.join(r"Microsoft\Edge\User Data\Default"),
        ),
        (
            "Brave",
            base.join(r"BraveSoftware\Brave-Browser\User Data\Default"),
        ),
    ];

    for (browser_name, profile_dir) in chromium_targets {
        for sub in [r"Cache\Cache_Data", "Code Cache", "GPUCache"] {
            let target = profile_dir.join(sub);
            if target.is_dir() {
                let (bytes, count, ts) = dir_stats_bounded(&target, 20_000);
                if bytes > 0 {
                    let is_running = running.iter().any(|r| r == browser_name);
                    out.push(CleanCandidate {
                        id: format!("browser:{browser_name}:{sub}"),
                        path: target.to_string_lossy().into_owned(),
                        display_name: format!("{browser_name} — {sub}"),
                        category: CleanCategory::BrowserCache,
                        size_bytes: bytes,
                        item_count: count,
                        last_modified_ts: ts,
                        requires_admin: false,
                        warning: if is_running {
                            Some(format!(
                                "{browser_name} está abierto; ciérralo para liberar todos los archivos."
                            ))
                        } else {
                            None
                        },
                    });
                }
            }
        }
    }

    // Firefox profiles cache2/entries
    let ff_profiles = base.join(r"Mozilla\Firefox\Profiles");
    if let Ok(rd) = std::fs::read_dir(&ff_profiles) {
        let is_running = running.iter().any(|r| r == "Mozilla Firefox");
        for entry in rd.flatten() {
            let cache_dir = entry.path().join(r"cache2\entries");
            if cache_dir.is_dir() {
                let (bytes, count, ts) = dir_stats_bounded(&cache_dir, 20_000);
                if bytes > 0 {
                    out.push(CleanCandidate {
                        id: format!("browser:firefox:{}", entry.file_name().to_string_lossy()),
                        path: cache_dir.to_string_lossy().into_owned(),
                        display_name: format!(
                            "Mozilla Firefox — Caché ({})",
                            entry.file_name().to_string_lossy()
                        ),
                        category: CleanCategory::BrowserCache,
                        size_bytes: bytes,
                        item_count: count,
                        last_modified_ts: ts,
                        requires_admin: false,
                        warning: if is_running {
                            Some(
                                "Mozilla Firefox está abierto; ciérralo antes de limpiar."
                                    .to_string(),
                            )
                        } else {
                            None
                        },
                    });
                }
            }
        }
    }
}

fn collect_package_manager_caches(out: &mut Vec<CleanCandidate>) {
    let Ok(local_app) = std::env::var("LOCALAPPDATA") else {
        return;
    };
    let local = PathBuf::from(local_app);
    let candidates = [
        ("Caché de npm", local.join("npm-cache")),
        ("Caché de pip", local.join(r"pip\Cache")),
        ("Caché de Composer", local.join(r"Composer\cache")),
        ("Store de pnpm", local.join("pnpm-store")),
    ];

    for (label, path) in candidates {
        if path.is_dir() {
            let (bytes, count, ts) = dir_stats_bounded(&path, 25_000);
            if bytes > 0 {
                out.push(CleanCandidate {
                    id: format!("devcache:{label}"),
                    path: path.to_string_lossy().into_owned(),
                    display_name: label.to_string(),
                    category: CleanCategory::DevArtifact,
                    size_bytes: bytes,
                    item_count: count,
                    last_modified_ts: ts,
                    requires_admin: false,
                    warning: None,
                });
            }
        }
    }
}

pub fn discover_cleanup_candidates(
    scan_tree: Option<&ScanTree>,
    min_large_mb: u64,
    min_age_days: u64,
) -> CleanupCandidatesReport {
    let running_browsers = detect_running_browsers();
    let (recycle_bin_bytes, recycle_bin_items) = query_recycle_bin();
    let mut candidates = Vec::new();

    if let Ok(user_temp) = std::env::var("TEMP") {
        collect_temp_children(
            Path::new(&user_temp),
            CleanCategory::UserTemp,
            false,
            &mut candidates,
        );
    }

    let win_temp = std::env::var("SystemRoot")
        .map(|s| PathBuf::from(s).join("Temp"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\Temp"));
    collect_temp_children(&win_temp, CleanCategory::SystemTemp, true, &mut candidates);

    collect_browser_caches(&running_browsers, &mut candidates);
    collect_package_manager_caches(&mut candidates);

    if let Some(tree) = scan_tree {
        let now_ts = chrono::Utc::now().timestamp();
        let seven_days_secs = 7 * 86_400;
        let min_large_bytes = min_large_mb.saturating_mul(1024 * 1024);
        let min_age_secs = (min_age_days as i64).saturating_mul(86_400);

        let mut dev_dirs: Vec<CleanCandidate> = Vec::new();
        let mut large_files: Vec<CleanCandidate> = Vec::new();

        for idx in 1..tree.nodes.len() {
            let node = &tree.nodes[idx];
            let name = tree.node_name(idx as u32);
            let name_lower = name.to_ascii_lowercase();

            if node.kind == NodeKind::Dir
                && node.allocated_size > 1024 * 1024
                && matches!(
                    name_lower.as_str(),
                    "node_modules" | ".venv" | "venv" | "target" | "vendor"
                )
            {
                // Avoid nested node_modules inside another node_modules
                let parent_is_dev = node.parent.is_some_and(|pid| {
                    let pname = tree.node_name(pid).to_ascii_lowercase();
                    matches!(pname.as_str(), "node_modules" | "target" | "vendor")
                });
                if !parent_is_dev {
                    let path = tree.reconstruct_path(idx as u32);
                    let recent = now_ts - node.last_write_ts < seven_days_secs;
                    dev_dirs.push(CleanCandidate {
                        id: format!("devdir:{idx}"),
                        path: path.to_string_lossy().into_owned(),
                        display_name: format!("Artefacto {name}"),
                        category: CleanCategory::DevArtifact,
                        size_bytes: node.allocated_size,
                        item_count: node.file_count as u64,
                        last_modified_ts: node.last_write_ts,
                        requires_admin: false,
                        warning: if recent {
                            Some("Modificado hace menos de 7 días (proyecto activo).".to_string())
                        } else {
                            None
                        },
                    });
                }
            } else if node.kind == NodeKind::File
                && node.allocated_size >= min_large_bytes
                && node.last_write_ts > 0
                && (now_ts - node.last_write_ts) >= min_age_secs
                && !matches!(
                    node.origin_cat,
                    OriginCategory::System
                        | OriginCategory::InstalledApps
                        | OriginCategory::RecycleBin
                )
            {
                let path = tree.reconstruct_path(idx as u32);
                large_files.push(CleanCandidate {
                    id: format!("large:{idx}"),
                    path: path.to_string_lossy().into_owned(),
                    display_name: name.to_string(),
                    category: CleanCategory::LargeOldFile,
                    size_bytes: node.allocated_size,
                    item_count: 1,
                    last_modified_ts: node.last_write_ts,
                    requires_admin: false,
                    warning: None,
                });
            }
        }

        dev_dirs.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
        dev_dirs.truncate(60);
        large_files.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
        large_files.truncate(80);

        candidates.extend(dev_dirs);
        candidates.extend(large_files);
    }

    CleanupCandidatesReport {
        is_elevated: is_elevated(),
        running_browsers,
        recycle_bin_bytes,
        recycle_bin_items,
        candidates,
    }
}
