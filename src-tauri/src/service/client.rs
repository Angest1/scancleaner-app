use std::fs;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_SHARE_NONE, OPEN_EXISTING,
};
use windows::Win32::System::Pipes::WaitNamedPipeW;

use crate::index::snapshot::load_volume_snapshot;
use crate::index::usn::{UsnEvent, UsnJournalInfo, UsnJournalReader};
use crate::scanner::derived::DerivedBuildContext;
use crate::scanner::model::{ScanEngineMode, ScanProgress, ScanTree};

use super::protocol::{
    encode_frame, validate_request, ServiceRequest, ServiceResponse, MAX_RESPONSE_FRAME_BYTES,
    PIPE_NAME,
};

fn connect_pipe(timeout_ms: u32) -> Result<HANDLE, String> {
    let pipe_w: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = unsafe { WaitNamedPipeW(PCWSTR(pipe_w.as_ptr()), timeout_ms) };
    let handle = unsafe {
        CreateFileW(
            PCWSTR(pipe_w.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|e| format!("Servicio de indexación no disponible: {e}"))?;
    if handle == INVALID_HANDLE_VALUE {
        return Err("Handle de Named Pipe inválido.".to_string());
    }
    Ok(handle)
}

fn read_exact(handle: HANDLE, buf: &mut [u8]) -> Result<(), String> {
    let mut off = 0usize;
    while off < buf.len() {
        let mut n = 0u32;
        unsafe { ReadFile(handle, Some(&mut buf[off..]), Some(&mut n), None) }
            .map_err(|e| format!("Error leyendo respuesta del servicio: {e}"))?;
        if n == 0 {
            return Err("EOF leyendo del servicio.".to_string());
        }
        off += n as usize;
    }
    Ok(())
}

fn write_all(handle: HANDLE, data: &[u8]) -> Result<(), String> {
    let mut off = 0usize;
    while off < data.len() {
        let mut n = 0u32;
        unsafe { WriteFile(handle, Some(&data[off..]), Some(&mut n), None) }
            .map_err(|e| format!("Error enviando solicitud al servicio: {e}"))?;
        if n == 0 {
            return Err("Escritura vacía en Named Pipe.".to_string());
        }
        off += n as usize;
    }
    Ok(())
}

fn send_request_raw(req: &ServiceRequest) -> Result<(HANDLE, ServiceResponse), String> {
    validate_request(req)?;
    let handle = connect_pipe(1_500)?;
    let body = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    let frame = encode_frame(&body)?;
    if let Err(e) = write_all(handle, &frame) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(e);
    }

    let mut len_buf = [0u8; 4];
    if let Err(e) = read_exact(handle, &mut len_buf) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(e);
    }
    let resp_len = u32::from_le_bytes(len_buf) as usize;
    if resp_len == 0 || resp_len > MAX_RESPONSE_FRAME_BYTES {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err("Frame de respuesta inválido.".to_string());
    }
    let mut resp_buf = vec![0u8; resp_len];
    if let Err(e) = read_exact(handle, &mut resp_buf) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(e);
    }
    let resp: ServiceResponse = match serde_json::from_slice(&resp_buf) {
        Ok(r) => r,
        Err(e) => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(format!("Respuesta JSON inválida: {e}"));
        }
    };
    Ok((handle, resp))
}

pub fn ping_service() -> bool {
    let Ok((h, resp)) = send_request_raw(&ServiceRequest::Ping) else {
        return false;
    };
    unsafe {
        let _ = CloseHandle(h);
    }
    matches!(
        resp,
        ServiceResponse::Pong {
            read_only: true,
            ..
        }
    )
}

pub fn try_scan_volume_via_service<F>(
    drive_root: &str,
    expected_serial: u32,
    include_ads: bool,
    _ctx: &DerivedBuildContext,
    on_progress: F,
) -> Result<ScanTree, String>
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let drive_letter = drive_root
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| "Letra de unidad inválida.".to_string())?;

    on_progress(ScanProgress {
        drive_root: drive_root.to_string(),
        phase: format!("Escaneando {drive_root} (Servicio MFT)…"),
        engine_mode: ScanEngineMode::MftService,
        scanned_files: 0,
        scanned_dirs: 0,
        total_allocated_bytes: 0,
        current_path: "$MFT (servicio solo-lectura)".to_string(),
        elapsed_ms: 0,
        mft_processed: None,
        mft_total: None,
        volume_used_bytes: None,
        usn_processed: None,
        usn_total: None,
        progress_ratio: None,
    });

    let (handle, resp) = send_request_raw(&ServiceRequest::ScanMft {
        drive_letter,
        include_ads,
    })?;

    let payload_bytes = match resp {
        ServiceResponse::MftSnapshotReady { payload_bytes, .. } => payload_bytes as usize,
        ServiceResponse::Error { message, .. } => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(message);
        }
        _ => {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err("Respuesta inesperada del servicio MFT.".to_string());
        }
    };

    if payload_bytes > MAX_RESPONSE_FRAME_BYTES {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err("Snapshot del servicio excede el límite máximo.".to_string());
    }

    let mut raw = vec![0u8; payload_bytes];
    let read_res = read_exact(handle, &mut raw);
    unsafe {
        let _ = CloseHandle(handle);
    }
    read_res?;

    let tmp_path = std::env::temp_dir().join(format!("sc-pipe-{}.scidx", uuid::Uuid::new_v4()));
    fs::write(&tmp_path, &raw).map_err(|e| e.to_string())?;
    let loaded = load_volume_snapshot(&tmp_path, Some(expected_serial));
    let _ = fs::remove_file(&tmp_path);

    let (mut tree, _) = loaded?;
    tree.stats.engine_mode = ScanEngineMode::MftService;
    Ok(tree)
}

/// USN reader backed by the optional elevated read-only indexing service over Named Pipe.
pub struct ServiceUsnJournalReader;

impl UsnJournalReader for ServiceUsnJournalReader {
    fn query_journal(&self, drive_root: &str) -> Result<UsnJournalInfo, String> {
        let drive_letter = drive_root
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .ok_or_else(|| "Letra de unidad inválida.".to_string())?;
        let (h, resp) = send_request_raw(&ServiceRequest::QueryUsnJournal { drive_letter })?;
        unsafe {
            let _ = CloseHandle(h);
        }
        match resp {
            ServiceResponse::UsnJournal { info } => Ok(info),
            ServiceResponse::Error { message, .. } => Err(message),
            _ => Err("Respuesta inesperada en QueryUsnJournal.".to_string()),
        }
    }

    fn read_records(
        &self,
        drive_root: &str,
        journal_id: u64,
        start_usn: i64,
    ) -> Result<(Vec<UsnEvent>, i64), String> {
        let drive_letter = drive_root
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .ok_or_else(|| "Letra de unidad inválida.".to_string())?;
        let (h, resp) = send_request_raw(&ServiceRequest::ReadUsnRecords {
            drive_letter,
            journal_id,
            start_usn,
        })?;
        unsafe {
            let _ = CloseHandle(h);
        }
        match resp {
            ServiceResponse::UsnRecords { events, next_usn } => Ok((events, next_usn)),
            ServiceResponse::Error { message, .. } => Err(message),
            _ => Err("Respuesta inesperada en ReadUsnRecords.".to_string()),
        }
    }
}
