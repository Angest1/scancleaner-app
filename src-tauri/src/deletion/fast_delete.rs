use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, DeleteFileW, FileDispositionInfoEx, GetFileAttributesW, MoveFileExW,
    RemoveDirectoryW, SetFileAttributesW, SetFileInformationByHandle, DELETE,
    FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
    FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    INVALID_FILE_ATTRIBUTES, MOVEFILE_REPLACE_EXISTING, OPEN_EXISTING,
};

use crate::cleaner::guardrails::{is_reparse_point, validate_target_path, SandboxGuard};
use crate::protection::is_protected_default;
use crate::scanner::derived::volume_root_of_path;
use crate::scanner::fallback_walk::read_dir_win32;

pub const TRASH_STAGING_DIR_NAME: &str = ".scancleaner-trash";

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

/// Ensures the hidden staging directory `<vol>:\.scancleaner-trash` (or inside sandbox for tests) exists.
pub fn ensure_trash_root(target: &Path, sandbox_root: Option<&Path>) -> Result<PathBuf, String> {
    let base = if let Some(sb) = sandbox_root {
        sb.join(TRASH_STAGING_DIR_NAME)
    } else {
        let vol = volume_root_of_path(target)
            .ok_or_else(|| format!("No se pudo determinar el volumen de {}", target.display()))?;
        PathBuf::from(vol).join(TRASH_STAGING_DIR_NAME)
    };

    if !base.exists() {
        fs::create_dir_all(&base)
            .map_err(|e| format!("No se pudo crear carpeta temporal de borrado: {e}"))?;
        let wide = to_extended_wide(&base);
        unsafe {
            let _ = SetFileAttributesW(
                PCWSTR(wide.as_ptr()),
                FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM,
            );
        }
    }
    Ok(base)
}

/// Step 1 of fast permanent deletion:
/// Re-checks `is_protected()` & guardrails immediately prior to rename, then atomically renames
/// `target` into `<vol>:\.scancleaner-trash\<job_id>\<uuid>_<leaf>` on the SAME volume (< 1ms!).
pub fn stage_to_same_volume_trash(
    target: &Path,
    job_id: &str,
    sandbox_guard: Option<&SandboxGuard>,
    sandbox_root: Option<&Path>,
) -> Result<PathBuf, String> {
    if !target.exists() {
        return Err("La ruta ya no existe.".to_string());
    }
    if is_reparse_point(target) {
        return Err("Reparse point bloqueado antes de renombrar a trash.".to_string());
    }

    // Mandatory `isProtected()` & guardrail check right before rename
    if let Some(guard) = sandbox_guard {
        guard.verify_inside_sandbox(target).map_err(|v| v.reason)?;
        let prot = is_protected_default(target);
        if prot.protected {
            return Err(prot
                .reason
                .unwrap_or_else(|| "Ruta protegida por lista negra antes de rename.".to_string()));
        }
    } else {
        // `validate_target_path` invokes `is_protected_default()` internally (avoids double 5k-entry walk)
        validate_target_path(target).map_err(|v| v.reason)?;
    }

    let trash_root = ensure_trash_root(target, sandbox_root)?;
    let job_dir = trash_root.join(job_id);
    fs::create_dir_all(&job_dir).map_err(|e| format!("No se pudo crear subcarpeta trash: {e}"))?;

    let leaf = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "item".to_string());
    let staged_path = job_dir.join(format!("{}_{leaf}", uuid::Uuid::new_v4().simple()));

    let src_w = to_extended_wide(target);
    let dst_w = to_extended_wide(&staged_path);
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING,
        )
    }
    .map_err(|e| format!("Rename instantáneo a .scancleaner-trash falló: {e}"))?;

    Ok(staged_path)
}

/// Attempts fast POSIX-semantics deletion via `SetFileInformationByHandle(..., FileDispositionInfoEx)`,
/// falling back automatically to clearing `READONLY` + `DeleteFileW` / `RemoveDirectoryW`.
///
/// TODO: verificar la versión mínima exacta de Windows 10 en hardware de destino:
/// `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` se introdujo en Windows 10 RS1 (1607 / build 14393)
/// y `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE` en Windows 10 1809 (build 17763).
/// El fallback automático cubre cualquier build anterior o sistemas de archivos FAT32/exFAT.
pub fn delete_single_entry_fast(path: &Path, is_dir: bool) -> Result<(), String> {
    let wide = to_extended_wide(path);

    // 1. Try FileDispositionInfoEx with POSIX semantics + ignore read-only
    let flags_open = FILE_FLAG_OPEN_REPARSE_POINT
        | if is_dir {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            Default::default()
        };

    if let Ok(handle) = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            DELETE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            flags_open,
            None,
        )
    } {
        if handle != INVALID_HANDLE_VALUE {
            let info = FILE_DISPOSITION_INFO_EX {
                Flags: windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO_EX_FLAGS(
                    FILE_DISPOSITION_FLAG_DELETE.0
                        | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                        | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
                ),
            };
            let ok = unsafe {
                SetFileInformationByHandle(
                    handle,
                    FileDispositionInfoEx,
                    (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
                )
            };
            unsafe {
                let _ = CloseHandle(handle);
            }
            if ok.is_ok() {
                return Ok(());
            }
        }
    }

    // 2. Fallback: clear READONLY attribute if set, then DeleteFileW / RemoveDirectoryW
    let attrs = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
    if attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_READONLY.0) != 0 {
        let new_attrs = attrs & !FILE_ATTRIBUTE_READONLY.0;
        let final_attrs = if new_attrs == 0 {
            FILE_ATTRIBUTE_NORMAL
        } else {
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(new_attrs)
        };
        unsafe {
            let _ = SetFileAttributesW(PCWSTR(wide.as_ptr()), final_attrs);
        }
    }

    if is_dir {
        unsafe { RemoveDirectoryW(PCWSTR(wide.as_ptr())) }
            .map_err(|e| format!("RemoveDirectoryW falló en {}: {e}", path.display()))
    } else {
        unsafe { DeleteFileW(PCWSTR(wide.as_ptr())) }
            .map_err(|e| format!("DeleteFileW falló en {}: {e}", path.display()))
    }
}

/// Step 2 of fast permanent deletion:
/// Recursively deletes a directory tree in parallel using Rayon:
/// 1. Collects all files and directories by depth (never following reparse points!).
/// 2. Deletes all files in parallel (`delete_single_entry_fast`).
/// 3. Deletes directories bottom-up (deepest level first, parallel per level).
///
/// Re-checks `is_protected()` on the root before starting (note: staged paths under `.scancleaner-trash`
/// are already validated before staging, and `enforce_protection` re-validates un-staged paths).
pub fn delete_tree_parallel_fast(
    root: &Path,
    cancel_flag: Option<&AtomicBool>,
    deleted_counter: Option<&AtomicU64>,
    enforce_protection: bool,
) -> Result<u64, String> {
    if !root.exists() {
        return Ok(0);
    }
    if is_reparse_point(root) {
        return Err("Reparse point bloqueado durante borrado paralelo.".to_string());
    }
    if enforce_protection {
        validate_target_path(root).map_err(|v| v.reason)?;
        let prot = is_protected_default(root);
        if prot.protected {
            return Err(prot
                .reason
                .unwrap_or_else(|| "Ruta protegida por lista negra.".to_string()));
        }
    }

    let meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        delete_single_entry_fast(root, false)?;
        if let Some(c) = deleted_counter {
            c.fetch_add(1, Ordering::Relaxed);
        }
        return Ok(1);
    }

    // BFS collect files & dirs grouped by depth (zero symlink/junction following)
    let mut files: Vec<PathBuf> = Vec::with_capacity(1024);
    let mut dirs_by_depth: Vec<Vec<PathBuf>> = vec![vec![root.to_path_buf()]];
    let mut current_depth = 0usize;

    while current_depth < dirs_by_depth.len() {
        if cancel_flag.is_some_and(|f| f.load(Ordering::Relaxed)) {
            return Err("Borrado cancelado por el usuario.".to_string());
        }
        let level_dirs = dirs_by_depth[current_depth].clone();
        let scan_results: Vec<(Vec<PathBuf>, Vec<PathBuf>)> = level_dirs
            .par_iter()
            .map(|dir| {
                let mut local_files = Vec::new();
                let mut local_dirs = Vec::new();
                if let Ok(entries) = read_dir_win32(dir) {
                    for e in entries {
                        let p = dir.join(&e.name);
                        if e.is_reparse {
                            // Never follow or traverse symlinks/junctions
                            continue;
                        }
                        if e.is_dir {
                            local_dirs.push(p);
                        } else {
                            local_files.push(p);
                        }
                    }
                }
                (local_files, local_dirs)
            })
            .collect();

        let mut next_level = Vec::new();
        for (lf, ld) in scan_results {
            files.extend(lf);
            next_level.extend(ld);
        }
        if !next_level.is_empty() {
            dirs_by_depth.push(next_level);
        }
        current_depth += 1;
    }

    // Delete all files first in parallel!
    let first_err: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    let total_deleted = AtomicU64::new(0);

    files.par_iter().for_each(|f| {
        if cancel_flag.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return;
        }
        match delete_single_entry_fast(f, false) {
            Ok(()) => {
                total_deleted.fetch_add(1, Ordering::Relaxed);
                if let Some(c) = deleted_counter {
                    c.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => {
                if let Ok(mut guard) = first_err.lock() {
                    if guard.is_none() {
                        *guard = Some(e);
                    }
                }
            }
        }
    });

    if cancel_flag.is_some_and(|f| f.load(Ordering::Relaxed)) {
        return Err("Borrado cancelado por el usuario.".to_string());
    }

    // Delete directories bottom-up (deepest level first) in parallel per level
    for level in dirs_by_depth.iter().rev() {
        level.par_iter().for_each(|d| {
            if cancel_flag.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return;
            }
            if let Err(e) = delete_single_entry_fast(d, true) {
                if let Ok(mut guard) = first_err.lock() {
                    if guard.is_none() {
                        *guard = Some(e);
                    }
                }
            } else {
                total_deleted.fetch_add(1, Ordering::Relaxed);
                if let Some(c) = deleted_counter {
                    c.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    if let Some(err) = first_err.into_inner().ok().flatten() {
        if root.exists() {
            return Err(err);
        }
    }

    Ok(total_deleted.load(Ordering::Relaxed))
}

/// Scans all local volumes on startup for leftover `<vol>:\.scancleaner-trash\*` directories
/// and drains them in background threads with reduced I/O priority.
pub fn resume_pending_trash_drains(volume_roots: Vec<String>) {
    std::thread::Builder::new()
        .name("trash-resume-drain".to_string())
        .spawn(move || {
            let _bg = crate::reconcile::queue::BackgroundIoGuard::enter_if_low_priority(
                crate::reconcile::queue::ReconcilePriority::Low,
            );
            for vol in volume_roots {
                let trash_dir = PathBuf::from(&vol).join(TRASH_STAGING_DIR_NAME);
                if !trash_dir.is_dir() {
                    continue;
                }
                if let Ok(rd) = fs::read_dir(&trash_dir) {
                    for entry in rd.flatten() {
                        let p = entry.path();
                        let _ = delete_tree_parallel_fast(&p, None, None, false);
                    }
                }
                // Remove empty trash container if completely drained
                let _ = fs::remove_dir(&trash_dir);
            }
        })
        .ok();
}

/// Returns true if any `<vol>:\.scancleaner-trash` currently has pending items.
pub fn has_pending_trash_on_volume(volume_root: &str) -> bool {
    let trash_dir = PathBuf::from(volume_root).join(TRASH_STAGING_DIR_NAME);
    if !trash_dir.is_dir() {
        return false;
    }
    fs::read_dir(&trash_dir)
        .ok()
        .is_some_and(|mut rd| rd.next().is_some())
}

// Re-export HANDLE cleanly only when needed
#[allow(dead_code)]
fn _handle_check(_h: HANDLE) {}
