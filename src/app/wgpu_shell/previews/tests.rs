use super::*;
use std::fs;

#[test]
fn selection_identity_and_preview_gates_follow_library_state() {
    let root = std::env::temp_dir().join(format!("zeff-preview-shell-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("song.wav"), b"fixture").unwrap();
    for name in ["A", "B"] {
        fs::write(root.join(format!("{name}.osu")), format!(
            "[General]\nMode:3\nAudioFilename:song.wav\nPreviewTime:12000\n[Metadata]\nTitle:{name}\n[Difficulty]\nCircleSize:4\n[HitObjects]\n64,192,1000,1,0\n"
        )).unwrap();
    }
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.library = AppLibrary::scan_roots_with_cache(&[root.clone()], &root.join("cache"));
    shell.window_focused = true;
    let request = shell.selected_preview().unwrap();
    assert_eq!(request.start_seconds, Some(12.0));
    assert_eq!(request.path, root.join("song.wav").canonicalize().unwrap());
    shell.library_index = 1;
    assert_eq!(shell.selected_preview().as_ref(), Some(&request));
    for screen in [
        AppScreen::Gameplay,
        AppScreen::Results,
        AppScreen::Calibration,
        AppScreen::Settings(SettingsPanel::Audio),
    ] {
        shell.state.screen = screen;
        assert!(shell.selected_preview().is_none());
    }
    shell.state.screen = AppScreen::SongSelect;
    shell.window_focused = false;
    assert!(shell.selected_preview().is_none());
    shell.window_focused = true;
    shell.help_visible = true;
    assert!(shell.selected_preview().is_none());
    shell.help_visible = false;
    shell.binding_capture = Some(0);
    assert!(shell.selected_preview().is_none());
    shell.binding_capture = None;
    shell.toggle_song_previews();
    assert!(shell.selected_preview().is_none());
    shell.toggle_song_previews();
    shell
        .preview_loader
        .request(Some(request.clone()), Instant::now());
    shell.prepare_selected_library_entry().unwrap();
    assert!(shell.selected_preview().is_none());
    assert!(!shell.preview_loader.is_loading());
    assert!(shell.preview_stream.is_none());
    shell.asset_load = None;
    shell.library_index = usize::MAX;
    assert!(shell.selected_preview().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn preview_gain_and_toggle_preserve_calibration_and_search_composition() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.audio_resolver = |_| panic!("preview controls must not change calibration");
    shell.state.settings.input.input_offset_ms = 19.0;
    shell.state.settings.audio.volume_percent = 35;
    shell.apply_volume();
    assert_eq!(shell.preview_volume.gain(), 0.35);
    shell.toggle_mute();
    assert_eq!(shell.preview_volume.gain(), 0.0);
    shell.settings_panel = SettingsPanel::Audio;
    shell.settings_row_index = 7;
    shell.adjust_selected_setting(1);
    assert!(!shell.state.settings.audio.song_previews);
    assert_eq!(shell.state.settings.input.input_offset_ms, 19.0);
    shell.search_active = true;
    shell.window_focused = true;
    assert!(!shell.search_input(KeyCode::F10, None, true));
    shell.search_preedit = "かな".into();
    assert!(shell.search_input(KeyCode::F10, None, true));
    assert_eq!(shell.search_preedit, "かな");
}

#[test]
#[ignore = "requires native output; both preview and calibration are muted"]
fn native_preview_stops_before_launch_calibration_and_on_focus_loss() {
    use crate::platform::audio::{AudioClip, AudioStreamOptions, PlaybackVolume};
    let target = output_stream_target(&AudioStreamOptions::default()).unwrap();
    let clip = Arc::new(AudioClip {
        samples: vec![0.0; 48_000],
        channels: 1,
        sample_rate: 48_000,
    });
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.settings.audio.muted = true;
    shell.state.settings.diagnostics.event_log_enabled = false;
    let start_preview = || {
        let stream = build_preview_stream(
            &target,
            clip.clone(),
            Some(0.0),
            None,
            PlaybackVolume::new(0.0),
        )
        .unwrap();
        stream.play().unwrap();
        stream
    };
    shell.preview_stream = Some(start_preview());
    shell.window_focused = false;
    shell.poll_song_preview();
    assert!(shell.preview_stream.is_none());
    shell.preview_stream = Some(start_preview());
    let options = PlaySessionOptions::from_app_calibration(&shell.state.settings).unwrap();
    shell.queue_chart_load(options);
    assert!(shell.preview_stream.is_none());
    assert!(shell.live_session.is_none());
    shell.asset_load = None;
    shell.preview_stream = Some(start_preview());
    shell.start_generated_calibration().unwrap();
    assert!(shell.preview_stream.is_none());
    assert!(shell.live_session.is_some());
    assert_eq!(shell.pending_session_options.as_ref().unwrap().volume, 0.0);
    shell.finish_live_session().unwrap();
}
