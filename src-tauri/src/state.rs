use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::categorizer::CategoryBreakdown;
use crate::cleaner::executor::CleanupPlan;
use crate::deletion::queue::DeletionJobManager;
use crate::dev_cleaner::schema::DevCleanupPlan;
use crate::index::default_index_dir;
use crate::index::watcher::VolumeDirWatcher;
use crate::reconcile::queue::{ReconciliationQueue, SyncStatusDto};
use crate::reconcile::triggers::UninstallLeftoverSuggestion;
use crate::scanner::derived::normalize_volume_key;
use crate::scanner::model::ScanTree;
use crate::settings::AppSettings;
use crate::storage::Storage;

pub struct VolumeSlot {
    pub tree: ScanTree,
    pub available: bool,
    pub stale: bool,
}

pub struct AppState {
    /// Multi-volume registry indexed by normalized root (e.g. `"C:\\"`).
    pub volumes: RwLock<HashMap<String, Arc<RwLock<VolumeSlot>>>>,
    /// Currently focused drive root in Explorer (e.g. `"C:\\"`).
    pub active_drive: RwLock<Option<String>>,
    pub active_breakdown: Mutex<Option<CategoryBreakdown>>,
    pub cancel_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub pending_plans: Mutex<HashMap<String, CleanupPlan>>,
    pub pending_dev_plans: Mutex<HashMap<String, DevCleanupPlan>>,
    pub storage: Storage,
    pub startup_drive: Option<String>,
    pub settings: RwLock<AppSettings>,
    pub deletion_jobs: Arc<DeletionJobManager>,
    pub reconcile_queue: Arc<Mutex<ReconciliationQueue>>,
    pub sync_status: Arc<Mutex<SyncStatusDto>>,
    pub uninstall_leftovers: Arc<Mutex<Vec<UninstallLeftoverSuggestion>>>,
    pub pending_system_file_watch: Arc<Mutex<Option<(String, u64)>>>,
    pub watchers: Mutex<HashMap<String, VolumeDirWatcher>>,
}

impl AppState {
    pub fn new(storage: Storage, startup_drive: Option<String>) -> Self {
        let settings = AppSettings::load();
        Self {
            volumes: RwLock::new(HashMap::new()),
            active_drive: RwLock::new(startup_drive.as_deref().map(normalize_volume_key)),
            active_breakdown: Mutex::new(None),
            cancel_flag: Mutex::new(None),
            pending_plans: Mutex::new(HashMap::new()),
            pending_dev_plans: Mutex::new(HashMap::new()),
            storage,
            startup_drive,
            settings: RwLock::new(settings),
            deletion_jobs: Arc::new(DeletionJobManager::new()),
            reconcile_queue: Arc::new(Mutex::new(ReconciliationQueue::new(
                Duration::from_millis(250),
                default_index_dir(),
            ))),
            sync_status: Arc::new(Mutex::new(SyncStatusDto::default())),
            uninstall_leftovers: Arc::new(Mutex::new(Vec::new())),
            pending_system_file_watch: Arc::new(Mutex::new(None)),
            watchers: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert_volume(&self, tree: ScanTree, stale: bool) {
        let key = normalize_volume_key(&tree.root_path.to_string_lossy());
        let breakdown = tree
            .derived
            .to_category_breakdown(tree.stats.total_allocated_bytes);
        if let Ok(mut map) = self.volumes.write() {
            map.insert(
                key.clone(),
                Arc::new(RwLock::new(VolumeSlot {
                    tree,
                    available: true,
                    stale,
                })),
            );
        }
        if let Ok(mut ad) = self.active_drive.write() {
            if ad.is_none() {
                *ad = Some(key);
            }
        }
        if let Ok(mut bd) = self.active_breakdown.lock() {
            *bd = Some(breakdown);
        }
    }

    pub fn get_volume_slot(&self, drive_root: Option<&str>) -> Option<Arc<RwLock<VolumeSlot>>> {
        let map = self.volumes.read().ok()?;
        if let Some(dr) = drive_root {
            let key = normalize_volume_key(dr);
            if let Some(slot) = map.get(&key) {
                return Some(slot.clone());
            }
        }
        if let Ok(ad) = self.active_drive.read() {
            if let Some(ref k) = *ad {
                if let Some(slot) = map.get(k) {
                    return Some(slot.clone());
                }
            }
        }
        map.values().next().cloned()
    }

    pub fn mark_volume_unavailable(&self, drive_root: &str) {
        let key = normalize_volume_key(drive_root);
        if let Ok(map) = self.volumes.read() {
            if let Some(slot) = map.get(&key) {
                if let Ok(mut guard) = slot.write() {
                    guard.available = false;
                }
            }
        }
    }

    pub fn all_available_volume_slots(&self) -> Vec<Arc<RwLock<VolumeSlot>>> {
        let Ok(map) = self.volumes.read() else {
            return Vec::new();
        };
        map.values()
            .filter(|s| s.read().map(|g| g.available).unwrap_or(false))
            .cloned()
            .collect()
    }
}
