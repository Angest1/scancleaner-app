use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, INVALID_HANDLE_VALUE};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FlushFileBuffers, ReadFile, WriteFile, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP,
    SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_STOPPED,
    SERVICE_STOP_PENDING, SERVICE_TABLE_ENTRYW, SERVICE_WIN32_OWN_PROCESS,
};

use crate::index::snapshot::save_volume_snapshot;
use crate::index::usn::{query_usn_journal_for_root, UsnJournalReader, WinUsnJournalReader};
use crate::scanner::derived::DerivedBuildContext;
use crate::scanner::drives::query_volume_meta;
use crate::scanner::mft::scan_volume_mft_direct;
use crate::scanner::model::ScanEngineMode;

use super::protocol::{
    build_pipe_sddl, decode_request_frame, encode_frame, ServiceRequest, ServiceResponse,
    MAX_REQUEST_FRAME_BYTES, PIPE_NAME, PROTOCOL_VERSION, SERVICE_NAME,
};

static SERVICE_STOP: AtomicBool = AtomicBool::new(false);
static ALLOWED_SID_GLOBAL: OnceLock<String> = OnceLock::new();
static STATUS_HANDLE_RAW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Entry point when `ScanCleaner.exe --service --allowed-sid <SID>` is launched by Windows SCM.
pub fn run_windows_service(allowed_sid: String) -> Result<(), String> {
    let _ = ALLOWED_SID_GLOBAL.set(allowed_sid);
    let mut name_w: Vec<u16> = SERVICE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: PWSTR(name_w.as_mut_ptr()),
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: PWSTR::null(),
            lpServiceProc: None,
        },
    ];

    unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) }
        .map_err(|e| format!("StartServiceCtrlDispatcherW falló: {e}"))
}

unsafe extern "system" fn service_ctrl_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            SERVICE_STOP.store(true, Ordering::SeqCst);
            let h = SERVICE_STATUS_HANDLE(STATUS_HANDLE_RAW.load(Ordering::SeqCst) as *mut c_void);
            if !h.0.is_null() {
                set_status(h, SERVICE_STOP_PENDING, 0);
            }
            0
        }
        _ => 0,
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let name_w: Vec<u16> = SERVICE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let Ok(status_handle) =
        RegisterServiceCtrlHandlerExW(PCWSTR(name_w.as_ptr()), Some(service_ctrl_handler), None)
    else {
        return;
    };
    STATUS_HANDLE_RAW.store(status_handle.0 as isize, Ordering::SeqCst);
    set_status(status_handle, SERVICE_START_PENDING, 0);
    set_status(
        status_handle,
        SERVICE_RUNNING,
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
    );

    let sid = ALLOWED_SID_GLOBAL
        .get()
        .cloned()
        .unwrap_or_else(|| "S-1-5-32-544".to_string());
    let _ = run_pipe_server_loop(&sid);

    set_status(status_handle, SERVICE_STOPPED, 0);
}

unsafe fn set_status(
    handle: SERVICE_STATUS_HANDLE,
    state: windows::Win32::System::Services::SERVICE_STATUS_CURRENT_STATE,
    controls: u32,
) {
    let st = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: controls,
        dwWin32ExitCode: 0,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 2000,
    };
    let _ = SetServiceStatus(handle, &st);
}

/// Runs the strictly read-only Named Pipe server loop with DACL restricted to SYSTEM + `allowed_sid`.
pub fn run_pipe_server_loop(allowed_sid: &str) -> Result<(), String> {
    let sddl = build_pipe_sddl(allowed_sid)?;
    let sddl_w: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let pipe_w: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

    while !SERVICE_STOP.load(Ordering::Relaxed) {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        let sd_ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl_w.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
        };
        if let Err(e) = sd_ok {
            return Err(format!("SDDL inválido: {e}"));
        }

        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };

        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_w.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                4,
                256 * 1024,
                64 * 1024,
                0,
                Some(&sa),
            )
        };

        unsafe {
            let _ = LocalFree(HLOCAL(sd.0));
        }

        if pipe == INVALID_HANDLE_VALUE {
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let connected = unsafe { ConnectNamedPipe(pipe, None) };
        if connected.is_ok() {
            handle_client_connection(pipe);
        }

        unsafe {
            let _ = FlushFileBuffers(pipe);
            let _ = DisconnectNamedPipe(pipe);
            let _ = CloseHandle(pipe);
        }
    }

    Ok(())
}

fn read_exact_pipe(pipe: HANDLE, buf: &mut [u8]) -> Result<(), String> {
    let mut offset = 0usize;
    while offset < buf.len() {
        let mut read = 0u32;
        unsafe { ReadFile(pipe, Some(&mut buf[offset..]), Some(&mut read), None) }
            .map_err(|e| format!("ReadFile pipe falló: {e}"))?;
        if read == 0 {
            return Err("EOF en Named Pipe.".to_string());
        }
        offset += read as usize;
    }
    Ok(())
}

fn write_all_pipe(pipe: HANDLE, data: &[u8]) -> Result<(), String> {
    let mut offset = 0usize;
    while offset < data.len() {
        let mut written = 0u32;
        unsafe { WriteFile(pipe, Some(&data[offset..]), Some(&mut written), None) }
            .map_err(|e| format!("WriteFile pipe falló: {e}"))?;
        if written == 0 {
            return Err("WriteFile 0 bytes en Named Pipe.".to_string());
        }
        offset += written as usize;
    }
    Ok(())
}

fn handle_client_connection(pipe: HANDLE) {
    let mut len_buf = [0u8; 4];
    if read_exact_pipe(pipe, &mut len_buf).is_err() {
        return;
    }
    let req_len = u32::from_le_bytes(len_buf) as usize;
    if req_len == 0 || req_len > MAX_REQUEST_FRAME_BYTES {
        let _ = send_json_response(
            pipe,
            &ServiceResponse::Error {
                code: "invalid_frame".to_string(),
                message: "Longitud de frame fuera de rango.".to_string(),
            },
        );
        return;
    }

    let mut req_buf = vec![0u8; req_len];
    if read_exact_pipe(pipe, &mut req_buf).is_err() {
        return;
    }

    let req = match decode_request_frame(&req_buf) {
        Ok(r) => r,
        Err(msg) => {
            let _ = send_json_response(
                pipe,
                &ServiceResponse::Error {
                    code: "validation_error".to_string(),
                    message: msg,
                },
            );
            return;
        }
    };

    match req {
        ServiceRequest::Ping => {
            let _ = send_json_response(
                pipe,
                &ServiceResponse::Pong {
                    protocol_version: PROTOCOL_VERSION,
                    read_only: true,
                },
            );
        }
        ServiceRequest::QueryUsnJournal { drive_letter } => {
            let root = format!("{}:\\", drive_letter.to_ascii_uppercase());
            match query_usn_journal_for_root(&root) {
                Ok(info) => {
                    let _ = send_json_response(pipe, &ServiceResponse::UsnJournal { info });
                }
                Err(msg) => {
                    let _ = send_json_response(
                        pipe,
                        &ServiceResponse::Error {
                            code: "usn_query_failed".to_string(),
                            message: msg,
                        },
                    );
                }
            }
        }
        ServiceRequest::ReadUsnRecords {
            drive_letter,
            journal_id,
            start_usn,
        } => {
            let root = format!("{}:\\", drive_letter.to_ascii_uppercase());
            let reader = WinUsnJournalReader;
            match reader.read_records(&root, journal_id, start_usn) {
                Ok((events, next_usn)) => {
                    let _ =
                        send_json_response(pipe, &ServiceResponse::UsnRecords { events, next_usn });
                }
                Err(msg) => {
                    let _ = send_json_response(
                        pipe,
                        &ServiceResponse::Error {
                            code: "usn_read_failed".to_string(),
                            message: msg,
                        },
                    );
                }
            }
        }
        ServiceRequest::ScanMft {
            drive_letter,
            include_ads,
        } => {
            let root = format!("{}:\\", drive_letter.to_ascii_uppercase());
            let (_, _, _, serial) = query_volume_meta(&root);
            let ctx = DerivedBuildContext::load_default();
            let cancel = Arc::new(AtomicBool::new(false));
            match scan_volume_mft_direct(
                PathBuf::from(&root),
                serial,
                include_ads,
                cancel,
                &ctx,
                ScanEngineMode::MftService,
                |_| {},
            ) {
                Ok(mut tree) => {
                    if let Ok(info) = query_usn_journal_for_root(&root) {
                        tree.usn_journal_id = info.usn_journal_id;
                        tree.last_usn = info.next_usn;
                    }
                    // Serialize into a temp dir under SYSTEM temp, read raw SCIDX bytes, then delete immediately
                    let tmp_dir = std::env::temp_dir().join("scancleaner-svc-stream");
                    if let Ok(snap_path) = save_volume_snapshot(&tree, &tmp_dir) {
                        if let Ok(bytes) = std::fs::read(&snap_path) {
                            let _ = std::fs::remove_file(&snap_path);
                            if send_json_response(
                                pipe,
                                &ServiceResponse::MftSnapshotReady {
                                    payload_bytes: bytes.len() as u64,
                                    usn_journal_id: tree.usn_journal_id,
                                    last_usn: tree.last_usn,
                                },
                            )
                            .is_ok()
                            {
                                let _ = write_all_pipe(pipe, &bytes);
                            }
                            return;
                        }
                        let _ = std::fs::remove_file(&snap_path);
                    }
                    let _ = send_json_response(
                        pipe,
                        &ServiceResponse::Error {
                            code: "serialize_error".to_string(),
                            message: "No se pudo serializar snapshot MFT.".to_string(),
                        },
                    );
                }
                Err(msg) => {
                    let _ = send_json_response(
                        pipe,
                        &ServiceResponse::Error {
                            code: "mft_scan_failed".to_string(),
                            message: msg,
                        },
                    );
                }
            }
        }
    }
}

fn send_json_response(pipe: HANDLE, resp: &ServiceResponse) -> Result<(), String> {
    let raw = serde_json::to_vec(resp).map_err(|e| e.to_string())?;
    let frame = encode_frame(&raw)?;
    write_all_pipe(pipe, &frame)
}
