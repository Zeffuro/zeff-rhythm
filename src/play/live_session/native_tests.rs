use super::*;
use crate::app::settings::AppSettings;
use crate::platform::audio::{AudioStreamOptions, output_stream_target};
use crate::platform::input::{NativeInputSource, NativeInputTimestampKind};
use rhythm_core::{Note, NoteId};
use std::thread;
use std::time::Duration;

#[test]
#[ignore = "requires an available native output device"]
fn native_release_cancels_pending_audio_resume_immediately() {
    let target = output_stream_target(&AudioStreamOptions::default()).unwrap();
    let rate = target.config.sample_rate;
    let mut chart = Chart::new(4);
    chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 0.0, 5.0));
    let clip = Arc::new(AudioClip {
        samples: vec![0.0; rate as usize * 2],
        sample_rate: rate,
        channels: 2,
    });
    let mut settings = AppSettings::default();
    settings.diagnostics.event_log_enabled = false;
    let mut options = PlaySessionOptions::from_app_calibration(&settings).unwrap();
    options.chart_start_seconds = Some(0.0);
    options.start_delay_seconds = Some(0.0);
    let mut session = LivePlaySession::start_with_assets(
        options,
        LiveSessionAssets {
            chart,
            clip,
            target,
            audio_label: "silent-pending-resume-test".into(),
        },
    )
    .unwrap();
    session
        .engine
        .submit_input(CoreInputEvent {
            key: GameKey::Lane(LaneIndex::new(0)),
            pressed: true,
            time_seconds: 0.0,
        })
        .unwrap();
    let clock = crate::platform::audio::PlaybackClock::new(1.0);
    session.chart_clock = ChartClock::new(clock.clone(), 0.0, 0.0);
    session
        .chart_clock
        .start_audio_if_due(&session.stream)
        .unwrap();
    let now = Instant::now();
    clock.begin_callback(now, 0, 100, 1000, Duration::ZERO, 1000);
    session.pause().unwrap();
    clock.begin_callback(
        now + Duration::from_millis(100),
        100,
        100,
        1000,
        Duration::ZERO,
        1000,
    );
    session.pause.set_lane(0, true);
    session.pause.awaiting_audio();
    session.chart_clock.resume(now);
    clock.begin_callback(
        now + Duration::from_secs(10),
        100,
        100,
        1000,
        Duration::from_secs(10),
        1000,
    );
    session
        .process_input(input(NativeInputEventKind::LaneRelease(0)))
        .unwrap();
    assert!(
        clock.is_paused(),
        "release must cancel resume before another render"
    );
    assert!(session.is_paused());
    assert!(!session.pause.mode_is_awaiting_audio());
    assert_eq!(session.engine.judged_count(), 0);
    assert_eq!(session.report.summary().misses, 0);
}

fn input(kind: NativeInputEventKind) -> NativeInputEvent {
    let now = Instant::now();
    NativeInputEvent {
        kind,
        source: NativeInputSource::Winit,
        timestamp_kind: NativeInputTimestampKind::ReceiptMonotonic,
        event_time: now,
        received_time: now,
        source_timestamp_ns: None,
        queue_age_ms: None,
    }
}

fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration) {
    let end = Instant::now() + timeout;
    while !condition() {
        assert!(Instant::now() < end, "native playback condition timed out");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires an available native output device"]
fn native_volume_changes_preserve_running_and_paused_clock() {
    let target = output_stream_target(&AudioStreamOptions::default()).unwrap();
    let rate = target.config.sample_rate;
    let clip = Arc::new(AudioClip {
        samples: vec![0.0; rate as usize * 2 * 4],
        sample_rate: rate,
        channels: 2,
    });
    let mut settings = AppSettings::default();
    settings.diagnostics.event_log_enabled = false;
    settings.audio.muted = true;
    let mut options = PlaySessionOptions::from_app_calibration(&settings).unwrap();
    options.chart_start_seconds = Some(0.0);
    options.start_delay_seconds = Some(0.0);
    let mut session = LivePlaySession::start_with_assets(
        options,
        LiveSessionAssets {
            chart: Chart::new(4),
            clip,
            target,
            audio_label: "silent-native-volume-test".into(),
        },
    )
    .unwrap();
    for gain in [0.0, 0.25, 1.0, 0.0] {
        session.set_volume(gain);
        assert_eq!(session.volume.gain(), gain);
        assert_eq!(session.options.volume, gain);
        let before = session.snapshot().song_time_seconds;
        wait_until(
            || {
                session.update().unwrap();
                session.snapshot().song_time_seconds > before + 0.05
            },
            Duration::from_secs(3),
        );
    }
    session.pause().unwrap();
    thread::sleep(Duration::from_millis(250));
    let frozen = session.snapshot().song_time_seconds;
    session.set_volume(0.5);
    thread::sleep(Duration::from_millis(100));
    session.update().unwrap();
    assert!((session.snapshot().song_time_seconds - frozen).abs() < 0.000001);
    assert!(session.is_paused());
}

#[test]
#[ignore = "requires an available native output device"]
fn native_pause_preserves_clock_holds_and_release_order() {
    for redraw_before_release in [false, true] {
        let target = output_stream_target(&AudioStreamOptions::default()).unwrap();
        let rate = target.config.sample_rate;
        let mut chart = Chart::new(4);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 0.25, 5.0));
        let clip = Arc::new(AudioClip {
            samples: vec![0.0; rate as usize * 2 * 8],
            sample_rate: rate,
            channels: 2,
        });
        let mut settings = AppSettings::default();
        settings.diagnostics.event_log_enabled = false;
        let mut options = PlaySessionOptions::from_app_calibration(&settings).unwrap();
        options.chart_start_seconds = Some(0.0);
        options.start_delay_seconds = Some(0.0);
        let mut session = LivePlaySession::start_with_assets(
            options,
            LiveSessionAssets {
                chart,
                clip,
                target,
                audio_label: "silent-native-pause-test".into(),
            },
        )
        .unwrap();
        wait_until(
            || {
                session.update().unwrap();
                session.snapshot().song_time_seconds >= 0.245
            },
            Duration::from_secs(3),
        );
        session
            .process_input(input(NativeInputEventKind::LanePress(0)))
            .unwrap();
        assert!(session.engine.lane_has_active_hold(LaneIndex::new(0)));
        session
            .process_input(input(NativeInputEventKind::FocusLost))
            .unwrap();
        thread::sleep(Duration::from_millis(250));
        let frozen = session.snapshot().song_time_seconds;
        let scheduled = session.snapshot().scheduled_time_seconds;
        thread::sleep(Duration::from_millis(200));
        session.update().unwrap();
        assert!((session.snapshot().song_time_seconds - frozen).abs() < 0.000001);
        assert_eq!(session.snapshot().scheduled_time_seconds, scheduled);
        assert_eq!(session.engine.judged_count(), 0);
        session.request_resume().unwrap();
        assert!(session.pause_countdown_seconds().is_none());
        session
            .process_input(input(NativeInputEventKind::FocusGained))
            .unwrap();
        assert!(session.is_paused());
        session.request_resume().unwrap();
        session
            .process_input(input(NativeInputEventKind::LanePress(0)))
            .unwrap();
        wait_until(
            || {
                session.update().unwrap();
                session.pause.mode_is_awaiting_audio()
            },
            Duration::from_secs(5),
        );
        wait_until(|| session.chart_clock.is_running(), Duration::from_secs(3));
        if redraw_before_release {
            session.update().unwrap();
        }
        session
            .process_input(input(NativeInputEventKind::LaneRelease(0)))
            .unwrap();
        assert!(!session.is_paused());
        assert_eq!(session.counts.miss, 1);
        assert_eq!(session.engine.judged_count(), 1);
        let report = session.report.summary();
        assert_eq!(report.hits, 0);
        assert_eq!(report.misses, 1);
        assert_eq!(report.hit_delta_samples_ms.len(), 1);
        assert_eq!(report.input_presses, 1);
        assert_eq!(report.input_releases, 1);
        println!(
            "native_pause redraw_before_release={redraw_before_release} frozen={frozen:.6} scheduled={scheduled:.6} misses={} head_samples={}",
            report.misses,
            report.hit_delta_samples_ms.len()
        );
    }
}
