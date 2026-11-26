use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::categorizer::CategoryBreakdown;
use crate::cleaner::executor::CleanupPlan;
use crate::scanner::model::ScanTree;
use crate::storage::Storage;

pub struct AppState {
    pub active_tree: Mutex<Option<ScanTree>>,
    pub active_breakdown: Mutex<Option<CategoryBreakdown>>,
    pub cancel_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub pending_plans: Mutex<HashMap<String, CleanupPlan>>,
    pub storage: Storage,
    pub startup_drive: Option<String>,
}

impl AppState {
    pub fn new(storage: Storage, startup_drive: Option<String>) -> Self {
        Self {
            active_tree: Mutex::new(None),
            active_breakdown: Mutex::new(None),
            cancel_flag: Mutex::new(None),
            pending_plans: Mutex::new(HashMap::new()),
            storage,
            startup_drive,
        }
    }
}
