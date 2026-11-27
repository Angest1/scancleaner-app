use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL};
use windows::Win32::System::IO::DeviceIoControl;

use crate::scanner::derived::{refresh_derived_for_subtree, DerivedBuildContext};
use crate::scanner::model::{NodeFlags, NodeId, NodeKind, ScanTree, NONE_NODE};

pub const USN_REASON_DATA_OVERWRITE: u32 = 0x0000_0001;
pub const USN_REASON_DATA_EXTEND: u32 = 0x0000_0002;
pub const USN_REASON_DATA_TRUNCATION: u32 = 0x0000_0004;
pub const USN_REASON_FILE_CREATE: u32 = 0x0000_0100;
pub const USN_REASON_FILE_DELETE: u32 = 0x0000_0200;
pub const USN_REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
pub const USN_REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;
pub const USN_REASON_BASIC_INFO_CHANGE: u32 = 0x0000_8000;
pub const USN_REASON_CLOSE: u32 = 0x8000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsnJournalInfo {
    pub usn_journal_id: u64,
    pub first_usn: i64,
    pub next_usn: i64,
    pub lowest_valid_usn: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsnEvent {
    pub usn: i64,
    pub frn: u64,
    pub parent_frn: u64,
    pub reason: u32,
    pub file_attributes: u32,
    pub file_name: String,
    pub timestamp_unix: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSizeSnapshot {
    pub logical_size: u64,
    pub allocated_size: u64,
    pub last_write_ts: i64,
    pub is_dir: bool,
}

/// Mockable metadata lookup for USN delta size updates (so tests never touch real disks).
pub trait FileMetaLookup: Send + Sync {
    fn lookup_by_path(&self, path: &Path) -> Option<FileSizeSnapshot>;
}

pub struct RealFileMetaLookup;

impl FileMetaLookup for RealFileMetaLookup {
    fn lookup_by_path(&self, path: &Path) -> Option<FileSizeSnapshot> {
        let meta = std::fs::symlink_metadata(path).ok()?;
        let is_dir = meta.is_dir();
        let len = if is_dir { 0 } else { meta.len() };
        let ts = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        Some(FileSizeSnapshot {
            logical_size: len,
            allocated_size: len,
            last_write_ts: ts,
            is_dir,
        })
    }
}

/// Mockable USN journal reader trait.
pub trait UsnJournalReader: Send + Sync {
    fn query_journal(&self, drive_root: &str) -> Result<UsnJournalInfo, String>;
    fn read_records(
        &self,
        drive_root: &str,
        journal_id: u64,
        start_usn: i64,
    ) -> Result<(Vec<UsnEvent>, i64), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsnSyncOutcome {
    UpToDate {
        applied_events: usize,
        new_last_usn: i64,
        touched_subtrees: Vec<NodeId>,
    },
    NeedsFullRescan {
        reason: String,
    },
    Unavailable {
        reason: String,
    },
}

#[repr(C)]
#[derive(Default)]
#[allow(non_snake_case)]
struct UsnJournalDataV0 {
    UsnJournalID: u64,
    FirstUsn: i64,
    NextUsn: i64,
    LowestValidUsn: i64,
    MaxUsn: i64,
    MaximumSize: u64,
    AllocationDelta: u64,
}

#[repr(C)]
#[allow(non_snake_case)]
struct ReadUsnJournalDataV0 {
    StartUsn: i64,
    ReasonMask: u32,
    ReturnOnlyOnClose: u32,
    Timeout: u64,
    BytesToWaitFor: u64,
    UsnJournalID: u64,
}

#[repr(C)]
#[allow(non_snake_case)]
struct UsnRecordV2Header {
    RecordLength: u32,
    MajorVersion: u16,
    MinorVersion: u16,
    FileReferenceNumber: u64,
    ParentFileReferenceNumber: u64,
    Usn: i64,
    TimeStamp: i64,
    Reason: u32,
    SourceInfo: u32,
    SecurityId: u32,
    FileAttributes: u32,
    FileNameLength: u16,
    FileNameOffset: u16,
}

fn open_volume_handle(drive_root: &str) -> Result<HANDLE, String> {
    let ch = drive_root
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("Drive inválido para USN: {drive_root}"))?;
    let vol_path = format!(r"\\.\{}:", ch.to_ascii_uppercase());
    let wide: Vec<u16> = vol_path.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|e| format!("No se pudo abrir volumen {vol_path} para USN: {e}"))?;
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("Handle inválido para {vol_path}"));
    }
    Ok(handle)
}

pub fn query_usn_journal_for_root(drive_root: &str) -> Result<UsnJournalInfo, String> {
    let handle = open_volume_handle(drive_root)?;
    let mut out = UsnJournalDataV0::default();
    let mut returned = 0u32;
    let res = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some((&mut out as *mut UsnJournalDataV0).cast()),
            std::mem::size_of::<UsnJournalDataV0>() as u32,
            Some(&mut returned),
            None,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    res.map_err(|e| format!("FSCTL_QUERY_USN_JOURNAL falló: {e}"))?;
    Ok(UsnJournalInfo {
        usn_journal_id: out.UsnJournalID,
        first_usn: out.FirstUsn,
        next_usn: out.NextUsn,
        lowest_valid_usn: out.LowestValidUsn,
    })
}

pub struct WinUsnJournalReader;

impl UsnJournalReader for WinUsnJournalReader {
    fn query_journal(&self, drive_root: &str) -> Result<UsnJournalInfo, String> {
        query_usn_journal_for_root(drive_root)
    }

    fn read_records(
        &self,
        drive_root: &str,
        journal_id: u64,
        start_usn: i64,
    ) -> Result<(Vec<UsnEvent>, i64), String> {
        let handle = open_volume_handle(drive_root)?;
        let mut events = Vec::new();
        let mut next_usn = start_usn;
        let mut buf = vec![0u8; 256 * 1024];

        let mask = USN_REASON_FILE_CREATE
            | USN_REASON_FILE_DELETE
            | USN_REASON_RENAME_OLD_NAME
            | USN_REASON_RENAME_NEW_NAME
            | USN_REASON_DATA_EXTEND
            | USN_REASON_DATA_TRUNCATION
            | USN_REASON_DATA_OVERWRITE
            | USN_REASON_BASIC_INFO_CHANGE
            | USN_REASON_CLOSE;

        loop {
            let req = ReadUsnJournalDataV0 {
                StartUsn: next_usn,
                ReasonMask: mask,
                ReturnOnlyOnClose: 0,
                Timeout: 0,
                BytesToWaitFor: 0,
                UsnJournalID: journal_id,
            };
            let mut bytes_ret = 0u32;
            let res = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_READ_USN_JOURNAL,
                    Some((&req as *const ReadUsnJournalDataV0).cast()),
                    std::mem::size_of::<ReadUsnJournalDataV0>() as u32,
                    Some(buf.as_mut_ptr().cast()),
                    buf.len() as u32,
                    Some(&mut bytes_ret),
                    None,
                )
            };
            if let Err(e) = res {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(format!("FSCTL_READ_USN_JOURNAL falló: {e}"));
            }
            if bytes_ret < 8 {
                break;
            }
            let new_next = i64::from_le_bytes(buf[0..8].try_into().unwrap());
            if bytes_ret == 8 || new_next == next_usn {
                next_usn = new_next;
                break;
            }
            next_usn = new_next;

            let mut off = 8usize;
            while off + std::mem::size_of::<UsnRecordV2Header>() <= bytes_ret as usize {
                let hdr = unsafe { &*(buf.as_ptr().add(off).cast::<UsnRecordV2Header>()) };
                let rec_len = hdr.RecordLength as usize;
                if rec_len == 0 || off + rec_len > bytes_ret as usize {
                    break;
                }
                if hdr.MajorVersion == 2 {
                    let name_off = off + hdr.FileNameOffset as usize;
                    let name_len = hdr.FileNameLength as usize;
                    if name_off + name_len <= off + rec_len {
                        let utf16: Vec<u16> = buf[name_off..name_off + name_len]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|c| u16::from_le_bytes(*c))
                            .collect();
                        let file_name = String::from_utf16_lossy(&utf16);
                        events.push(UsnEvent {
                            usn: hdr.Usn,
                            frn: hdr.FileReferenceNumber & 0x0000_FFFF_FFFF_FFFF,
                            parent_frn: hdr.ParentFileReferenceNumber & 0x0000_FFFF_FFFF_FFFF,
                            reason: hdr.Reason,
                            file_attributes: hdr.FileAttributes,
                            file_name,
                            timestamp_unix: crate::scanner::mft::filetime_to_unix_secs(
                                hdr.TimeStamp,
                            ),
                        });
                    }
                }
                off += rec_len;
            }
        }

        unsafe {
            let _ = CloseHandle(handle);
        }
        Ok((events, next_usn))
    }
}

/// Validates journal continuity (`UsnJournalID` + `FirstUsn`) and applies incremental USN deltas
/// directly onto `tree`, bubbling size deltas up parent chains and refreshing affected derived indexes.
pub fn sync_tree_with_usn(
    tree: &mut ScanTree,
    reader: &dyn UsnJournalReader,
    meta_lookup: &dyn FileMetaLookup,
    ctx: &DerivedBuildContext,
) -> UsnSyncOutcome {
    let drive_root = tree.root_path.to_string_lossy().into_owned();
    let info = match reader.query_journal(&drive_root) {
        Ok(i) => i,
        Err(reason) => return UsnSyncOutcome::Unavailable { reason },
    };

    if tree.usn_journal_id == 0 {
        return UsnSyncOutcome::NeedsFullRescan {
            reason: "Snapshot no tiene UsnJournalID previo.".to_string(),
        };
    }
    if info.usn_journal_id != tree.usn_journal_id {
        return UsnSyncOutcome::NeedsFullRescan {
            reason: format!(
                "UsnJournalID cambió (guardado={}, actual={}): journal recreado.",
                tree.usn_journal_id, info.usn_journal_id
            ),
        };
    }
    if tree.last_usn < info.first_usn {
        return UsnSyncOutcome::NeedsFullRescan {
            reason: format!(
                "USN guardado ({}) anterior a FirstUsn ({}): journal truncado.",
                tree.last_usn, info.first_usn
            ),
        };
    }
    if tree.last_usn >= info.next_usn {
        return UsnSyncOutcome::UpToDate {
            applied_events: 0,
            new_last_usn: tree.last_usn,
            touched_subtrees: Vec::new(),
        };
    }

    let (events, new_next_usn) =
        match reader.read_records(&drive_root, info.usn_journal_id, tree.last_usn) {
            Ok(v) => v,
            Err(reason) => return UsnSyncOutcome::NeedsFullRescan { reason },
        };

    let (applied, touched) = apply_usn_events_to_tree(tree, &events, meta_lookup, ctx);
    tree.last_usn = new_next_usn.max(tree.last_usn);
    tree.stats.stale = false;

    UsnSyncOutcome::UpToDate {
        applied_events: applied,
        new_last_usn: tree.last_usn,
        touched_subtrees: touched,
    }
}

pub fn apply_usn_events_to_tree(
    tree: &mut ScanTree,
    events: &[UsnEvent],
    meta_lookup: &dyn FileMetaLookup,
    ctx: &DerivedBuildContext,
) -> (usize, Vec<NodeId>) {
    let mut touched_parents: HashSet<NodeId> = HashSet::new();
    let mut applied = 0usize;

    for ev in events {
        let is_dir = (ev.file_attributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0;

        // 1. Deletion
        if (ev.reason & USN_REASON_FILE_DELETE) != 0 {
            if let Some(nid) = tree.frn_to_node.get(&ev.frn).copied() {
                if let Some(pid) = tree.remove_subtree(nid) {
                    if pid != NONE_NODE {
                        touched_parents.insert(pid);
                    }
                    applied += 1;
                }
            }
            continue;
        }

        // 2. Rename / Move (`RENAME_NEW_NAME` carries final parent_frn and final file_name)
        if (ev.reason & USN_REASON_RENAME_NEW_NAME) != 0 {
            if let Some(nid) = tree.frn_to_node.get(&ev.frn).copied() {
                let old_parent = tree.parent_of(nid);
                tree.rename_node(nid, &ev.file_name);
                if let Some(new_parent) = tree.frn_to_node.get(&ev.parent_frn).copied() {
                    if new_parent != old_parent && tree.move_node(nid, new_parent) {
                        if old_parent != NONE_NODE {
                            touched_parents.insert(old_parent);
                        }
                        touched_parents.insert(new_parent);
                    } else if old_parent != NONE_NODE {
                        touched_parents.insert(old_parent);
                    }
                } else if old_parent != NONE_NODE {
                    touched_parents.insert(old_parent);
                }
                applied += 1;
                continue;
            }
        }

        if (ev.reason & USN_REASON_RENAME_OLD_NAME) != 0
            && (ev.reason & USN_REASON_RENAME_NEW_NAME) == 0
        {
            // Wait for paired RENAME_NEW_NAME
            continue;
        }

        // 3. Create or Data size change
        let is_create = (ev.reason & USN_REASON_FILE_CREATE) != 0;
        let is_size_change = (ev.reason
            & (USN_REASON_DATA_EXTEND
                | USN_REASON_DATA_TRUNCATION
                | USN_REASON_DATA_OVERWRITE
                | USN_REASON_CLOSE))
            != 0;

        if let Some(nid) = tree.frn_to_node.get(&ev.frn).copied() {
            if is_size_change && !is_dir {
                let full_path = tree.reconstruct_path(nid);
                if let Some(snap) = meta_lookup.lookup_by_path(&full_path) {
                    tree.update_file_sizes(
                        nid,
                        snap.logical_size,
                        snap.allocated_size,
                        snap.last_write_ts.max(ev.timestamp_unix),
                    );
                    let pid = tree.parent_of(nid);
                    if pid != NONE_NODE {
                        touched_parents.insert(pid);
                    }
                    applied += 1;
                }
            }
        } else if is_create || is_size_change {
            if let Some(parent_nid) = tree.frn_to_node.get(&ev.parent_frn).copied() {
                let child_path = tree.reconstruct_path(parent_nid).join(&ev.file_name);
                if is_dir {
                    let cid = tree.add_child_with_frn(
                        parent_nid,
                        &ev.file_name,
                        NodeKind::Dir,
                        0,
                        0,
                        ev.timestamp_unix,
                        NodeFlags::empty(),
                        ev.frn,
                    );
                    tree.stats.total_dirs += 1;
                    let _ = cid;
                    touched_parents.insert(parent_nid);
                    applied += 1;
                } else {
                    let snap =
                        meta_lookup
                            .lookup_by_path(&child_path)
                            .unwrap_or(FileSizeSnapshot {
                                logical_size: 0,
                                allocated_size: 0,
                                last_write_ts: ev.timestamp_unix,
                                is_dir: false,
                            });
                    let cid = tree.add_child_with_frn(
                        parent_nid,
                        &ev.file_name,
                        NodeKind::File,
                        snap.logical_size,
                        snap.allocated_size,
                        snap.last_write_ts.max(ev.timestamp_unix),
                        NodeFlags::empty(),
                        ev.frn,
                    );
                    tree.propagate_delta_upwards(
                        parent_nid,
                        snap.allocated_size as i64,
                        snap.logical_size as i64,
                        1,
                    );
                    // Classify the new file and add its contribution to category totals
                    let full_low = child_path.to_string_lossy().to_ascii_lowercase();
                    let name_low = ev.file_name.to_ascii_lowercase();
                    let ext = name_low.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
                    let tcat =
                        crate::categorizer::classify_extension(ext, &ctx.rules_config.custom);
                    let ocat = crate::categorizer::classify_origin_path(
                        &full_low,
                        &name_low,
                        &ctx.rules_config,
                    );
                    tree.set_type_cat(cid, tcat);
                    tree.set_origin_cat(cid, ocat);
                    tree.derived.apply_file_delta(
                        tcat,
                        ocat,
                        snap.allocated_size as i64,
                        snap.logical_size as i64,
                        1,
                    );
                    touched_parents.insert(parent_nid);
                    applied += 1;
                }
            }
        }
    }

    let mut touched_vec: Vec<NodeId> = touched_parents.into_iter().collect();
    touched_vec.sort_unstable();
    for &pid in &touched_vec {
        refresh_derived_for_subtree(tree, pid, ctx);
    }
    (applied, touched_vec)
}
