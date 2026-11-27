use serde::{Deserialize, Serialize};

use crate::index::usn::{UsnEvent, UsnJournalInfo};

pub const SERVICE_NAME: &str = "ScanCleanerIndexService";
pub const SERVICE_DISPLAY_NAME: &str = "ScanCleaner MFT/USN Read-Only Indexer";
pub const PIPE_NAME: &str = r"\\.\pipe\ScanCleanerIndexSvc_v1";
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_FRAME_BYTES: usize = 4096;
pub const MAX_RESPONSE_FRAME_BYTES: usize = 128 * 1024 * 1024; // 128 MB max compressed snapshot

/// Strictly read-only metadata requests accepted by the indexing service.
/// No write, delete, rename, or process-launch opcodes exist in this protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ServiceRequest {
    Ping,
    QueryUsnJournal {
        drive_letter: char,
    },
    ReadUsnRecords {
        drive_letter: char,
        journal_id: u64,
        start_usn: i64,
    },
    ScanMft {
        drive_letter: char,
        include_ads: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ServiceResponse {
    Pong {
        protocol_version: u32,
        read_only: bool,
    },
    UsnJournal {
        info: UsnJournalInfo,
    },
    UsnRecords {
        events: Vec<UsnEvent>,
        next_usn: i64,
    },
    /// Base64 or raw binary length-prefixed snapshot bytes follow on the pipe
    MftSnapshotReady {
        payload_bytes: u64,
        usn_journal_id: u64,
        last_usn: i64,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Validates every incoming client request before execution.
pub fn validate_request(req: &ServiceRequest) -> Result<(), String> {
    match req {
        ServiceRequest::Ping => Ok(()),
        ServiceRequest::QueryUsnJournal { drive_letter }
        | ServiceRequest::ScanMft { drive_letter, .. } => validate_drive_letter(*drive_letter),
        ServiceRequest::ReadUsnRecords {
            drive_letter,
            start_usn,
            ..
        } => {
            validate_drive_letter(*drive_letter)?;
            if *start_usn < 0 {
                return Err("start_usn no puede ser negativo.".to_string());
            }
            Ok(())
        }
    }
}

pub fn validate_drive_letter(ch: char) -> Result<(), String> {
    if ch.is_ascii_alphabetic() {
        Ok(())
    } else {
        Err(format!("Letra de unidad inválida: '{ch}'"))
    }
}

/// Builds the SDDL string granting GENERIC_ALL exclusively to SYSTEM (`SY`) and the installing user SID.
pub fn build_pipe_sddl(allowed_user_sid: &str) -> Result<String, String> {
    let trimmed = allowed_user_sid.trim();
    if !trimmed.starts_with("S-1-")
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == 'S')
    {
        return Err(format!(
            "SID de usuario inválido para ACL de pipe: {trimmed}"
        ));
    }
    Ok(format!("D:(A;;GA;;;SY)(A;;GA;;;{trimmed})"))
}

pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > MAX_RESPONSE_FRAME_BYTES {
        return Err("Frame supera el límite máximo permitido.".to_string());
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode_request_frame(raw: &[u8]) -> Result<ServiceRequest, String> {
    if raw.len() > MAX_REQUEST_FRAME_BYTES {
        return Err("Solicitud excede MAX_REQUEST_FRAME_BYTES.".to_string());
    }
    let req: ServiceRequest =
        serde_json::from_slice(raw).map_err(|e| format!("JSON de solicitud inválido: {e}"))?;
    validate_request(&req)?;
    Ok(req)
}
