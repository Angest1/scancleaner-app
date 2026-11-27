use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::reconcile::triggers::FreedSpaceMeasurement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionJobPhase {
    Queued,
    Staging,
    DeletingBackground,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionJobStatus {
    pub job_id: String,
    pub mode: String,
    pub phase: DeletionJobPhase,
    pub total_items: usize,
    pub completed_items: usize,
    pub failed_items: usize,
    pub estimated_bytes: u64,
    pub real_freed_bytes: Option<u64>,
    pub discrepancy_explanation: Option<String>,
    pub errors: Vec<String>,
    pub measurements: Vec<FreedSpaceMeasurement>,
}

struct ActiveJobEntry {
    status: DeletionJobStatus,
    cancel_flag: Arc<AtomicBool>,
}

/// Manages background deletion jobs with progress tracking, cancellation, and optimistic rollback handles.
#[derive(Default)]
pub struct DeletionJobManager {
    jobs: Mutex<HashMap<String, ActiveJobEntry>>,
}

impl DeletionJobManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_job(
        &self,
        job_id: String,
        mode: String,
        total_items: usize,
        estimated_bytes: u64,
    ) -> Arc<AtomicBool> {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let status = DeletionJobStatus {
            job_id: job_id.clone(),
            mode,
            phase: DeletionJobPhase::Queued,
            total_items,
            completed_items: 0,
            failed_items: 0,
            estimated_bytes,
            real_freed_bytes: None,
            discrepancy_explanation: None,
            errors: Vec::new(),
            measurements: Vec::new(),
        };
        if let Ok(mut guard) = self.jobs.lock() {
            guard.insert(
                job_id,
                ActiveJobEntry {
                    status,
                    cancel_flag: cancel_flag.clone(),
                },
            );
        }
        cancel_flag
    }

    pub fn update_job<F>(&self, job_id: &str, f: F)
    where
        F: FnOnce(&mut DeletionJobStatus),
    {
        if let Ok(mut guard) = self.jobs.lock() {
            if let Some(entry) = guard.get_mut(job_id) {
                f(&mut entry.status);
            }
        }
    }

    pub fn cancel_job(&self, job_id: &str) -> bool {
        if let Ok(mut guard) = self.jobs.lock() {
            if let Some(entry) = guard.get_mut(job_id) {
                entry.cancel_flag.store(true, Ordering::SeqCst);
                entry.status.phase = DeletionJobPhase::Cancelled;
                return true;
            }
        }
        false
    }

    pub fn list_jobs(&self) -> Vec<DeletionJobStatus> {
        self.jobs
            .lock()
            .map(|g| g.values().map(|e| e.status.clone()).collect())
            .unwrap_or_default()
    }

    pub fn get_job(&self, job_id: &str) -> Option<DeletionJobStatus> {
        self.jobs
            .lock()
            .ok()
            .and_then(|g| g.get(job_id).map(|e| e.status.clone()))
    }
}
