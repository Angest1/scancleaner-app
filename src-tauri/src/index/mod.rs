pub mod snapshot;
pub mod usn;
pub mod watcher;

use std::path::{Path, PathBuf};

/// Returns `%LOCALAPPDATA%\ScanCleaner\index`.
pub fn default_index_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local).join("ScanCleaner").join("index")
    } else {
        std::env::temp_dir().join("ScanCleaner").join("index")
    }
}

/// Returns true if `path` resides inside the ScanCleaner index directory.
/// Used by watchers and the reconciliation queue to guarantee zero self-triggering loops.
pub fn is_inside_index_dir(path: &Path) -> bool {
    is_inside_dir_prefix(path, &default_index_dir())
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
