use super::*;
use crate::platform::input::{NativeInputEventKind, NativeInputSource};
use crate::play::JudgementCounts;
use rhythm_core::{
    Chart, GameKey, InputEvent, JudgementWindows, LaneIndex, Note, NoteId, RhythmEngine,
};
use std::time::Instant;

fn native(pressed: bool) -> NativeInputEvent {
    let now = Instant::now();
    NativeInputEvent {
        kind: if pressed {
            NativeInputEventKind::LanePress(0)
        } else {
            NativeInputEventKind::LaneRelease(0)
        },
        source: NativeInputSource::Winit,
        timestamp_kind: NativeInputTimestampKind::ReceiptMonotonic,
        event_time: now,
        received_time: now,
        source_timestamp_ns: None,
        queue_age_ms: Some(0.2),
    }
}

#[test]
fn paused_inputs_and_cancelled_countdown_do_not_add_scoring_or_timing_samples() {
    let mut report = PlayReport::new(None).unwrap();
    for event in [
        "pause",
        "resume_countdown",
        "pause",
        "resume_countdown",
        "resume",
    ] {
        report.record_transport(event, 1.0).unwrap();
    }
    report
        .record_paused_input(0, true, native(true), 1.0)
        .unwrap();
    report
        .record_paused_input(0, false, native(false), 1.0)
        .unwrap();
    let summary = report.summary();
    assert_eq!(summary.input_presses + summary.input_releases, 0);
    assert_eq!(summary.hits + summary.misses + summary.unmatched_inputs, 0);
    assert!(summary.hit_delta_ms.is_none());
    assert!(summary.input_queue_age_ms.is_none());
    assert!(summary.input_timestamp_kind.is_none());
}

#[test]
fn completed_and_broken_holds_have_one_final_score_and_separate_head_timing() {
    for release_time in [1.5, 2.0] {
        let mut chart = Chart::new(1);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
        let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
        let mut report = PlayReport::new(None).unwrap();
        let mut counts = JudgementCounts::default();
        let head = engine
            .submit_input(InputEvent {
                key: GameKey::Lane(LaneIndex::new(0)),
                pressed: true,
                time_seconds: 1.025,
            })
            .unwrap();
        report.record_input_press(0, native(true), 1.025).unwrap();
        report.record_hit(head, native(true)).unwrap();
        counts.add(head);
        assert_eq!(report.summary().hits, 0);
        assert_eq!(counts.perfect, 0);
        let tail = engine
            .submit_input(InputEvent {
                key: GameKey::Lane(LaneIndex::new(0)),
                pressed: false,
                time_seconds: release_time,
            })
            .unwrap();
        report
            .record_input_release(0, native(false), release_time)
            .unwrap();
        report
            .record_hold_tail(tail, Some(native(false)), release_time)
            .unwrap();
        counts.add(tail);
        let summary = report.summary();
        assert_eq!(summary.hits + summary.misses, 1);
        assert_eq!(counts.perfect + counts.miss, 1);
        assert_eq!(summary.hit_delta_samples_ms.len(), 1);
        assert!((summary.hit_delta_samples_ms[0] - 25.0).abs() < 1e-9);
        assert_eq!(summary.input_presses, 1);
        assert_eq!(summary.input_releases, 1);
        assert_eq!(summary.hits, usize::from(release_time == 2.0));
    }
}

#[test]
fn automatic_tail_csv_has_no_fabricated_input_timestamp_or_calibration_delta() {
    let path = std::env::temp_dir().join(format!(
        "zeff-hold-report-{}-{}.csv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut report = PlayReport::new(Some(&path)).unwrap();
    let mut chart = Chart::new(1);
    chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
    let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
    let head = engine
        .submit_input(InputEvent {
            key: GameKey::Lane(LaneIndex::new(0)),
            pressed: true,
            time_seconds: 1.0,
        })
        .unwrap();
    report.record_hit(head, native(true)).unwrap();
    let mut results = Vec::new();
    engine.collect_judgements(2.1, &mut results);
    report.record_hold_tail(results[0], None, 2.1).unwrap();
    report.flush().unwrap();
    drop(report);
    let source = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = source.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("hold_head,"));
    let columns: Vec<_> = lines[2].split(',').collect();
    assert_eq!(columns[0], "hold_complete");
    assert_eq!(columns[4], "Marvelous");
    assert_eq!(&columns[5..10], &["", "", "", "", ""]);
    assert_eq!(columns[10], "2.000000");
    std::fs::remove_file(path).unwrap();
}
