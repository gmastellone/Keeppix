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

/// Finestra notturna a piena velocità. Fase 10 Task 21: allineata a
/// **2:00-7:00** per tenere la promessa fatta all'utente dal documento
/// funzionale UI (§57), che dichiarava già quella finestra mentre questa
/// funzione ne restituiva un'altra (2:00-6:00).
#[must_use]
pub fn default_night_window() -> (NaiveTime, NaiveTime) {
    let start = NaiveTime::from_hms_opt(2, 0, 0).unwrap_or(NaiveTime::MIN);
    let end = NaiveTime::from_hms_opt(7, 0, 0).unwrap_or(NaiveTime::MIN);
    (start, end)
}

/// Soglia di default della pausa automatica dell'analisi (Fase 10 Task 20):
/// 4 secondi dall'ultimo cambio di vista. Un valore di partenza da un
/// prototipo senza carico reale, non una misura — per questo ogni chiamante
/// la passa come parametro invece di trovarla cablata dentro
/// `ActivityTracker::analysis_should_run`.
pub const DEFAULT_ANALYSIS_IDLE_MS: u64 = 4000;

/// I due livelli di velocità dell'analisi IA (Fase 7). Non esiste ancora un
/// job reale che li consumi: i tempi per foto sono gli obiettivi dichiarati
/// dal documento funzionale, che Fase 7 misurerà sul proprio hardware e
/// modello. Vive qui (non in Fase 7) perché la soglia di pausa e i livelli
/// sono la stessa decisione di prodotto — «quanto costa, e quanto in fretta
/// ci si ferma quando qualcuno naviga».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisLevel {
    Full,
    Reduced,
}

impl AnalysisLevel {
    /// Tempo target per foto, in millisecondi. `Reduced` è ~6× più lento di
    /// `Full` — la stessa coda residua viene dichiarata all'utente come
    /// proporzionalmente più lunga (documento funzionale UI).
    #[must_use]
    pub const fn ms_per_photo(self) -> u64 {
        match self {
            Self::Full => 42,
            Self::Reduced => 260,
        }
    }
}

#[must_use]
pub fn worker_count(cpu: usize) -> usize {
    cpu.saturating_sub(1).clamp(1, 8)
}

/// Unix-ts dell'ultima richiesta autenticata. L'API lo toccherà in 1c.
///
/// `last_viewport_unix` (Fase 10 Task 20) è un secondo segnale, indipendente
/// dal primo: guida solo la pausa automatica dell'analisi, con una soglia di
/// secondi non di minuti, quindi ha bisogno della propria risoluzione in
/// millisecondi — `last_auth_unix` tronca ai secondi, il che sarebbe troppo
/// grossolano per una soglia di 4000 ms.
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

    /// Il server registra un cambio di vista (`POST /viewport`), qualunque
    /// sia l'esito della promozione dei job — è il segnale "l'utente sta
    /// navigando", non "c'era qualcosa da promuovere".
    pub fn notify_viewport_activity(&self) {
        self.notify_viewport_activity_at(Utc::now());
    }

    pub fn notify_viewport_activity_at(&self, at: DateTime<Utc>) {
        self.last_viewport_unix_ms
            .store(at.timestamp_millis(), Ordering::Relaxed);
    }

    /// Se l'analisi (Fase 7, non ancora esistente) può girare ora: falsa se
    /// l'ultimo cambio di vista è più recente di `idle_threshold_ms`, vera
    /// altrimenti — incluso il caso "nessun cambio di vista mai osservato",
    /// dove non c'è nulla da cui essere in pausa.
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
