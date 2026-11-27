use chrono::Utc;
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use crate::scanner::derived::DerivedIndexes;
use crate::scanner::model::{FileSystemType, ScanEngineMode, ScanStats, ScanTree};

use super::default_index_dir;

pub const SNAPSHOT_MAGIC: &[u8; 8] = b"SCIDX001";
pub const SNAPSHOT_VERSION: u32 = 1;
const HEADER_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub version: u32,
    pub volume_serial: u32,
    pub usn_journal_id: u64,
    pub last_usn: i64,
    pub created_ts: i64,
    pub include_ads: bool,
    pub fs_type: FileSystemType,
}

#[derive(Serialize, Deserialize)]
struct SnapshotTailMeta {
    root_path: String,
    stats: ScanStats,
    derived: DerivedIndexes,
    access_denied: Vec<String>,
}

pub fn snapshot_path_for_volume(index_dir: &Path, drive_root: &str, volume_serial: u32) -> PathBuf {
    let drive_tag: String = drive_root
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let tag = if drive_tag.is_empty() {
        "vol".to_string()
    } else {
        drive_tag.to_ascii_uppercase()
    };
    index_dir.join(format!("vol_{tag}_{volume_serial:08X}.scidx"))
}

pub fn default_snapshot_path(drive_root: &str, volume_serial: u32) -> PathBuf {
    snapshot_path_for_volume(&default_index_dir(), drive_root, volume_serial)
}

/// Atomically persists a versioned, LZ4-compressed, CRC32-checked binary snapshot of `ScanTree`.
pub fn save_volume_snapshot(tree: &ScanTree, index_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(index_dir)
        .map_err(|e| format!("No se pudo crear carpeta de índice: {e}"))?;
    let root_str = tree.root_path.to_string_lossy().into_owned();
    let final_path = snapshot_path_for_volume(index_dir, &root_str, tree.volume_serial);

    let raw_payload = serialize_soa_payload(tree)?;
    let compressed = lz4_flex::compress(&raw_payload);
    let crc = crc32fast::hash(&compressed);

    let mut header = [0u8; HEADER_LEN];
    header[0..8].copy_from_slice(SNAPSHOT_MAGIC);
    header[8..12].copy_from_slice(&SNAPSHOT_VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&tree.volume_serial.to_le_bytes());
    header[16..24].copy_from_slice(&tree.usn_journal_id.to_le_bytes());
    header[24..32].copy_from_slice(&tree.last_usn.to_le_bytes());
    header[32..40].copy_from_slice(&Utc::now().timestamp().to_le_bytes());
    let flags: u32 = u32::from(tree.include_ads);
    header[40..44].copy_from_slice(&flags.to_le_bytes());
    header[44..48].copy_from_slice(&(tree.fs_type as u32).to_le_bytes());
    header[48..56].copy_from_slice(&(raw_payload.len() as u64).to_le_bytes());
    header[56..60].copy_from_slice(&crc.to_le_bytes());

    let tmp_name = format!(".tmp-{}-{}.scidx", std::process::id(), uuid::Uuid::new_v4());
    let tmp_path = index_dir.join(tmp_name);

    {
        let mut f = File::create(&tmp_path)
            .map_err(|e| format!("Error creando archivo temporal de snapshot: {e}"))?;
        f.write_all(&header)
            .and_then(|()| f.write_all(&compressed))
            .and_then(|()| f.sync_data())
            .map_err(|e| format!("Error escribiendo snapshot: {e}"))?;
    }

    atomic_replace_file(&tmp_path, &final_path)?;
    Ok(final_path)
}

fn atomic_replace_file(src: &Path, dst: &Path) -> Result<(), String> {
    let src_w: Vec<u16> = src
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let dst_w: Vec<u16> = dst
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| {
        let _ = fs::remove_file(src);
        format!("MoveFileExW atómico falló: {e}")
    })
}

/// Loads a binary snapshot using `memmap2::Mmap`, verifies magic, version, volume serial,
/// and CRC32 checksum, and decompresses directly into flat SoA arrays.
pub fn load_volume_snapshot(
    path: &Path,
    expected_serial: Option<u32>,
) -> Result<(ScanTree, SnapshotMeta), String> {
    let file = File::open(path).map_err(|e| format!("No se pudo abrir snapshot: {e}"))?;
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("mmap falló: {e}"))?;
    if mmap.len() < HEADER_LEN {
        return Err("Snapshot truncado (cabecera incompleta).".to_string());
    }

    if &mmap[0..8] != SNAPSHOT_MAGIC {
        return Err("Snapshot inválido (magic incorrecto).".to_string());
    }
    let version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
    if version != SNAPSHOT_VERSION {
        return Err(format!("Versión de snapshot incompatible: v{version}"));
    }
    let volume_serial = u32::from_le_bytes(mmap[12..16].try_into().unwrap());
    if let Some(exp) = expected_serial {
        if exp != 0 && volume_serial != exp {
            return Err(format!(
                "Número de serie de volumen no coincide (esperado {exp:08X}, snapshot {volume_serial:08X})."
            ));
        }
    }
    let usn_journal_id = u64::from_le_bytes(mmap[16..24].try_into().unwrap());
    let last_usn = i64::from_le_bytes(mmap[24..32].try_into().unwrap());
    let created_ts = i64::from_le_bytes(mmap[32..40].try_into().unwrap());
    let flags = u32::from_le_bytes(mmap[40..44].try_into().unwrap());
    let fs_type =
        FileSystemType::from_u8(u32::from_le_bytes(mmap[44..48].try_into().unwrap()) as u8);
    let uncompressed_len = u64::from_le_bytes(mmap[48..56].try_into().unwrap()) as usize;
    let stored_crc = u32::from_le_bytes(mmap[56..60].try_into().unwrap());

    let compressed = &mmap[HEADER_LEN..];
    let computed_crc = crc32fast::hash(compressed);
    if computed_crc != stored_crc {
        return Err(format!(
            "Checksum CRC32 corrupto (esperado {stored_crc:08X}, calculado {computed_crc:08X})."
        ));
    }

    let raw = lz4_flex::decompress(compressed, uncompressed_len)
        .map_err(|e| format!("Descompresión LZ4 falló: {e}"))?;

    let meta = SnapshotMeta {
        version,
        volume_serial,
        usn_journal_id,
        last_usn,
        created_ts,
        include_ads: (flags & 1) != 0,
        fs_type,
    };

    let tree = deserialize_soa_payload(&raw, &meta)?;
    Ok((tree, meta))
}

fn push_u32_slice(out: &mut Vec<u8>, slice: &[u32]) {
    for &v in slice {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn push_u16_slice(out: &mut Vec<u8>, slice: &[u16]) {
    for &v in slice {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn push_u64_slice(out: &mut Vec<u8>, slice: &[u64]) {
    for &v in slice {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn serialize_soa_payload(tree: &ScanTree) -> Result<Vec<u8>, String> {
    let n = tree.len();
    let tail = SnapshotTailMeta {
        root_path: tree.root_path.to_string_lossy().into_owned(),
        stats: tree.stats.clone(),
        derived: tree.derived.clone(),
        access_denied: tree
            .access_denied
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    };
    let tail_json =
        serde_json::to_vec(&tail).map_err(|e| format!("Error serializando metadatos: {e}"))?;

    let est = 16 + n * 55 + tree.name_pool.len() + tail_json.len();
    let mut out = Vec::with_capacity(est);
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(tree.name_pool.len() as u32).to_le_bytes());
    out.extend_from_slice(&(tail_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    push_u32_slice(&mut out, &tree.parent);
    push_u32_slice(&mut out, &tree.first_child);
    push_u32_slice(&mut out, &tree.next_sibling);
    push_u32_slice(&mut out, &tree.name_offset);
    push_u16_slice(&mut out, &tree.name_len);
    push_u64_slice(&mut out, &tree.logical_size);
    push_u64_slice(&mut out, &tree.allocated_size);
    push_u32_slice(&mut out, &tree.file_count);
    push_u32_slice(&mut out, &tree.last_write_ts);
    out.extend_from_slice(&tree.kind);
    push_u16_slice(&mut out, &tree.flags);
    out.extend_from_slice(&tree.type_cat);
    out.extend_from_slice(&tree.origin_cat);
    push_u64_slice(&mut out, &tree.file_ref);
    out.extend_from_slice(&tree.name_pool);
    out.extend_from_slice(&tail_json);
    Ok(out)
}

fn read_bytes<'a>(buf: &'a [u8], pos: &mut usize, count: usize) -> Result<&'a [u8], String> {
    if *pos + count > buf.len() {
        return Err("Payload de snapshot truncado.".to_string());
    }
    let slice = &buf[*pos..*pos + count];
    *pos += count;
    Ok(slice)
}

fn read_u32_vec(buf: &[u8], pos: &mut usize, n: usize) -> Result<Vec<u32>, String> {
    let bytes = read_bytes(buf, pos, n * 4)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

fn read_u16_vec(buf: &[u8], pos: &mut usize, n: usize) -> Result<Vec<u16>, String> {
    let bytes = read_bytes(buf, pos, n * 2)?;
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect())
}

fn read_u64_vec(buf: &[u8], pos: &mut usize, n: usize) -> Result<Vec<u64>, String> {
    let bytes = read_bytes(buf, pos, n * 8)?;
    Ok(bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_le_bytes(*c))
        .collect())
}

fn deserialize_soa_payload(buf: &[u8], meta: &SnapshotMeta) -> Result<ScanTree, String> {
    let mut pos = 0usize;
    let hdr = read_bytes(buf, &mut pos, 16)?;
    let n = u32::from_le_bytes(hdr[0..4].try_into().unwrap()) as usize;
    let pool_len = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
    let tail_len = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;

    let parent = read_u32_vec(buf, &mut pos, n)?;
    let first_child = read_u32_vec(buf, &mut pos, n)?;
    let next_sibling = read_u32_vec(buf, &mut pos, n)?;
    let name_offset = read_u32_vec(buf, &mut pos, n)?;
    let name_len = read_u16_vec(buf, &mut pos, n)?;
    let logical_size = read_u64_vec(buf, &mut pos, n)?;
    let allocated_size = read_u64_vec(buf, &mut pos, n)?;
    let file_count = read_u32_vec(buf, &mut pos, n)?;
    let last_write_ts = read_u32_vec(buf, &mut pos, n)?;
    let kind = read_bytes(buf, &mut pos, n)?.to_vec();
    let flags = read_u16_vec(buf, &mut pos, n)?;
    let type_cat = read_bytes(buf, &mut pos, n)?.to_vec();
    let origin_cat = read_bytes(buf, &mut pos, n)?.to_vec();
    let file_ref = read_u64_vec(buf, &mut pos, n)?;
    let name_pool = read_bytes(buf, &mut pos, pool_len)?.to_vec();
    let tail_bytes = read_bytes(buf, &mut pos, tail_len)?;

    let mut tail: SnapshotTailMeta = serde_json::from_slice(tail_bytes)
        .map_err(|e| format!("Error deserializando tail de snapshot: {e}"))?;
    tail.stats.engine_mode = ScanEngineMode::SnapshotCached;

    let mut tree = ScanTree::new(
        PathBuf::from(&tail.root_path),
        meta.volume_serial,
        meta.fs_type,
    );
    tree.usn_journal_id = meta.usn_journal_id;
    tree.last_usn = meta.last_usn;
    tree.include_ads = meta.include_ads;
    tree.parent = parent;
    tree.first_child = first_child;
    tree.next_sibling = next_sibling;
    tree.name_offset = name_offset;
    tree.name_len = name_len;
    tree.logical_size = logical_size;
    tree.allocated_size = allocated_size;
    tree.file_count = file_count;
    tree.last_write_ts = last_write_ts;
    tree.kind = kind;
    tree.flags = flags;
    tree.type_cat = type_cat;
    tree.origin_cat = origin_cat;
    tree.file_ref = file_ref;
    tree.name_pool = name_pool;
    tree.access_denied = tail.access_denied.into_iter().map(PathBuf::from).collect();
    tree.stats = tail.stats;
    tree.derived = tail.derived;
    tree.rebuild_frn_index();
    Ok(tree)
}
