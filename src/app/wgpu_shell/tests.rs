use super::{PendingSessionSource, ResultAction, WgpuAppShell, WgpuShellOptions};
use crate::app::persistence::{AppPersistence, CalibrationDeviceKey, SavedCalibrationOffset};
use crate::app::settings::InputBackendPreference;
use crate::app::state::{AppScreen, SettingsPanel};
use crate::play::metrics::MetricStats;
use crate::play::{JudgementCounts, LiveAudioSummary, LiveRunSummary, PlayReportSummary};
use crate::render::settings::RenderPresentModePreference;

#[test]
fn library_options_accept_multiple_roots() {
    let options = WgpuShellOptions::parse(&[
        "--library".to_owned(),
        "songs".to_owned(),
        "--library".to_owned(),
        "packs".to_owned(),
    ])
    .unwrap();
    assert_eq!(
        options.library_roots,
        vec![
            std::path::PathBuf::from("songs"),
            std::path::PathBuf::from("packs")
        ]
    );
    assert!(WgpuShellOptions::parse(&["--library".to_owned()]).is_err());
}

#[test]
fn empty_song_select_is_renderable() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.open_song_select();
    assert!(!shell.build_ui_rects(960.0, 640.0).is_empty());
}

#[test]
fn retry_of_deleted_chart_returns_to_song_select() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell
        .state
        .select_chart(crate::app::state::ChartSelection::new(
            "missing-library-test-chart.osu",
        ));
    shell.state.screen = AppScreen::Results;
    shell.pending_session_source = Some(PendingSessionSource::Chart);
    shell.restart_live_session().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while shell.asset_load.is_some() && std::time::Instant::now() < deadline {
        shell.poll_chart_load();
        std::thread::yield_now();
    }
    assert_eq!(shell.state.screen, AppScreen::SongSelect);
    assert!(shell.library_launch_error.is_some());
    assert!(shell.live_session.is_none());
}

#[test]
fn cycles_audio_settings_without_device_enumeration() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.settings_panel = SettingsPanel::Audio;

    shell.settings_row_index = 1;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.audio.host.as_deref(), Some("WASAPI"));

    shell.state.settings.audio.device_id = Some("device-id".to_owned());
    shell.state.settings.audio.device_label = Some("Device".to_owned());
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.audio.host, None);
    assert_eq!(shell.state.settings.audio.device_id, None);
    assert_eq!(shell.state.settings.audio.device_label, None);

    shell.settings_row_index = 3;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.audio.sample_rate, Some(44_100));

    shell.settings_row_index = 4;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.audio.buffer_frames, Some(240));
}

#[test]
fn cycles_input_video_gameplay_and_diagnostic_settings() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());

    shell.settings_panel = SettingsPanel::Input;
    shell.settings_row_index = 1;
    shell.adjust_selected_setting(1);
    assert_eq!(
        shell.state.settings.input.backend,
        InputBackendPreference::Winit
    );
    shell.settings_row_index = 2;
    shell.adjust_selected_setting(1);
    shell.settings_row_index = 3;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.input.input_offset_ms, 11.0);

    shell.settings_panel = SettingsPanel::Video;
    shell.settings_row_index = 2;
    shell.adjust_selected_setting(1);
    assert_eq!(
        shell.state.settings.video.render_latency.present_mode,
        RenderPresentModePreference::Mailbox
    );
    shell.adjust_selected_setting(1);
    assert_eq!(
        shell.state.settings.video.render_latency.present_mode,
        RenderPresentModePreference::Immediate
    );
    assert!(!shell.state.settings.video.vsync);

    shell.settings_row_index = 3;
    shell.adjust_selected_setting(1);
    assert_eq!(
        shell
            .state
            .settings
            .video
            .render_latency
            .desired_maximum_frame_latency,
        2
    );
    shell.settings_row_index = 4;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.video.target_frame_rate, Some(120));

    shell.settings_panel = SettingsPanel::Gameplay;
    shell.settings_row_index = 3;
    shell.adjust_selected_setting(1);
    assert_eq!(shell.state.settings.gameplay.judgement_preset, "strict");

    shell.settings_panel = SettingsPanel::Diagnostics;
    shell.settings_row_index = 1;
    shell.adjust_selected_setting(1);
    assert!(!shell.state.settings.diagnostics.event_log_enabled);
}

#[test]
fn result_apply_offset_updates_current_setting() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.screen = AppScreen::Results;
    shell.latest_run = Some(run_summary(22.0, -8.0, 4));

    shell.confirm_results().unwrap();

    assert_eq!(shell.state.settings.input.input_offset_ms, 30.0);
}

#[test]
fn calibration_apply_offset_uses_repeated_trial_average() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.screen = AppScreen::Results;
    shell.latest_run_source = Some(PendingSessionSource::Calibration);
    let first = run_summary(20.0, -5.0, 10);
    let second = run_summary(24.0, 1.0, 20);
    shell.calibration_history.record_run(&first);
    shell.calibration_history.record_run(&second);
    shell.latest_run = Some(second);

    shell.confirm_results().unwrap();

    assert!((shell.state.settings.input.input_offset_ms - 23.666).abs() < 0.01);
}

#[test]
fn clear_calibration_action_resets_history() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.screen = AppScreen::Results;
    shell.latest_run_source = Some(PendingSessionSource::Calibration);
    shell.latest_run = Some(run_summary(20.0, -5.0, 10));
    shell
        .calibration_history
        .record_run(shell.latest_run.as_ref().unwrap());
    shell.result_row_index = shell
        .result_actions()
        .iter()
        .position(|action| *action == ResultAction::ClearCalibration)
        .unwrap();

    shell.confirm_results().unwrap();

    assert_eq!(shell.calibration_history.attempts(), 0);
    assert!(shell.calibration_history.aggregate().is_none());
    assert!(shell.suggested_results_offset_ms().is_none());
    assert!(!shell.result_actions().contains(&ResultAction::ApplyOffset));
}

#[test]
fn startup_applies_matching_saved_calibration_offset() {
    let mut persistence = AppPersistence::disabled();
    persistence
        .upsert_calibration_offset(SavedCalibrationOffset::new(
            CalibrationDeviceKey::from_audio(&LiveAudioSummary {
                host_name: "WASAPI".to_owned(),
                device_label: "Speakers".to_owned(),
                device_id: Some("device-a".to_owned()),
                sample_rate: 96_000,
                requested_buffer_frames: None,
                first_callback_frames: Some(960),
                channel_count: 2,
                sample_format: "F32".to_owned(),
            }),
            -12.5,
            72,
            3,
            "STABLE",
        ))
        .unwrap();

    let mut shell = WgpuAppShell::new_with_persistence(WgpuShellOptions::default(), persistence);
    shell.update_resolved_audio(Some(audio("device-a")), true);

    assert_eq!(shell.state.settings.input.input_offset_ms, -12.5);
    assert!(shell.saved_calibration_for_current_settings().is_some());
}

fn run_summary(input_offset_ms: f64, mean_delta_ms: f64, hits: usize) -> LiveRunSummary {
    LiveRunSummary {
        title: "test".to_owned(),
        input_offset_ms,
        audio: LiveAudioSummary::default(),
        judged_count: hits,
        complete: false,
        counts: JudgementCounts::default(),
        report: PlayReportSummary {
            hits,
            hit_delta_ms: Some(MetricStats {
                count: hits,
                mean: mean_delta_ms,
                stddev: 1.0,
                min: mean_delta_ms - 1.0,
                p50: mean_delta_ms,
                p95: mean_delta_ms + 1.0,
                p99: mean_delta_ms + 1.0,
                max: mean_delta_ms + 1.0,
            }),
            ..PlayReportSummary::default()
        },
        event_log_path: None,
    }
}

fn audio(device: &str) -> LiveAudioSummary {
    LiveAudioSummary {
        host_name: "WASAPI".to_owned(),
        device_id: Some(device.to_owned()),
        device_label: "Speakers".to_owned(),
        sample_rate: 96_000,
        channel_count: 2,
        sample_format: "F32".to_owned(),
        ..LiveAudioSummary::default()
    }
}

#[test]
fn default_endpoint_changes_reset_trials_and_automatic_offset() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    let mut run = run_summary(0.0, -20.0, 12);
    run.audio = audio("device-a");
    shell.update_resolved_audio(Some(run.audio.clone()), false);
    shell.calibration_history.record_run(&run);
    shell.latest_run_source = Some(PendingSessionSource::Calibration);
    shell.latest_run = Some(run.clone());
    shell.state.screen = AppScreen::Results;
    shell.confirm_results().unwrap();
    assert_eq!(shell.state.settings.input.input_offset_ms, 20.0);

    shell.update_resolved_audio(Some(audio("device-b")), false);
    assert_eq!(shell.state.settings.input.input_offset_ms, 0.0);
    assert!(shell.calibration_history.aggregate().is_none());
    assert!(shell.suggested_results_offset_ms().is_none());
    assert!(shell.saved_calibration_for_current_settings().is_none());

    shell.update_resolved_audio(Some(audio("device-a")), false);
    assert_eq!(shell.state.settings.input.input_offset_ms, 20.0);
}

#[test]
fn manual_offset_is_fallback_for_uncalibrated_output() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.settings_panel = SettingsPanel::Input;
    shell.settings_row_index = 3;
    shell.adjust_selected_setting(1);
    shell.update_resolved_audio(Some(audio("device-b")), true);
    assert_eq!(shell.state.settings.input.input_offset_ms, 10.0);
}

#[test]
fn callback_observation_keeps_trials_and_manual_adjustment() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    let mut run = run_summary(0.0, -20.0, 12);
    run.audio = audio("device-a");
    shell.update_resolved_audio(Some(run.audio.clone()), false);
    shell.calibration_history.record_run(&run);
    shell.state.settings.input.input_offset_ms = 7.0;
    run.audio.first_callback_frames = Some(960);
    shell.update_resolved_audio(Some(run.audio), false);
    assert_eq!(shell.calibration_history.aggregate().unwrap().hit_count, 12);
    assert_eq!(shell.state.settings.input.input_offset_ms, 7.0);
}
