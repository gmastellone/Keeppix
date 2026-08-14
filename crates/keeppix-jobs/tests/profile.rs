#![allow(clippy::unwrap_used)]

use chrono::{NaiveTime, TimeZone, Utc};
use keeppix_domain::JobPriority;
use keeppix_jobs::{ActivityTracker, EnergyProfile, RamGate, worker_count};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn worker_count_leaves_one_core_for_http() {
    assert_eq!(worker_count(1), 1);
    assert_eq!(worker_count(2), 1);
    assert_eq!(worker_count(8), 7);
    assert_eq!(worker_count(16), 8);
}

#[test]
fn paused_accepts_only_interactive() {
    assert_eq!(
        EnergyProfile::Paused.max_priority(),
        JobPriority::Interactive
    );
}

#[test]
fn interactive_excludes_background_priority() {
    assert_eq!(
        EnergyProfile::Interactive.max_priority(),
        JobPriority::Visible
    );
    assert!(EnergyProfile::Interactive.max_priority() < JobPriority::Background);
}

#[test]
fn activity_within_five_minutes_is_interactive() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 14, 12, 0, 0).unwrap();
    tracker.notify_authenticated_request_at(now);
    let night = (
        NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
    );
    assert_eq!(
        tracker.current_profile(now + chrono::Duration::minutes(4), night, false),
        EnergyProfile::Interactive
    );
}

#[test]
fn five_minutes_idle_becomes_background() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 14, 12, 0, 0).unwrap();
    tracker.notify_authenticated_request_at(now);
    let night = (
        NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
    );
    assert_eq!(
        tracker.current_profile(now + chrono::Duration::minutes(5), night, false),
        EnergyProfile::Background
    );
}

#[test]
fn night_window_yields_night_unless_interactive() {
    let tracker = ActivityTracker::new();
    let night = (
        NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
        NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
    );
    let three_am = Utc.with_ymd_and_hms(2026, 8, 14, 3, 0, 0).unwrap();
    assert_eq!(
        tracker.current_profile(three_am, night, false),
        EnergyProfile::Night
    );
    tracker.notify_authenticated_request_at(three_am);
    assert_eq!(
        tracker.current_profile(three_am, night, false),
        EnergyProfile::Interactive
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn two_heavy_jobs_do_not_run_in_parallel() {
    let gate = Arc::new(RamGate::new(100 * 1024));
    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let run = |gate: Arc<RamGate>, current: Arc<AtomicUsize>, peak: Arc<AtomicUsize>| async move {
        let _permit = gate.acquire(75 * 1024).await.unwrap();
        let n = current.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(n, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(40)).await;
        current.fetch_sub(1, Ordering::SeqCst);
    };

    tokio::join!(
        run(Arc::clone(&gate), Arc::clone(&current), Arc::clone(&peak)),
        run(gate, current, Arc::clone(&peak)),
    );

    assert_eq!(peak.load(Ordering::SeqCst), 1);
}
