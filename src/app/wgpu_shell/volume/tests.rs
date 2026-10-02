use super::*;

#[test]
fn keys_repeat_volume_but_never_repeat_mute_and_preserve_timing_and_calibration() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.audio_resolver = |_| panic!("gain must not reopen the output or reapply calibration");
    shell.state.settings.input.input_offset_ms = 23.0;
    shell.pending_session_options =
        Some(PlaySessionOptions::from_app_calibration(&shell.state.settings).unwrap());
    let before = shell.pending_session_options.clone().unwrap();
    shell.handle_volume_key(KeyCode::F7, true, false);
    shell.handle_volume_key(KeyCode::F7, true, true);
    shell.handle_volume_key(KeyCode::F7, false, false);
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
    shell.handle_volume_key(KeyCode::F9, true, false);
    shell.handle_volume_key(KeyCode::F9, true, true);
    shell.handle_volume_key(KeyCode::F9, false, false);
    assert!(shell.state.settings.audio.muted);
    let options = shell.pending_session_options.as_ref().unwrap();
    assert_eq!(options.volume, 0.0);
    assert_eq!(options.input_offset_ms, before.input_offset_ms);
    assert_eq!(options.chart_start_seconds, before.chart_start_seconds);
    assert_eq!(options.start_delay_seconds, before.start_delay_seconds);
    assert_eq!(options.lead_in_seconds, before.lead_in_seconds);
    assert_eq!(options.lookahead_seconds, before.lookahead_seconds);
    shell.handle_volume_key(KeyCode::F9, true, false);
    assert_eq!(shell.pending_session_options.as_ref().unwrap().volume, 0.9);
    assert!(!shell.handle_volume_key(KeyCode::KeyD, true, false));
    shell.binding_capture = Some(0);
    assert!(!shell.handle_volume_key(KeyCode::F7, true, false));
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
}

#[test]
fn wheel_adjusts_only_with_alt_or_over_indicator_and_accumulates_trackpad_motion() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.cursor_position = (250.0, 250.0);
    assert!(!shell.handle_volume_wheel(MouseScrollDelta::LineDelta(0.0, -1.0)));
    assert_eq!(shell.state.settings.audio.volume_percent, 100);
    shell.modifiers = ModifiersState::ALT;
    assert!(shell.handle_volume_wheel(MouseScrollDelta::LineDelta(0.0, -2.0)));
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
    for _ in 0..3 {
        assert!(shell.handle_volume_wheel(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -10.0)
        )));
    }
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
    shell.handle_volume_wheel(MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, -10.0),
    ));
    assert_eq!(shell.state.settings.audio.volume_percent, 85);
    shell.modifiers = ModifiersState::empty();
    let (width, height) = shell.viewport();
    let (x, y, _, _) = shell.volume_indicator(width, height);
    shell.cursor_position = (x + 5.0, y + 5.0);
    assert!(shell.handle_volume_wheel(MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
    shell.binding_capture = Some(0);
    assert!(!shell.handle_volume_wheel(MouseScrollDelta::LineDelta(0.0, -1.0)));
    assert_eq!(shell.state.settings.audio.volume_percent, 90);
}

#[test]
fn volume_shortcuts_pass_through_search_and_work_under_help() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.search_active = true;
    shell.window_focused = true;
    for key in [KeyCode::F7, KeyCode::F8, KeyCode::F9] {
        assert!(!shell.search_input(key, None, true));
    }
    shell.search_preedit = "かな".into();
    for key in [KeyCode::F7, KeyCode::F8, KeyCode::F9] {
        assert!(shell.search_input(key, None, true));
    }
    assert_eq!(shell.search_preedit, "かな");
    assert_eq!(shell.state.settings.audio.volume_percent, 100);
    assert!(!shell.state.settings.audio.muted);
    shell.search_preedit.clear();
    shell.help_visible = true;
    assert!(shell.handle_volume_key(KeyCode::F9, true, false));
    assert!(shell.state.settings.audio.muted);
    assert!(shell.search_query.is_empty());
}

#[test]
fn pause_volume_buttons_and_indicator_fit_supported_windows_and_apply_the_same_setting() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    for (width, height) in [(760.0, 520.0), (960.0, 640.0), (1280.0, 720.0)] {
        shell.state.settings.audio.volume_percent = 50;
        shell.state.settings.audio.muted = false;
        for (index, (x, y, w, h)) in volume_buttons(width, height).into_iter().enumerate() {
            assert!(x >= 0.0 && x + w < width && y >= 0.0 && y + h < height - 40.0);
            assert!(shell.click_volume_button(x + 5.0, y + 5.0, width, height));
            assert_eq!(
                shell.state.settings.audio.volume_percent,
                if index == 0 { 45 } else { 50 }
            );
            assert_eq!(shell.state.settings.audio.muted, index == 2);
        }
        for screen in [AppScreen::SongSelect, AppScreen::Gameplay] {
            shell.state.screen = screen;
            let (x, y, w, h) = shell.volume_indicator(width, height);
            assert!(x >= 0.0 && y >= 0.0 && x + w <= width && y + h <= height);
            assert!(shell.volume_hovered(x + 1.0, y + 1.0, width, height));
        }
    }
}
