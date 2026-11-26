use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::categorizer::CategoryBucket;
use crate::scanner::model::ScanTree;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopDirSnapshot {
    pub name: String,
    pub allocated_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSnapshot {
    pub id: i64,
    pub drive_root: String,
    pub scanned_at: String,
    pub total_disk_bytes: u64,
    pub free_disk_bytes: u64,
    pub scanned_allocated_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub duration_ms: u64,
    pub categories: Vec<CategoryBucket>,
    pub top_dirs: Vec<TopDirSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub older_id: i64,
    pub newer_id: i64,
    pub drive_root: String,
    pub allocated_delta_bytes: i64,
    pub file_count_delta: i64,
    pub top_dir_deltas: Vec<DirDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirDelta {
    pub name: String,
    pub older_bytes: u64,
    pub newer_bytes: u64,
    pub delta_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupLogEntry {
    pub timestamp: String,
    pub plan_id: String,
    pub path: String,
    pub category: String,
    pub bytes: u64,
    pub mode: String,
    pub status: String,
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

pub fn app_data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata).join("ScanCleaner")
    } else {
        PathBuf::from(".scancleaner")
    }
}

pub fn default_db_path() -> PathBuf {
    app_data_dir().join("history.db")
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

pub struct Storage {
    db_path: PathBuf,
}

impl Storage {
    pub fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scan_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                drive_root TEXT NOT NULL,
                scanned_at TEXT NOT NULL,
                total_disk_bytes INTEGER NOT NULL,
                free_disk_bytes INTEGER NOT NULL,
                scanned_allocated_bytes INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                dir_count INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                categories_json TEXT NOT NULL,
                top_dirs_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_drive ON scan_snapshots(drive_root, id DESC);",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { db_path })
    }

    pub fn save_snapshot(
        &self,
        tree: &ScanTree,
        total_disk_bytes: u64,
        free_disk_bytes: u64,
        categories: &[CategoryBucket],
    ) -> Result<i64, String> {
        let mut top_dirs: Vec<TopDirSnapshot> = tree
            .children_iter(0)
            .filter_map(|cid| {
                let n = &tree.nodes[cid as usize];
                if n.allocated_size > 0 {
                    Some(TopDirSnapshot {
                        name: tree.node_name(cid).to_string(),
                        allocated_bytes: n.allocated_size,
                    })
                } else {
                    None
                }
            })
            .collect();
        top_dirs.sort_by_key(|a| std::cmp::Reverse(a.allocated_bytes));
        top_dirs.truncate(20);

        let conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        let now = Utc::now().to_rfc3339();
        let cats_json = serde_json::to_string(categories).map_err(|e| e.to_string())?;
        let dirs_json = serde_json::to_string(&top_dirs).map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT INTO scan_snapshots (
                drive_root, scanned_at, total_disk_bytes, free_disk_bytes,
                scanned_allocated_bytes, file_count, dir_count, duration_ms,
                categories_json, top_dirs_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                tree.root_path.to_string_lossy().as_ref(),
                now,
                total_disk_bytes as i64,
                free_disk_bytes as i64,
                tree.stats.total_allocated_bytes as i64,
                tree.stats.total_files as i64,
                tree.stats.total_dirs as i64,
                tree.stats.duration_ms as i64,
                cats_json,
                dirs_json
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }

    pub fn list_snapshots(&self, drive_root: Option<&str>) -> Result<Vec<ScanSnapshot>, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let sql_all = "SELECT id, drive_root, scanned_at, total_disk_bytes, free_disk_bytes,
            scanned_allocated_bytes, file_count, dir_count, duration_ms, categories_json, top_dirs_json
            FROM scan_snapshots ORDER BY id DESC LIMIT 100";
        let sql_filtered = "SELECT id, drive_root, scanned_at, total_disk_bytes, free_disk_bytes,
            scanned_allocated_bytes, file_count, dir_count, duration_ms, categories_json, top_dirs_json
            FROM scan_snapshots WHERE lower(drive_root) = lower(?1) ORDER BY id DESC LIMIT 100";

        let mut stmt = conn
            .prepare(if drive_root.is_some() {
                sql_filtered
            } else {
                sql_all
            })
            .map_err(|e| e.to_string())?;

        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ScanSnapshot> {
            let cats_raw: String = row.get(9)?;
            let dirs_raw: String = row.get(10)?;
            Ok(ScanSnapshot {
                id: row.get(0)?,
                drive_root: row.get(1)?,
                scanned_at: row.get(2)?,
                total_disk_bytes: row.get::<_, i64>(3)?.max(0) as u64,
                free_disk_bytes: row.get::<_, i64>(4)?.max(0) as u64,
                scanned_allocated_bytes: row.get::<_, i64>(5)?.max(0) as u64,
                file_count: row.get::<_, i64>(6)?.max(0) as u64,
                dir_count: row.get::<_, i64>(7)?.max(0) as u64,
                duration_ms: row.get::<_, i64>(8)?.max(0) as u64,
                categories: serde_json::from_str(&cats_raw).unwrap_or_default(),
                top_dirs: serde_json::from_str(&dirs_raw).unwrap_or_default(),
            })
        };

        if let Some(dr) = drive_root {
            let rows = stmt
                .query_map(params![dr], map_row)
                .map_err(|e| e.to_string())?;
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
        } else {
            let rows = stmt.query_map([], map_row).map_err(|e| e.to_string())?;
            for r in rows {
                out.push(r.map_err(|e| e.to_string())?);
            }
        }
        Ok(out)
    }

    pub fn diff_snapshots(&self, older_id: i64, newer_id: i64) -> Result<SnapshotDiff, String> {
        let all = self.list_snapshots(None)?;
        let older = all
            .iter()
            .find(|s| s.id == older_id)
            .ok_or_else(|| format!("Snapshot {older_id} no encontrado"))?;
        let newer = all
            .iter()
            .find(|s| s.id == newer_id)
            .ok_or_else(|| format!("Snapshot {newer_id} no encontrado"))?;

        let mut dir_map: std::collections::HashMap<String, (u64, u64)> =
            std::collections::HashMap::new();
        for d in &older.top_dirs {
            dir_map.entry(d.name.clone()).or_insert((0, 0)).0 = d.allocated_bytes;
        }
        for d in &newer.top_dirs {
            dir_map.entry(d.name.clone()).or_insert((0, 0)).1 = d.allocated_bytes;
        }

        let mut top_dir_deltas: Vec<DirDelta> = dir_map
            .into_iter()
            .map(|(name, (old_b, new_b))| DirDelta {
                name,
                older_bytes: old_b,
                newer_bytes: new_b,
                delta_bytes: (new_b as i128 - old_b as i128) as i64,
            })
            .collect();
        top_dir_deltas.sort_by_key(|a| std::cmp::Reverse(a.delta_bytes.abs()));

        Ok(SnapshotDiff {
            older_id,
            newer_id,
            drive_root: newer.drive_root.clone(),
            allocated_delta_bytes: (newer.scanned_allocated_bytes as i128
                - older.scanned_allocated_bytes as i128) as i64,
            file_count_delta: (newer.file_count as i128 - older.file_count as i128) as i64,
            top_dir_deltas,
        })
    }
}

pub fn append_cleanup_log(log_dir: &Path, entry: &CleanupLogEntry) -> Result<(), String> {
    fs::create_dir_all(log_dir).map_err(|e| e.to_string())?;
    let date = Utc::now().format("%Y-%m-%d");
    let file_path = log_dir.join(format!("cleanup-{date}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_cleanup_logs(log_dir: &Path, limit: usize) -> Vec<CleanupLogEntry> {
    let Ok(rd) = fs::read_dir(log_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    files.reverse();

    let mut entries = Vec::new();
    for file_path in files {
        let Ok(file) = fs::File::open(file_path) else {
            continue;
        };
        let reader = BufReader::new(file);
        let mut day_entries: Vec<CleanupLogEntry> = reader
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<CleanupLogEntry>(&line).ok())
            .collect();
        day_entries.reverse();
        for item in day_entries {
            entries.push(item);
            if entries.len() >= limit {
                return entries;
            }
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::model::FileSystemType;

    #[test]
    fn saves_and_diffs_snapshots_in_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("test.db");
        let storage = Storage::open(db).unwrap();

        let mut tree1 = ScanTree::new(PathBuf::from(r"D:\"), 1, FileSystemType::Ntfs);
        tree1.add_child(
            0,
            "Videos",
            crate::scanner::model::NodeKind::File,
            1_000,
            1_000,
            100,
            crate::scanner::model::NodeFlags::empty(),
        );
        tree1.aggregate_bottom_up();
        let id1 = storage.save_snapshot(&tree1, 10_000, 9_000, &[]).unwrap();

        let mut tree2 = ScanTree::new(PathBuf::from(r"D:\"), 1, FileSystemType::Ntfs);
        tree2.add_child(
            0,
            "Videos",
            crate::scanner::model::NodeKind::File,
            2_500,
            2_500,
            200,
            crate::scanner::model::NodeFlags::empty(),
        );
        tree2.aggregate_bottom_up();
        let id2 = storage.save_snapshot(&tree2, 10_000, 7_500, &[]).unwrap();

        let snaps = storage.list_snapshots(Some(r"D:\")).unwrap();
        assert_eq!(snaps.len(), 2);

        let diff = storage.diff_snapshots(id1, id2).unwrap();
        assert_eq!(diff.allocated_delta_bytes, 1_500);
    }
}
