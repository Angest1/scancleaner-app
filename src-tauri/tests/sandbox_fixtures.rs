use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::TempDir;
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, SetEndOfFile, SetFilePointerEx, FILE_BEGIN, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::FSCTL_SET_SPARSE;
use windows::Win32::System::IO::DeviceIoControl;

use scancleaner_lib::categorizer::{classify_tree, RulesConfig};
use scancleaner_lib::cleaner::executor::{
    build_cleanup_plan, execute_cleanup_plan, CleanupMode, CleanupTargetInput,
    PreviewCleanupRequest,
};
use scancleaner_lib::cleaner::guardrails::SandboxGuard;
use scancleaner_lib::scanner::model::NodeKind;
use scancleaner_lib::scanner::scan_directory;
use scancleaner_lib::storage::read_cleanup_logs;

fn create_sparse_file(path: &std::path::Path, logical_size: i64) -> std::io::Result<()> {
    {
        let _f = File::create(path)?;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        let mut bytes_returned = 0u32;
        DeviceIoControl(
            handle,
            FSCTL_SET_SPARSE,
            None,
            0,
            None,
            0,
            Some(&mut bytes_returned),
            None,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        SetFilePointerEx(handle, logical_size, None, FILE_BEGIN)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        SetEndOfFile(handle).map_err(|e| std::io::Error::other(e.to_string()))?;
        let _ = CloseHandle(handle);
    }
    Ok(())
}

#[test]
fn scanner_handles_hardlinks_sparse_files_and_cyclic_junctions() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // 1. Normal file + real hardlink (should count allocated bytes once)
    let sub_a = root.join("dir_a");
    let sub_b = root.join("dir_b");
    fs::create_dir_all(&sub_a).unwrap();
    fs::create_dir_all(&sub_b).unwrap();

    let payload = vec![0x42u8; 64 * 1024];
    let original = sub_a.join("shared.bin");
    {
        let mut f = File::create(&original).unwrap();
        f.write_all(&payload).unwrap();
        f.sync_all().unwrap();
    }
    let hardlink = sub_b.join("shared_hardlink.bin");
    fs::hard_link(&original, &hardlink).unwrap();

    // 2. Real sparse file (10 MB logical, 0 bytes allocated)
    let sparse_path = sub_a.join("sparse_10mb.dat");
    create_sparse_file(&sparse_path, 10 * 1024 * 1024).unwrap();

    // 3. Cyclic directory junction pointing back to root
    let junction_path = sub_a.join("loop_to_root");
    let status = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &junction_path.to_string_lossy(),
            &root.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "mklink /J failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let cancel = Arc::new(AtomicBool::new(false));
    let mut tree = scan_directory(root.to_path_buf(), cancel, |_| {});
    let rules = RulesConfig::load();
    let breakdown = classify_tree(&mut tree, &rules);

    assert!(!tree.stats.cancelled);
    assert_eq!(
        tree.stats.junctions_skipped, 1,
        "Cyclic junction must be skipped"
    );
    assert_eq!(
        tree.stats.hardlinks_deduplicated, 1,
        "Hardlink must be deduplicated on NTFS"
    );

    // Junction node exists in tree with size 0
    let j_node = tree
        .find_node_by_path(&junction_path)
        .expect("Junction node should exist");
    assert_eq!(tree.kind_of(j_node), NodeKind::Junction);
    assert_eq!(tree.allocated_size[j_node as usize], 0);

    // Sparse file logical size is 10 MB, allocated size must be strictly smaller (0)
    let s_node = tree
        .find_node_by_path(&sparse_path)
        .expect("Sparse node should exist");
    assert_eq!(tree.logical_size[s_node as usize], 10 * 1024 * 1024);
    assert!(
        tree.allocated_size[s_node as usize] < tree.logical_size[s_node as usize],
        "Sparse file allocated size should be < logical size"
    );

    // Breakdown sum matches tree total
    let type_sum: u64 = breakdown.by_type.iter().map(|b| b.allocated_bytes).sum();
    assert_eq!(type_sum, tree.stats.total_allocated_bytes);
}

#[test]
fn cleaner_dry_run_sandbox_guard_and_jsonl_log() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let log_dir = root.join("logs");

    let target_dir = root.join("node_modules");
    fs::create_dir_all(&target_dir).unwrap();
    let disposable = target_dir.join("bundle.js");
    fs::write(&disposable, vec![b'a'; 8192]).unwrap();

    let keep_file = root.join("important.txt");
    fs::write(&keep_file, b"keep me").unwrap();

    let guard = SandboxGuard::new(root);

    let plan = build_cleanup_plan(PreviewCleanupRequest {
        targets: vec![
            CleanupTargetInput {
                path: target_dir.to_string_lossy().into_owned(),
                category: "dev_artifact".to_string(),
                is_temp_dir_contents: false,
            },
            CleanupTargetInput {
                path: r"C:\Windows\System32".to_string(),
                category: "system".to_string(),
                is_temp_dir_contents: false,
            },
        ],
        mode: CleanupMode::Permanent,
    });

    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.blocked_items.len(), 1);
    assert_eq!(plan.total_bytes, 8192);

    let result =
        execute_cleanup_plan(&plan, true, Some(&log_dir), Some(&guard)).expect("cleanup runs");
    assert_eq!(result.succeeded_count, 1);
    assert_eq!(result.freed_bytes, 8192);
    assert!(!target_dir.exists());
    assert!(keep_file.exists());

    let logs = read_cleanup_logs(&log_dir, 20);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status, "deleted_permanent");
    assert_eq!(logs[0].bytes, 8192);
}

#[test]
#[ignore = "Touches real Windows Recycle Bin; run manually when verifying IFileOperation"]
fn manual_real_recycle_bin_test() {
    let tmp = TempDir::new().unwrap();
    let f = tmp.path().join("recycle_me.txt");
    fs::write(&f, b"hello recycle bin").unwrap();

    let plan = build_cleanup_plan(PreviewCleanupRequest {
        targets: vec![CleanupTargetInput {
            path: f.to_string_lossy().into_owned(),
            category: "user_temp".to_string(),
            is_temp_dir_contents: false,
        }],
        mode: CleanupMode::RecycleBin,
    });
    let guard = SandboxGuard::new(tmp.path());
    let log_dir = tmp.path().join("logs");
    let res = execute_cleanup_plan(&plan, false, Some(&log_dir), Some(&guard)).unwrap();
    assert_eq!(res.succeeded_count, 1);
    assert!(!f.exists());
}
