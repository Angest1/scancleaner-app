pub mod snapshot;
pub mod usn;
pub mod watcher;

use std::path::{Path, PathBuf};

/// Returns `%LOCALAPPDATA%\ScanCleaner`.
pub fn default_app_local_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join("ScanCleaner")
    } else {
        std::env::temp_dir().join("ScanCleaner")
    }
}

/// Returns `%LOCALAPPDATA%\ScanCleaner\index`.
pub fn default_index_dir() -> PathBuf {
    default_app_local_dir().join("index")
}

/// Returns true if `path` resides inside the ScanCleaner index directory.
/// Used by watchers and the reconciliation queue to guarantee zero self-triggering loops.
pub fn is_inside_index_dir(path: &Path) -> bool {
    is_inside_dir_prefix(path, &default_index_dir())
}

/// Returns true if `path` should be ignored by continuous `ReadDirectoryChangesW` watchers
/// to avoid self-triggering loops (WebView2 cache, logs, `.scidx` index) and noisy OS telemetry churn.
pub fn is_watcher_ignored_path(path: &Path) -> bool {
    if is_inside_dir_prefix(path, &default_app_local_dir())
        || is_inside_dir_prefix(path, &crate::storage::app_data_dir())
    {
        return true;
    }
    let norm = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    const NOISY_SUBSTRINGS: &[&str] = &[
        r"\system volume information",
        r"\windows\prefetch",
        r"\windows\logs",
        r"\windows\system32\winevt",
        r"\windows\system32\config",
        r"\windows\system32\sru",
        r"\windows\servicing",
        r"\windows\softwaredistribution",
        r"\programdata\microsoft\windows defender",
        r"\programdata\microsoft\search",
        r"\programdata\microsoft\diagnosis",
        r"\appdata\local\microsoft\edge\user data",
        r"\appdata\local\google\chrome\user data",
        r"\$extend",
        r"\$logfile",
        r"\pagefile.sys",
        r"\swapfile.sys",
        r"\hiberfil.sys",
    ];
    NOISY_SUBSTRINGS.iter().any(|s| norm.contains(s))
}

pub fn is_inside_dir_prefix(path: &Path, prefix_dir: &Path) -> bool {
    let p_norm = path
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    let idx_norm = prefix_dir
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    if idx_norm.is_empty() {
        return false;
    }
    p_norm == idx_norm || p_norm.starts_with(&format!(r"{idx_norm}\"))
}
