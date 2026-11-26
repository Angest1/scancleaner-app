use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};

use super::model::FileSystemType;

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    RamDisk,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub root_path: String,
    pub label: String,
    pub kind: DriveKind,
    pub fs_type: FileSystemType,
    pub fs_name: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub volume_serial: u32,
    pub available: bool,
}

pub fn parse_fs_type(fs_name: &str, kind: DriveKind) -> FileSystemType {
    if kind == DriveKind::Network {
        return FileSystemType::Network;
    }
    match fs_name.to_ascii_uppercase().as_str() {
        "NTFS" => FileSystemType::Ntfs,
        "FAT32" => FileSystemType::Fat32,
        "EXFAT" => FileSystemType::ExFat,
        "REFS" => FileSystemType::Refs,
        _ => FileSystemType::Unknown,
    }
}

pub fn query_volume_meta(root: &str) -> (String, FileSystemType, String, u32) {
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut vol_name = [0u16; 260];
    let mut fs_name = [0u16; 64];
    let mut serial = 0u32;

    let ok = unsafe {
        GetVolumeInformationW(
            PCWSTR(wide.as_ptr()),
            Some(&mut vol_name),
            Some(&mut serial),
            None,
            None,
            Some(&mut fs_name),
        )
    };

    if ok.is_ok() {
        let label = utf16_z_to_string(&vol_name);
        let fs = utf16_z_to_string(&fs_name);
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        let kind = map_drive_kind(drive_type);
        let fs_type = parse_fs_type(&fs, kind);
        (label, fs_type, fs, serial)
    } else {
        (String::new(), FileSystemType::Unknown, String::new(), 0)
    }
}

fn map_drive_kind(drive_type: u32) -> DriveKind {
    match drive_type {
        DRIVE_FIXED => DriveKind::Fixed,
        DRIVE_REMOVABLE => DriveKind::Removable,
        DRIVE_REMOTE => DriveKind::Network,
        DRIVE_CDROM => DriveKind::Optical,
        DRIVE_RAMDISK => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    }
}

fn utf16_z_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end])
        .to_string_lossy()
        .into_owned()
}

pub fn list_drives(include_network: bool) -> Vec<DriveInfo> {
    let len = unsafe { GetLogicalDriveStringsW(None) };
    if len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; (len as usize) + 2];
    let written = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if written == 0 {
        return Vec::new();
    }

    let mut roots = Vec::new();
    let mut start = 0usize;
    let slice = &buf[..written as usize];
    for (idx, &ch) in slice.iter().enumerate() {
        if ch == 0 {
            if idx > start {
                let s = OsString::from_wide(&slice[start..idx])
                    .to_string_lossy()
                    .into_owned();
                roots.push(s);
            }
            start = idx + 1;
        }
    }

    let mut local_roots = Vec::new();
    let mut network_roots = Vec::new();

    for root in roots {
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let dtype = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        let kind = map_drive_kind(dtype);
        match kind {
            DriveKind::Fixed | DriveKind::Removable | DriveKind::RamDisk => {
                local_roots.push((root, kind));
            }
            DriveKind::Network if include_network => {
                network_roots.push((root, kind));
            }
            _ => {}
        }
    }

    let mut drives: Vec<DriveInfo> = local_roots
        .into_iter()
        .map(|(root, kind)| probe_drive(&root, kind))
        .collect();

    if !network_roots.is_empty() {
        // Probe network shares concurrently so disconnected shares do not stall sequentially
        let handles: Vec<_> = network_roots
            .into_iter()
            .map(|(root, kind)| std::thread::spawn(move || probe_drive(&root, kind)))
            .collect();
        for handle in handles {
            if let Ok(info) = handle.join() {
                drives.push(info);
            }
        }
    }

    drives.sort_by(|a, b| a.root_path.cmp(&b.root_path));
    drives
}

fn probe_drive(root: &str, kind: DriveKind) -> DriveInfo {
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let mut free_avail = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;

    let space_ok = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_avail),
            Some(&mut total),
            Some(&mut total_free),
        )
    }
    .is_ok();

    let (label, fs_type, fs_name, volume_serial) = if space_ok {
        query_volume_meta(root)
    } else {
        (String::new(), FileSystemType::Unknown, String::new(), 0)
    };

    let used = total.saturating_sub(total_free);
    DriveInfo {
        root_path: root.to_string(),
        label,
        kind,
        fs_type,
        fs_name,
        total_bytes: total,
        free_bytes: total_free,
        used_bytes: used,
        volume_serial,
        available: space_ok && total > 0,
    }
}
