use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SIZE, FILE_NOTIFY_INFORMATION, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::CancelIoEx;

use super::is_watcher_ignored_path;

/// Handle to a running per-volume filesystem watcher (`ReadDirectoryChangesW`).
pub struct VolumeDirWatcher {
    stop_flag: Arc<AtomicBool>,
    handle_raw: isize,
    thread: Option<JoinHandle<()>>,
}

impl VolumeDirWatcher {
    pub fn start<F>(root_path: PathBuf, on_changed_paths: F) -> Result<Self, String>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        let wide: Vec<u16> = root_path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

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
        .map_err(|e| format!("ReadDirectoryChangesW CreateFileW falló: {e}"))?;

        if handle == INVALID_HANDLE_VALUE {
            return Err("Handle inválido para watcher.".to_string());
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let handle_raw = handle.0 as isize;

        let thread = std::thread::Builder::new()
            .name(format!("watcher-{}", root_path.display()))
            .spawn(move || {
                let h = HANDLE(handle_raw as *mut core::ffi::c_void);
                let mut buf = vec![0u32; (64 * 1024) / 4];
                let filter = FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE;

                while !stop_clone.load(Ordering::Relaxed) {
                    let mut bytes_returned = 0u32;
                    let ok = unsafe {
                        ReadDirectoryChangesW(
                            h,
                            buf.as_mut_ptr().cast(),
                            (buf.len() * 4) as u32,
                            true,
                            filter,
                            Some(&mut bytes_returned),
                            None,
                            None,
                        )
                    };
                    if ok.is_err() || bytes_returned == 0 {
                        if stop_clone.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }

                    let mut changed = Vec::new();
                    let base = buf.as_ptr().cast::<u8>();
                    let mut off = 0usize;
                    loop {
                        if off + std::mem::size_of::<FILE_NOTIFY_INFORMATION>()
                            > bytes_returned as usize
                        {
                            break;
                        }
                        let info = unsafe { &*(base.add(off).cast::<FILE_NOTIFY_INFORMATION>()) };
                        let chars = (info.FileNameLength as usize) / 2;
                        let name_slice =
                            unsafe { std::slice::from_raw_parts(info.FileName.as_ptr(), chars) };
                        let rel = OsString::from_wide(name_slice);
                        let full = root_path.join(rel);
                        // Ignore changes inside ScanCleaner's own AppData (index/webview2/logs) and OS churn
                        if !is_watcher_ignored_path(&full) && !crate::scanner::is_volume_root(&full)
                        {
                            // Skip directory-only `FILE_ACTION_MODIFIED` (3) mtime bumps: any file added,
                            // removed, or resized inside a directory already emits its own notification,
                            // and passing a parent directory like `C:\Users\<user>` would coalesce churn!
                            let is_dir_mtime_only = info.Action.0 == 3
                                && std::fs::symlink_metadata(&full)
                                    .map(|m| m.is_dir())
                                    .unwrap_or(false);
                            if !is_dir_mtime_only {
                                changed.push(full);
                            }
                        }
                        if info.NextEntryOffset == 0 {
                            break;
                        }
                        off += info.NextEntryOffset as usize;
                    }

                    if !changed.is_empty() {
                        on_changed_paths(changed);
                    }
                }

                unsafe {
                    let _ = CloseHandle(h);
                }
            })
            .map_err(|e| format!("Error iniciando hilo watcher: {e}"))?;

        Ok(Self {
            stop_flag,
            handle_raw,
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        let h = HANDLE(self.handle_raw as *mut core::ffi::c_void);
        if !h.is_invalid() {
            unsafe {
                let _ = CancelIoEx(h, None);
            }
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for VolumeDirWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
