use super::*;

impl WgpuAppShell {
    pub(super) fn adjust_scroll_speed(&mut self, direction: i32) {
        self.state.settings.gameplay.adjust_scroll_speed(direction);
        if let Some(options) = self.pending_session_options.as_mut() {
            options.lookahead_seconds = self.state.settings.gameplay.scroll_time_seconds();
        }
        self.persist_current_settings();
        self.request_redraw();
    }

    pub(super) fn handle_scroll_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> bool {
        let direction = match code {
            KeyCode::F5 => -1,
            KeyCode::F6 => 1,
            _ => return false,
        };
        if pressed && !repeat {
            self.adjust_scroll_speed(direction);
        }
        true
    }

    pub(super) fn click_scroll_button(&mut self, x: f32, y: f32, width: f32, height: f32) -> bool {
        for (index, (bx, by, bw, bh)) in scroll_buttons(width, height).into_iter().enumerate() {
            if (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y) {
                self.adjust_scroll_speed(if index == 0 { -1 } else { 1 });
                return true;
            }
        }
        false
    }

    pub(super) fn push_scroll_buttons(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        for (index, (x, y, w, _)) in scroll_buttons(width, height).into_iter().enumerate() {
            push_button(rects, x, y, w, ["F5 SLOWER", "F6 FASTER"][index], false);
        }
    }
}

fn scroll_buttons(width: f32, height: f32) -> [(f32, f32, f32, f32); 2] {
    [
        (width * 0.5 - 180.0, height * 0.5 + 100.0, 170.0, 36.0),
        (width * 0.5 + 10.0, height * 0.5 + 100.0, 170.0, 36.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_speed_only_changes_presentation_settings_and_retry_lookahead() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.state.settings.input.input_offset_ms = 27.0;
        let settings = shell.state.settings.clone();
        shell.pending_session_options =
            Some(PlaySessionOptions::from_app_calibration(&settings).unwrap());
        let original_options = shell.pending_session_options.clone().unwrap();
        assert!(shell.handle_scroll_key(KeyCode::F6, true, false));
        assert!((shell.state.settings.gameplay.scroll_time_seconds() - 1.0 / 1.1).abs() < 1e-9);
        let options = shell.pending_session_options.as_ref().unwrap();
        assert_eq!(
            options.lookahead_seconds,
            shell.state.settings.gameplay.scroll_time_seconds()
        );
        assert_eq!(options.input_offset_ms, original_options.input_offset_ms);
        assert_eq!(options.lead_in_seconds, original_options.lead_in_seconds);
        assert_eq!(
            options.start_delay_seconds,
            original_options.start_delay_seconds
        );
        assert_eq!(
            options.chart_start_seconds,
            original_options.chart_start_seconds
        );
        shell.state.settings.gameplay.scroll_speed = settings.gameplay.scroll_speed;
        assert_eq!(shell.state.settings, settings);
    }

    #[test]
    fn speed_shortcuts_do_not_repeat_on_release_or_consume_lane_input() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        assert!(shell.handle_scroll_key(KeyCode::F6, true, false));
        assert!(shell.handle_scroll_key(KeyCode::F6, true, true));
        assert!(shell.handle_scroll_key(KeyCode::F6, false, false));
        assert!(!shell.handle_scroll_key(KeyCode::KeyD, true, false));
        assert_eq!(shell.state.settings.gameplay.scroll_speed, 1.1);
        shell.state.screen = AppScreen::SongSelect;
        shell.search_active = true;
        assert!(!shell.search_input(KeyCode::F5, None, true));
        assert!(shell.search_query.is_empty());
    }

    #[test]
    fn pause_speed_mouse_buttons_adjust_same_setting_and_fit_small_window() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        for (width, height) in [(760.0, 520.0), (960.0, 640.0), (1280.0, 720.0)] {
            for (index, (x, y, w, h)) in scroll_buttons(width, height).into_iter().enumerate() {
                assert!(x >= 0.0 && x + w < width && y + h < height - 40.0);
                let before = shell.state.settings.gameplay.effective_scroll_speed();
                assert!(shell.click_scroll_button(x + 4.0, y + 4.0, width, height));
                assert_eq!(
                    shell.state.settings.gameplay.effective_scroll_speed() > before,
                    index == 1
                );
            }
        }
    }
}
