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
    FindNextFileW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE,
    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FIND_FIRST_EX_LARGE_FETCH, OPEN_EXISTING, WIN32_FIND_DATAW,
};
use windows::Win32::System::IO::IO_STATUS_BLOCK;

use super::derived::{finalize_tree_and_derived, DerivedBuildContext};
use super::model::{FileSystemType, NodeFlags, NodeKind, ScanEngineMode, ScanProgress, ScanTree};

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
    pub is_recall_open: bool,
    pub is_offline: bool,
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

/// Enumerates a directory using `NtQueryDirectoryFile` (`FileIdBothDirectoryInformation`)
/// with a large 128 KB buffer. ZERO per-file `stat` / `GetFileAttributesEx` / `GetCompressedFileSizeW` calls.
/// TODO: verificar en Windows 10/11 con CompactOS (WOF) si `AllocationSize` de `FileIdBothDirectoryInformation`
/// refleja exactamente el tamaño comprimido WOF sin invocar `GetCompressedFileSizeW`.
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

    thread_local! {
        static NT_DIR_BUF: std::cell::RefCell<Vec<u64>> =
            std::cell::RefCell::new(vec![0u64; (64 * 1024) / 8]);
    }

    let mut entries = Vec::new();
    let mut io_status = IO_STATUS_BLOCK::default();
    let mut restart: u8 = 1;

    let res = NT_DIR_BUF.with(|cell| -> Result<(), DirReadError> {
        let mut buf = cell.borrow_mut();
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
                return Err(DirReadError::AccessDenied);
            }
            if status.0 != STATUS_SUCCESS_RAW {
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
                    let is_recall_data = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
                    let is_recall_open = (attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN.0) != 0;
                    let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;

                    let logical = info.EndOfFile.max(0) as u64;
                    let allocated = if is_dir {
                        0
                    } else {
                        info.AllocationSize.max(0) as u64
                    };

                    let is_cloud_only = !is_dir && (is_recall_data || is_recall_open || is_offline);

                    entries.push(RawEntry {
                        name,
                        is_dir,
                        is_reparse,
                        reparse_tag,
                        is_compressed,
                        is_sparse,
                        is_cloud_only,
                        is_recall_open,
                        is_offline,
                        logical_size: logical,
                        allocated_size: allocated,
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
        Ok(())
    });

    unsafe {
        let _ = CloseHandle(handle);
    }
    res?;
    Ok(entries)
}

/// Enumerates a directory using `FindFirstFileExW` with `FindExInfoBasic` and
/// `FIND_FIRST_EX_LARGE_FETCH`. Zero per-file `stat` / `GetFileAttributesEx` calls.
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
            let is_recall_data = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
            let is_recall_open = (attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN.0) != 0;
            let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;

            let logical =
                ((find_data.nFileSizeHigh as u64) << 32) | (find_data.nFileSizeLow as u64);
            let ft = ((find_data.ftLastWriteTime.dwHighDateTime as i64) << 32)
                | (find_data.ftLastWriteTime.dwLowDateTime as i64);

            let is_cloud_only = !is_dir && (is_recall_data || is_recall_open || is_offline);
            let allocated = if is_dir || is_cloud_only { 0 } else { logical };

            entries.push(RawEntry {
                name,
                is_dir,
                is_reparse,
                reparse_tag,
                is_compressed,
                is_sparse,
                is_cloud_only,
                is_recall_open,
                is_offline,
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

pub fn read_dir_fast(dir_path: &Path, use_nt: bool) -> Result<Vec<RawEntry>, DirReadError> {
    if use_nt {
        match read_dir_nt(dir_path) {
            Ok(v) => Ok(v),
            Err(DirReadError::UnsupportedNtQuery) => read_dir_win32(dir_path),
            Err(e) => Err(e),
        }
    } else {
        read_dir_win32(dir_path)
    }
}

struct SharedWalkState<'a, F>
where
    F: Fn(ScanProgress) + Send + Sync,
{
    tree: Mutex<ScanTree>,
    seen_file_ids: Mutex<HashSet<u64>>,
    scanned_files: AtomicU64,
    scanned_dirs: AtomicU64,
    total_bytes: AtomicU64,
    last_emit: Mutex<Instant>,
    cancel_flag: Arc<AtomicBool>,
    use_nt: bool,
    root_str: String,
    volume_used_bytes: Option<u64>,
    start: Instant,
    on_progress: &'a F,
}

fn walk_dir_work_stealing<'scope, F>(
    scope: &rayon::Scope<'scope>,
    state: &'scope SharedWalkState<'scope, F>,
    parent_id: u32,
    dir_path: PathBuf,
) where
    F: Fn(ScanProgress) + Send + Sync,
{
    if state.cancel_flag.load(Ordering::Relaxed) {
        return;
    }

    let res = read_dir_fast(&dir_path, state.use_nt);
    state.scanned_dirs.fetch_add(1, Ordering::Relaxed);

    let entries = match res {
        Ok(e) => e,
        Err(DirReadError::AccessDenied) => {
            if let Ok(mut t) = state.tree.lock() {
                t.access_denied.push(dir_path);
            }
            return;
        }
        Err(_) => return,
    };

    // Deduplicate hardlinks & insert directory batch under brief mutex lock
    let mut subdirs: Vec<(u32, PathBuf)> = Vec::new();
    let mut local_files = 0u64;
    let mut local_bytes = 0u64;

    {
        let mut tree = state.tree.lock().expect("tree mutex poisoned");
        let mut seen = state.seen_file_ids.lock().expect("seen mutex poisoned");

        for entry in entries {
            if entry.is_reparse {
                if entry.reparse_tag == IO_REPARSE_TAG_SYMLINK {
                    tree.add_child_with_frn(
                        parent_id,
                        &entry.name,
                        NodeKind::Symlink,
                        0,
                        0,
                        entry.last_write_ts,
                        NodeFlags::empty(),
                        entry.file_id,
                    );
                    tree.stats.symlinks_skipped += 1;
                    continue;
                }
                if entry.reparse_tag == IO_REPARSE_TAG_MOUNT_POINT {
                    tree.add_child_with_frn(
                        parent_id,
                        &entry.name,
                        NodeKind::Junction,
                        0,
                        0,
                        entry.last_write_ts,
                        NodeFlags::empty(),
                        entry.file_id,
                    );
                    tree.stats.junctions_skipped += 1;
                    continue;
                }
                // Cloud directory reparse point: descend without hydrating
            }

            if entry.is_dir {
                let cid = tree.add_child_with_frn(
                    parent_id,
                    &entry.name,
                    NodeKind::Dir,
                    0,
                    0,
                    entry.last_write_ts,
                    NodeFlags::empty(),
                    entry.file_id,
                );
                subdirs.push((cid, dir_path.join(&entry.name)));
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
                if entry.is_recall_open {
                    flags |= NodeFlags::RECALL_ON_OPEN;
                }
                if entry.is_offline {
                    flags |= NodeFlags::OFFLINE;
                }

                let mut allocated = entry.allocated_size;
                if state.use_nt && entry.file_id != 0 && !seen.insert(entry.file_id) {
                    flags |= NodeFlags::HARDLINK_DUP;
                    allocated = 0;
                    tree.stats.hardlinks_deduplicated += 1;
                }

                let kind = if entry.is_cloud_only {
                    NodeKind::CloudPlaceholder
                } else {
                    NodeKind::File
                };

                tree.add_child_with_frn(
                    parent_id,
                    &entry.name,
                    kind,
                    entry.logical_size,
                    allocated,
                    entry.last_write_ts,
                    flags,
                    entry.file_id,
                );

                local_files += 1;
                local_bytes = local_bytes.saturating_add(allocated);
            }
        }
    }

    let files_total = state
        .scanned_files
        .fetch_add(local_files, Ordering::Relaxed)
        + local_files;
    let bytes_total = state.total_bytes.fetch_add(local_bytes, Ordering::Relaxed) + local_bytes;

    if let Ok(mut last) = state.last_emit.try_lock() {
        if last.elapsed() >= Duration::from_millis(100) {
            *last = Instant::now();
            let ratio = state
                .volume_used_bytes
                .filter(|&u| u > 0)
                .map(|u| ((bytes_total as f64) / (u as f64)).min(0.95));
            (state.on_progress)(ScanProgress {
                drive_root: state.root_str.clone(),
                phase: format!("Escaneando {}…", state.root_str),
                engine_mode: ScanEngineMode::FallbackWorkStealing,
                scanned_files: files_total,
                scanned_dirs: state.scanned_dirs.load(Ordering::Relaxed),
                total_allocated_bytes: bytes_total,
                current_path: dir_path.to_string_lossy().into_owned(),
                elapsed_ms: state.start.elapsed().as_millis() as u64,
                mft_processed: None,
                mft_total: None,
                volume_used_bytes: state.volume_used_bytes,
                usn_processed: None,
                usn_total: None,
                progress_ratio: ratio,
            });
        }
    }

    // Spawn child directories onto the Rayon work-stealing deque (keep the last one inline to reduce spawn overhead)
    if let Some((last_id, last_path)) = subdirs.pop() {
        for (cid, sub_path) in subdirs {
            scope.spawn(move |s| {
                walk_dir_work_stealing(s, state, cid, sub_path);
            });
        }
        walk_dir_work_stealing(scope, state, last_id, last_path);
    }
}

/// Parallel work-stealing directory enumeration fallback (for non-admin NTFS, FAT32, exFAT, ReFS, network).
#[allow(clippy::too_many_arguments)]
pub fn walk_tree_work_stealing<F>(
    root_path: PathBuf,
    volume_serial: u32,
    fs_type: FileSystemType,
    volume_used_bytes: Option<u64>,
    cancel_flag: Arc<AtomicBool>,
    ctx: &DerivedBuildContext,
    on_progress: F,
) -> ScanTree
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let start = Instant::now();
    let root_str = root_path.to_string_lossy().into_owned();
    let use_nt = matches!(fs_type, FileSystemType::Ntfs | FileSystemType::Refs);

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 12))
        .unwrap_or(4);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .ok();

    let state = SharedWalkState {
        tree: Mutex::new(ScanTree::with_capacity(
            root_path.clone(),
            volume_serial,
            fs_type,
            16_384,
        )),
        seen_file_ids: Mutex::new(HashSet::with_capacity(16_384)),
        scanned_files: AtomicU64::new(0),
        scanned_dirs: AtomicU64::new(0),
        total_bytes: AtomicU64::new(0),
        last_emit: Mutex::new(Instant::now() - Duration::from_millis(200)),
        cancel_flag: cancel_flag.clone(),
        use_nt,
        root_str: root_str.clone(),
        volume_used_bytes,
        start,
        on_progress: &on_progress,
    };

    let run = || {
        rayon::scope(|s| {
            walk_dir_work_stealing(s, &state, 0, root_path.clone());
        });
    };

    match &pool {
        Some(p) => p.install(run),
        None => run(),
    }

    let mut tree = state.tree.into_inner().expect("tree mutex poisoned");
    tree.stats.cancelled = cancel_flag.load(Ordering::Relaxed);
    tree.stats.engine_mode = ScanEngineMode::FallbackWorkStealing;
    tree.shrink_build_caches();

    on_progress(ScanProgress {
        drive_root: root_str.clone(),
        phase: "Calculando categorías…".to_string(),
        engine_mode: ScanEngineMode::FallbackWorkStealing,
        scanned_files: state.scanned_files.load(Ordering::Relaxed),
        scanned_dirs: state.scanned_dirs.load(Ordering::Relaxed),
        total_allocated_bytes: state.total_bytes.load(Ordering::Relaxed),
        current_path: root_str.clone(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        mft_processed: None,
        mft_total: None,
        volume_used_bytes,
        usn_processed: None,
        usn_total: None,
        progress_ratio: Some(0.97),
    });

    finalize_tree_and_derived(&mut tree, ctx);
    tree.stats.duration_ms = start.elapsed().as_millis() as u64;

    on_progress(ScanProgress {
        drive_root: root_str.clone(),
        phase: "Preparando limpieza…".to_string(),
        engine_mode: ScanEngineMode::FallbackWorkStealing,
        scanned_files: tree.stats.total_files,
        scanned_dirs: tree.stats.total_dirs,
        total_allocated_bytes: tree.stats.total_allocated_bytes,
        current_path: root_str,
        elapsed_ms: tree.stats.duration_ms,
        mft_processed: None,
        mft_total: None,
        volume_used_bytes,
        usn_processed: None,
        usn_total: None,
        progress_ratio: Some(1.0),
    });

    tree
}

/// Queries metadata for a single entry by exact path (`NtQueryDirectoryFile` single-entry filter on NTFS/ReFS,
/// falling back to `FindFirstFileExW`) without enumerating sibling files or hydrating cloud placeholders.
fn query_single_entry_fast(target_path: &Path, use_nt: bool) -> Option<RawEntry> {
    let file_name_os = target_path.file_name()?;
    let parent_dir = target_path.parent()?;

    if use_nt {
        if let Some(nt_query) = get_nt_query_directory_file() {
            let wide_dir = to_extended_wide(parent_dir);
            if let Ok(handle) = unsafe {
                CreateFileW(
                    PCWSTR(wide_dir.as_ptr()),
                    FILE_LIST_DIRECTORY.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS,
                    None,
                )
            } {
                if handle != INVALID_HANDLE_VALUE {
                    let mut filter_utf16: Vec<u16> =
                        file_name_os.to_string_lossy().encode_utf16().collect();
                    let byte_len = (filter_utf16.len() * 2).min(u16::MAX as usize) as u16;
                    let us = UNICODE_STRING {
                        Length: byte_len,
                        MaximumLength: byte_len,
                        Buffer: windows::core::PWSTR(filter_utf16.as_mut_ptr()),
                    };
                    let mut buf = vec![0u64; 256];
                    let mut io_status = IO_STATUS_BLOCK::default();
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
                            1,
                            &us,
                            1,
                        )
                    };
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                    if status.0 == STATUS_SUCCESS_RAW {
                        let info = unsafe { &*(buf.as_ptr().cast::<FileIdBothDirInfo>()) };
                        let name_len_chars = (info.FileNameLength as usize) / 2;
                        let name_slice = unsafe {
                            std::slice::from_raw_parts(info.FileName.as_ptr(), name_len_chars)
                        };
                        let name = OsString::from_wide(name_slice)
                            .to_string_lossy()
                            .into_owned();
                        let attrs = info.FileAttributes;
                        let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
                        let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
                        let reparse_tag = if is_reparse { info.EaSize } else { 0 };
                        let is_compressed = (attrs & FILE_ATTRIBUTE_COMPRESSED.0) != 0;
                        let is_sparse = (attrs & FILE_ATTRIBUTE_SPARSE_FILE.0) != 0;
                        let is_recall_data = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
                        let is_recall_open = (attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN.0) != 0;
                        let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;
                        let logical = info.EndOfFile.max(0) as u64;
                        let allocated = if is_dir {
                            0
                        } else {
                            info.AllocationSize.max(0) as u64
                        };
                        let is_cloud_only =
                            !is_dir && (is_recall_data || is_recall_open || is_offline);
                        return Some(RawEntry {
                            name,
                            is_dir,
                            is_reparse,
                            reparse_tag,
                            is_compressed,
                            is_sparse,
                            is_cloud_only,
                            is_recall_open,
                            is_offline,
                            logical_size: logical,
                            allocated_size: allocated,
                            last_write_ts: filetime_to_unix_secs(info.LastWriteTime),
                            file_id: info.FileId.max(0) as u64,
                        });
                    }
                }
            }
        }
    }

    let wide = to_extended_wide(target_path);
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
    }
    .ok()?;
    unsafe {
        let _ = FindClose(handle);
    }

    let end = find_data
        .cFileName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(find_data.cFileName.len());
    let name = OsString::from_wide(&find_data.cFileName[..end])
        .to_string_lossy()
        .into_owned();
    let attrs = find_data.dwFileAttributes;
    let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;
    let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
    let reparse_tag = if is_reparse { find_data.dwReserved0 } else { 0 };
    let is_compressed = (attrs & FILE_ATTRIBUTE_COMPRESSED.0) != 0;
    let is_sparse = (attrs & FILE_ATTRIBUTE_SPARSE_FILE.0) != 0;
    let is_recall_data = (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0) != 0;
    let is_recall_open = (attrs & FILE_ATTRIBUTE_RECALL_ON_OPEN.0) != 0;
    let is_offline = (attrs & FILE_ATTRIBUTE_OFFLINE.0) != 0;
    let logical = ((find_data.nFileSizeHigh as u64) << 32) | (find_data.nFileSizeLow as u64);
    let ft = ((find_data.ftLastWriteTime.dwHighDateTime as i64) << 32)
        | (find_data.ftLastWriteTime.dwLowDateTime as i64);
    let is_cloud_only = !is_dir && (is_recall_data || is_recall_open || is_offline);
    let allocated = if is_dir || is_cloud_only { 0 } else { logical };

    Some(RawEntry {
        name,
        is_dir,
        is_reparse,
        reparse_tag,
        is_compressed,
        is_sparse,
        is_cloud_only,
        is_recall_open,
        is_offline,
        logical_size: logical,
        allocated_size: allocated,
        last_write_ts: filetime_to_unix_secs(ft),
        file_id: 0,
    })
}

fn entry_node_flags_and_kind(entry: &RawEntry) -> (NodeFlags, NodeKind) {
    let mut flags = NodeFlags::empty();
    if entry.is_compressed {
        flags |= NodeFlags::COMPRESSED;
    }
    if entry.is_sparse {
        flags |= NodeFlags::SPARSE;
    }
    if entry.is_cloud_only {
        flags |= NodeFlags::CLOUD_ONLY;
    }
    if entry.is_recall_open {
        flags |= NodeFlags::RECALL_ON_OPEN;
    }
    if entry.is_offline {
        flags |= NodeFlags::OFFLINE;
    }
    let kind = if entry.is_reparse && entry.reparse_tag == IO_REPARSE_TAG_SYMLINK {
        NodeKind::Symlink
    } else if entry.is_reparse && entry.reparse_tag == IO_REPARSE_TAG_MOUNT_POINT {
        NodeKind::Junction
    } else if entry.is_dir {
        NodeKind::Dir
    } else if entry.is_cloud_only {
        NodeKind::CloudPlaceholder
    } else {
        NodeKind::File
    };
    (flags, kind)
}

/// Fast path for `VolumeDirWatcher` notifications: updates single files in O(1) and diffs only
/// direct children (plus brand-new subdirectories) on directory events without re-walking huge subtrees.
pub fn reconcile_watcher_path(
    tree: &mut ScanTree,
    target_path: &Path,
    ctx: &DerivedBuildContext,
) -> bool {
    reconcile_path_incremental(tree, target_path, false, ctx)
}

/// Targeted subtree reconciliation helper used by the Section 9 reconciliation queue.
/// Performs an in-place, non-destructive diff against disk: keeps unchanged directories & files
/// untouched (zero unnecessary `TOMBSTONE`s), updates resized files in-place, removes vanished entries
/// (recycling slots into `free_list`), and walks added entries.
pub fn reconcile_subtree_fallback(
    tree: &mut ScanTree,
    target_path: &Path,
    ctx: &DerivedBuildContext,
) -> bool {
    reconcile_path_incremental(tree, target_path, true, ctx)
}

fn reconcile_path_incremental(
    tree: &mut ScanTree,
    target_path: &Path,
    recurse_existing_subdirs: bool,
    ctx: &DerivedBuildContext,
) -> bool {
    let use_nt = matches!(tree.fs_type, FileSystemType::Ntfs | FileSystemType::Refs);

    // 1. If the target disappeared on disk, remove only that node/subtree in-place!
    let Ok(meta) = std::fs::symlink_metadata(target_path) else {
        if let Some(existing_id) = tree.find_node_by_path(target_path) {
            if existing_id != 0 {
                if let Some(pid) = tree.remove_subtree(existing_id) {
                    super::derived::refresh_derived_for_parent_shallow(tree, pid, ctx);
                    return true;
                }
            }
        }
        return false;
    };

    // 2. Single file (or file symlink) fast O(1) path — NEVER wipes or re-walks the parent directory!
    if !meta.is_dir() {
        let Some(parent_path) = target_path.parent() else {
            return false;
        };
        let Some(entry) = query_single_entry_fast(target_path, use_nt) else {
            return false;
        };
        let Some(parent_nid) = tree.ensure_dir_path(parent_path) else {
            return false;
        };
        let (flags, kind) = entry_node_flags_and_kind(&entry);

        let mut existing_cid = None;
        for cid in tree.children_iter(parent_nid) {
            if tree.node_name(cid).eq_ignore_ascii_case(&entry.name) {
                existing_cid = Some(cid);
                break;
            }
        }

        if let Some(cid) = existing_cid {
            if tree.kind_of(cid) != kind {
                tree.remove_subtree(cid);
            } else {
                if tree.node_name(cid) != entry.name {
                    tree.rename_node(cid, &entry.name);
                }
                if matches!(kind, NodeKind::Symlink | NodeKind::Junction) {
                    return false;
                }
                let cidx = cid as usize;
                if tree.logical_size[cidx] == entry.logical_size
                    && tree.allocated_size[cidx] == entry.allocated_size
                {
                    if entry.last_write_ts > tree.last_write_ts_of(cid) {
                        tree.last_write_ts[cidx] =
                            entry.last_write_ts.clamp(0, u32::MAX as i64) as u32;
                    }
                    return false;
                }
                tree.update_file_sizes(
                    cid,
                    entry.logical_size,
                    entry.allocated_size,
                    entry.last_write_ts,
                );
                super::derived::refresh_derived_for_single_node(tree, cid, ctx);
                super::derived::refresh_derived_for_parent_shallow(tree, parent_nid, ctx);
                return true;
            }
        }

        // Brand-new file (or replaced incompatible node kind)
        let alloc = if matches!(kind, NodeKind::Symlink | NodeKind::Junction) {
            0
        } else {
            entry.allocated_size
        };
        let logical = if matches!(kind, NodeKind::Symlink | NodeKind::Junction) {
            0
        } else {
            entry.logical_size
        };
        let cid = tree.add_child_with_frn(
            parent_nid,
            &entry.name,
            kind,
            logical,
            alloc,
            entry.last_write_ts,
            flags,
            entry.file_id,
        );
        if matches!(kind, NodeKind::File | NodeKind::CloudPlaceholder) {
            tree.propagate_delta_upwards(parent_nid, alloc as i64, logical as i64, 1);
            super::derived::refresh_derived_for_single_node(tree, cid, ctx);
            let tcat = tree.type_cat_of(cid);
            let ocat = tree.origin_cat_of(cid);
            tree.derived
                .apply_file_delta(tcat, ocat, alloc as i64, logical as i64, 1);
        }
        super::derived::refresh_derived_for_parent_shallow(tree, parent_nid, ctx);
        return true;
    }

    // 3. Directory in-place diff (only touches added/removed/resized entries!)
    let existed_before = tree.find_node_by_path(target_path).is_some();
    let Some(dir_nid) = tree.ensure_dir_path(target_path) else {
        return false;
    };

    // `(dir_nid, dir_path, is_newly_created_dir)`
    let mut stack: Vec<(u32, PathBuf, bool)> =
        vec![(dir_nid, target_path.to_path_buf(), !existed_before)];
    let mut seen_new_ids: HashSet<u64> = HashSet::new();
    let mut mutated = !existed_before;
    let mut added_subdirs_or_deep = !existed_before;

    while let Some((pid, cur_path, is_new_dir)) = stack.pop() {
        let Ok(entries) = read_dir_fast(&cur_path, use_nt) else {
            continue;
        };

        let mut existing_by_name: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        if !is_new_dir {
            for cid in tree.children_iter(pid) {
                existing_by_name.insert(tree.node_name(cid).to_ascii_lowercase(), cid);
            }
        }

        for entry in entries {
            let key = entry.name.to_ascii_lowercase();
            let (mut flags, kind) = entry_node_flags_and_kind(&entry);

            if let Some(existing_cid) = existing_by_name.remove(&key) {
                let old_kind = tree.kind_of(existing_cid);
                if old_kind == kind {
                    if tree.node_name(existing_cid) != entry.name {
                        tree.rename_node(existing_cid, &entry.name);
                        mutated = true;
                    }
                    match kind {
                        NodeKind::Symlink | NodeKind::Junction => {}
                        NodeKind::Dir => {
                            if recurse_existing_subdirs {
                                stack.push((existing_cid, cur_path.join(&entry.name), false));
                            }
                        }
                        NodeKind::File | NodeKind::CloudPlaceholder => {
                            let cidx = existing_cid as usize;
                            if tree.logical_size[cidx] != entry.logical_size
                                || tree.allocated_size[cidx] != entry.allocated_size
                            {
                                tree.update_file_sizes(
                                    existing_cid,
                                    entry.logical_size,
                                    entry.allocated_size,
                                    entry.last_write_ts,
                                );
                                mutated = true;
                            } else if entry.last_write_ts > tree.last_write_ts_of(existing_cid) {
                                tree.last_write_ts[cidx] =
                                    entry.last_write_ts.clamp(0, u32::MAX as i64) as u32;
                            }
                        }
                    }
                    continue;
                }
                // Entry changed kind (e.g. file replaced by directory) -> remove old first
                tree.remove_subtree(existing_cid);
            }

            // Insert brand-new entry on disk
            mutated = true;
            match kind {
                NodeKind::Symlink | NodeKind::Junction => {
                    tree.add_child_with_frn(
                        pid,
                        &entry.name,
                        kind,
                        0,
                        0,
                        entry.last_write_ts,
                        NodeFlags::empty(),
                        entry.file_id,
                    );
                }
                NodeKind::Dir => {
                    let cid = tree.add_child_with_frn(
                        pid,
                        &entry.name,
                        NodeKind::Dir,
                        0,
                        0,
                        entry.last_write_ts,
                        NodeFlags::empty(),
                        entry.file_id,
                    );
                    tree.stats.total_dirs += 1;
                    added_subdirs_or_deep = true;
                    // Always walk brand-new subdirectories even on shallow watcher events!
                    stack.push((cid, cur_path.join(&entry.name), true));
                }
                NodeKind::File | NodeKind::CloudPlaceholder => {
                    let mut alloc = entry.allocated_size;
                    if use_nt && entry.file_id != 0 && !seen_new_ids.insert(entry.file_id) {
                        flags |= NodeFlags::HARDLINK_DUP;
                        alloc = 0;
                    }
                    let cid = tree.add_child_with_frn(
                        pid,
                        &entry.name,
                        kind,
                        entry.logical_size,
                        alloc,
                        entry.last_write_ts,
                        flags,
                        entry.file_id,
                    );
                    tree.propagate_delta_upwards(pid, alloc as i64, entry.logical_size as i64, 1);
                    let child_full_low = cur_path
                        .join(&entry.name)
                        .to_string_lossy()
                        .to_ascii_lowercase();
                    let ext = key.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
                    let tcat =
                        crate::categorizer::classify_extension(ext, &ctx.rules_config.custom);
                    let ocat = crate::categorizer::classify_origin_path(
                        &child_full_low,
                        &key,
                        &ctx.rules_config,
                    );
                    tree.set_type_cat(cid, tcat);
                    tree.set_origin_cat(cid, ocat);
                    tree.derived
                        .apply_file_delta(tcat, ocat, alloc as i64, entry.logical_size as i64, 1);
                }
            }
        }

        // Remove any children that disappeared on disk (recycled into `free_list`)
        for (_, gone_cid) in existing_by_name {
            tree.remove_subtree(gone_cid);
            mutated = true;
        }
    }

    if mutated {
        if recurse_existing_subdirs || added_subdirs_or_deep {
            super::derived::refresh_derived_for_subtree(tree, dir_nid, ctx);
        } else {
            super::derived::refresh_derived_for_parent_shallow(tree, dir_nid, ctx);
        }
    }
    mutated
}
