use super::*;

fn callback(clock: &PlaybackClock, at: Instant, frame: u64, count: usize) -> bool {
    clock.begin_callback(at, frame, count, 100, Duration::from_millis(100), 1_000)
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn initial_playback_keeps_negative_output_lead_and_scheduled_time() {
    let clock = PlaybackClock::new(10.0);
    let now = Instant::now();
    assert!(callback(&clock, now + Duration::from_millis(100), 0, 10));
    near(clock.song_time_at(now), -0.1);
    near(clock.scheduled_time_seconds(), 0.1);
    assert_eq!(clock.output_latency_seconds(), Some(0.1));
    assert_eq!(clock.first_callback_frames(), Some(10));
    assert!(clock.is_started());
    let state = clock.inner.lock().unwrap();
    assert!(!running_at(&state, now));
    assert!(running_at(&state, now + Duration::from_millis(100)));
}

#[test]
fn pause_drains_to_media_boundary_then_freezes_through_long_silence() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    assert!(callback(&clock, start, 0, 10));
    clock.set_paused(true);
    assert!(clock.is_paused());
    assert!(!callback(
        &clock,
        start + Duration::from_millis(100),
        10,
        10
    ));
    near(clock.song_time_at(start + Duration::from_millis(50)), 0.05);
    near(clock.song_time_at(start + Duration::from_secs(600)), 0.1);
    assert!(!callback(&clock, start + Duration::from_secs(600), 10, 10));
    near(clock.song_time_at(start + Duration::from_secs(700)), 0.1);
    near(clock.scheduled_time_seconds(), 0.1);
}

#[test]
fn resume_waits_for_audible_boundary_and_reanchors_same_frame() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    callback(&clock, start, 0, 10);
    clock.set_paused(true);
    callback(&clock, start + Duration::from_millis(100), 10, 10);
    clock.set_paused(false);
    near(clock.song_time_at(start + Duration::from_secs(10)), 0.1);
    assert!(callback(&clock, start + Duration::from_secs(20), 10, 10));
    near(
        clock.song_time_at(start + Duration::from_millis(19_950)),
        0.1,
    );
    near(
        clock.song_time_at(start + Duration::from_millis(20_050)),
        0.15,
    );
    let state = clock.inner.lock().unwrap();
    assert!(pause_applied_at(
        &state,
        start + Duration::from_millis(19_950)
    ));
    assert!(!running_at(&state, start + Duration::from_millis(19_950)));
    assert!(!pause_applied_at(
        &state,
        start + Duration::from_millis(20_050)
    ));
    assert!(running_at(&state, start + Duration::from_millis(20_050)));
}

#[test]
fn pause_before_start_stays_zero_until_resumed_callback_is_audible() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    clock.set_paused(true);
    assert!(!callback(&clock, start, 0, 10));
    assert!(!clock.is_started());
    clock.set_paused(false);
    assert!(callback(&clock, start + Duration::from_secs(10), 0, 10));
    near(clock.song_time_at(start + Duration::from_secs(9)), 0.0);
    near(
        clock.song_time_at(start + Duration::from_millis(10_050)),
        0.05,
    );
}

#[test]
fn cancelled_pause_before_callback_does_not_affect_initial_playback() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    clock.set_paused(true);
    clock.set_paused(true);
    clock.set_paused(false);
    assert!(callback(&clock, start, 0, 10));
    assert!(!clock.is_pause_applied());
    near(clock.song_time_at(start + Duration::from_millis(50)), 0.05);
}

#[test]
fn queued_rapid_transitions_keep_each_audible_silence_interval() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    callback(&clock, start, 0, 10);
    clock.set_paused(true);
    callback(&clock, start + Duration::from_millis(100), 10, 10);
    clock.set_paused(false);
    callback(&clock, start + Duration::from_millis(200), 10, 10);
    clock.set_paused(true);
    callback(&clock, start + Duration::from_millis(300), 20, 10);
    clock.set_paused(false);
    callback(&clock, start + Duration::from_millis(400), 20, 10);
    callback(&clock, start + Duration::from_millis(500), 30, 10);
    {
        let state = clock.inner.lock().unwrap();
        assert!(pause_applied_at(&state, start + Duration::from_millis(50)));
        assert!(!running_at(&state, start + Duration::from_millis(50)));
        assert!(!pause_applied_at(
            &state,
            start + Duration::from_millis(450)
        ));
        assert!(running_at(&state, start + Duration::from_millis(450)));
    }
    for (milliseconds, expected) in [
        (50, 0.05),
        (150, 0.1),
        (250, 0.15),
        (350, 0.2),
        (450, 0.25),
        (550, 0.35),
    ] {
        near(
            clock.song_time_at(start + Duration::from_millis(milliseconds)),
            expected,
        );
    }
}

#[test]
fn nonzero_lead_drains_prior_media_and_holds_until_resumed_audio() {
    let clock = PlaybackClock::new(10.0);
    let callback_wall = Instant::now();
    callback(&clock, callback_wall + Duration::from_millis(100), 0, 10);
    clock.set_paused(true);
    callback(&clock, callback_wall + Duration::from_millis(200), 10, 10);
    near(
        clock.song_time_at(callback_wall + Duration::from_millis(150)),
        0.05,
    );
    near(
        clock.song_time_at(callback_wall + Duration::from_millis(250)),
        0.1,
    );
    clock.set_paused(false);
    near(
        clock.song_time_at(callback_wall + Duration::from_secs(30)),
        0.1,
    );
    callback(
        &clock,
        callback_wall + Duration::from_millis(30_100),
        10,
        10,
    );
    near(
        clock.song_time_at(callback_wall + Duration::from_millis(30_050)),
        0.1,
    );
    near(
        clock.song_time_at(callback_wall + Duration::from_millis(30_150)),
        0.15,
    );
}

#[test]
fn exhausted_song_clock_can_pause_and_resume_its_tail() {
    let clock = PlaybackClock::new(0.1);
    let start = Instant::now();
    assert!(clock.begin_callback(start, 0, 10, 100, Duration::ZERO, 10));
    clock.set_paused(true);
    assert!(!clock.begin_callback(
        start + Duration::from_millis(100),
        10,
        10,
        100,
        Duration::ZERO,
        10
    ));
    near(clock.song_time_at(start + Duration::from_secs(100)), 0.1);
    clock.set_paused(false);
    assert!(clock.begin_callback(
        start + Duration::from_secs(100),
        10,
        10,
        100,
        Duration::ZERO,
        10
    ));
    near(
        clock.song_time_at(start + Duration::from_millis(100_050)),
        0.15,
    );
}

#[test]
fn callback_transition_storage_is_reserved_by_controls() {
    let clock = PlaybackClock::new(10.0);
    let start = Instant::now();
    for index in 0..100 {
        clock.set_paused(index % 2 == 0);
        let capacity = clock.inner.lock().unwrap().segments.capacity();
        callback(
            &clock,
            start + Duration::from_millis(index * 100),
            index,
            10,
        );
        assert_eq!(clock.inner.lock().unwrap().segments.capacity(), capacity);
    }
}

#[test]
fn delayed_input_from_silence_is_rejected_after_resume_becomes_audible() {
    let clock = PlaybackClock::new(10.0);
    let now = Instant::now();
    let start = now - Duration::from_secs(3);
    callback(&clock, start, 0, 10);
    clock.set_paused(true);
    callback(&clock, start + Duration::from_millis(100), 10, 10);
    clock.set_paused(false);
    let resume_at = now - Duration::from_secs(1);
    callback(&clock, resume_at, 10, 10);
    callback(&clock, resume_at + Duration::from_millis(500), 60, 10);

    assert!(clock.is_running());
    assert!(!clock.is_running_at(resume_at - Duration::from_millis(50)));
    assert!(!clock.is_running_at(start + Duration::from_millis(50)));
    assert!(clock.is_running_at(resume_at));
    assert!(clock.is_running_at(resume_at + Duration::from_millis(50)));
    clock.set_paused(true);
    assert!(!clock.is_running_at(now));
    callback(&clock, now, 110, 10);
    clock.set_paused(false);
    assert!(!clock.is_running_at(now + Duration::from_secs(1)));
}
