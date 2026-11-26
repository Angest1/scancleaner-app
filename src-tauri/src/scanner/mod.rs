pub mod drives;
pub mod model;
pub mod win_walk;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use drives::query_volume_meta;
use model::{ScanProgress, ScanTree};

pub fn scan_directory<F>(
    root_path: PathBuf,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> ScanTree
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let root_str = root_path.to_string_lossy();
    let volume_root = if root_str.len() >= 3 && root_str.as_bytes()[1] == b':' {
        format!("{}\\", &root_str[..2])
    } else {
        root_str.into_owned()
    };
    let (_, fs_type, _, volume_serial) = query_volume_meta(&volume_root);
    win_walk::walk_tree(root_path, volume_serial, fs_type, cancel_flag, on_progress)
}
