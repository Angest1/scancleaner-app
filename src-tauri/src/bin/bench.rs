use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use scancleaner_lib::categorizer::RulesConfig;
use scancleaner_lib::cleaner::detectors::discover_cleanup_candidates_from_trees;
use scancleaner_lib::cleaner::guardrails::SandboxGuard;
use scancleaner_lib::deletion::fast_delete::{
    delete_tree_parallel_fast, stage_to_same_volume_trash,
};
use scancleaner_lib::dev_cleaner::scan_dev_cleanup_from_trees;
use scancleaner_lib::dev_cleaner::schema::DevCleanerConfig;
use scancleaner_lib::elevation::is_elevated;
use scancleaner_lib::index::snapshot::{load_volume_snapshot, save_volume_snapshot};
use scancleaner_lib::index::usn::{
    apply_usn_events_to_tree, FileMetaLookup, FileSizeSnapshot, UsnEvent, USN_REASON_CLOSE,
    USN_REASON_FILE_CREATE,
};
use scancleaner_lib::protection::ProtectionEngine;
use scancleaner_lib::scanner::derived::DerivedBuildContext;
use scancleaner_lib::scanner::drives::{list_drives, DriveKind};
use scancleaner_lib::scanner::fallback_walk::{
    reconcile_subtree_fallback, walk_tree_work_stealing,
};
use scancleaner_lib::scanner::mft::parse_mft_record_bytes;
use scancleaner_lib::scanner::model::{FileSystemType, ScanTree};
use scancleaner_lib::scanner::{scan_directory_with_options, ScanOptions};

#[repr(C)]
#[allow(non_snake_case)]
struct ProcessMemoryCounters {
    cb: u32,
    PageFaultCount: u32,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
}

type K32GetProcessMemoryInfoFn = unsafe extern "system" fn(
    process: windows::Win32::Foundation::HANDLE,
    ppsmemcounters: *mut ProcessMemoryCounters,
    cb: u32,
) -> i32;

fn peak_working_set_mb() -> f64 {
    use windows::core::PCSTR;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    use windows::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let Ok(hmod) = GetModuleHandleA(PCSTR(c"kernel32.dll".as_ptr().cast())) else {
            return 0.0;
        };
        let Some(proc) = GetProcAddress(hmod, PCSTR(c"K32GetProcessMemoryInfo".as_ptr().cast()))
        else {
            return 0.0;
        };
        let func: K32GetProcessMemoryInfoFn = std::mem::transmute(proc);
        let mut pmc = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            PageFaultCount: 0,
            PeakWorkingSetSize: 0,
            WorkingSetSize: 0,
            QuotaPeakPagedPoolUsage: 0,
            QuotaPagedPoolUsage: 0,
            QuotaPeakNonPagedPoolUsage: 0,
            QuotaNonPagedPoolUsage: 0,
            PagefileUsage: 0,
            PeakPagefileUsage: 0,
        };
        if func(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
            pmc.PeakWorkingSetSize as f64 / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriveScanBench {
    drive: String,
    fs_type: String,
    mode: String,
    duration_ms: u64,
    snapshot_save_ms: u64,
    snapshot_bytes: u64,
    snapshot_load_ms: u64,
    files: u64,
    dirs: u64,
    allocated_bytes: u64,
    files_per_sec: f64,
    access_denied: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchReport {
    phase: String,
    elevated: bool,
    cpu_threads: usize,
    cold_tti_ms: u64,
    warm_tti_ms: u64,
    incremental_usn_500_events_ms: u64,
    mft_parser_200k_records_ms: u64,
    mft_parser_records_per_sec: f64,
    drive_scans: Vec<DriveScanBench>,
    cleaner_open_ms: u64,
    cleaner_candidates: usize,
    dev_cleaner_open_cold_cache_ms: u64,
    dev_cleaner_open_ms: u64,
    dev_cleaner_candidates: usize,
    delete_50k_files: usize,
    delete_50k_perceived_ms: u64,
    delete_50k_total_ms: u64,
    reconcile_subtree_ms: u64,
    peak_memory_mb: f64,
}

fn create_synthetic_node_modules(base: &Path, total_files: usize) -> Result<PathBuf, String> {
    let nm = base.join("node_modules");
    fs::create_dir_all(&nm).map_err(|e| e.to_string())?;
    let files_per_pkg = 25usize;
    let pkg_count = total_files.div_ceil(files_per_pkg);

    let mut created = 0usize;
    for p in 0..pkg_count {
        let pkg_dir = nm.join(format!("pkg-{p:04}")).join("dist");
        fs::create_dir_all(&pkg_dir).map_err(|e| e.to_string())?;
        for f in 0..files_per_pkg {
            if created >= total_files {
                break;
            }
            let file_path = pkg_dir.join(format!("mod_{f:02}.js"));
            fs::write(&file_path, b"export const v = 1;\n").map_err(|e| e.to_string())?;
            created += 1;
        }
    }
    Ok(nm)
}

fn run_fast_delete_bench(total_files: usize) -> (u64, u64) {
    let bench_root =
        std::env::temp_dir().join(format!("scancleaner-bench-nm-{}", uuid::Uuid::new_v4()));
    let _ = fs::create_dir_all(&bench_root);

    let nm_path = match create_synthetic_node_modules(&bench_root, total_files) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error creating synthetic node_modules: {e}");
            let _ = fs::remove_dir_all(&bench_root);
            return (0, 0);
        }
    };

    let guard = SandboxGuard::new(&bench_root);

    // Phase 1 (User perceived): instant same-volume rename into `.scancleaner-trash`
    let t_perceived = Instant::now();
    let staged = stage_to_same_volume_trash(&nm_path, "bench-job", Some(&guard), Some(&bench_root));
    let perceived_ms = t_perceived.elapsed().as_millis() as u64;

    // Phase 2 (Background worker): parallel POSIX FileDispositionInfoEx deletion
    let total_ms = if let Ok(staged_path) = staged {
        let _ = delete_tree_parallel_fast(&staged_path, None, None, false);
        t_perceived.elapsed().as_millis() as u64
    } else {
        perceived_ms
    };

    let _ = fs::remove_dir_all(&bench_root);
    (perceived_ms, total_ms)
}

fn run_subtree_reconcile_bench(files: usize, ctx: &DerivedBuildContext) -> u64 {
    let bench_root =
        std::env::temp_dir().join(format!("scancleaner-bench-sub-{}", uuid::Uuid::new_v4()));
    let nm = create_synthetic_node_modules(&bench_root, files).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut tree = walk_tree_work_stealing(
        bench_root.clone(),
        0xBEEF,
        FileSystemType::Ntfs,
        None,
        cancel,
        ctx,
        |_| {},
    );
    // Simulate deleting half of the subtree packages and reconciling ONLY `node_modules`
    let sub_del = nm.join("pkg-0000");
    let _ = fs::remove_dir_all(&sub_del);

    let t0 = Instant::now();
    let _ = reconcile_subtree_fallback(&mut tree, &nm, ctx);
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    let _ = fs::remove_dir_all(&bench_root);
    elapsed_ms
}

fn build_sample_mft_record() -> Vec<u8> {
    let mut buf = vec![0u8; 1024];
    buf[0..4].copy_from_slice(b"FILE");
    buf[4..6].copy_from_slice(&48u16.to_le_bytes());
    buf[6..8].copy_from_slice(&3u16.to_le_bytes());
    buf[20..22].copy_from_slice(&56u16.to_le_bytes());
    buf[22..24].copy_from_slice(&1u16.to_le_bytes()); // in_use file

    // $FILE_NAME at offset 56
    let off = 56usize;
    let name: Vec<u16> = "module_bundle_index.js".encode_utf16().collect();
    let val_len = 66 + name.len() * 2;
    let attr_len = (24 + val_len + 7) & !7;
    buf[off..off + 4].copy_from_slice(&0x30u32.to_le_bytes());
    buf[off + 4..off + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    buf[off + 16..off + 20].copy_from_slice(&(val_len as u32).to_le_bytes());
    buf[off + 20..off + 22].copy_from_slice(&24u16.to_le_bytes());
    buf[off + 24..off + 32].copy_from_slice(&5u64.to_le_bytes());
    buf[off + 24 + 64] = name.len() as u8;
    buf[off + 24 + 65] = 1; // Win32
    for (i, ch) in name.iter().enumerate() {
        buf[off + 24 + 66 + i * 2..off + 24 + 68 + i * 2].copy_from_slice(&ch.to_le_bytes());
    }

    // $DATA non-resident
    let d_off = off + attr_len;
    let d_len = 80usize;
    buf[d_off..d_off + 4].copy_from_slice(&0x80u32.to_le_bytes());
    buf[d_off + 4..d_off + 8].copy_from_slice(&(d_len as u32).to_le_bytes());
    buf[d_off + 8] = 1;
    buf[d_off + 32..d_off + 34].copy_from_slice(&64u16.to_le_bytes());
    buf[d_off + 40..d_off + 48].copy_from_slice(&16_384u64.to_le_bytes());
    buf[d_off + 48..d_off + 56].copy_from_slice(&12_345u64.to_le_bytes());
    buf[d_off + 64..d_off + 68].copy_from_slice(&[0x11, 4, 100, 0]);

    let end = d_off + d_len;
    buf[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    // Stamp valid USA fixup
    buf[48] = 0x11;
    buf[49] = 0x22;
    buf[50] = buf[510];
    buf[51] = buf[511];
    buf[510] = 0x11;
    buf[511] = 0x22;
    buf[52] = buf[1022];
    buf[53] = buf[1023];
    buf[1022] = 0x11;
    buf[1023] = 0x22;
    buf
}

fn bench_mft_parser(record_count: usize) -> (u64, f64) {
    let template = build_sample_mft_record();
    let mut raw_block = vec![0u8; record_count * 1024];
    for chunk in raw_block.as_chunks_mut::<1024>().0 {
        chunk.copy_from_slice(&template);
    }

    let t0 = Instant::now();
    let parsed_count: usize = raw_block
        .par_chunks_exact(1024)
        .enumerate()
        .filter_map(|(i, rec)| parse_mft_record_bytes(rec, i as u64 + 100, 512, 4096))
        .count();
    let ms = t0.elapsed().as_millis().max(1) as u64;
    let rps = (parsed_count as f64) / (t0.elapsed().as_secs_f64().max(1e-6));
    (ms, rps)
}

struct BenchMetaLookup;
impl FileMetaLookup for BenchMetaLookup {
    fn lookup_by_path(&self, _path: &Path) -> Option<FileSizeSnapshot> {
        Some(FileSizeSnapshot {
            logical_size: 4096,
            allocated_size: 4096,
            last_write_ts: 1_700_000_000,
            is_dir: false,
        })
    }
}

fn bench_usn_incremental(tree: &mut ScanTree, ctx: &DerivedBuildContext) -> u64 {
    let root_frn = tree.file_ref[0];
    let events: Vec<UsnEvent> = (0..500u64)
        .map(|i| UsnEvent {
            usn: 1000 + (i as i64) * 10,
            frn: 90_000_000 + i,
            parent_frn: root_frn,
            reason: USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
            file_attributes: 0x20,
            file_name: format!("usn_delta_{i}.tmp"),
            timestamp_unix: 1_700_000_000,
        })
        .collect();
    let t0 = Instant::now();
    let _ = apply_usn_events_to_tree(tree, &events, &BenchMetaLookup, ctx);
    t0.elapsed().as_millis() as u64
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ci_mode = args.iter().any(|a| a == "--ci");
    let delete_files = if ci_mode { 10_000 } else { 52_000 };
    let mft_records = if ci_mode { 50_000 } else { 200_000 };

    let elevated = is_elevated();
    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let ctx = DerivedBuildContext::new(RulesConfig::load(), ProtectionEngine::load_default(), None);
    let snap_dir =
        std::env::temp_dir().join(format!("scancleaner-bench-idx-{}", uuid::Uuid::new_v4()));
    let _ = fs::create_dir_all(&snap_dir);

    let mut drive_scans = Vec::new();
    let mut trees: Vec<ScanTree> = Vec::new();
    let mut total_cold_ms = 0u64;
    let mut total_warm_ms = 0u64;

    if ci_mode {
        let synth_root =
            std::env::temp_dir().join(format!("scancleaner-bench-ci-{}", uuid::Uuid::new_v4()));
        let _ = create_synthetic_node_modules(&synth_root, 15_000);
        let t0 = Instant::now();
        let cancel = Arc::new(AtomicBool::new(false));
        let tree = walk_tree_work_stealing(
            synth_root.clone(),
            0xC100,
            FileSystemType::Ntfs,
            None,
            cancel,
            &ctx,
            |_| {},
        );
        let dur = t0.elapsed().as_millis() as u64;
        total_cold_ms += dur;

        let ts0 = Instant::now();
        let snap_path = save_volume_snapshot(&tree, &snap_dir).unwrap();
        let save_ms = ts0.elapsed().as_millis() as u64;
        let snap_bytes = fs::metadata(&snap_path).map(|m| m.len()).unwrap_or(0);

        let tl0 = Instant::now();
        let (_loaded, _) = load_volume_snapshot(&snap_path, Some(0xC100)).unwrap();
        let load_ms = tl0.elapsed().as_millis() as u64;
        total_warm_ms += load_ms;

        let fps = if dur > 0 {
            (tree.stats.total_files as f64) / (dur as f64 / 1000.0)
        } else {
            0.0
        };
        drive_scans.push(DriveScanBench {
            drive: "SYNTH-CI".to_string(),
            fs_type: format!("{:?}", tree.fs_type),
            mode: format!("{:?}", tree.stats.engine_mode),
            duration_ms: dur,
            snapshot_save_ms: save_ms,
            snapshot_bytes: snap_bytes,
            snapshot_load_ms: load_ms,
            files: tree.stats.total_files,
            dirs: tree.stats.total_dirs,
            allocated_bytes: tree.stats.total_allocated_bytes,
            files_per_sec: fps,
            access_denied: tree.stats.access_denied_count,
        });
        trees.push(tree);
        let _ = fs::remove_dir_all(&synth_root);
    } else {
        let drives: Vec<_> = list_drives(false)
            .into_iter()
            .filter(|d| {
                d.available
                    && matches!(
                        d.kind,
                        DriveKind::Fixed | DriveKind::Removable | DriveKind::RamDisk
                    )
            })
            .collect();

        let mut saved_snaps: Vec<(PathBuf, u32, usize)> = Vec::new();
        for d in drives {
            eprintln!("[bench] Scanning {} ({})...", d.root_path, d.fs_name);
            let t0 = Instant::now();
            let cancel = Arc::new(AtomicBool::new(false));
            let tree = scan_directory_with_options(
                PathBuf::from(&d.root_path),
                ScanOptions {
                    include_ads: false,
                    prefer_mft: true,
                },
                cancel,
                |_| {},
            );
            let dur = t0.elapsed().as_millis() as u64;
            total_cold_ms += dur;

            let ts0 = Instant::now();
            let snap_path = save_volume_snapshot(&tree, &snap_dir).unwrap();
            let save_ms = ts0.elapsed().as_millis() as u64;
            let snap_bytes = fs::metadata(&snap_path).map(|m| m.len()).unwrap_or(0);

            let fps = if dur > 0 {
                (tree.stats.total_files as f64) / (dur as f64 / 1000.0)
            } else {
                0.0
            };
            let idx = drive_scans.len();
            drive_scans.push(DriveScanBench {
                drive: d.root_path.clone(),
                fs_type: d.fs_name.clone(),
                mode: format!("{:?}", tree.stats.engine_mode),
                duration_ms: dur,
                snapshot_save_ms: save_ms,
                snapshot_bytes: snap_bytes,
                snapshot_load_ms: 0,
                files: tree.stats.total_files,
                dirs: tree.stats.total_dirs,
                allocated_bytes: tree.stats.total_allocated_bytes,
                files_per_sec: fps,
                access_denied: tree.stats.access_denied_count,
            });
            saved_snaps.push((snap_path, tree.volume_serial, idx));
            trees.push(tree);
        }

        // Parallel warm startup load across all 3 saved volume snapshots (mmap + LZ4)
        let t_warm = Instant::now();
        let per_drive_loads: Vec<(usize, u64)> = saved_snaps
            .par_iter()
            .map(|(p, serial, idx)| {
                let tl0 = Instant::now();
                let (_loaded, _) = load_volume_snapshot(p, Some(*serial)).unwrap();
                (*idx, tl0.elapsed().as_millis() as u64)
            })
            .collect();
        total_warm_ms = t_warm.elapsed().as_millis() as u64;
        for (idx, ms) in per_drive_loads {
            drive_scans[idx].snapshot_load_ms = ms;
        }
    }

    eprintln!(
        "[bench] Measuring instant Cleaner & Dev Cleaner open latency from precomputed indexes..."
    );
    let tree_refs: Vec<&ScanTree> = trees.iter().collect();
    let t_clean = Instant::now();
    let clean_report = discover_cleanup_candidates_from_trees(&tree_refs, 100, 30);
    let cleaner_open_ms = t_clean.elapsed().as_millis() as u64;

    let dev_cfg = DevCleanerConfig::load();
    let t_dev_cold = Instant::now();
    let _dev_cold = scan_dev_cleanup_from_trees(&dev_cfg, &tree_refs).ok();
    let dev_cleaner_open_cold_cache_ms = t_dev_cold.elapsed().as_millis() as u64;

    // Second open uses warm tool/git cache (steady-state UX)
    let t_dev = Instant::now();
    let dev_report = scan_dev_cleanup_from_trees(&dev_cfg, &tree_refs).ok();
    let dev_cleaner_open_ms = t_dev.elapsed().as_millis() as u64;
    let dev_cleaner_candidates = dev_report.as_ref().map_or(0, |r| r.candidates.len());

    eprintln!("[bench] Measuring incremental USN 500-event delta apply...");
    let incremental_usn_500_events_ms = if let Some(first) = trees.first_mut() {
        bench_usn_incremental(first, &ctx)
    } else {
        0
    };

    drop(trees);

    eprintln!("[bench] Measuring MFT binary record parser throughput ({mft_records} records)...");
    let (mft_parser_200k_records_ms, mft_parser_records_per_sec) = bench_mft_parser(mft_records);

    eprintln!("[bench] Measuring {delete_files}-file synthetic node_modules deletion (rename + parallel POSIX)...");
    let (delete_perceived_ms, delete_total_ms) = run_fast_delete_bench(delete_files);

    eprintln!("[bench] Measuring subtree reconciliation (5,000 files)...");
    let reconcile_subtree_ms = run_subtree_reconcile_bench(5_000, &ctx);

    let _ = fs::remove_dir_all(&snap_dir);
    let peak_memory_mb = peak_working_set_mb();

    let report = BenchReport {
        phase: "final".to_string(),
        elevated,
        cpu_threads,
        cold_tti_ms: total_cold_ms,
        warm_tti_ms: total_warm_ms,
        incremental_usn_500_events_ms,
        mft_parser_200k_records_ms,
        mft_parser_records_per_sec,
        drive_scans,
        cleaner_open_ms,
        cleaner_candidates: clean_report.candidates.len(),
        dev_cleaner_open_cold_cache_ms,
        dev_cleaner_open_ms,
        dev_cleaner_candidates,
        delete_50k_files: delete_files,
        delete_50k_perceived_ms: delete_perceived_ms,
        delete_50k_total_ms: delete_total_ms,
        reconcile_subtree_ms,
        peak_memory_mb,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_default()
    );
    let _ = HashMap::<String, u64>::new();
}
