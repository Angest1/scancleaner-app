use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS,
    STATUS_NO_MORE_FILES, UNICODE_STRING,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW,
    FindNextFileW, GetCompressedFileSizeW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_SPARSE_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FIND_FIRST_EX_LARGE_FETCH, INVALID_FILE_ATTRIBUTES,
    OPEN_EXISTING, WIN32_FIND_DATAW,
};
use windows::Win32::System::IO::IO_STATUS_BLOCK;

use super::model::{FileSystemType, NodeFlags, NodeKind, ScanProgress, ScanTree};

const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
const FILE_ID_BOTH_DIRECTORY_INFORMATION_CLASS: u32 = 37;
const STATUS_ACCESS_DENIED_RAW: i32 = -1073741790; // 0xC0000022
const STATUS_SUCCESS_RAW: i32 = 0;

#[repr(C)]
#[allow(non_snake_case)]
struct FileIdBothDirInfo {
    NextEntryOffset: u32,
    FileIndex: u32,
    CreationTime: i64,
    LastAccessTime: i64,
    LastWriteTime: i64,
    ChangeTime: i64,
    EndOfFile: i64,
    AllocationSize: i64,
    FileAttributes: u32,
    FileNameLength: u32,
    EaSize: u32,
    ShortNameLength: i8,
    ShortName: [u16; 12],
    FileId: i64,
    FileName: [u16; 1],
}

type NtQueryDirectoryFileFn = unsafe extern "system" fn(
    file_handle: HANDLE,
    event: HANDLE,
    apc_routine: *const core::ffi::c_void,
    apc_context: *const core::ffi::c_void,
    io_status_block: *mut IO_STATUS_BLOCK,
    file_information: *mut core::ffi::c_void,
    length: u32,
    file_information_class: u32,
    return_single_entry: u8,
    file_name: *const UNICODE_STRING,
    restart_scan: u8,
) -> NTSTATUS;

fn get_nt_query_directory_file() -> Option<NtQueryDirectoryFileFn> {
    use std::sync::OnceLock;
    use windows::core::PCSTR;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    static FN: OnceLock<Option<NtQueryDirectoryFileFn>> = OnceLock::new();
    *FN.get_or_init(|| unsafe {
        let hmod = GetModuleHandleA(PCSTR(c"ntdll.dll".as_ptr().cast())).ok()?;
        let proc = GetProcAddress(hmod, PCSTR(c"NtQueryDirectoryFile".as_ptr().cast()))?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            NtQueryDirectoryFileFn,
        >(proc))
    })
}

pub struct RawEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_reparse: bool,
    pub reparse_tag: u32,
    pub is_compressed: bool,
    pub is_sparse: bool,
    pub is_cloud_only: bool,
    pub logical_size: u64,
    pub allocated_size: u64,
    pub last_write_ts: i64,
    pub file_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirReadError {
    AccessDenied,
    UnsupportedNtQuery,
    Other,
}

pub fn filetime_to_unix_secs(ft_100ns: i64) -> i64 {
    const WINDOWS_TO_UNIX_EPOCH_100NS: i64 = 116_444_736_000_000_000;
    if ft_100ns <= WINDOWS_TO_UNIX_EPOCH_100NS {
        0
    } else {
        (ft_100ns - WINDOWS_TO_UNIX_EPOCH_100NS) / 10_000_000
    }
}

pub fn is_cloud_reparse_tag(tag: u32) -> bool {
    (tag & 0xFFFF_0000) == 0x9000_0000
}

fn to_extended_wide(path: &Path) -> Vec<u16> {
    let s = path.as_os_str().to_string_lossy();
    let extended = if s.starts_with(r"\\?\") || s.starts_with(r"\\.\") {
        s.into_owned()
    } else if let Some(stripped) = s.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{stripped}")
    } else {
        format!(r"\\?\{s}")
    };
    extended.encode_utf16().chain(std::iter::once(0)).collect()
}

fn query_compressed_size(full_path: &Path, fallback: u64) -> u64 {
    let wide = to_extended_wide(full_path);
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };
    if low == INVALID_FILE_ATTRIBUTES {
        // Fallback when GetCompressedFileSizeW cannot query (e.g., locked or access denied)
        fallback
    } else {
        ((high as u64) << 32) | (low as u64)
    }
}

pub fn read_dir_nt(dir_path: &Path) -> Result<Vec<RawEntry>, DirReadError> {
    let nt_query = get_nt_query_directory_file().ok_or(DirReadError::UnsupportedNtQuery)?;
    let wide = to_extended_wide(dir_path);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|e| {
        if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
            DirReadError::AccessDenied
        } else {
            DirReadError::Other
        }
    })?;

    if handle == INVALID_HANDLE_VALUE {
        return Err(DirReadError::Other);
    }

    let mut entries = Vec::new();
    let mut buf = vec![0u64; (64 * 1024) / 8]; // 8-byte aligned 64 KB buffer
    let mut io_status = IO_STATUS_BLOCK::default();
    let mut restart: u8 = 1;

    loop {
        let status = unsafe {
            nt_query(
                handle,
                HANDLE::default(),
                std::ptr::null(),
                std::ptr::null(),
                &mut io_status,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                FILE_ID_BOTH_DIRECTORY_INFORMATION_CLASS,
                0,
                std::ptr::null(),
                restart,
            )
        };
        restart = 0;

        if status.0 == STATUS_NO_MORE_FILES.0 {
            break;
        }
        if status.0 == STATUS_ACCESS_DENIED_RAW {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(DirReadError::AccessDenied);
        }
        if status.0 != STATUS_SUCCESS_RAW {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(DirReadError::UnsupportedNtQuery);
        }

        let base_ptr = buf.as_ptr().cast::<u8>();
        let mut offset = 0usize;
        loop {
            let info = unsafe { &*(base_ptr.add(offset).cast::<FileIdBothDirInfo>()) };
            let name_len_chars = (info.FileNameLength as usize) / 2;
            let name_ptr = info.FileName.as_ptr();
            let name_slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len_chars) };
            let name = OsString::from_wide(name_slice)
                .to_string_lossy()
                .into_owned();

            if name != "." && name != ".." {
                let attrs = info.FileAttributes;
                let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
                let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
                let reparse_tag = if is_reparse { info.EaSize } else { 0 };
                let is_compressed = (attrs & FILE_ATTRIBUTE_COMPRESSED.0) != 0;
                let is_sparse = (attrs & FILE_ATTRIBUTE_SPARSE_FILE.0) != 0;
                let is_recall = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
                let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;

                let logical = info.EndOfFile.max(0) as u64;
                let mut allocated = info.AllocationSize.max(0) as u64;

                let is_cloud_only = !is_dir
                    && ((is_recall && allocated == 0)
                        || (is_offline && is_cloud_reparse_tag(reparse_tag) && allocated == 0));

                if !is_dir && !is_cloud_only && (is_compressed || is_sparse) {
                    let child_path = dir_path.join(&name);
                    allocated = query_compressed_size(&child_path, allocated);
                }

                entries.push(RawEntry {
                    name,
                    is_dir,
                    is_reparse,
                    reparse_tag,
                    is_compressed,
                    is_sparse,
                    is_cloud_only,
                    logical_size: logical,
                    allocated_size: if is_dir { 0 } else { allocated },
                    last_write_ts: filetime_to_unix_secs(info.LastWriteTime),
                    file_id: info.FileId.max(0) as u64,
                });
            }

            if info.NextEntryOffset == 0 {
                break;
            }
            offset += info.NextEntryOffset as usize;
        }
    }

    unsafe {
        let _ = CloseHandle(handle);
    }
    Ok(entries)
}

pub fn read_dir_win32(dir_path: &Path) -> Result<Vec<RawEntry>, DirReadError> {
    let mut pattern_os = dir_path.as_os_str().to_os_string();
    let s = pattern_os.to_string_lossy();
    if !s.ends_with('\\') {
        pattern_os.push("\\");
    }
    pattern_os.push("*");
    let pattern_str = pattern_os.to_string_lossy();
    let extended = if pattern_str.starts_with(r"\\?\") {
        pattern_str.into_owned()
    } else if let Some(stripped) = pattern_str.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{stripped}")
    } else {
        format!(r"\\?\{pattern_str}")
    };
    let wide: Vec<u16> = extended.encode_utf16().chain(std::iter::once(0)).collect();

    let mut find_data = WIN32_FIND_DATAW::default();
    let handle = unsafe {
        FindFirstFileExW(
            PCWSTR(wide.as_ptr()),
            FindExInfoBasic,
            (&mut find_data as *mut WIN32_FIND_DATAW).cast(),
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };

    let handle = match handle {
        Ok(h) => h,
        Err(e) => {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                return Err(DirReadError::AccessDenied);
            }
            return Err(DirReadError::Other);
        }
    };

    let mut entries = Vec::new();
    loop {
        let end = find_data
            .cFileName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(find_data.cFileName.len());
        let name = OsString::from_wide(&find_data.cFileName[..end])
            .to_string_lossy()
            .into_owned();

        if name != "." && name != ".." {
            let attrs = find_data.dwFileAttributes;
            let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
            let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
            let reparse_tag = if is_reparse { find_data.dwReserved0 } else { 0 };
            let is_compressed = (attrs & FILE_ATTRIBUTE_COMPRESSED.0) != 0;
            let is_sparse = (attrs & FILE_ATTRIBUTE_SPARSE_FILE.0) != 0;
            let is_recall = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
            let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;

            let logical =
                ((find_data.nFileSizeHigh as u64) << 32) | (find_data.nFileSizeLow as u64);
            let ft = ((find_data.ftLastWriteTime.dwHighDateTime as i64) << 32)
                | (find_data.ftLastWriteTime.dwLowDateTime as i64);

            let is_cloud_only = !is_dir && is_recall;
            let allocated =
                if is_dir || is_cloud_only || (is_offline && is_cloud_reparse_tag(reparse_tag)) {
                    0
                } else if is_compressed || is_sparse {
                    query_compressed_size(&dir_path.join(&name), logical)
                } else {
                    logical
                };

            entries.push(RawEntry {
                name,
                is_dir,
                is_reparse,
                reparse_tag,
                is_compressed,
                is_sparse,
                is_cloud_only,
                logical_size: logical,
                allocated_size: allocated,
                last_write_ts: filetime_to_unix_secs(ft),
                file_id: 0,
            });
        }

        let next_res = unsafe { FindNextFileW(handle, &mut find_data) };
        if let Err(e) = next_res {
            if e.code() == ERROR_NO_MORE_FILES.to_hresult() {
                break;
            }
            break;
        }
    }

    unsafe {
        let _ = FindClose(handle);
    }
    Ok(entries)
}

struct DirOutcome {
    dir_path: PathBuf,
    entries: Vec<RawEntry>,
    access_denied: bool,
}

pub fn walk_tree<F>(
    root_path: PathBuf,
    volume_serial: u32,
    fs_type: FileSystemType,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> ScanTree
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let start = Instant::now();
    let mut tree = ScanTree::new(root_path.clone(), volume_serial, fs_type);
    let use_nt = matches!(fs_type, FileSystemType::Ntfs | FileSystemType::Refs);

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(4);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .ok();

    let seen_file_ids: Mutex<HashSet<u64>> = Mutex::new(HashSet::new());
    let scanned_files = AtomicU64::new(0);
    let scanned_dirs = AtomicU64::new(0);
    let total_bytes = AtomicU64::new(0);
    let last_emit = Mutex::new(Instant::now() - Duration::from_millis(200));

    // Breadth/batch parallel directory enumeration:
    // Each level processes all queued directories in parallel using rayon, then inserts into ScanTree
    // in a single owner step so parent-before-child ordering and zero lock contention on ScanTree hold.
    let mut frontier: Vec<(u32, PathBuf)> = vec![(0, root_path)];

    while !frontier.is_empty() && !cancel_flag.load(Ordering::Relaxed) {
        let batch = std::mem::take(&mut frontier);
        let read_batch = || -> Vec<(u32, DirOutcome)> {
            use rayon::prelude::*;
            batch
                .into_par_iter()
                .map(|(node_id, dir_path)| {
                    if cancel_flag.load(Ordering::Relaxed) {
                        return (
                            node_id,
                            DirOutcome {
                                dir_path,
                                entries: Vec::new(),
                                access_denied: false,
                            },
                        );
                    }
                    let res = if use_nt {
                        match read_dir_nt(&dir_path) {
                            Ok(v) => Ok(v),
                            Err(DirReadError::UnsupportedNtQuery) => read_dir_win32(&dir_path),
                            Err(e) => Err(e),
                        }
                    } else {
                        read_dir_win32(&dir_path)
                    };

                    match res {
                        Ok(entries) => (
                            node_id,
                            DirOutcome {
                                dir_path,
                                entries,
                                access_denied: false,
                            },
                        ),
                        Err(DirReadError::AccessDenied) => (
                            node_id,
                            DirOutcome {
                                dir_path,
                                entries: Vec::new(),
                                access_denied: true,
                            },
                        ),
                        Err(_) => (
                            node_id,
                            DirOutcome {
                                dir_path,
                                entries: Vec::new(),
                                access_denied: false,
                            },
                        ),
                    }
                })
                .collect()
        };

        let outcomes = match &pool {
            Some(p) => p.install(read_batch),
            None => read_batch(),
        };

        for (parent_id, outcome) in outcomes {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            scanned_dirs.fetch_add(1, Ordering::Relaxed);
            if outcome.access_denied {
                tree.access_denied.push(outcome.dir_path);
                continue;
            }

            let mut file_id_guard = seen_file_ids.lock().expect("file_id mutex poisoned");
            for entry in outcome.entries {
                if entry.is_reparse {
                    if entry.reparse_tag == IO_REPARSE_TAG_SYMLINK {
                        tree.add_child(
                            parent_id,
                            &entry.name,
                            NodeKind::Symlink,
                            0,
                            0,
                            entry.last_write_ts,
                            NodeFlags::empty(),
                        );
                        tree.stats.symlinks_skipped += 1;
                        continue;
                    }
                    if entry.reparse_tag == IO_REPARSE_TAG_MOUNT_POINT {
                        tree.add_child(
                            parent_id,
                            &entry.name,
                            NodeKind::Junction,
                            0,
                            0,
                            entry.last_write_ts,
                            NodeFlags::empty(),
                        );
                        tree.stats.junctions_skipped += 1;
                        continue;
                    }
                    // Cloud reparse tags (OneDrive) when it's a directory: descend normally without hydrating
                }

                if entry.is_dir {
                    let child_id = tree.add_child(
                        parent_id,
                        &entry.name,
                        NodeKind::Dir,
                        0,
                        0,
                        entry.last_write_ts,
                        NodeFlags::empty(),
                    );
                    frontier.push((child_id, outcome.dir_path.join(&entry.name)));
                } else {
                    let mut flags = NodeFlags::empty();
                    if entry.is_compressed {
                        flags |= NodeFlags::COMPRESSED;
                    }
                    if entry.is_sparse {
                        flags |= NodeFlags::SPARSE;
                    }
                    if entry.is_cloud_only {
                        flags |= NodeFlags::CLOUD_ONLY;
                        tree.stats.cloud_placeholders += 1;
                    }

                    let mut allocated = entry.allocated_size;
                    if use_nt && entry.file_id != 0 && !file_id_guard.insert(entry.file_id) {
                        flags |= NodeFlags::HARDLINK_DUP;
                        allocated = 0;
                        tree.stats.hardlinks_deduplicated += 1;
                    }

                    let kind = if entry.is_cloud_only {
                        NodeKind::CloudPlaceholder
                    } else {
                        NodeKind::File
                    };

                    tree.add_child(
                        parent_id,
                        &entry.name,
                        kind,
                        entry.logical_size,
                        allocated,
                        entry.last_write_ts,
                        flags,
                    );

                    scanned_files.fetch_add(1, Ordering::Relaxed);
                    total_bytes.fetch_add(allocated, Ordering::Relaxed);
                }
            }
            drop(file_id_guard);

            let mut last = last_emit.lock().expect("emit mutex poisoned");
            if last.elapsed() >= Duration::from_millis(100) {
                *last = Instant::now();
                on_progress(ScanProgress {
                    scanned_files: scanned_files.load(Ordering::Relaxed),
                    scanned_dirs: scanned_dirs.load(Ordering::Relaxed),
                    total_allocated_bytes: total_bytes.load(Ordering::Relaxed),
                    current_path: outcome.dir_path.to_string_lossy().into_owned(),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                });
            }
        }
    }

    tree.stats.cancelled = cancel_flag.load(Ordering::Relaxed);
    tree.aggregate_bottom_up();
    tree.stats.duration_ms = start.elapsed().as_millis() as u64;

    on_progress(ScanProgress {
        scanned_files: tree.stats.total_files,
        scanned_dirs: tree.stats.total_dirs,
        total_allocated_bytes: tree.stats.total_allocated_bytes,
        current_path: tree.root_path.to_string_lossy().into_owned(),
        elapsed_ms: tree.stats.duration_ms,
    });

    tree
}
