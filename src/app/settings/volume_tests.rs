use super::*;
use crate::app::state::{ChartSelection, PlayLaunchRequest};
use crate::play::PlaySessionOptions;

#[test]
fn legacy_audio_settings_keep_full_volume_and_existing_preferences() {
    let mut settings = AppSettings::default();
    settings.audio.device_id = Some("unchanged-device".into());
    settings.audio.buffer_frames = Some(480);
    settings.input.input_offset_ms = 21.0;
    let legacy = toml::to_string(&settings)
        .unwrap()
        .lines()
        .filter(|line| {
            !line.starts_with("volume_percent =")
                && !line.starts_with("muted =")
                && !line.starts_with("song_previews =")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let loaded: AppSettings = toml::from_str(&legacy).unwrap();
    assert_eq!(loaded, settings);
    assert_eq!(loaded.audio.gain(), 1.0);
    assert!(loaded.audio.song_previews);
    settings.audio.song_previews = false;
    assert!(
        !toml::from_str::<AppSettings>(&toml::to_string(&settings).unwrap())
            .unwrap()
            .audio
            .song_previews
    );
}

#[test]
fn volume_is_bounded_and_mute_keeps_the_chosen_level() {
    let mut audio = AudioSettings::default();
    for _ in 0..30 {
        audio.adjust_volume(-1);
    }
    assert_eq!(audio.volume_percent, 0);
    assert_eq!(audio.gain(), 0.0);
    for _ in 0..7 {
        audio.adjust_volume(1);
    }
    audio.muted = true;
    assert_eq!(audio.volume_percent, 35);
    assert_eq!(audio.gain(), 0.0);
    assert_eq!(audio.volume_label(), "MUTED 35%");
    audio.muted = false;
    assert!((audio.gain() - 0.35).abs() < 1e-6);
    audio.muted = true;
    audio.adjust_volume(1);
    assert!(!audio.muted);
    assert_eq!(audio.volume_percent, 40);
    audio.volume_percent = 255;
    assert_eq!(audio.gain(), 1.0);
    audio.adjust_volume(-1);
    assert_eq!(audio.volume_percent, 95);
    for _ in 0..30 {
        audio.adjust_volume(1);
    }
    assert_eq!(audio.volume_percent, 100);
}

#[test]
fn music_calibration_and_sdl_launches_share_saved_gain_without_timing_changes() {
    let mut settings = AppSettings::default();
    settings.audio.volume_percent = 35;
    settings.input.input_offset_ms = 17.0;
    let launch = PlayLaunchRequest {
        chart: ChartSelection {
            chart_path: "fixture.osu".into(),
            chart_index: 0,
            audio_path: None,
        },
        settings: settings.clone(),
    };
    for options in [
        PlaySessionOptions::from_app_launch(&launch).unwrap(),
        PlaySessionOptions::from_app_launch_for_sdl_harness(&launch).unwrap(),
        PlaySessionOptions::from_app_calibration(&settings).unwrap(),
    ] {
        assert_eq!(options.volume, settings.audio.gain());
        assert_eq!(options.input_offset_ms, 17.0);
        assert_eq!(options.lookahead_seconds, 1.0);
    }
    settings.audio.muted = true;
    assert_eq!(
        PlaySessionOptions::from_app_calibration(&settings)
            .unwrap()
            .volume,
        0.0
    );
}
