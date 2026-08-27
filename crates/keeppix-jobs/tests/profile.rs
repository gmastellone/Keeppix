#![allow(clippy::unwrap_used)]

use chrono::{NaiveTime, TimeZone, Utc};
use keeppix_domain::JobPriority;
use keeppix_jobs::{
    ActivityTracker, AnalysisLevel, EnergyProfile, RamGate, default_night_window,
    max_claimable_priority, worker_count,
};
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

// The UI promises the user "2:00-7:00"; the code must keep that promise,
// not some other window picked arbitrarily.
#[test]
fn default_night_window_matches_the_promise_made_in_the_ui() {
    assert_eq!(
        default_night_window(),
        (
            NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
        )
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

// The analysis auto-pause is a server behavior driven by the viewport, not
// by the generic authenticated request (that one has a 5-minute window, too
// long for "resume 4 seconds after the user stopped scrolling").

#[test]
fn analysis_pauses_right_after_a_viewport_change() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    tracker.notify_viewport_activity_at(now);
    assert!(!tracker.analysis_should_run(now, 4000));
}

#[test]
fn analysis_resumes_exactly_at_the_idle_threshold() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    tracker.notify_viewport_activity_at(now);
    assert!(!tracker.analysis_should_run(now + chrono::Duration::milliseconds(3999), 4000));
    assert!(tracker.analysis_should_run(now + chrono::Duration::milliseconds(4000), 4000));
}

#[test]
fn analysis_runs_when_no_viewport_activity_was_ever_recorded() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    assert!(tracker.analysis_should_run(now, 4000));
}

#[test]
fn the_idle_threshold_is_a_caller_supplied_parameter_not_a_baked_in_constant() {
    let tracker = ActivityTracker::new();
    let now = Utc.with_ymd_and_hms(2026, 8, 20, 12, 0, 0).unwrap();
    tracker.notify_viewport_activity_at(now);
    let half_a_second_later = now + chrono::Duration::milliseconds(500);
    // A caller with a shorter threshold resumes sooner than the same caller
    // with a longer one, on the exact same tracker state.
    assert!(tracker.analysis_should_run(half_a_second_later, 250));
    assert!(!tracker.analysis_should_run(half_a_second_later, 4000));
}

#[test]
fn reduced_level_is_about_six_times_slower_than_full_using_measured_ms() {
    // Measured on a real CI benchmark: vision OpenCLIP XLM-R IT/EN ≈ 57 ms/photo.
    // Reduced stays ~6x that (per the UI functional design), not a second made-up number.
    assert_eq!(AnalysisLevel::Full.ms_per_photo(), Some(57));
    assert_eq!(AnalysisLevel::Reduced.ms_per_photo(), Some(342));
    assert_eq!(AnalysisLevel::Off.ms_per_photo(), None);
    assert!(AnalysisLevel::Full.is_enabled());
    assert!(AnalysisLevel::Reduced.is_enabled());
    assert!(!AnalysisLevel::Off.is_enabled());
    let ratio = f64::from(u32::try_from(AnalysisLevel::Reduced.ms_per_photo().unwrap()).unwrap())
        / f64::from(u32::try_from(AnalysisLevel::Full.ms_per_photo().unwrap()).unwrap());
    assert!((5.5..6.5).contains(&ratio), "ratio was {ratio}");
}

#[test]
fn analysis_pause_caps_claimable_priority_below_background() {
    // At night / in background, without a pause: even Background jobs
    // (EmbedAssets) are claimed. With a fresh viewport the analysis queue
    // stops, the rest doesn't.
    assert_eq!(
        max_claimable_priority(EnergyProfile::Night, true),
        JobPriority::Background
    );
    assert_eq!(
        max_claimable_priority(EnergyProfile::Night, false),
        JobPriority::Visible
    );
    assert_eq!(
        max_claimable_priority(EnergyProfile::Background, false),
        JobPriority::Visible
    );
    // Interactive was already below Background: the pause changes nothing.
    assert_eq!(
        max_claimable_priority(EnergyProfile::Interactive, false),
        JobPriority::Visible
    );
    assert_eq!(
        max_claimable_priority(EnergyProfile::Paused, false),
        JobPriority::Interactive
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
