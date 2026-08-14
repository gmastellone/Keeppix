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
    /// Tetto di priorità che i worker possono reclamare. `Paused` lascia
    /// passare solo il livello 0 (qualcuno sta aspettando una preview).
    #[must_use]
    pub const fn max_priority(self) -> JobPriority {
        match self {
            Self::Paused => JobPriority::Interactive,
            Self::Interactive => JobPriority::Visible,
            Self::Background | Self::Night => JobPriority::Background,
        }
    }
}

#[must_use]
pub fn worker_count(cpu: usize) -> usize {
    cpu.saturating_sub(1).clamp(1, 8)
}

/// Unix-ts dell'ultima richiesta autenticata. L'API lo toccherà in 1c.
pub struct ActivityTracker {
    last_auth_unix: AtomicI64,
}

impl ActivityTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_auth_unix: AtomicI64::new(0),
        }
    }

    pub fn notify_authenticated_request(&self) {
        self.notify_authenticated_request_at(Utc::now());
    }

    pub fn notify_authenticated_request_at(&self, at: DateTime<Utc>) {
        self.last_auth_unix.store(at.timestamp(), Ordering::Relaxed);
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
