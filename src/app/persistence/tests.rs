use super::{AppConfigFile, AppPersistence, CalibrationDeviceKey, SavedCalibrationOffset};
use crate::play::LiveAudioSummary;
use std::fs;

fn audio() -> LiveAudioSummary {
    LiveAudioSummary {
        host_name: "WASAPI".to_owned(),
        device_label: "Speakers".to_owned(),
        device_id: Some("wasapi:{device}".to_owned()),
        sample_rate: 96_000,
        requested_buffer_frames: Some(960),
        first_callback_frames: Some(2112),
        channel_count: 8,
        sample_format: "F32".to_owned(),
    }
}

fn saved(audio: &LiveAudioSummary, offset: f64) -> SavedCalibrationOffset {
    SavedCalibrationOffset::new(
        CalibrationDeviceKey::from_audio(audio),
        offset,
        72,
        3,
        "STABLE",
    )
}

#[test]
fn round_trips_settings_and_calibration_offsets() {
    let path = temp_config_path_for_test("round-trip");
    let mut persistence = AppPersistence::load_from_path(path.clone()).unwrap();
    let mut settings = persistence.settings().clone();
    settings.input.input_offset_ms = -20.5;
    settings.gameplay.scroll_speed = 1.7;
    settings.audio.volume_percent = 35;
    settings.audio.muted = true;
    persistence.set_manual_input_offset_ms(7.0);
    persistence.save_settings(&settings).unwrap();
    persistence
        .upsert_calibration_offset(saved(&audio(), -20.5))
        .unwrap();

    let loaded = AppPersistence::load_from_path(path.clone()).unwrap();
    assert_eq!(loaded.settings().input.input_offset_ms, -20.5);
    assert_eq!(loaded.settings().gameplay.scroll_speed, 1.7);
    assert_eq!(loaded.settings().audio.volume_percent, 35);
    assert!(loaded.settings().audio.muted);
    let launch = crate::play::PlaySessionOptions::from_app_calibration(loaded.settings()).unwrap();
    assert_eq!(launch.lookahead_seconds, 1.0 / 1.7);
    assert_eq!(launch.volume, 0.0);
    assert_eq!(loaded.manual_input_offset_ms(), 7.0);
    assert_eq!(loaded.config.calibration_offsets.len(), 1);
    assert_eq!(
        loaded.calibration_for_audio(&audio()).unwrap().hit_count,
        72
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn device_key_prefers_resolved_audio_device_id() {
    let key = CalibrationDeviceKey::from_audio(&audio());
    assert_eq!(key.version, 2);
    assert_eq!(key.audio_host, "WASAPI");
    assert_eq!(key.cpal_device_id.as_deref(), Some("wasapi:{device}"));
    assert_eq!(key.sample_rate, 96_000);
    assert_eq!(key.requested_buffer_frames, Some(960));
    assert_eq!(key.first_callback_frames, Some(2112));
    assert_eq!(key.channel_count, 8);
    assert_eq!(key.stream_direction, "output");
}

#[test]
fn resolved_audio_matches_saved_runtime_key() {
    let mut persistence = AppPersistence::disabled();
    persistence
        .upsert_calibration_offset(saved(&audio(), -20.5))
        .unwrap();
    let mut resolved = audio();
    resolved.first_callback_frames = None;
    resolved.device_label = "Renamed speakers".to_owned();
    assert_eq!(
        persistence
            .calibration_for_audio(&resolved)
            .unwrap()
            .input_offset_ms,
        -20.5
    );
}

#[test]
fn rejects_changed_endpoint_and_stream_format() {
    let mut persistence = AppPersistence::disabled();
    persistence
        .upsert_calibration_offset(saved(&audio(), -20.5))
        .unwrap();
    let mut changes = vec![audio(); 6];
    changes[0].device_id = Some("wasapi:{other}".to_owned());
    changes[1].sample_rate = 48_000;
    changes[2].channel_count = 2;
    changes[3].sample_format = "I16".to_owned();
    changes[4].requested_buffer_frames = None;
    changes[5].host_name = "Other".to_owned();
    for changed in changes {
        assert!(
            persistence.calibration_for_audio(&changed).is_none(),
            "{changed:?}"
        );
    }
}

#[test]
fn callback_observations_replace_one_stream_profile() {
    let mut persistence = AppPersistence::disabled();
    let mut first = saved(&audio(), -20.5);
    first.updated_at_ms = u64::MAX;
    let mut duplicate = first.clone();
    duplicate.key.first_callback_frames = Some(960);
    persistence.config.calibration_offsets = vec![first, duplicate];
    let mut current = audio();
    current.first_callback_frames = Some(480);
    persistence
        .upsert_calibration_offset(saved(&current, -10.0))
        .unwrap();
    assert_eq!(persistence.config.calibration_offsets.len(), 1);
    assert_eq!(
        persistence
            .calibration_for_audio(&current)
            .unwrap()
            .input_offset_ms,
        -10.0
    );
}

#[test]
fn legacy_default_host_requires_the_same_resolved_device() {
    let mut persistence = AppPersistence::disabled();
    let mut previous = audio();
    previous.host_name = "default".to_owned();
    persistence
        .upsert_calibration_offset(saved(&previous, -20.5))
        .unwrap();
    assert!(persistence.calibration_for_audio(&audio()).is_some());
    let mut other = audio();
    other.device_id = Some("wasapi:{other}".to_owned());
    assert!(persistence.calibration_for_audio(&other).is_none());
}

#[test]
fn legacy_manual_offset_survives_first_calibration() {
    let mut config = AppConfigFile::default();
    config.settings.input.input_offset_ms = 25.0;
    let source = toml::to_string(&config).unwrap();
    let config: AppConfigFile = toml::from_str(&source).unwrap();
    let mut persistence = AppPersistence { path: None, config };
    assert_eq!(persistence.manual_input_offset_ms(), 25.0);
    persistence
        .upsert_calibration_offset(saved(&audio(), 25.0))
        .unwrap();
    assert_eq!(persistence.manual_input_offset_ms(), 25.0);
}

#[test]
fn legacy_automatic_offset_is_not_a_manual_fallback() {
    let mut config = AppConfigFile::default();
    config.settings.input.input_offset_ms = 25.0;
    config.calibration_offsets.push(saved(&audio(), 25.0));
    let persistence = AppPersistence { path: None, config };
    assert_eq!(persistence.manual_input_offset_ms(), 0.0);
}

#[test]
fn legacy_manual_fallback_is_captured_before_settings_overwrite_and_reload() {
    let path = temp_config_path_for_test("legacy-manual");
    let mut config = AppConfigFile::default();
    config.settings.input.input_offset_ms = 7.0;
    fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    let mut persistence = AppPersistence::load_from_path(path.clone()).unwrap();
    let mut settings = persistence.settings().clone();
    settings.input.input_offset_ms = 20.0;
    persistence.set_settings(&settings);
    persistence
        .upsert_calibration_offset(saved(&audio(), 20.0))
        .unwrap();
    let loaded = AppPersistence::load_from_path(path.clone()).unwrap();
    assert_eq!(loaded.manual_input_offset_ms(), 7.0);
    let mut other = audio();
    other.device_id = Some("wasapi:{other}".to_owned());
    assert!(loaded.calibration_for_audio(&other).is_none());
    fs::remove_file(path).unwrap();
}

fn temp_config_path_for_test(label: &str) -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("zeff-rhythm-{label}-{unique}.toml"))
}
