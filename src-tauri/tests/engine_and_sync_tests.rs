use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use scancleaner_lib::categorizer::RulesConfig;
use scancleaner_lib::cleaner::guardrails::SandboxGuard;
use scancleaner_lib::deletion::fast_delete::{
    delete_tree_parallel_fast, stage_to_same_volume_trash,
};
use scancleaner_lib::index::snapshot::{load_volume_snapshot, save_volume_snapshot};
use scancleaner_lib::index::usn::{
    apply_usn_events_to_tree, sync_tree_with_usn, FileMetaLookup, FileSizeSnapshot, UsnEvent,
    UsnJournalInfo, UsnJournalReader, UsnSyncOutcome, USN_REASON_CLOSE, USN_REASON_DATA_EXTEND,
    USN_REASON_FILE_CREATE, USN_REASON_FILE_DELETE, USN_REASON_RENAME_NEW_NAME,
    USN_REASON_RENAME_OLD_NAME,
};
use scancleaner_lib::protection::ProtectionEngine;
use scancleaner_lib::reconcile::queue::{
    coalesce_paths, ReconcileOrigin, ReconcilePriority, ReconcileRequest, ReconciliationQueue,
};
use scancleaner_lib::reconcile::triggers::{
    decode_device_unitmask, evaluate_free_space_drift, measure_real_freed_space,
    watch_native_uninstall_completion, DiskSpaceProvider, UninstallPresenceProbe,
};
use scancleaner_lib::scanner::derived::DerivedBuildContext;
use scancleaner_lib::scanner::fallback_walk::{
    reconcile_subtree_fallback, reconcile_watcher_path, walk_tree_work_stealing,
};
use scancleaner_lib::scanner::mft::{
    build_tree_from_mft_records, parse_data_runs, parse_mft_record_bytes, ParsedMftRecord,
};
use scancleaner_lib::scanner::model::{FileSystemType, NodeFlags, NodeKind, ScanEngineMode};
use scancleaner_lib::service::protocol::{
    build_pipe_sddl, decode_request_frame, encode_frame, validate_request, ServiceRequest,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Synthetic NTFS MFT binary record builder (1024-byte FILE records w/ USA fixup)
// ---------------------------------------------------------------------------

struct MftRecordBuilder {
    buf: Vec<u8>,
    attr_cursor: usize,
}

impl MftRecordBuilder {
    fn new(in_use: bool, is_dir: bool, base_frn: u64) -> Self {
        let mut buf = vec![0u8; 1024];
        buf[0..4].copy_from_slice(b"FILE");
        // USA offset = 48, USA count = 3 (1 USN word + 2 sector fixup words for 2x512B sectors)
        buf[4..6].copy_from_slice(&48u16.to_le_bytes());
        buf[6..8].copy_from_slice(&3u16.to_le_bytes());
        // First attribute offset = 56 (8-byte aligned after USA)
        buf[20..22].copy_from_slice(&56u16.to_le_bytes());
        let mut flags = 0u16;
        if in_use {
            flags |= 0x0001;
        }
        if is_dir {
            flags |= 0x0002;
        }
        buf[22..24].copy_from_slice(&flags.to_le_bytes());
        buf[32..40].copy_from_slice(&base_frn.to_le_bytes());
        Self {
            buf,
            attr_cursor: 56,
        }
    }

    fn push_resident_attr(&mut self, attr_type: u32, name_utf16: &[u16], value: &[u8]) {
        let name_off = 24usize;
        let name_bytes = name_utf16.len() * 2;
        let val_off = (name_off + name_bytes + 7) & !7;
        let attr_len = (val_off + value.len() + 7) & !7;
        let off = self.attr_cursor;
        assert!(off + attr_len + 4 <= self.buf.len(), "MFT record overflow");

        self.buf[off..off + 4].copy_from_slice(&attr_type.to_le_bytes());
        self.buf[off + 4..off + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        self.buf[off + 8] = 0; // resident
        self.buf[off + 9] = name_utf16.len() as u8;
        self.buf[off + 10..off + 12].copy_from_slice(&(name_off as u16).to_le_bytes());
        self.buf[off + 16..off + 20].copy_from_slice(&(value.len() as u32).to_le_bytes());
        self.buf[off + 20..off + 22].copy_from_slice(&(val_off as u16).to_le_bytes());
        for (i, ch) in name_utf16.iter().enumerate() {
            let p = off + name_off + i * 2;
            self.buf[p..p + 2].copy_from_slice(&ch.to_le_bytes());
        }
        self.buf[off + val_off..off + val_off + value.len()].copy_from_slice(value);
        self.attr_cursor += attr_len;
    }

    #[allow(clippy::too_many_arguments)]
    fn push_non_resident_data(
        &mut self,
        name: Option<&str>,
        attr_flags: u16,
        start_vcn: u64,
        alloc_size: u64,
        real_size: u64,
        data_runs: &[u8],
    ) {
        let name_utf16: Vec<u16> = name.map(|s| s.encode_utf16().collect()).unwrap_or_default();
        let name_off = 64usize;
        let run_off = (name_off + name_utf16.len() * 2 + 7) & !7;
        let attr_len = (run_off + data_runs.len() + 1 + 7) & !7;
        let off = self.attr_cursor;
        assert!(off + attr_len + 4 <= self.buf.len(), "MFT record overflow");

        self.buf[off..off + 4].copy_from_slice(&0x80u32.to_le_bytes());
        self.buf[off + 4..off + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        self.buf[off + 8] = 1; // non-resident
        self.buf[off + 9] = name_utf16.len() as u8;
        self.buf[off + 10..off + 12].copy_from_slice(&(name_off as u16).to_le_bytes());
        self.buf[off + 12..off + 14].copy_from_slice(&attr_flags.to_le_bytes());
        self.buf[off + 16..off + 24].copy_from_slice(&start_vcn.to_le_bytes());
        self.buf[off + 32..off + 34].copy_from_slice(&(run_off as u16).to_le_bytes());
        self.buf[off + 40..off + 48].copy_from_slice(&alloc_size.to_le_bytes());
        self.buf[off + 48..off + 56].copy_from_slice(&real_size.to_le_bytes());
        for (i, ch) in name_utf16.iter().enumerate() {
            let p = off + name_off + i * 2;
            self.buf[p..p + 2].copy_from_slice(&ch.to_le_bytes());
        }
        self.buf[off + run_off..off + run_off + data_runs.len()].copy_from_slice(data_runs);
        self.buf[off + run_off + data_runs.len()] = 0; // run terminator
        self.attr_cursor += attr_len;
    }

    fn add_standard_info(&mut self, attrs: u32, mtime_ft: i64) {
        let mut val = vec![0u8; 48];
        val[8..16].copy_from_slice(&mtime_ft.to_le_bytes());
        val[32..36].copy_from_slice(&attrs.to_le_bytes());
        self.push_resident_attr(0x10, &[], &val);
    }

    fn add_file_name(&mut self, parent_frn: u64, namespace: u8, name: &str, stale_size: u64) {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let mut val = vec![0u8; 66 + utf16.len() * 2];
        val[0..8].copy_from_slice(&parent_frn.to_le_bytes());
        // Intentionally store stale sizes in $FILE_NAME to verify parser ignores them in favor of $DATA
        val[40..48].copy_from_slice(&stale_size.to_le_bytes());
        val[48..56].copy_from_slice(&stale_size.to_le_bytes());
        val[64] = utf16.len() as u8;
        val[65] = namespace;
        for (i, ch) in utf16.iter().enumerate() {
            val[66 + i * 2..68 + i * 2].copy_from_slice(&ch.to_le_bytes());
        }
        self.push_resident_attr(0x30, &[], &val);
    }

    fn add_reparse_point(&mut self, tag: u32) {
        let val = tag.to_le_bytes();
        self.push_resident_attr(0xC0, &[], &val);
    }

    fn add_attribute_list(&mut self, ext_frns: &[u64]) {
        let mut val = Vec::new();
        for &frn in ext_frns {
            let mut entry = vec![0u8; 32];
            entry[0..4].copy_from_slice(&0x80u32.to_le_bytes());
            entry[4..6].copy_from_slice(&32u16.to_le_bytes());
            entry[16..24].copy_from_slice(&frn.to_le_bytes());
            val.extend_from_slice(&entry);
        }
        self.push_resident_attr(0x20, &[], &val);
    }

    fn finish(mut self) -> Vec<u8> {
        self.buf[self.attr_cursor..self.attr_cursor + 4]
            .copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        // Apply NTFS USA fixup encoding (USN = 0x5A3C)
        let usn = [0x3Cu8, 0x5Au8];
        self.buf[48] = usn[0];
        self.buf[49] = usn[1];
        // Save real end-of-sector bytes into USA and stamp USN
        self.buf[50] = self.buf[510];
        self.buf[51] = self.buf[511];
        self.buf[510] = usn[0];
        self.buf[511] = usn[1];

        self.buf[52] = self.buf[1022];
        self.buf[53] = self.buf[1023];
        self.buf[1022] = usn[0];
        self.buf[1023] = usn[1];
        self.buf
    }
}

fn default_ctx() -> DerivedBuildContext {
    DerivedBuildContext::new(RulesConfig::load(), ProtectionEngine::load_default(), None)
}

// ---------------------------------------------------------------------------
// 1. MFT PARSER TESTS
// ---------------------------------------------------------------------------

#[test]
fn mft_parser_handles_win32_vs_dos_data_runs_sparse_hardlinks_ads_attrlist_cloud_and_reparse() {
    let ctx = default_ctx();
    let cluster = 4096u64;
    let mtime_ft = 133_800_000_000_000_000i64; // valid post-1970 FILETIME

    // FRN 5: Root directory
    let mut r_root = MftRecordBuilder::new(true, true, 0);
    r_root.add_standard_info(0x10, mtime_ft);
    r_root.add_file_name(5, 3, ".", 0);

    // FRN 40: Dir "DirA"
    let mut r_dir_a = MftRecordBuilder::new(true, true, 0);
    r_dir_a.add_standard_info(0x10, mtime_ft);
    r_dir_a.add_file_name(5, 1, "DirA", 0);

    // FRN 41: Dir "DirB"
    let mut r_dir_b = MftRecordBuilder::new(true, true, 0);
    r_dir_b.add_standard_info(0x10, mtime_ft);
    r_dir_b.add_file_name(5, 1, "DirB", 0);

    // FRN 50: File with DOS 8.3 + Unicode Win32 name, stale $FILE_NAME size (999_999),
    // real $DATA size (12_000 logical, 16_384 allocated) + ADS ":Zone.Identifier" (512B logical, 4096B alloc)
    let mut r_unicode = MftRecordBuilder::new(true, false, 0);
    r_unicode.add_standard_info(0x20, mtime_ft);
    r_unicode.add_file_name(40, 2, "PROYEC~1.BIN", 999_999);
    r_unicode.add_file_name(40, 1, "proyecto_日本語_🚀.bin", 999_999);
    // Data run: 4 clusters at LCN 100 -> header 0x11, len=4, lcn=100
    r_unicode.push_non_resident_data(None, 0, 0, 16_384, 12_000, &[0x11, 4, 100]);
    r_unicode.push_non_resident_data(Some("Zone.Identifier"), 0, 0, 4096, 512, &[0x11, 1, 110]);

    // FRN 51: Sparse file (logical 100 MiB, 2 real clusters = 8192 bytes + 100 sparse hole clusters)
    // Run 1: 2 clusters @ LCN 200 (2-byte signed LCN -> 0x21, 2, 200, 0); Run 2: 100 sparse clusters (0x01, 100)
    let mut r_sparse = MftRecordBuilder::new(true, false, 0);
    r_sparse.add_standard_info(0x20 | 0x200, mtime_ft); // FILE_ATTRIBUTE_SPARSE_FILE = 0x200
    r_sparse.add_file_name(40, 1, "sparse_vm.img", 0);
    r_sparse.push_non_resident_data(
        None,
        0x8000, // sparse attr flag
        0,
        104_857_600,
        104_857_600,
        &[0x21, 2, 200, 0, 0x01, 100],
    );

    // FRN 52: Hardlinked file appearing in BOTH DirA and DirB -> must be counted ONCE!
    let mut r_hardlink = MftRecordBuilder::new(true, false, 0);
    r_hardlink.add_standard_info(0x20, mtime_ft);
    r_hardlink.add_file_name(40, 1, "shared.dll", 0);
    r_hardlink.add_file_name(41, 1, "shared_link.dll", 0);
    r_hardlink.push_non_resident_data(None, 0, 0, 32_768, 30_000, &[0x11, 8, 50]);

    // FRN 53 + Extension FRN 54 ($ATTRIBUTE_LIST): base has name + attr list, ext has non-resident $DATA
    let mut r_ext_base = MftRecordBuilder::new(true, false, 0);
    r_ext_base.add_standard_info(0x20, mtime_ft);
    r_ext_base.add_attribute_list(&[54]);
    r_ext_base.add_file_name(41, 1, "extended_attr.dat", 0);

    let mut r_ext_child = MftRecordBuilder::new(true, false, 53);
    r_ext_child.push_non_resident_data(None, 0, 0, 24_576, 20_000, &[0x11, 6, 80]);

    // FRN 55: OneDrive cloud placeholder (RECALL_ON_DATA_ACCESS = 0x0040_0000 | OFFLINE = 0x1000)
    let mut r_cloud = MftRecordBuilder::new(true, false, 0);
    r_cloud.add_standard_info(0x0040_0000 | 0x1000, mtime_ft);
    r_cloud.add_file_name(41, 1, "onedrive_video.mp4", 0);
    // 50 MB logical in cloud, 0 bytes sparse locally
    r_cloud.push_non_resident_data(None, 0x8000, 0, 0, 50_000_000, &[0x01, 100]);

    // FRN 56: Junction pointing elsewhere (must be skipped as traversal parent)
    let mut r_junction = MftRecordBuilder::new(true, true, 0);
    r_junction.add_standard_info(0x10 | 0x400, mtime_ft);
    r_junction.add_file_name(40, 1, "junction_link", 0);
    r_junction.add_reparse_point(0xA000_0003);

    // FRN 57: Symlink
    let mut r_symlink = MftRecordBuilder::new(true, false, 0);
    r_symlink.add_standard_info(0x400, mtime_ft);
    r_symlink.add_file_name(40, 1, "sym.lnk", 0);
    r_symlink.add_reparse_point(0xA000_000C);

    // FRN 58: Corrupt/torn USA sector -> must return None safely
    let mut torn = r_dir_a.finish();
    let dir_a_bytes = torn.clone();
    torn[510] ^= 0xFF;
    assert!(parse_mft_record_bytes(&torn, 58, 512, cluster).is_none());

    let raw_pairs: Vec<(u64, Vec<u8>)> = vec![
        (5, r_root.finish()),
        (40, dir_a_bytes),
        (41, r_dir_b.finish()),
        (50, r_unicode.finish()),
        (51, r_sparse.finish()),
        (52, r_hardlink.finish()),
        (53, r_ext_base.finish()),
        (54, r_ext_child.finish()),
        (55, r_cloud.finish()),
        (56, r_junction.finish()),
        (57, r_symlink.finish()),
    ];

    let parsed: Vec<ParsedMftRecord> = raw_pairs
        .iter()
        .filter_map(|(frn, bytes)| parse_mft_record_bytes(bytes, *frn, 512, cluster))
        .collect();
    assert_eq!(parsed.len(), 11);

    // Case A: Default (include_ads = false)
    let tree = build_tree_from_mft_records(
        PathBuf::from(r"C:\"),
        0xCAFE_BABE,
        parsed.clone(),
        false,
        &ctx,
        12,
        ScanEngineMode::MftDirect,
    );

    // Expected allocated total WITHOUT ADS:
    // unicode(16384) + sparse(8192) + hardlink(32768 counted ONCE!) + ext(24576) + cloud(0) = 81920
    assert_eq!(
        tree.stats.total_allocated_bytes,
        16_384 + 8_192 + 32_768 + 24_576
    );
    assert_eq!(tree.stats.hardlinks_deduplicated, 1);
    assert_eq!(tree.stats.cloud_placeholders, 1);
    assert_eq!(tree.stats.junctions_skipped, 1);
    assert_eq!(tree.stats.symlinks_skipped, 1);
    assert_eq!(tree.stats.ads_streams_counted, 0);

    // Verify Win32 Unicode name chosen over DOS 8.3
    let unicode_nid = tree.frn_to_node[&50];
    assert_eq!(tree.node_name(unicode_nid), "proyecto_日本語_🚀.bin");
    assert_eq!(tree.logical_size[unicode_nid as usize], 12_000);
    assert_eq!(tree.allocated_size[unicode_nid as usize], 16_384);

    // Verify cloud placeholder flag + local allocated = 0
    let cloud_nid = tree.frn_to_node[&55];
    assert!(
        NodeFlags::from_bits_truncate(tree.flags[cloud_nid as usize])
            .contains(NodeFlags::CLOUD_ONLY)
    );
    assert_eq!(tree.allocated_size[cloud_nid as usize], 0);
    assert_eq!(tree.logical_size[cloud_nid as usize], 50_000_000);

    // Case B: include_ads = true adds the 4096B ADS stream!
    let tree_ads = build_tree_from_mft_records(
        PathBuf::from(r"C:\"),
        0xCAFE_BABE,
        parsed,
        true,
        &ctx,
        12,
        ScanEngineMode::MftDirect,
    );
    assert_eq!(
        tree_ads.stats.total_allocated_bytes,
        tree.stats.total_allocated_bytes + 4096
    );
    assert_eq!(tree_ads.stats.ads_streams_counted, 1);

    // Direct data runs unit check (2-byte positive LCN 200 + 100 sparse clusters)
    let runs = parse_data_runs(&[0x21, 2, 200, 0, 0x01, 100, 0x00]);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].cluster_count, 2);
    assert_eq!(runs[0].lcn, Some(200));
    assert_eq!(runs[1].cluster_count, 100);
    assert_eq!(runs[1].lcn, None);
}

// ---------------------------------------------------------------------------
// 2. SNAPSHOT PERSISTENCE (LZ4 + MMAP + CRC32) & USN INCREMENTAL CONSISTENCY
// ---------------------------------------------------------------------------

struct MockUsnReader {
    info: UsnJournalInfo,
    events: Vec<UsnEvent>,
    next_usn: i64,
}

impl UsnJournalReader for MockUsnReader {
    fn query_journal(&self, _drive_root: &str) -> Result<UsnJournalInfo, String> {
        Ok(self.info)
    }

    fn read_records(
        &self,
        _drive_root: &str,
        _journal_id: u64,
        _start_usn: i64,
    ) -> Result<(Vec<UsnEvent>, i64), String> {
        Ok((self.events.clone(), self.next_usn))
    }
}

struct MockMetaLookup(HashMap<String, FileSizeSnapshot>);

impl FileMetaLookup for MockMetaLookup {
    fn lookup_by_path(&self, path: &Path) -> Option<FileSizeSnapshot> {
        let key = path.to_string_lossy().to_ascii_lowercase();
        self.0.get(&key).copied()
    }
}

#[test]
fn snapshot_atomic_save_mmap_load_and_crc_corruption_detection() {
    let dir = TempDir::new().unwrap();
    let data_root = dir.path().join("vol");
    let idx_dir = dir.path().join("index");
    fs::create_dir_all(data_root.join("alpha")).unwrap();
    fs::write(data_root.join("alpha").join("doc.txt"), vec![b'x'; 8192]).unwrap();

    let ctx = default_ctx();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut tree = walk_tree_work_stealing(
        data_root.clone(),
        0x1234_ABCD,
        FileSystemType::Ntfs,
        None,
        cancel,
        &ctx,
        |_| {},
    );
    tree.usn_journal_id = 777;
    tree.last_usn = 4200;

    let snap_path = save_volume_snapshot(&tree, &idx_dir).unwrap();
    assert!(snap_path.exists());

    // Valid load via mmap
    let (loaded, meta) = load_volume_snapshot(&snap_path, Some(0x1234_ABCD)).unwrap();
    assert_eq!(meta.usn_journal_id, 777);
    assert_eq!(meta.last_usn, 4200);
    assert_eq!(loaded.parent.len(), tree.parent.len());
    assert_eq!(
        loaded.stats.total_allocated_bytes,
        tree.stats.total_allocated_bytes
    );

    // Wrong volume serial rejected
    assert!(load_volume_snapshot(&snap_path, Some(0x9999_9999)).is_err());

    // Corrupt a byte in compressed payload -> CRC32 mismatch detected!
    let mut raw = fs::read(&snap_path).unwrap();
    let last = raw.len() - 1;
    raw[last] ^= 0x55;
    fs::write(&snap_path, &raw).unwrap();
    let err = load_volume_snapshot(&snap_path, Some(0x1234_ABCD)).unwrap_err();
    assert!(err.contains("CRC32"), "Expected CRC32 error, got: {err}");
}

#[test]
fn usn_deltas_match_full_scan_and_detect_recreated_or_truncated_journal() {
    let ctx = default_ctx();
    let root = PathBuf::from(r"Z:\");
    let mtime = 133_800_000_000_000_000i64;

    // Build initial MFT tree with user FRNs (>= 16): 5 (root), 100 (FolderA), 101 (FolderB), 120 (keep.txt = 4096), 121 (del.log = 8192)
    let build_base_records = || {
        let mut r5 = MftRecordBuilder::new(true, true, 0);
        r5.add_standard_info(0x10, mtime);
        r5.add_file_name(5, 3, ".", 0);

        let mut r100 = MftRecordBuilder::new(true, true, 0);
        r100.add_standard_info(0x10, mtime);
        r100.add_file_name(5, 1, "FolderA", 0);

        let mut r101 = MftRecordBuilder::new(true, true, 0);
        r101.add_standard_info(0x10, mtime);
        r101.add_file_name(5, 1, "FolderB", 0);

        let mut r120 = MftRecordBuilder::new(true, false, 0);
        r120.add_standard_info(0x20, mtime);
        r120.add_file_name(100, 1, "keep.txt", 0);
        r120.push_non_resident_data(None, 0, 0, 4096, 3000, &[0x11, 1, 10]);

        let mut r121 = MftRecordBuilder::new(true, false, 0);
        r121.add_standard_info(0x20, mtime);
        r121.add_file_name(100, 1, "del.log", 0);
        r121.push_non_resident_data(None, 0, 0, 8192, 7000, &[0x11, 2, 20]);

        vec![
            (5u64, r5.finish()),
            (100, r100.finish()),
            (101, r101.finish()),
            (120, r120.finish()),
            (121, r121.finish()),
        ]
        .into_iter()
        .filter_map(|(frn, b)| parse_mft_record_bytes(&b, frn, 512, 4096))
        .collect::<Vec<_>>()
    };

    let mut tree = build_tree_from_mft_records(
        root.clone(),
        1,
        build_base_records(),
        false,
        &ctx,
        5,
        ScanEngineMode::MftDirect,
    );
    tree.usn_journal_id = 500;
    tree.last_usn = 1000;

    // 1. Recreated journal (UsnJournalID changed) -> NeedsFullRescan
    let reader_recreated = MockUsnReader {
        info: UsnJournalInfo {
            usn_journal_id: 999,
            first_usn: 0,
            next_usn: 1500,
            lowest_valid_usn: 0,
        },
        events: vec![],
        next_usn: 1500,
    };
    assert!(matches!(
        sync_tree_with_usn(
            &mut tree,
            &reader_recreated,
            &MockMetaLookup(HashMap::new()),
            &ctx
        ),
        UsnSyncOutcome::NeedsFullRescan { .. }
    ));

    // 2. Truncated journal (saved last_usn < first_usn) -> NeedsFullRescan
    let reader_truncated = MockUsnReader {
        info: UsnJournalInfo {
            usn_journal_id: 500,
            first_usn: 2000,
            next_usn: 3000,
            lowest_valid_usn: 2000,
        },
        events: vec![],
        next_usn: 3000,
    };
    assert!(matches!(
        sync_tree_with_usn(
            &mut tree,
            &reader_truncated,
            &MockMetaLookup(HashMap::new()),
            &ctx
        ),
        UsnSyncOutcome::NeedsFullRescan { .. }
    ));

    // 3. Apply valid USN deltas:
    //    - Delete del.log (FRN 21)
    //    - Rename & move keep.txt (FRN 20) from FolderA -> FolderB\moved.txt
    //    - Extend moved.txt (FRN 20) to 16384 allocated / 15000 logical
    //    - Create FolderA\added.mp4 (FRN 22) = 40960 allocated / 40000 logical
    let mut meta_map = HashMap::new();
    meta_map.insert(
        r"z:\folderb\moved.txt".to_string(),
        FileSizeSnapshot {
            logical_size: 15_000,
            allocated_size: 16_384,
            last_write_ts: 1_700_000_000,
            is_dir: false,
        },
    );
    meta_map.insert(
        r"z:\foldera\added.mp4".to_string(),
        FileSizeSnapshot {
            logical_size: 40_000,
            allocated_size: 40_960,
            last_write_ts: 1_700_000_010,
            is_dir: false,
        },
    );
    let lookup = MockMetaLookup(meta_map);

    let events = vec![
        UsnEvent {
            usn: 1010,
            frn: 121,
            parent_frn: 100,
            reason: USN_REASON_FILE_DELETE | USN_REASON_CLOSE,
            file_attributes: 0x20,
            file_name: "del.log".to_string(),
            timestamp_unix: 1_700_000_000,
        },
        UsnEvent {
            usn: 1020,
            frn: 120,
            parent_frn: 100,
            reason: USN_REASON_RENAME_OLD_NAME,
            file_attributes: 0x20,
            file_name: "keep.txt".to_string(),
            timestamp_unix: 1_700_000_000,
        },
        UsnEvent {
            usn: 1030,
            frn: 120,
            parent_frn: 101,
            reason: USN_REASON_RENAME_NEW_NAME,
            file_attributes: 0x20,
            file_name: "moved.txt".to_string(),
            timestamp_unix: 1_700_000_000,
        },
        UsnEvent {
            usn: 1040,
            frn: 120,
            parent_frn: 101,
            reason: USN_REASON_DATA_EXTEND | USN_REASON_CLOSE,
            file_attributes: 0x20,
            file_name: "moved.txt".to_string(),
            timestamp_unix: 1_700_000_000,
        },
        UsnEvent {
            usn: 1050,
            frn: 122,
            parent_frn: 100,
            reason: USN_REASON_FILE_CREATE | USN_REASON_CLOSE,
            file_attributes: 0x20,
            file_name: "added.mp4".to_string(),
            timestamp_unix: 1_700_000_010,
        },
    ];

    let reader_ok = MockUsnReader {
        info: UsnJournalInfo {
            usn_journal_id: 500,
            first_usn: 500,
            next_usn: 1100,
            lowest_valid_usn: 500,
        },
        events,
        next_usn: 1100,
    };

    let outcome = sync_tree_with_usn(&mut tree, &reader_ok, &lookup, &ctx);
    assert!(matches!(
        outcome,
        UsnSyncOutcome::UpToDate {
            applied_events: 4,
            new_last_usn: 1100,
            ..
        }
    ));

    // Build expected full-scan ground truth matching the exact post-USN state!
    let mut r5 = MftRecordBuilder::new(true, true, 0);
    r5.add_standard_info(0x10, mtime);
    r5.add_file_name(5, 3, ".", 0);
    let mut r100 = MftRecordBuilder::new(true, true, 0);
    r100.add_standard_info(0x10, mtime);
    r100.add_file_name(5, 1, "FolderA", 0);
    let mut r101 = MftRecordBuilder::new(true, true, 0);
    r101.add_standard_info(0x10, mtime);
    r101.add_file_name(5, 1, "FolderB", 0);
    let mut r120 = MftRecordBuilder::new(true, false, 0);
    r120.add_standard_info(0x20, mtime);
    r120.add_file_name(101, 1, "moved.txt", 0);
    r120.push_non_resident_data(None, 0, 0, 16_384, 15_000, &[0x11, 4, 10]);
    let mut r122 = MftRecordBuilder::new(true, false, 0);
    r122.add_standard_info(0x20, mtime);
    r122.add_file_name(100, 1, "added.mp4", 0);
    r122.push_non_resident_data(None, 0, 0, 40_960, 40_000, &[0x11, 10, 30]);

    let full_records: Vec<ParsedMftRecord> = vec![
        (5u64, r5.finish()),
        (100, r100.finish()),
        (101, r101.finish()),
        (120, r120.finish()),
        (122, r122.finish()),
    ]
    .into_iter()
    .filter_map(|(frn, b)| parse_mft_record_bytes(&b, frn, 512, 4096))
    .collect();

    let expected = build_tree_from_mft_records(
        root,
        1,
        full_records,
        false,
        &ctx,
        5,
        ScanEngineMode::MftDirect,
    );

    // Consistency assertion: incremental USN tree == fresh full scan!
    assert_eq!(tree.allocated_size[0], expected.allocated_size[0]);
    assert_eq!(tree.logical_size[0], expected.logical_size[0]);
    assert_eq!(tree.file_count[0], expected.file_count[0]);

    let fa_inc = tree.frn_to_node[&100];
    let fa_exp = expected.frn_to_node[&100];
    assert_eq!(
        tree.allocated_size[fa_inc as usize],
        expected.allocated_size[fa_exp as usize]
    );

    let fb_inc = tree.frn_to_node[&101];
    let fb_exp = expected.frn_to_node[&101];
    assert_eq!(
        tree.allocated_size[fb_inc as usize],
        expected.allocated_size[fb_exp as usize]
    );

    // Ensure dummy helper compiles cleanly
    let _ = apply_usn_events_to_tree;
}

// ---------------------------------------------------------------------------
// 3. OPTIMISTIC DELETION, ROLLBACK, FAST TRASH STAGE & isProtected() SECURITY
// ---------------------------------------------------------------------------

#[test]
fn optimistic_hide_and_revert_plus_fast_delete_and_protection_enforcement() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("sandbox");
    let nm = root.join("proj").join("node_modules");
    fs::create_dir_all(nm.join("pkg_a")).unwrap();
    fs::write(nm.join("pkg_a").join("index.js"), vec![b'a'; 16_384]).unwrap();
    let ro_file = nm.join("pkg_a").join("readonly.txt");
    fs::write(&ro_file, vec![b'r'; 4096]).unwrap();
    let mut perms = fs::metadata(&ro_file).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&ro_file, perms).unwrap();

    // Protected path check: .git and C:\Windows MUST be rejected BEFORE rename!
    let git_dir = root.join("proj").join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();

    let guard = SandboxGuard::new(&root);
    let err_git = stage_to_same_volume_trash(&git_dir, "job-sec", Some(&guard), Some(&root))
        .expect_err(".git MUST be blocked by isProtected() before rename");
    assert!(
        err_git.to_lowercase().contains("proteg") || err_git.to_lowercase().contains(".git"),
        "Unexpected error: {err_git}"
    );
    assert!(git_dir.exists(), ".git must never be moved or touched!");

    assert!(
        stage_to_same_volume_trash(Path::new(r"C:\Windows"), "job-win", None, None).is_err(),
        "C:\\Windows MUST be blocked before rename"
    );

    // Scan sandbox -> test optimistic_hide & optimistic_revert
    let ctx = default_ctx();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut tree = walk_tree_work_stealing(
        root.clone(),
        0x1111,
        FileSystemType::Ntfs,
        None,
        cancel.clone(),
        &ctx,
        |_| {},
    );
    let initial_alloc = tree.allocated_size[0];
    let nm_nid = tree
        .find_node_by_path(&nm)
        .expect("node_modules node found");
    let nm_alloc = tree.allocated_size[nm_nid as usize];
    assert!(nm_alloc > 0);

    assert!(tree.optimistic_hide(nm_nid));
    assert_eq!(tree.allocated_size[0], initial_alloc - nm_alloc);
    assert!(NodeFlags::from_bits_truncate(tree.flags[nm_nid as usize])
        .contains(NodeFlags::OPTIMISTIC_HIDDEN));

    // Simulate failure -> revert restores exact totals!
    assert!(tree.optimistic_revert(nm_nid));
    assert_eq!(tree.allocated_size[0], initial_alloc);
    assert!(!NodeFlags::from_bits_truncate(tree.flags[nm_nid as usize])
        .contains(NodeFlags::OPTIMISTIC_HIDDEN));

    // Real fast staging + parallel POSIX deletion inside sandbox TempDir
    let staged = stage_to_same_volume_trash(&nm, "job-fast", Some(&guard), Some(&root)).unwrap();
    assert!(
        !nm.exists(),
        "Original path must be freed immediately after rename"
    );
    assert!(staged.exists());

    let deleted = delete_tree_parallel_fast(&staged, None, None, false).unwrap();
    assert!(deleted >= 4);
    assert!(
        !staged.exists(),
        "Staged tree including read-only files must be completely deleted"
    );

    // Post-deletion subtree reconciliation must match a fresh full scan!
    let proj_dir = root.join("proj");
    let touched = reconcile_subtree_fallback(&mut tree, &proj_dir, &ctx);
    assert!(touched);

    // Remove staging container so fresh scan sees exact clean state
    let _ = fs::remove_dir_all(root.join(".scancleaner-trash"));
    let fresh = walk_tree_work_stealing(
        root.clone(),
        0x1111,
        FileSystemType::Ntfs,
        None,
        cancel,
        &ctx,
        |_| {},
    );
    assert_eq!(tree.allocated_size[0], fresh.allocated_size[0]);
    assert_eq!(tree.file_count[0], fresh.file_count[0]);
}

// ---------------------------------------------------------------------------
// 4. NAMED PIPE INDEXING SERVICE PROTOCOL & ACL VALIDATION
// ---------------------------------------------------------------------------

#[test]
fn service_protocol_is_strictly_read_only_and_enforces_acl_and_framing() {
    assert!(validate_request(&ServiceRequest::Ping).is_ok());
    assert!(validate_request(&ServiceRequest::QueryUsnJournal { drive_letter: 'C' }).is_ok());
    assert!(validate_request(&ServiceRequest::QueryUsnJournal { drive_letter: '1' }).is_err());
    assert!(validate_request(&ServiceRequest::ReadUsnRecords {
        drive_letter: 'C',
        journal_id: 1,
        start_usn: -5,
    })
    .is_err());

    // Any unknown/delete opcode MUST fail deserialization
    let delete_json = br#"{"op":"delete_file","path":"C:\\foo"}"#;
    assert!(decode_request_frame(delete_json).is_err());

    // Oversized request frame rejected
    let huge = vec![b' '; 5000];
    assert!(decode_request_frame(&huge).is_err());

    // Valid frame roundtrip
    let valid = serde_json::to_vec(&ServiceRequest::ScanMft {
        drive_letter: 'D',
        include_ads: false,
    })
    .unwrap();
    let framed = encode_frame(&valid).unwrap();
    assert_eq!(framed.len(), 4 + valid.len());
    let decoded = decode_request_frame(&valid).unwrap();
    assert_eq!(
        decoded,
        ServiceRequest::ScanMft {
            drive_letter: 'D',
            include_ads: false
        }
    );

    // Strict SDDL DACL: SYSTEM + single installing user SID only
    let sddl = build_pipe_sddl("S-1-5-21-3623811015-3361044348-30300820-1013").unwrap();
    assert_eq!(
        sddl,
        "D:(A;;GA;;;SY)(A;;GA;;;S-1-5-21-3623811015-3361044348-30300820-1013)"
    );
    assert!(build_pipe_sddl("S-1-5-21-123)(A;;GA;;;WD").is_err());
    assert!(build_pipe_sddl("WD").is_err());
}

// ---------------------------------------------------------------------------
// 5. RECONCILIATION QUEUE, COALESCING, LOOP PREVENTION & TRIGGERS
// ---------------------------------------------------------------------------

struct MockDiskSpace(HashMap<String, (u64, u64)>);

impl DiskSpaceProvider for MockDiskSpace {
    fn query_space(&self, drive_root: &str) -> Option<(u64, u64)> {
        self.0.get(&drive_root.to_ascii_uppercase()).copied()
    }
}

struct MockUninstallProbe {
    polls_until_gone: AtomicUsize,
}

impl UninstallPresenceProbe for MockUninstallProbe {
    fn is_app_still_registered(&self, _app_id: &str) -> bool {
        let prev = self.polls_until_gone.fetch_sub(1, Ordering::SeqCst);
        prev > 0
    }

    fn wait_for_uninstall_change(&self, _max_wait: Duration) {
        // Instant in unit tests
    }
}

#[test]
fn reconciliation_queue_coalesces_debounces_ignores_index_dir_and_prioritizes() {
    let index_dir = PathBuf::from(r"C:\Users\Dev\AppData\Local\ScanCleaner\index");

    // Coalescing & self-loop exclusion
    let coalesced = coalesce_paths(
        &[
            PathBuf::from(r"C:\a\b\c"),
            PathBuf::from(r"C:\a"),
            PathBuf::from(r"C:\a\b"),
            PathBuf::from(r"C:\Users\Dev\AppData\Local\ScanCleaner\index\vol_C.scidx"),
            PathBuf::from(r"C:\d\e"),
        ],
        &index_dir,
    );
    assert_eq!(coalesced.len(), 2);
    assert_eq!(coalesced[0], PathBuf::from(r"C:\a"));
    assert_eq!(coalesced[1], PathBuf::from(r"C:\d\e"));

    let mut q = ReconciliationQueue::new(Duration::from_millis(250), index_dir.clone());
    let t0 = Instant::now();

    // Events inside own index dir are completely ignored (returns false -> no loop!)
    assert!(!q.enqueue(
        ReconcileRequest {
            volume: r"C:\".to_string(),
            paths: vec![index_dir.join("vol_C_1234.scidx")],
            origin: ReconcileOrigin::DirWatcher,
            priority: ReconcilePriority::Low,
        },
        t0,
    ));
    assert_eq!(q.pending_count(), 0);

    // Burst on C:\ (Low + High coalesced) and D:\ (Medium)
    q.enqueue(
        ReconcileRequest {
            volume: r"D:\".to_string(),
            paths: vec![PathBuf::from(r"D:\games")],
            origin: ReconcileOrigin::FreeSpaceDrift,
            priority: ReconcilePriority::Medium,
        },
        t0,
    );
    q.enqueue(
        ReconcileRequest {
            volume: r"C:\".to_string(),
            paths: vec![PathBuf::from(r"C:\proj\sub")],
            origin: ReconcileOrigin::UsnJournal,
            priority: ReconcilePriority::Low,
        },
        t0 + Duration::from_millis(50),
    );
    q.enqueue(
        ReconcileRequest {
            volume: r"C:\".to_string(),
            paths: vec![PathBuf::from(r"C:\proj")],
            origin: ReconcileOrigin::UserDeletion,
            priority: ReconcilePriority::High,
        },
        t0 + Duration::from_millis(100),
    );

    // Before 250ms debounce, nothing pops
    assert!(q.pop_ready(t0 + Duration::from_millis(200)).is_none());

    // After debounce elapses for both, High priority C:\ pops BEFORE Medium D:\, coalesced to C:\proj
    let first = q.pop_ready(t0 + Duration::from_millis(400)).unwrap();
    assert_eq!(first.volume, r"C:\");
    assert_eq!(first.priority, ReconcilePriority::High);
    assert_eq!(first.paths, vec![PathBuf::from(r"C:\proj")]);

    let second = q.pop_ready(t0 + Duration::from_millis(400)).unwrap();
    assert_eq!(second.volume, r"D:\");
    assert_eq!(second.priority, ReconcilePriority::Medium);

    // Retry backoff delays immediate re-pop
    q.schedule_retry(second, t0 + Duration::from_millis(400));
    assert!(q.pop_ready(t0 + Duration::from_millis(800)).is_none());
    assert!(q.pop_ready(t0 + Duration::from_secs(5)).is_some());
}

#[test]
fn triggers_uninstall_drift_freed_space_and_device_unitmask() {
    // 1. Simulated uninstaller that relaunches from %TEMP%: stays registered for 3 polls, then vanishes
    let sandbox = TempDir::new().unwrap();
    let app_dir = sandbox.path().join("FakeApp");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("leftover.dat"), vec![1u8; 4096]).unwrap();

    let probe = MockUninstallProbe {
        polls_until_gone: AtomicUsize::new(3),
    };
    let prot = ProtectionEngine::load_default();
    let res = watch_native_uninstall_completion(
        "win32:hkcu:FakeApp",
        "FakeApp",
        Some(&app_dir.to_string_lossy()),
        &[],
        Duration::from_secs(5),
        Duration::from_millis(1),
        &probe,
        &prot,
    );
    assert!(res.key_removed);
    assert!(!res.timed_out);
    assert_eq!(res.reconcile_requests.len(), 1);
    assert_eq!(res.reconcile_requests[0].priority, ReconcilePriority::High);
    assert_eq!(res.leftovers.len(), 1);
    assert_eq!(res.leftovers[0].nivel, "revisar");

    // 2. Free-space drift detection (2% or 1 GiB)
    let mut map = HashMap::new();
    let gb = 1024u64 * 1024 * 1024;
    map.insert(r"C:\".to_string(), (500 * gb, 300 * gb)); // real used = 200 GiB
    let provider = MockDiskSpace(map);

    let ok_drift = evaluate_free_space_drift(r"C:\", 198 * gb, &provider, 2.0, 1).unwrap();
    assert!(
        !ok_drift.drifted,
        "2 GiB diff on 500 GiB drive is within 2% (10 GiB)"
    );

    let big_drift = evaluate_free_space_drift(r"C:\", 180 * gb, &provider, 2.0, 1).unwrap();
    assert!(big_drift.drifted, "20 GiB diff exceeds 10 GiB threshold");

    // 3. Real vs estimated freed space explanation
    let meas = measure_real_freed_space(
        r"C:\",
        300 * gb, // before = 300 GiB, after = 300 GiB -> real freed = 0
        500 * 1024 * 1024,
        &provider,
        false,
        true, // touched Docker .vhdx
        false,
    );
    assert_eq!(meas.real_freed_bytes, 0);
    assert!(meas
        .discrepancy_explanation
        .as_deref()
        .unwrap_or("")
        .contains(".vhdx"));

    // 4. Device hotplug bitmask decode (bits 2=C, 3=D, 4=E)
    assert_eq!(decode_device_unitmask(0b11000), vec![r"D:\", r"E:\"]);

    // Verify NodeKind is accessible
    assert_ne!(NodeKind::File, NodeKind::Dir);
}

#[test]
fn memory_regression_slot_recycling_incremental_diff_and_snapshot_compaction() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("vol");
    let proj = root.join("proj");
    let nm = proj.join("node_modules");
    let sibling_heavy = root.join("heavy_untouched");
    fs::create_dir_all(nm.join("pkg")).unwrap();
    fs::create_dir_all(&sibling_heavy).unwrap();
    fs::write(proj.join("package.json"), br#"{"name":"demo"}"#).unwrap();
    fs::write(nm.join("pkg").join("index.js"), vec![b'x'; 8192]).unwrap();
    for i in 0..25 {
        fs::write(sibling_heavy.join(format!("asset_{i}.bin")), vec![b'z'; 1024]).unwrap();
    }

    let catalog = scancleaner_lib::dev_cleaner::schema::DevRulesCatalog::load_embedded().ok();
    let ctx = DerivedBuildContext::new(
        RulesConfig::load(),
        ProtectionEngine::load_default(),
        catalog.as_ref(),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let mut tree = walk_tree_work_stealing(
        root.clone(),
        0x7777,
        FileSystemType::Ntfs,
        None,
        cancel,
        &ctx,
        |_| {},
    );

    let baseline_len = tree.len();
    let baseline_dev_count = tree.derived.dev_artifacts.len();
    assert_eq!(baseline_dev_count, 1, "node_modules detected once");

    // 1. Churn a single file 40 times via `reconcile_watcher_path`:
    //    `tree.len()` must grow by at most 1 slot and NEVER duplicate `dev_artifacts`!
    let churn_file = root.join("ntuser_sim.dat");
    for cycle in 0..40u64 {
        fs::write(&churn_file, vec![b'a'; (1024 + (cycle % 5) * 512) as usize]).unwrap();
        let _ = reconcile_watcher_path(&mut tree, &churn_file, &ctx);
    }
    assert!(
        tree.len() <= baseline_len + 2,
        "Expected bounded node table, got {} vs baseline {}",
        tree.len(),
        baseline_len
    );
    assert_eq!(tree.derived.dev_artifacts.len(), 1);

    // 2.Repeatedly delete & recreate `churn_file` and `proj/temp_build`: slots MUST recycle via `free_list`!
    let len_after_single = tree.len();
    for _ in 0..20 {
        fs::remove_file(&churn_file).unwrap();
        assert!(reconcile_watcher_path(&mut tree, &churn_file, &ctx));
        fs::write(&churn_file, vec![b'b'; 4096]).unwrap();
        assert!(reconcile_watcher_path(&mut tree, &churn_file, &ctx));
    }
    assert_eq!(
        tree.len(),
        len_after_single,
        "Recycled free_list slots must prevent Vec growth across delete/create cycles"
    );

    // 3. Reconciling `proj` multiple times in-place must NOT create tombstones for unchanged files or duplicate dev_artifacts
    for _ in 0..10 {
        let _ = reconcile_subtree_fallback(&mut tree, &proj, &ctx);
    }
    assert_eq!(tree.derived.dev_artifacts.len(), 1);
    assert_eq!(tree.tombstone_count(), 0);

    // 4. Delete `sibling_heavy` (26 nodes -> 26 tombstones), save snapshot -> snapshot MUST strip all tombstones!
    fs::remove_dir_all(&sibling_heavy).unwrap();
    assert!(reconcile_subtree_fallback(&mut tree, &sibling_heavy, &ctx));
    assert!(tree.tombstone_count() >= 25);

    let idx_dir = dir.path().join("idx");
    let snap = save_volume_snapshot(&tree, &idx_dir).unwrap();
    let (loaded, _) = load_volume_snapshot(&snap, Some(0x7777)).unwrap();
    assert_eq!(loaded.tombstone_count(), 0);
    assert!(
        loaded.len() <= baseline_len - 24,
        "Compacted snapshot must exclude tombstones: loaded_len={} baseline={}",
        loaded.len(),
        baseline_len
    );
    assert_eq!(loaded.allocated_size[0], tree.allocated_size[0]);
}
