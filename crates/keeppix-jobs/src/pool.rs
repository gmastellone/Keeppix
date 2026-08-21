use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::{NaiveTime, Utc};
use keeppix_db::{Db, JobRepo};
use tokio::sync::{Semaphore, SemaphorePermit};
use uuid::Uuid;

use crate::JobError;
use crate::dispatch::JobHandler;
use crate::profile::{ActivityTracker, DEFAULT_ANALYSIS_IDLE_MS, max_claimable_priority};

/// Semaforo pesato in KiB. Un job che stima più della capienza prende tutta
/// la capienza e aspetta da solo — non fa cadere il processo.
pub struct RamGate {
    sem: Semaphore,
    cap_kib: u32,
}

impl RamGate {
    #[must_use]
    pub fn new(cap_bytes: u64) -> Self {
        let cap_kib = u32::try_from((cap_bytes / 1024).clamp(1, u64::from(u32::MAX))).unwrap_or(1);
        Self {
            sem: Semaphore::new(cap_kib as usize),
            cap_kib,
        }
    }

    /// # Errors
    /// `JobError::Worker` se il semaforo è stato chiuso (non succede in 1b).
    pub async fn acquire(&self, hint_bytes: u64) -> Result<SemaphorePermit<'_>, JobError> {
        let kib = (hint_bytes / 1024).max(1);
        let kib = u32::try_from(kib).unwrap_or(u32::MAX).min(self.cap_kib);
        self.sem
            .acquire_many(kib)
            .await
            .map_err(|_| JobError::Worker("ram gate closed".to_owned()))
    }
}

pub struct WorkerPool<H> {
    db: Db,
    handler: H,
    tracker: Arc<ActivityTracker>,
    ram: RamGate,
    worker_id: Uuid,
    night: (NaiveTime, NaiveTime),
    paused: Arc<AtomicBool>,
}

impl<H: JobHandler> WorkerPool<H> {
    #[must_use]
    pub fn new(
        db: Db,
        handler: H,
        tracker: Arc<ActivityTracker>,
        ram_bytes: u64,
        night: (NaiveTime, NaiveTime),
        paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            db,
            handler,
            tracker,
            ram: RamGate::new(ram_bytes),
            worker_id: Uuid::now_v7(),
            night,
            paused,
        }
    }

    /// Un giro: claim, ram, handle, complete/fail. `false` se la coda è vuota.
    ///
    /// # Errors
    /// Errori di database o del gate; gli errori dell'handler diventano `fail`.
    pub async fn step(&self) -> Result<bool, JobError> {
        let now = Utc::now();
        let paused = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        let profile = self.tracker.current_profile(now, self.night, paused);
        let analysis_ok = self
            .tracker
            .analysis_should_run(now, DEFAULT_ANALYSIS_IDLE_MS);
        let max_priority = max_claimable_priority(profile, analysis_ok);
        let Some(job) = JobRepo::new(&self.db)
            .claim(self.worker_id, max_priority)
            .await?
        else {
            return Ok(false);
        };
        let _ram = self.ram.acquire(self.handler.ram_hint_bytes(&job)).await?;
        match self.handler.handle(&job).await {
            Ok(()) => JobRepo::new(&self.db).complete(job.id).await?,
            Err(e) => JobRepo::new(&self.db).fail(job.id, &e.to_string()).await?,
        }
        Ok(true)
    }
}
