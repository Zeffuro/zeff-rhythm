use super::{EventLogFormat, EventLogRow, RunAnalysis};

#[test]
fn parses_hit_rows() {
    let row = EventLogRow::parse(
        "hit,1.000000,2,42,Perfect,-12.500,0.200,123,sdl,sdl_ticks_ns,1.012500",
        EventLogFormat::Current,
    )
    .unwrap();

    assert_eq!(row.event, "hit");
    assert_eq!(row.rating, Some("Perfect"));
    assert_eq!(row.delta_ms, Some(-12.5));
    assert_eq!(row.input_queue_age_ms, Some(0.2));
    assert_eq!(row.input_timestamp_kind, Some("sdl_ticks_ns"));
}

#[test]
fn aggregates_hit_metrics() {
    let mut analysis = RunAnalysis::default();
    analysis.record(
        EventLogRow::parse(
            "hit,1.000000,2,42,Perfect,-12.500,0.200,123,sdl,sdl_ticks_ns,1.012500",
            EventLogFormat::Current,
        )
        .unwrap(),
    );
    analysis.record(
        EventLogRow::parse(
            "miss,1.200000,0,43,Miss,,,,,,1.000000",
            EventLogFormat::Current,
        )
        .unwrap(),
    );

    assert_eq!(analysis.hits, 1);
    assert_eq!(analysis.misses, 1);
    assert_eq!(analysis.perfect, 1);
    assert_eq!(analysis.hit_delta_ms, vec![-12.5]);
    let group = analysis.timestamp_kind_groups.get("sdl_ticks_ns").unwrap();
    assert_eq!(group.hits, 1);
    assert_eq!(group.hit_delta_ms, vec![-12.5]);
}

#[test]
fn queue_age_uses_raw_input_rows_only() {
    let mut analysis = RunAnalysis::default();
    analysis.record(
        EventLogRow::parse(
            "input_press,1.000000,2,,,0.000,0.200,123,sdl,sdl_ticks_ns,",
            EventLogFormat::Current,
        )
        .unwrap(),
    );
    analysis.record(
        EventLogRow::parse(
            "hit,1.000000,2,42,Perfect,-12.500,0.200,123,sdl,sdl_ticks_ns,1.012500",
            EventLogFormat::Current,
        )
        .unwrap(),
    );
    analysis.record(
        EventLogRow::parse(
            "unmatched_input,1.200000,2,,,0.000,0.300,124,sdl,sdl_ticks_ns,",
            EventLogFormat::Current,
        )
        .unwrap(),
    );

    assert_eq!(analysis.input_presses, 1);
    assert_eq!(analysis.hits, 1);
    assert_eq!(analysis.unmatched_inputs, 1);
    assert_eq!(analysis.input_queue_age_ms, vec![0.2]);
    assert_eq!(analysis.input_timestamp_kinds.get("sdl_ticks_ns"), Some(&1));
    let group = analysis.timestamp_kind_groups.get("sdl_ticks_ns").unwrap();
    assert_eq!(group.input_events, 1);
    assert_eq!(group.input_queue_age_ms, vec![0.2]);
}

#[test]
fn parses_legacy_rows() {
    let row = EventLogRow::parse(
        "hit,1.000000,2,42,Perfect,-12.500,0.200,123,1.012500",
        EventLogFormat::Legacy,
    )
    .unwrap();

    assert_eq!(row.event, "hit");
    assert_eq!(row.rating, Some("Perfect"));
    assert_eq!(row.delta_ms, Some(-12.5));
    assert_eq!(row.input_queue_age_ms, Some(0.2));
    assert_eq!(row.input_timestamp_kind, None);
}

#[test]
fn robust_hit_delta_mean_trims_outliers() {
    let mean = super::robust_hit_delta_mean_ms(&[-100.0, -2.0, 0.0, 2.0, 100.0]).unwrap();

    assert_eq!(mean, 0.0);
}

#[test]
fn paused_input_and_transport_rows_do_not_enter_scoring_or_calibration_metrics() {
    let mut analysis = RunAnalysis::default();
    for line in [
        "pause,1,,,,,,,,,",
        "paused_input_press,1,0,,,0,10,123,winit,receipt_monotonic,",
        "paused_input_release,1,0,,,0,20,124,winit,receipt_monotonic,",
        "resume_countdown,1,,,,,,,,,",
        "resume,1,,,,,,,,,",
    ] {
        analysis.record(EventLogRow::parse(line, EventLogFormat::Current).unwrap());
    }
    assert_eq!(analysis.input_presses + analysis.input_releases, 0);
    assert_eq!(
        analysis.hits + analysis.misses + analysis.unmatched_inputs,
        0
    );
    assert!(analysis.hit_delta_ms.is_empty());
    assert!(analysis.input_queue_age_ms.is_empty());
    assert!(analysis.input_timestamp_kinds.is_empty());
}

#[test]
fn hold_heads_provide_timing_but_tails_provide_final_scores() {
    let mut analysis = RunAnalysis::default();
    for line in [
        "hold_head,1.025,0,1,Perfect,25,0.2,123,winit,receipt_monotonic,1",
        "hold_break,1.5,0,1,Miss,,0.3,124,winit,receipt_monotonic,2",
        "hold_head,3.0,1,2,Marvelous,0,0.2,125,winit,receipt_monotonic,3",
        "hold_complete,4,1,2,Marvelous,,,,,,4",
    ] {
        analysis.record(EventLogRow::parse(line, EventLogFormat::Current).unwrap());
    }
    assert_eq!(analysis.hits, 1);
    assert_eq!(analysis.misses, 1);
    assert_eq!(analysis.marvelous, 1);
    assert_eq!(analysis.perfect, 0);
    assert_eq!(analysis.hit_delta_ms, vec![25.0, 0.0]);
    assert!(analysis.input_queue_age_ms.is_empty());
}
