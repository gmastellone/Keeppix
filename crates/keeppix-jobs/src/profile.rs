use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, NaiveTime, Utc};
use keeppix_domain::JobPriority;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyProfile {
    Interactive,
    Background,
    Night,
    Paused,
}

impl EnergyProfile {
    /// Priority ceiling the workers may claim. `Paused` only lets level 0
    /// through (someone is waiting on a preview).
    #[must_use]
    pub const fn max_priority(self) -> JobPriority {
        match self {
            Self::Paused => JobPriority::Interactive,
            Self::Interactive => JobPriority::Visible,
            Self::Background | Self::Night => JobPriority::Background,
        }
    }
}

/// Full-speed night window, aligned to **2:00-7:00** to keep the promise
/// already made to the user by the UI functional design, which declared
/// that window while this function used to return a different one
/// (2:00-6:00).
#[must_use]
pub fn default_night_window() -> (NaiveTime, NaiveTime) {
    let start = NaiveTime::from_hms_opt(2, 0, 0).unwrap_or(NaiveTime::MIN);
    let end = NaiveTime::from_hms_opt(7, 0, 0).unwrap_or(NaiveTime::MIN);
    (start, end)
}

/// Default threshold for the analysis auto-pause: 4 seconds since the last
/// view change. A starting value from a prototype with no real load behind
/// it, not a measurement — which is why every caller passes it as a
/// parameter instead of it being hardcoded inside
/// `ActivityTracker::analysis_should_run`.
pub const DEFAULT_ANALYSIS_IDLE_MS: u64 = 4000;

/// The three AI-analysis speed levels. The milliseconds per photo are
/// **measured** on `OpenCLIP` XLM-R IT/EN via ort, from a real CI
/// benchmark, not provisional targets. `Off` turns analysis off (pgvector
/// missing, insufficient RAM, or an operator choice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisLevel {
    Full,
    Reduced,
    Off,
}

impl AnalysisLevel {
    /// Measured time per photo, in milliseconds. `None` if analysis is off.
    ///
    /// `Full` ≈ 57 ms (vision `OpenCLIP` XLM-R IT/EN). `Reduced` ≈ 6×
    /// (`Full`), as specified by the UI functional design.
    #[must_use]
    pub const fn ms_per_photo(self) -> Option<u64> {
        match self {
            Self::Full => Some(57),
            Self::Reduced => Some(342),
            Self::Off => None,
        }
    }

    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Maximum priority claimable by workers, accounting for the analysis
/// auto-pause (fresh viewport activity → no `Background`, so no
/// `EmbedAssets` backfill).
#[must_use]
pub fn max_claimable_priority(profile: EnergyProfile, analysis_should_run: bool) -> JobPriority {
    let max = profile.max_priority();
    if analysis_should_run || max < JobPriority::Background {
        return max;
    }
    JobPriority::Visible
}

#[must_use]
pub fn worker_count(cpu: usize) -> usize {
    cpu.saturating_sub(1).clamp(1, 8)
}

/// Out of `total` workers, how many stay `always_background`
/// (`WorkerPool::with_always_background`) — willing to claim `Background`
/// jobs no matter how `Interactive` the session looks, so a large bulk
/// import still makes *some* progress while someone is actively using the
/// app instead of stopping dead. Roughly a third, floored at 1 — but not
/// on a single-worker machine: there, reserving the only worker would
/// remove the responsiveness protection `EnergyProfile::Interactive`
/// exists for in the first place, exactly where it matters most (the
/// smallest hardware, e.g. a Pi Zero-class box).
#[must_use]
pub fn background_reserved_workers(total: usize) -> usize {
    if total <= 1 { 0 } else { (total / 3).max(1) }
}

/// Unix timestamp of the last authenticated request. Touched by the API on
/// authenticated requests.
///
/// `last_viewport_unix` is a second, independent signal: it only drives the
/// analysis auto-pause, with a threshold measured in seconds rather than
/// minutes, so it needs its own millisecond resolution — `last_auth_unix`
/// truncates to seconds, which would be too coarse for a 4000 ms threshold.
pub struct ActivityTracker {
    last_auth_unix: AtomicI64,
    last_viewport_unix_ms: AtomicI64,
}

impl ActivityTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_auth_unix: AtomicI64::new(0),
            last_viewport_unix_ms: AtomicI64::new(0),
        }
    }

    pub fn notify_authenticated_request(&self) {
        self.notify_authenticated_request_at(Utc::now());
    }

    pub fn notify_authenticated_request_at(&self, at: DateTime<Utc>) {
        self.last_auth_unix.store(at.timestamp(), Ordering::Relaxed);
    }

    /// The server records a view change (`POST /viewport`) regardless of
    /// whether any job got promoted — it's the "the user is browsing"
    /// signal, not "there was something to promote".
    pub fn notify_viewport_activity(&self) {
        self.notify_viewport_activity_at(Utc::now());
    }

    pub fn notify_viewport_activity_at(&self, at: DateTime<Utc>) {
        self.last_viewport_unix_ms
            .store(at.timestamp_millis(), Ordering::Relaxed);
    }

    /// Whether analysis can run right now: false if the last view change is
    /// more recent than `idle_threshold_ms`, true otherwise — including the
    /// "no view change ever observed" case, where there is nothing to be
    /// paused from.
    #[must_use]
    pub fn analysis_should_run(&self, now: DateTime<Utc>, idle_threshold_ms: u64) -> bool {
        let last = self.last_viewport_unix_ms.load(Ordering::Relaxed);
        if last == 0 {
            return true;
        }
        let idle_ms = now.timestamp_millis().saturating_sub(last);
        idle_ms >= i64::try_from(idle_threshold_ms).unwrap_or(i64::MAX)
    }

    #[must_use]
    pub fn current_profile(
        &self,
        now: DateTime<Utc>,
        night: (NaiveTime, NaiveTime),
        paused: bool,
    ) -> EnergyProfile {
        if paused {
            return EnergyProfile::Paused;
        }
        let last = self.last_auth_unix.load(Ordering::Relaxed);
        if last > 0 {
            let idle = now.timestamp().saturating_sub(last);
            if idle < 5 * 60 {
                return EnergyProfile::Interactive;
            }
        }
        if in_night_window(now.time(), night) {
            return EnergyProfile::Night;
        }
        EnergyProfile::Background
    }
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn in_night_window(now: NaiveTime, (start, end): (NaiveTime, NaiveTime)) -> bool {
    if start <= end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}
