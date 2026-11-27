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
use crate::scanner::model::{NodeFlags, NodeKind, OriginCategory, ScanTree};

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

/// Instant, zero-disk-scan candidate builder from precomputed `DerivedIndexes` across all indexed volumes.
/// Falls back to bounded disk probe ONLY when no indexed volume is loaded yet.
pub fn discover_cleanup_candidates_from_trees(
    trees: &[&ScanTree],
    min_large_mb: u64,
    min_age_days: u64,
) -> CleanupCandidatesReport {
    let running_browsers = detect_running_browsers();
    let (recycle_bin_bytes, recycle_bin_items) = query_recycle_bin();
    let elevated = is_elevated();

    if trees.is_empty() {
        // Fallback when no volume snapshot/index is available yet
        return discover_cleanup_candidates_disk_fallback(
            &running_browsers,
            recycle_bin_bytes,
            recycle_bin_items,
            elevated,
        );
    }

    let now_ts = chrono::Utc::now().timestamp();
    let seven_days_secs = 7 * 86_400;
    let min_large_bytes = min_large_mb.saturating_mul(1024 * 1024);
    let min_age_secs = (min_age_days as i64).saturating_mul(86_400);

    let mut candidates: Vec<CleanCandidate> = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut dev_dirs: Vec<CleanCandidate> = Vec::new();
    let mut large_files: Vec<CleanCandidate> = Vec::new();

    for &tree in trees {
        // 1. Precomputed global caches (UserTemp, SystemTemp, Browser caches, npm/pip/pnpm/composer)
        for cache in &tree.derived.global_caches {
            let idx = cache.node_id as usize;
            if idx >= tree.len() {
                continue;
            }
            let flags = tree.flags_of(cache.node_id);
            if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
            {
                continue;
            }
            let size = tree.allocated_size[idx];
            let items = tree.file_count[idx] as u64;
            if size == 0 && items == 0 {
                continue;
            }

            let cat = match cache.category.as_str() {
                "user_temp" => CleanCategory::UserTemp,
                "system_temp" => CleanCategory::SystemTemp,
                "browser_cache" => CleanCategory::BrowserCache,
                "dev_artifact" => CleanCategory::DevArtifact,
                _ => continue,
            };

            let path = tree
                .reconstruct_path(cache.node_id)
                .to_string_lossy()
                .into_owned();
            let norm = path.to_ascii_lowercase();
            if !seen_paths.insert(norm) {
                continue;
            }

            let requires_admin = cat == CleanCategory::SystemTemp;
            let warning = if requires_admin && !elevated {
                Some("Requiere permisos de administrador para limpiar.".to_string())
            } else if cat == CleanCategory::BrowserCache {
                running_browsers
                    .iter()
                    .find(|b| cache.label.starts_with(b.as_str()))
                    .map(|b| format!("{b} está abierto; ciérralo para liberar todos los archivos."))
            } else {
                None
            };

            candidates.push(CleanCandidate {
                id: cache.cache_key.clone(),
                path,
                display_name: cache.label.clone(),
                category: cat,
                size_bytes: size,
                item_count: items.max(1),
                last_modified_ts: tree.last_write_ts_of(cache.node_id),
                requires_admin,
                warning,
            });
        }

        // 2. Precomputed Dev artifacts from single-pass derived index
        for art in &tree.derived.dev_artifacts {
            let idx = art.node_id as usize;
            if idx >= tree.len() {
                continue;
            }
            let flags = tree.flags_of(art.node_id);
            if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
            {
                continue;
            }
            let size = tree.allocated_size[idx];
            if size < 1024 * 1024 {
                continue;
            }
            let path = tree
                .reconstruct_path(art.node_id)
                .to_string_lossy()
                .into_owned();
            if !seen_paths.insert(path.to_ascii_lowercase()) {
                continue;
            }
            let ts = tree.last_write_ts_of(art.node_id);
            let recent = now_ts - ts < seven_days_secs;
            dev_dirs.push(CleanCandidate {
                id: format!("devdir:{}:{}", tree.volume_serial, art.node_id),
                path,
                display_name: format!("Artefacto {}", tree.node_name(art.node_id)),
                category: CleanCategory::DevArtifact,
                size_bytes: size,
                item_count: tree.file_count[idx] as u64,
                last_modified_ts: ts,
                requires_admin: false,
                warning: if recent {
                    Some("Modificado hace menos de 7 días (proyecto activo).".to_string())
                } else {
                    None
                },
            });
        }

        // 3. Precomputed Top-N largest files (O(TOP_N) instead of scanning millions of nodes!)
        for &nid in &tree.derived.top_files {
            let idx = nid as usize;
            if idx >= tree.len() {
                continue;
            }
            let flags = tree.flags_of(nid);
            if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
            {
                continue;
            }
            if tree.kind_of(nid) != NodeKind::File {
                continue;
            }
            let alloc = tree.allocated_size[idx];
            let ts = tree.last_write_ts_of(nid);
            if alloc < min_large_bytes || ts <= 0 || (now_ts - ts) < min_age_secs {
                continue;
            }
            if matches!(
                tree.origin_cat_of(nid),
                OriginCategory::System | OriginCategory::InstalledApps | OriginCategory::RecycleBin
            ) {
                continue;
            }
            let path = tree.reconstruct_path(nid).to_string_lossy().into_owned();
            if !seen_paths.insert(path.to_ascii_lowercase()) {
                continue;
            }
            large_files.push(CleanCandidate {
                id: format!("large:{}:{nid}", tree.volume_serial),
                path,
                display_name: tree.node_name(nid).to_string(),
                category: CleanCategory::LargeOldFile,
                size_bytes: alloc,
                item_count: 1,
                last_modified_ts: ts,
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

    CleanupCandidatesReport {
        is_elevated: elevated,
        running_browsers,
        recycle_bin_bytes,
        recycle_bin_items,
        candidates,
    }
}

pub fn discover_cleanup_candidates(
    scan_tree: Option<&ScanTree>,
    min_large_mb: u64,
    min_age_days: u64,
) -> CleanupCandidatesReport {
    match scan_tree {
        Some(t) => discover_cleanup_candidates_from_trees(&[t], min_large_mb, min_age_days),
        None => discover_cleanup_candidates_from_trees(&[], min_large_mb, min_age_days),
    }
}

fn discover_cleanup_candidates_disk_fallback(
    running_browsers: &[String],
    recycle_bin_bytes: u64,
    recycle_bin_items: u64,
    elevated: bool,
) -> CleanupCandidatesReport {
    let mut candidates = Vec::new();
    if let Ok(user_temp) = std::env::var("TEMP") {
        let p = Path::new(&user_temp);
        let (bytes, count, ts) = dir_stats_bounded(p, 2_000);
        if count > 0 {
            candidates.push(CleanCandidate {
                id: "temp:UserTemp".to_string(),
                path: user_temp,
                display_name: "Archivos temporales del usuario (%TEMP%)".to_string(),
                category: CleanCategory::UserTemp,
                size_bytes: bytes,
                item_count: count,
                last_modified_ts: ts,
                requires_admin: false,
                warning: None,
            });
        }
    }
    let win_temp = std::env::var("SystemRoot")
        .map(|s| PathBuf::from(s).join("Temp"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\Temp"));
    let (bytes, count, ts) = dir_stats_bounded(&win_temp, 2_000);
    if count > 0 {
        candidates.push(CleanCandidate {
            id: "temp:SystemTemp".to_string(),
            path: win_temp.to_string_lossy().into_owned(),
            display_name: "Archivos temporales del sistema (C:\\Windows\\Temp)".to_string(),
            category: CleanCategory::SystemTemp,
            size_bytes: bytes,
            item_count: count,
            last_modified_ts: ts,
            requires_admin: true,
            warning: if !elevated {
                Some("Requiere permisos de administrador para limpiar.".to_string())
            } else {
                None
            },
        });
    }

    CleanupCandidatesReport {
        is_elevated: elevated,
        running_browsers: running_browsers.to_vec(),
        recycle_bin_bytes,
        recycle_bin_items,
        candidates,
    }
}
