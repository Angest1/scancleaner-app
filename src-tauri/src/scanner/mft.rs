use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, SetFilePointerEx, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_BEGIN,
    FILE_FLAG_SEQUENTIAL_SCAN, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::FSCTL_GET_NTFS_VOLUME_DATA;
use windows::Win32::System::IO::DeviceIoControl;

use super::derived::{finalize_tree_and_derived, DerivedBuildContext};
use super::model::{FileSystemType, NodeFlags, NodeKind, ScanEngineMode, ScanProgress, ScanTree};

const MFT_SIGNATURE: &[u8; 4] = b"FILE";
const ATTR_STANDARD_INFO: u32 = 0x10;
const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_REPARSE_POINT: u32 = 0xC0;
const ATTR_END: u32 = 0xFFFF_FFFF;

const NAMESPACE_POSIX: u8 = 0;
const NAMESPACE_WIN32: u8 = 1;
#[allow(dead_code)]
const NAMESPACE_DOS: u8 = 2;
const NAMESPACE_WIN32_AND_DOS: u8 = 3;

const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
const MFT_ROOT_DIR_FRN: u64 = 5;
const MFT_READ_CHUNK_BYTES: usize = 8 * 1024 * 1024; // 8 MB sequential blocks

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(non_snake_case)]
pub struct NtfsVolumeDataBuffer {
    pub VolumeSerialNumber: i64,
    pub NumberSectors: i64,
    pub TotalClusters: i64,
    pub FreeClusters: i64,
    pub TotalReserved: i64,
    pub BytesPerSector: u32,
    pub BytesPerCluster: u32,
    pub BytesPerFileRecordSegment: u32,
    pub ClustersPerFileRecordSegment: u32,
    pub MftValidDataLength: i64,
    pub MftStartLcn: i64,
    pub Mft2StartLcn: i64,
    pub MftZoneStart: i64,
    pub MftZoneEnd: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataRun {
    /// Length in clusters
    pub cluster_count: u64,
    /// Starting LCN (`None` for sparse/compressed hole runs)
    pub lcn: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ParsedFileName {
    pub parent_frn: u64,
    pub namespace: u8,
    pub name: String,
    pub last_write_ts: i64,
}

#[derive(Debug, Clone)]
pub struct ParsedMftRecord {
    pub frn: u64,
    pub base_frn: u64,
    pub in_use: bool,
    pub is_dir: bool,
    pub attrs: u32,
    pub reparse_tag: u32,
    pub last_write_ts: i64,
    pub names: Vec<ParsedFileName>,
    pub default_logical_size: u64,
    pub default_allocated_size: u64,
    pub ads_logical_size: u64,
    pub ads_allocated_size: u64,
    pub ads_count: u32,
    pub has_default_data: bool,
    pub attr_list_ext_frns: Vec<u64>,
}

pub fn filetime_to_unix_secs(ft_100ns: i64) -> i64 {
    const WINDOWS_TO_UNIX_EPOCH_100NS: i64 = 116_444_736_000_000_000;
    if ft_100ns <= WINDOWS_TO_UNIX_EPOCH_100NS {
        0
    } else {
        (ft_100ns - WINDOWS_TO_UNIX_EPOCH_100NS) / 10_000_000
    }
}

/// Applies the NTFS Update Sequence Array (USA) fixup to a raw MFT record copy.
pub fn apply_usa_fixup(record: &mut [u8], sector_size: usize) -> bool {
    if record.len() < 48 || &record[0..4] != MFT_SIGNATURE {
        return false;
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_count < 2 || usa_offset + usa_count * 2 > record.len() {
        return false;
    }
    let usn0 = record[usa_offset];
    let usn1 = record[usa_offset + 1];

    for i in 1..usa_count {
        let sector_end = i * sector_size;
        if sector_end > record.len() || sector_end < 2 {
            break;
        }
        if record[sector_end - 2] != usn0 || record[sector_end - 1] != usn1 {
            // Corrupt/torn sector
            return false;
        }
        let fix_off = usa_offset + i * 2;
        record[sector_end - 2] = record[fix_off];
        record[sector_end - 1] = record[fix_off + 1];
    }
    true
}

/// Decodes NTFS non-resident data runs (`mapping pairs`).
/// Sparse/zero runs have `lcn: None` so compressed & sparse files only count real allocated clusters.
pub fn parse_data_runs(bytes: &[u8]) -> Vec<DataRun> {
    let mut runs = Vec::new();
    let mut pos = 0usize;
    let mut current_lcn: i64 = 0;

    while pos < bytes.len() {
        let header = bytes[pos];
        if header == 0 {
            break;
        }
        pos += 1;
        let len_size = (header & 0x0F) as usize;
        let off_size = ((header >> 4) & 0x0F) as usize;
        if len_size == 0 || len_size > 8 || off_size > 8 || pos + len_size + off_size > bytes.len()
        {
            break;
        }

        let mut cluster_count: u64 = 0;
        for i in 0..len_size {
            cluster_count |= (bytes[pos + i] as u64) << (i * 8);
        }
        pos += len_size;

        if off_size == 0 {
            // Sparse / compressed zero-fill run
            runs.push(DataRun {
                cluster_count,
                lcn: None,
            });
        } else {
            // Signed little-endian delta LCN
            let mut delta: i64 = 0;
            for i in 0..off_size {
                delta |= (bytes[pos + i] as i64) << (i * 8);
            }
            if (bytes[pos + off_size - 1] & 0x80) != 0 && off_size < 8 {
                delta |= !0i64 << (off_size * 8);
            }
            pos += off_size;
            current_lcn = current_lcn.saturating_add(delta);
            runs.push(DataRun {
                cluster_count,
                lcn: if current_lcn >= 0 {
                    Some(current_lcn as u64)
                } else {
                    None
                },
            });
        }
    }
    runs
}

/// Parses a single raw MFT `FILE` record buffer (e.g. 1024 bytes).
/// Handles `$STANDARD_INFORMATION`, `$ATTRIBUTE_LIST` (resident & non-resident),
/// `$FILE_NAME` (Win32 / POSIX / DOS 8.3), `$DATA` (resident, non-resident, compressed, sparse, ADS),
/// and `$REPARSE_POINT`.
pub fn parse_mft_record_bytes(
    raw: &[u8],
    record_index: u64,
    sector_size: usize,
    bytes_per_cluster: u64,
) -> Option<ParsedMftRecord> {
    if raw.len() < 48 || &raw[0..4] != MFT_SIGNATURE {
        return None;
    }
    let mut buf = raw.to_vec();
    if !apply_usa_fixup(&mut buf, sector_size) {
        return None;
    }

    let first_attr_offset = u16::from_le_bytes([buf[20], buf[21]]) as usize;
    let flags = u16::from_le_bytes([buf[22], buf[23]]);
    let in_use = (flags & 0x0001) != 0;
    if !in_use {
        return None;
    }
    let is_dir = (flags & 0x0002) != 0;

    let base_ref_raw = u64::from_le_bytes(buf[32..40].try_into().ok()?);
    let base_frn = base_ref_raw & 0x0000_FFFF_FFFF_FFFF;

    let mut rec = ParsedMftRecord {
        frn: record_index,
        base_frn: if base_frn == 0 {
            record_index
        } else {
            base_frn
        },
        in_use,
        is_dir,
        attrs: 0,
        reparse_tag: 0,
        last_write_ts: 0,
        names: Vec::new(),
        default_logical_size: 0,
        default_allocated_size: 0,
        ads_logical_size: 0,
        ads_allocated_size: 0,
        ads_count: 0,
        has_default_data: false,
        attr_list_ext_frns: Vec::new(),
    };

    let mut off = first_attr_offset;
    while off + 16 <= buf.len() {
        let attr_type = u32::from_le_bytes(buf[off..off + 4].try_into().ok()?);
        if attr_type == ATTR_END {
            break;
        }
        let attr_len = u32::from_le_bytes(buf[off + 4..off + 8].try_into().ok()?) as usize;
        if attr_len < 16 || off + attr_len > buf.len() {
            break;
        }
        let non_resident = buf[off + 8] != 0;
        let name_len = buf[off + 9] as usize;
        let attr_flags = u16::from_le_bytes([buf[off + 12], buf[off + 13]]);
        let is_compressed_attr = (attr_flags & 0x0001) != 0;
        let is_sparse_attr = (attr_flags & 0x8000) != 0;

        match attr_type {
            ATTR_STANDARD_INFO if !non_resident => {
                let val_len = u32::from_le_bytes(buf[off + 16..off + 20].try_into().ok()?) as usize;
                let val_off = u16::from_le_bytes([buf[off + 20], buf[off + 21]]) as usize;
                if val_len >= 36 && off + val_off + 36 <= off + attr_len {
                    let v = &buf[off + val_off..off + val_off + val_len];
                    let mtime_ft = i64::from_le_bytes(v[8..16].try_into().ok()?);
                    rec.last_write_ts = filetime_to_unix_secs(mtime_ft);
                    rec.attrs = u32::from_le_bytes(v[32..36].try_into().ok()?);
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                let val_len = u32::from_le_bytes(buf[off + 16..off + 20].try_into().ok()?) as usize;
                let val_off = u16::from_le_bytes([buf[off + 20], buf[off + 21]]) as usize;
                if val_len >= 66 && off + val_off + 66 <= off + attr_len {
                    let v = &buf[off + val_off..off + val_off + val_len];
                    let parent_raw = u64::from_le_bytes(v[0..8].try_into().ok()?);
                    let parent_frn = parent_raw & 0x0000_FFFF_FFFF_FFFF;
                    let mtime_ft = i64::from_le_bytes(v[16..24].try_into().ok()?);
                    let fn_attrs = u32::from_le_bytes(v[56..60].try_into().ok()?);
                    let fname_chars = v[64] as usize;
                    let namespace = v[65];
                    if 66 + fname_chars * 2 <= v.len() {
                        let utf16: Vec<u16> = v[66..66 + fname_chars * 2]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|c| u16::from_le_bytes(*c))
                            .collect();
                        let name = String::from_utf16_lossy(&utf16);
                        if rec.attrs == 0 {
                            rec.attrs = fn_attrs;
                        }
                        rec.names.push(ParsedFileName {
                            parent_frn,
                            namespace,
                            name,
                            last_write_ts: filetime_to_unix_secs(mtime_ft),
                        });
                    }
                }
            }
            ATTR_DATA => {
                // Always take logical & allocated sizes from $DATA, NEVER from $FILE_NAME!
                let (logical, allocated) = if !non_resident {
                    let val_len =
                        u32::from_le_bytes(buf[off + 16..off + 20].try_into().ok()?) as u64;
                    // TODO: verificar en Windows 10/11 si los archivos residentes (<~700B dentro del registro MFT)
                    // deben reportar allocated=0 (como "Tamaño en disco: 0 bytes" en Propiedades de Windows)
                    // o el tamaño del registro; usamos 0 para coincidir con el espacio asignado en clusters.
                    (val_len, 0u64)
                } else if off + 64 <= off + attr_len {
                    let start_vcn = u64::from_le_bytes(buf[off + 16..off + 24].try_into().ok()?);
                    let run_off = u16::from_le_bytes([buf[off + 32], buf[off + 33]]) as usize;
                    let alloc_hdr = u64::from_le_bytes(buf[off + 40..off + 48].try_into().ok()?);
                    let real_size = u64::from_le_bytes(buf[off + 48..off + 56].try_into().ok()?);

                    let logical = if start_vcn == 0 { real_size } else { 0 };
                    let allocated = if is_compressed_attr || is_sparse_attr {
                        // Sum ONLY real (non-sparse) clusters from data runs
                        if off + run_off < off + attr_len {
                            let runs = parse_data_runs(&buf[off + run_off..off + attr_len]);
                            let real_clusters: u64 = runs
                                .iter()
                                .filter(|r| r.lcn.is_some())
                                .map(|r| r.cluster_count)
                                .sum();
                            real_clusters.saturating_mul(bytes_per_cluster)
                        } else if off + 72 <= off + attr_len {
                            // CompressedSize field at offset +64 for compressed/sparse extent 0
                            u64::from_le_bytes(buf[off + 64..off + 72].try_into().ok()?)
                        } else {
                            alloc_hdr
                        }
                    } else if start_vcn == 0 {
                        alloc_hdr
                    } else if off + run_off < off + attr_len {
                        // Extension $DATA extent (start_vcn > 0): sum its real runs if base didn't cover
                        let runs = parse_data_runs(&buf[off + run_off..off + attr_len]);
                        let real_clusters: u64 = runs
                            .iter()
                            .filter(|r| r.lcn.is_some())
                            .map(|r| r.cluster_count)
                            .sum();
                        real_clusters.saturating_mul(bytes_per_cluster)
                    } else {
                        0
                    };
                    (logical, allocated)
                } else {
                    (0, 0)
                };

                if name_len == 0 {
                    rec.has_default_data = true;
                    rec.default_logical_size = rec.default_logical_size.max(logical);
                    if is_compressed_attr || is_sparse_attr {
                        rec.default_allocated_size =
                            rec.default_allocated_size.saturating_add(allocated);
                    } else {
                        rec.default_allocated_size = rec.default_allocated_size.max(allocated);
                    }
                } else {
                    rec.ads_count += 1;
                    rec.ads_logical_size = rec.ads_logical_size.saturating_add(logical);
                    rec.ads_allocated_size = rec.ads_allocated_size.saturating_add(allocated);
                }
            }
            ATTR_REPARSE_POINT if !non_resident => {
                let val_len = u32::from_le_bytes(buf[off + 16..off + 20].try_into().ok()?) as usize;
                let val_off = u16::from_le_bytes([buf[off + 20], buf[off + 21]]) as usize;
                if val_len >= 4 && off + val_off + 4 <= off + attr_len {
                    rec.reparse_tag =
                        u32::from_le_bytes(buf[off + val_off..off + val_off + 4].try_into().ok()?);
                }
            }
            ATTR_ATTRIBUTE_LIST if !non_resident => {
                let val_len = u32::from_le_bytes(buf[off + 16..off + 20].try_into().ok()?) as usize;
                let val_off = u16::from_le_bytes([buf[off + 20], buf[off + 21]]) as usize;
                if off + val_off + val_len <= off + attr_len {
                    parse_attribute_list_entries(
                        &buf[off + val_off..off + val_off + val_len],
                        record_index,
                        &mut rec.attr_list_ext_frns,
                    );
                }
            }
            _ => {}
        }

        off += attr_len;
    }

    Some(rec)
}

pub fn parse_attribute_list_entries(data: &[u8], base_frn: u64, out_ext_frns: &mut Vec<u64>) {
    let mut pos = 0usize;
    while pos + 26 <= data.len() {
        let entry_len = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as usize;
        if entry_len < 26 || pos + entry_len > data.len() {
            break;
        }
        let ref_raw = u64::from_le_bytes(data[pos + 16..pos + 24].try_into().unwrap_or([0; 8]));
        let ext_frn = ref_raw & 0x0000_FFFF_FFFF_FFFF;
        if ext_frn != base_frn && ext_frn != 0 && !out_ext_frns.contains(&ext_frn) {
            out_ext_frns.push(ext_frn);
        }
        pos += entry_len;
    }
}

/// Filters `$FILE_NAME` entries to prefer Win32/Win32+DOS/POSIX names over DOS 8.3 short names,
/// while preserving distinct hardlinks (multiple parents or distinct Win32 names in same parent).
pub fn select_preferred_names(names: &[ParsedFileName]) -> Vec<&ParsedFileName> {
    if names.is_empty() {
        return Vec::new();
    }
    let mut by_parent: HashMap<u64, Vec<&ParsedFileName>> = HashMap::new();
    for n in names {
        by_parent.entry(n.parent_frn).or_default().push(n);
    }

    let mut chosen = Vec::new();
    for (_parent, group) in by_parent {
        let non_dos: Vec<&ParsedFileName> = group
            .iter()
            .copied()
            .filter(|n| {
                matches!(
                    n.namespace,
                    NAMESPACE_WIN32 | NAMESPACE_WIN32_AND_DOS | NAMESPACE_POSIX
                )
            })
            .collect();
        if !non_dos.is_empty() {
            chosen.extend(non_dos);
        } else {
            // Only DOS 8.3 name present for this parent
            chosen.extend(group);
        }
    }
    chosen
}

/// Builds a finalized flat `ScanTree` (with derived indexes) from parsed MFT records.
/// Merges `$ATTRIBUTE_LIST` extension records into their base FRN, deduplicates hardlinks,
/// optionally includes ADS, and preserves cloud placeholder & reparse point semantics.
pub fn build_tree_from_mft_records(
    root_path: PathBuf,
    volume_serial: u32,
    records: Vec<ParsedMftRecord>,
    include_ads: bool,
    ctx: &DerivedBuildContext,
    duration_ms: u64,
    engine_mode: ScanEngineMode,
) -> ScanTree {
    // 1. Merge extension records (`base_frn != frn`) into their base record
    let mut base_map: HashMap<u64, ParsedMftRecord> = HashMap::with_capacity(records.len());
    let mut extensions: Vec<ParsedMftRecord> = Vec::new();

    for rec in records {
        if rec.base_frn == rec.frn {
            base_map.insert(rec.frn, rec);
        } else {
            extensions.push(rec);
        }
    }

    for ext in extensions {
        if let Some(base) = base_map.get_mut(&ext.base_frn) {
            base.names.extend(ext.names);
            if ext.has_default_data {
                base.has_default_data = true;
                base.default_logical_size = base.default_logical_size.max(ext.default_logical_size);
                base.default_allocated_size = base
                    .default_allocated_size
                    .saturating_add(ext.default_allocated_size);
            }
            base.ads_count += ext.ads_count;
            base.ads_logical_size = base.ads_logical_size.saturating_add(ext.ads_logical_size);
            base.ads_allocated_size = base
                .ads_allocated_size
                .saturating_add(ext.ads_allocated_size);
            if ext.reparse_tag != 0 && base.reparse_tag == 0 {
                base.reparse_tag = ext.reparse_tag;
            }
            if ext.last_write_ts > base.last_write_ts {
                base.last_write_ts = ext.last_write_ts;
            }
        }
    }

    let mut tree = ScanTree::with_capacity(
        root_path,
        volume_serial,
        FileSystemType::Ntfs,
        base_map.len().saturating_add(64),
    );
    tree.include_ads = include_ads;
    tree.stats.engine_mode = engine_mode;
    // Map NTFS root directory FRN (5) to NodeId 0
    tree.file_ref[0] = MFT_ROOT_DIR_FRN;
    tree.frn_to_node.insert(MFT_ROOT_DIR_FRN, 0);

    // Pre-create directory nodes first so parent lookups succeed in a single pass
    // Build parent->children adjacency by FRN, skipping junctions/symlinks as parents
    let mut children_by_parent: HashMap<u64, Vec<(u64, usize)>> = HashMap::new();
    let mut ordered_frns: Vec<u64> = base_map.keys().copied().collect();
    ordered_frns.sort_unstable();

    // Store resolved preferred names per FRN
    let mut resolved_names: HashMap<u64, Vec<ParsedFileName>> =
        HashMap::with_capacity(base_map.len());
    for &frn in &ordered_frns {
        if frn == MFT_ROOT_DIR_FRN {
            continue;
        }
        // Skip internal NTFS metafiles ($MFT..$Extend = FRN 0..15 except root 5)
        if frn < 16 {
            continue;
        }
        let rec = &base_map[&frn];
        let chosen: Vec<ParsedFileName> = select_preferred_names(&rec.names)
            .into_iter()
            .cloned()
            .collect();
        for (link_idx, pfn) in chosen.iter().enumerate() {
            if pfn.name == "." || pfn.name == ".." {
                continue;
            }
            children_by_parent
                .entry(pfn.parent_frn)
                .or_default()
                .push((frn, link_idx));
        }
        resolved_names.insert(frn, chosen);
    }

    // BFS from NTFS root FRN (5) so unreachable/orphaned MFT entries and junction targets are skipped
    let mut queue: Vec<(u64, u32)> = vec![(MFT_ROOT_DIR_FRN, 0)];
    let mut qi = 0usize;

    while qi < queue.len() {
        let (parent_frn, parent_nid) = queue[qi];
        qi += 1;

        let Some(kids) = children_by_parent.get(&parent_frn) else {
            continue;
        };

        for &(child_frn, link_idx) in kids {
            let Some(rec) = base_map.get(&child_frn) else {
                continue;
            };
            let Some(pfn) = resolved_names.get(&child_frn).and_then(|v| v.get(link_idx)) else {
                continue;
            };

            let is_reparse =
                (rec.attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0 || rec.reparse_tag != 0;
            if is_reparse {
                if rec.reparse_tag == IO_REPARSE_TAG_SYMLINK {
                    tree.add_child_with_frn(
                        parent_nid,
                        &pfn.name,
                        NodeKind::Symlink,
                        0,
                        0,
                        rec.last_write_ts.max(pfn.last_write_ts),
                        NodeFlags::empty(),
                        child_frn,
                    );
                    tree.stats.symlinks_skipped += 1;
                    continue;
                }
                if rec.reparse_tag == IO_REPARSE_TAG_MOUNT_POINT {
                    tree.add_child_with_frn(
                        parent_nid,
                        &pfn.name,
                        NodeKind::Junction,
                        0,
                        0,
                        rec.last_write_ts.max(pfn.last_write_ts),
                        NodeFlags::empty(),
                        child_frn,
                    );
                    tree.stats.junctions_skipped += 1;
                    continue;
                }
            }

            if rec.is_dir || (rec.attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0 {
                let cid = tree.add_child_with_frn(
                    parent_nid,
                    &pfn.name,
                    NodeKind::Dir,
                    0,
                    0,
                    rec.last_write_ts.max(pfn.last_write_ts),
                    NodeFlags::empty(),
                    child_frn,
                );
                // Only enqueue directory traversal once (link_idx == 0)
                if link_idx == 0 {
                    queue.push((child_frn, cid));
                }
            } else {
                let mut flags = NodeFlags::empty();
                let is_compressed = (rec.attrs & FILE_ATTRIBUTE_COMPRESSED.0) != 0;
                let is_sparse = (rec.attrs & FILE_ATTRIBUTE_SPARSE_FILE.0) != 0;
                let is_recall_data = (rec.attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
                let is_recall_open = (rec.attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN.0) != 0;
                let is_offline = (rec.attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;

                if is_compressed {
                    flags |= NodeFlags::COMPRESSED;
                }
                if is_sparse {
                    flags |= NodeFlags::SPARSE;
                }
                if is_recall_open {
                    flags |= NodeFlags::RECALL_ON_OPEN;
                }
                if is_offline {
                    flags |= NodeFlags::OFFLINE;
                }
                if rec.ads_count > 0 {
                    flags |= NodeFlags::HAS_ADS;
                }

                let is_cloud = is_recall_data || is_recall_open || is_offline;
                if is_cloud {
                    flags |= NodeFlags::CLOUD_ONLY;
                    tree.stats.cloud_placeholders += 1;
                }

                let mut logical = rec.default_logical_size;
                let mut allocated = rec.default_allocated_size;
                if include_ads && rec.ads_count > 0 {
                    logical = logical.saturating_add(rec.ads_logical_size);
                    allocated = allocated.saturating_add(rec.ads_allocated_size);
                    tree.stats.ads_streams_counted += rec.ads_count as u64;
                }

                // Count hardlinks once (first link_idx == 0 keeps allocated bytes, duplicates get 0)
                if link_idx > 0 {
                    flags |= NodeFlags::HARDLINK_DUP;
                    allocated = 0;
                    tree.stats.hardlinks_deduplicated += 1;
                }

                let kind = if is_cloud {
                    NodeKind::CloudPlaceholder
                } else {
                    NodeKind::File
                };

                tree.add_child_with_frn(
                    parent_nid,
                    &pfn.name,
                    kind,
                    logical,
                    allocated,
                    rec.last_write_ts.max(pfn.last_write_ts),
                    flags,
                    child_frn,
                );
            }
        }
    }

    tree.shrink_build_caches();
    finalize_tree_and_derived(&mut tree, ctx);
    tree.stats.duration_ms = duration_ms;
    tree
}

fn open_raw_volume_readonly(drive_letter: char) -> Result<HANDLE, String> {
    let vol_path = format!(r"\\.\{}:", drive_letter.to_ascii_uppercase());
    let wide: Vec<u16> = vol_path.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_SEQUENTIAL_SCAN,
            None,
        )
    }
    .map_err(|e| format!("No se pudo abrir volumen raw {vol_path}: {e}"))?;

    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("Handle inválido abriendo {vol_path}"));
    }
    Ok(handle)
}

pub fn query_ntfs_volume_data(handle: HANDLE) -> Result<NtfsVolumeDataBuffer, String> {
    let mut data = NtfsVolumeDataBuffer::default();
    let mut bytes_returned = 0u32;
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_NTFS_VOLUME_DATA,
            None,
            0,
            Some((&mut data as *mut NtfsVolumeDataBuffer).cast()),
            std::mem::size_of::<NtfsVolumeDataBuffer>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|e| format!("FSCTL_GET_NTFS_VOLUME_DATA falló: {e}"))?;
    Ok(data)
}

fn read_volume_at(handle: HANDLE, offset: i64, buf: &mut [u8]) -> Result<usize, String> {
    unsafe {
        SetFilePointerEx(handle, offset, None, FILE_BEGIN)
            .map_err(|e| format!("SetFilePointerEx({offset}) falló: {e}"))?;
        let mut read = 0u32;
        ReadFile(handle, Some(buf), Some(&mut read), None)
            .map_err(|e| format!("ReadFile({offset}) falló: {e}"))?;
        Ok(read as usize)
    }
}

/// Extracts `$MFT` data runs from record 0 so fragmented `$MFT` streams are read sequentially.
fn extract_mft_self_runs(record0: &[u8], sector_size: usize) -> Option<Vec<DataRun>> {
    let mut buf = record0.to_vec();
    if !apply_usa_fixup(&mut buf, sector_size) {
        return None;
    }
    let mut off = u16::from_le_bytes([buf[20], buf[21]]) as usize;
    while off + 16 <= buf.len() {
        let attr_type = u32::from_le_bytes(buf[off..off + 4].try_into().ok()?);
        if attr_type == ATTR_END {
            break;
        }
        let attr_len = u32::from_le_bytes(buf[off + 4..off + 8].try_into().ok()?) as usize;
        if attr_len < 16 || off + attr_len > buf.len() {
            break;
        }
        let non_resident = buf[off + 8] != 0;
        let name_len = buf[off + 9];
        if attr_type == ATTR_DATA && non_resident && name_len == 0 && off + 34 <= off + attr_len {
            let run_off = u16::from_le_bytes([buf[off + 32], buf[off + 33]]) as usize;
            if off + run_off < off + attr_len {
                return Some(parse_data_runs(&buf[off + run_off..off + attr_len]));
            }
        }
        off += attr_len;
    }
    None
}

/// Scans an NTFS volume directly via `$MFT` in large sequential blocks (8 MB) with parallel Rayon parsing.
/// Requires Administrator privileges (or being called from the elevated read-only indexing service).
#[allow(clippy::too_many_arguments)]
pub fn scan_volume_mft_direct<F>(
    root_path: PathBuf,
    volume_serial: u32,
    include_ads: bool,
    cancel_flag: Arc<AtomicBool>,
    ctx: &DerivedBuildContext,
    engine_mode: ScanEngineMode,
    on_progress: F,
) -> Result<ScanTree, String>
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let start = Instant::now();
    let root_str = root_path.to_string_lossy().into_owned();
    let drive_char = root_str
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("Ruta no válida para lectura MFT: {root_str}"))?;

    let handle = open_raw_volume_readonly(drive_char)?;
    let vol_data = match query_ntfs_volume_data(handle) {
        Ok(d) => d,
        Err(e) => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(e);
        }
    };

    let sector_size = vol_data.BytesPerSector.max(512) as usize;
    let bytes_per_cluster = vol_data.BytesPerCluster.max(512) as u64;
    let record_size = vol_data.BytesPerFileRecordSegment.max(1024) as usize;
    let mft_valid_len = vol_data.MftValidDataLength.max(0) as u64;
    let total_records = mft_valid_len / (record_size as u64);

    if total_records == 0 {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err("MftValidDataLength es 0".to_string());
    }

    // Read record 0 to discover $MFT data runs (handles fragmented $MFT)
    let mft_start_offset = vol_data
        .MftStartLcn
        .saturating_mul(bytes_per_cluster as i64);
    let mut rec0_buf = vec![0u8; record_size.max(sector_size)];
    if let Err(e) = read_volume_at(handle, mft_start_offset, &mut rec0_buf) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(e);
    }

    let runs = extract_mft_self_runs(&rec0_buf[..record_size], sector_size).unwrap_or_else(|| {
        vec![DataRun {
            cluster_count: mft_valid_len.div_ceil(bytes_per_cluster),
            lcn: Some(vol_data.MftStartLcn.max(0) as u64),
        }]
    });

    let mut all_records: Vec<ParsedMftRecord> = Vec::with_capacity((total_records as usize) / 2);
    let mut chunk_buf = vec![0u8; MFT_READ_CHUNK_BYTES];
    let processed_records = AtomicU64::new(0);
    let last_emit = Mutex::new(Instant::now() - Duration::from_millis(200));

    let mut current_record_idx = 0u64;
    let mut bytes_remaining = mft_valid_len;

    for run in runs {
        if cancel_flag.load(Ordering::Relaxed) || bytes_remaining == 0 {
            break;
        }
        let run_bytes = run
            .cluster_count
            .saturating_mul(bytes_per_cluster)
            .min(bytes_remaining);
        let Some(lcn) = run.lcn else {
            let skipped_recs = run_bytes / (record_size as u64);
            current_record_idx += skipped_recs;
            bytes_remaining = bytes_remaining.saturating_sub(run_bytes);
            continue;
        };

        let mut run_offset = lcn.saturating_mul(bytes_per_cluster);
        let mut left_in_run = run_bytes;

        while left_in_run > 0 && !cancel_flag.load(Ordering::Relaxed) {
            let to_read =
                (left_in_run as usize).min(MFT_READ_CHUNK_BYTES) / record_size * record_size;
            if to_read == 0 {
                break;
            }
            let got = match read_volume_at(handle, run_offset as i64, &mut chunk_buf[..to_read]) {
                Ok(n) => n,
                Err(_) => break,
            };
            let recs_in_chunk = got / record_size;
            if recs_in_chunk == 0 {
                break;
            }

            let base_idx = current_record_idx;
            let parsed_chunk: Vec<ParsedMftRecord> = chunk_buf[..recs_in_chunk * record_size]
                .par_chunks_exact(record_size)
                .enumerate()
                .filter_map(|(i, slice)| {
                    parse_mft_record_bytes(
                        slice,
                        base_idx + i as u64,
                        sector_size,
                        bytes_per_cluster,
                    )
                })
                .collect();

            all_records.extend(parsed_chunk);
            current_record_idx += recs_in_chunk as u64;
            let done_now = processed_records.fetch_add(recs_in_chunk as u64, Ordering::Relaxed)
                + recs_in_chunk as u64;
            run_offset += got as u64;
            left_in_run = left_in_run.saturating_sub(got as u64);
            bytes_remaining = bytes_remaining.saturating_sub(got as u64);

            if let Ok(mut last) = last_emit.lock() {
                if last.elapsed() >= Duration::from_millis(100) {
                    *last = Instant::now();
                    let ratio = ((done_now as f64) / (total_records.max(1) as f64)).min(0.95);
                    on_progress(ScanProgress {
                        drive_root: root_str.clone(),
                        phase: format!("Escaneando {root_str} (MFT)…"),
                        engine_mode,
                        scanned_files: all_records.len() as u64,
                        scanned_dirs: 0,
                        total_allocated_bytes: 0,
                        current_path: format!("$MFT ({done_now}/{total_records})"),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        mft_processed: Some(done_now),
                        mft_total: Some(total_records),
                        volume_used_bytes: None,
                        usn_processed: None,
                        usn_total: None,
                        progress_ratio: Some(ratio),
                    });
                }
            }
        }
    }

    unsafe {
        let _ = CloseHandle(handle);
    }

    on_progress(ScanProgress {
        drive_root: root_str.clone(),
        phase: "Calculando categorías…".to_string(),
        engine_mode,
        scanned_files: all_records.len() as u64,
        scanned_dirs: 0,
        total_allocated_bytes: 0,
        current_path: root_str.clone(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        mft_processed: Some(total_records),
        mft_total: Some(total_records),
        volume_used_bytes: None,
        usn_processed: None,
        usn_total: None,
        progress_ratio: Some(0.97),
    });

    let mut tree = build_tree_from_mft_records(
        root_path,
        volume_serial,
        all_records,
        include_ads,
        ctx,
        start.elapsed().as_millis() as u64,
        engine_mode,
    );
    tree.stats.cancelled = cancel_flag.load(Ordering::Relaxed);

    on_progress(ScanProgress {
        drive_root: root_str.clone(),
        phase: "Preparando limpieza…".to_string(),
        engine_mode,
        scanned_files: tree.stats.total_files,
        scanned_dirs: tree.stats.total_dirs,
        total_allocated_bytes: tree.stats.total_allocated_bytes,
        current_path: root_str,
        elapsed_ms: tree.stats.duration_ms,
        mft_processed: Some(total_records),
        mft_total: Some(total_records),
        volume_used_bytes: None,
        usn_processed: None,
        usn_total: None,
        progress_ratio: Some(1.0),
    });

    Ok(tree)
}
