use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN, THREAD_MODE_BACKGROUND_END,
};

use crate::index::is_inside_dir_prefix;
use crate::scanner::derived::normalize_volume_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcilePriority {
    High = 0,
    Medium = 1,
    Low = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileOrigin {
    UserDeletion,
    OfficialCommand,
    NativeUninstall,
    EmptyRecycleBin,
    SystemFileAdjust,
    UsnJournal,
    DirWatcher,
    WindowFocus,
    FreeSpaceDrift,
    DeviceChange,
    ManualBackup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileRequest {
    pub volume: String,
    pub paths: Vec<PathBuf>,
    pub origin: ReconcileOrigin,
    pub priority: ReconcilePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoalescedBatch {
    pub volume: String,
    pub paths: Vec<PathBuf>,
    pub priority: ReconcilePriority,
    pub origins: Vec<ReconcileOrigin>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDto {
    pub syncing: bool,
    pub pending_batches: usize,
    pub last_synced_ts: Option<i64>,
    pub last_origin: Option<String>,
    pub observations: Vec<String>,
    pub revision: u64,
}

/// Coalesces paths so that if both `C:\a` and `C:\a\b` (or `C:\a\b\c`) arrive,
/// only the minimal ancestor set (`C:\a`) is reconciled.
/// Also filters out paths inside `ignored_index_dir` to guarantee zero feedback loops.
pub fn coalesce_paths(paths: &[PathBuf], ignored_index_dir: &Path) -> Vec<PathBuf> {
    let mut normalized: Vec<(String, PathBuf)> = paths
        .iter()
        .filter(|p| !p.as_os_str().is_empty() && !is_inside_dir_prefix(p, ignored_index_dir))
        .map(|p| {
            let norm = p
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase();
            // Restore trailing backslash for drive roots (`c:` -> `c:\`)
            let key = if norm.len() == 2 && norm.ends_with(':') {
                format!(r"{norm}\")
            } else {
                norm
            };
            (key, p.clone())
        })
        .collect();

    // Sort shortest (shallowest) paths first so ancestors always precede descendants
    normalized.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(&b.0)));

    let mut kept_keys: Vec<String> = Vec::new();
    let mut kept_paths: Vec<PathBuf> = Vec::new();

    'outer: for (key, orig) in normalized {
        for anc in &kept_keys {
            if key == *anc {
                continue 'outer;
            }
            if anc.ends_with('\\') {
                if key.starts_with(anc) {
                    continue 'outer;
                }
            } else if key.starts_with(anc) && key.as_bytes().get(anc.len()) == Some(&b'\\') {
                continue 'outer;
            }
        }
        kept_keys.push(key);
        kept_paths.push(orig);
    }

    kept_paths
}

struct PendingVolumeSlot {
    paths: Vec<PathBuf>,
    priority: ReconcilePriority,
    origins: Vec<ReconcileOrigin>,
    last_enqueued: Instant,
}

/// Per-volume coalescing & debounced reconciliation queue with strict priority ordering.
pub struct ReconciliationQueue {
    debounce: Duration,
    ignored_index_dir: PathBuf,
    slots: HashMap<String, PendingVolumeSlot>,
    retry_after: HashMap<String, (Instant, u32)>,
}

impl ReconciliationQueue {
    pub fn new(debounce: Duration, ignored_index_dir: PathBuf) -> Self {
        Self {
            debounce,
            ignored_index_dir,
            slots: HashMap::new(),
            retry_after: HashMap::new(),
        }
    }

    pub fn enqueue(&mut self, req: ReconcileRequest, now: Instant) -> bool {
        let filtered = coalesce_paths(&req.paths, &self.ignored_index_dir);
        if filtered.is_empty() {
            return false;
        }
        let vol = normalize_volume_key(&req.volume);
        let entry = self.slots.entry(vol).or_insert_with(|| PendingVolumeSlot {
            paths: Vec::new(),
            priority: req.priority,
            origins: Vec::new(),
            last_enqueued: now,
        });
        entry.paths.extend(filtered);
        entry.paths = coalesce_paths(&entry.paths, &self.ignored_index_dir);
        if req.priority < entry.priority {
            entry.priority = req.priority;
        }
        if !entry.origins.contains(&req.origin) {
            entry.origins.push(req.origin);
        }
        entry.last_enqueued = now;
        true
    }

    pub fn pending_count(&self) -> usize {
        self.slots.len()
    }

    /// Pops the highest-priority volume batch whose debounce window (and any retry backoff) has elapsed.
    pub fn pop_ready(&mut self, now: Instant) -> Option<CoalescedBatch> {
        let mut ready_candidates: BTreeMap<(ReconcilePriority, String), ()> = BTreeMap::new();
        for (vol, slot) in &self.slots {
            if let Some(&(retry_ts, _)) = self.retry_after.get(vol) {
                if now < retry_ts {
                    continue;
                }
            }
            if now.duration_since(slot.last_enqueued) >= self.debounce {
                ready_candidates.insert((slot.priority, vol.clone()), ());
            }
        }

        let (_, chosen_vol) = ready_candidates.keys().next()?.clone();
        let slot = self.slots.remove(&chosen_vol)?;
        Some(CoalescedBatch {
            volume: chosen_vol,
            paths: slot.paths,
            priority: slot.priority,
            origins: slot.origins,
        })
    }

    /// Forces immediate readiness for all pending batches (e.g. on window focus).
    pub fn flush_all_ready(&mut self) -> Vec<CoalescedBatch> {
        let mut entries: Vec<(ReconcilePriority, String)> = self
            .slots
            .iter()
            .map(|(v, s)| (s.priority, v.clone()))
            .collect();
        entries.sort();
        let mut out = Vec::with_capacity(entries.len());
        for (_, vol) in entries {
            if let Some(slot) = self.slots.remove(&vol) {
                out.push(CoalescedBatch {
                    volume: vol,
                    paths: slot.paths,
                    priority: slot.priority,
                    origins: slot.origins,
                });
            }
        }
        out
    }

    /// Schedules a failed batch for exponential backoff retry (1s, 2s, 4s, ..., max 60s).
    pub fn schedule_retry(&mut self, batch: CoalescedBatch, now: Instant) {
        let attempts = self
            .retry_after
            .get(&batch.volume)
            .map(|(_, a)| a + 1)
            .unwrap_or(1);
        let backoff_secs = (1u64 << attempts.min(6)).min(60);
        self.retry_after.insert(
            batch.volume.clone(),
            (now + Duration::from_secs(backoff_secs), attempts),
        );
        self.slots.insert(
            batch.volume,
            PendingVolumeSlot {
                paths: batch.paths,
                priority: batch.priority,
                origins: batch.origins,
                last_enqueued: now,
            },
        );
    }

    pub fn clear_retry(&mut self, volume: &str) {
        self.retry_after.remove(&normalize_volume_key(volume));
    }
}

/// RAII guard that lowers the current thread's I/O & CPU scheduling priority to background mode
/// (`THREAD_MODE_BACKGROUND_BEGIN`) for Medium/Low reconciliation work so foreground apps aren't impacted.
pub struct BackgroundIoGuard {
    active: bool,
}

impl BackgroundIoGuard {
    pub fn enter_if_low_priority(priority: ReconcilePriority) -> Self {
        if matches!(priority, ReconcilePriority::Medium | ReconcilePriority::Low) {
            let ok = unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN).is_ok()
            };
            Self { active: ok }
        } else {
            Self { active: false }
        }
    }
}

impl Drop for BackgroundIoGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_END);
            }
        }
    }
}
