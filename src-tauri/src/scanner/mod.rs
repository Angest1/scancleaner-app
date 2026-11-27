pub mod derived;
pub mod drives;
pub mod fallback_walk;
pub mod mft;
pub mod model;
pub mod treemap_layout;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::elevation::is_elevated;
use derived::DerivedBuildContext;
use drives::{list_drives, query_volume_meta};
use model::{FileSystemType, ScanEngineMode, ScanProgress, ScanTree};

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub include_ads: bool,
    pub prefer_mft: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_ads: false,
            prefer_mft: true,
        }
    }
}

pub fn is_volume_root(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    let trimmed = s.trim();
    (trimmed.len() == 3 && trimmed.as_bytes()[0].is_ascii_alphabetic() && &trimmed[1..3] == r":\")
        || (trimmed.len() == 2
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.ends_with(':'))
}

pub fn scan_directory_with_options<F>(
    root_path: PathBuf,
    options: ScanOptions,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> ScanTree
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let root_str = root_path.to_string_lossy().into_owned();
    let volume_root = if root_str.len() >= 2 && root_str.as_bytes()[1] == b':' {
        format!("{}\\", root_str[..2].to_ascii_uppercase())
    } else {
        root_str.clone()
    };

    let (_, fs_type, _, volume_serial) = query_volume_meta(&volume_root);
    let volume_used_bytes = list_drives(false)
        .into_iter()
        .find(|d| d.root_path.eq_ignore_ascii_case(&volume_root))
        .map(|d| d.used_bytes);

    let ctx = DerivedBuildContext::load_default();

    // Fast path: NTFS volume root + (elevated OR optional indexing service)
    if options.prefer_mft && fs_type == FileSystemType::Ntfs && is_volume_root(&root_path) {
        if is_elevated() {
            match mft::scan_volume_mft_direct(
                root_path.clone(),
                volume_serial,
                options.include_ads,
                cancel_flag.clone(),
                &ctx,
                ScanEngineMode::MftDirect,
                &on_progress,
            ) {
                Ok(mut tree) => {
                    // Record current USN journal metadata so next warm startup applies deltas
                    if let Ok(info) = crate::index::usn::query_usn_journal_for_root(&volume_root) {
                        tree.usn_journal_id = info.usn_journal_id;
                        tree.last_usn = info.next_usn;
                    }
                    return tree;
                }
                Err(e) => {
                    crate::diagnostics::log_warn(&format!(
                        "MFT direct scan fallback for {volume_root}: {e}"
                    ));
                }
            }
        } else if let Ok(tree) = crate::service::client::try_scan_volume_via_service(
            &volume_root,
            volume_serial,
            options.include_ads,
            &ctx,
            &on_progress,
        ) {
            return tree;
        }
    }

    fallback_walk::walk_tree_work_stealing(
        root_path,
        volume_serial,
        fs_type,
        volume_used_bytes,
        cancel_flag,
        &ctx,
        on_progress,
    )
}

pub fn scan_directory<F>(
    root_path: PathBuf,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> ScanTree
where
    F: Fn(ScanProgress) + Send + Sync,
{
    scan_directory_with_options(root_path, ScanOptions::default(), cancel_flag, on_progress)
}
