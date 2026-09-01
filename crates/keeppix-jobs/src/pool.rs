use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::{NaiveTime, Utc};
use keeppix_db::{Db, JobRepo};
use tokio::sync::{Semaphore, SemaphorePermit};
use uuid::Uuid;

use crate::JobError;
use crate::dispatch::JobHandler;
use crate::profile::{ActivityTracker, DEFAULT_ANALYSIS_IDLE_MS, max_claimable_priority};

/// Semaphore weighted in KiB. A job that estimates more than the total
/// capacity takes all of it and waits alone — it does not crash the process.
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
    /// `JobError::Worker` if the semaphore has been closed (does not happen
    /// in current usage).
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
    always_background: bool,
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
            always_background: false,
        }
    }

    /// This worker ignores `EnergyProfile` entirely and claims up to
    /// `Background` regardless of recent activity (still subject to
    /// `Paused`). Reserving a handful of workers this way, out of the
    /// full pool, is how bulk background processing degrades to *slower*
    /// instead of *stopped* while someone is actively using the app —
    /// see `background_reserved_workers`.
    #[must_use]
    pub fn with_always_background(mut self, always_background: bool) -> Self {
        self.always_background = always_background;
        self
    }

    /// One round: claim, ram, handle, complete/fail. `false` if the queue
    /// is empty.
    ///
    /// # Errors
    /// Database or gate errors; handler errors turn into a `fail`.
    pub async fn step(&self) -> Result<bool, JobError> {
        let now = Utc::now();
        let paused = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        let max_priority = if self.always_background && !paused {
            keeppix_domain::JobPriority::Background
        } else {
            let profile = self.tracker.current_profile(now, self.night, paused);
            let analysis_ok = self
                .tracker
                .analysis_should_run(now, DEFAULT_ANALYSIS_IDLE_MS);
            max_claimable_priority(profile, analysis_ok)
        };
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
