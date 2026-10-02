use super::*;
use winit::event::MouseScrollDelta;

impl WgpuAppShell {
    pub(super) fn apply_volume(&mut self) {
        let gain = self.state.settings.audio.gain();
        self.preview_volume.set_gain(gain);
        if let Some(session) = self.live_session.as_mut() {
            session.set_volume(gain);
        }
        if let Some(options) = self.pending_session_options.as_mut() {
            options.volume = gain;
        }
        self.persist_current_settings();
        self.request_redraw();
    }

    pub(super) fn adjust_volume(&mut self, steps: i32) {
        let before = self.state.settings.audio.clone();
        for _ in 0..steps.unsigned_abs().min(20) {
            self.state.settings.audio.adjust_volume(steps.signum());
        }
        if self.state.settings.audio != before {
            self.apply_volume();
        }
    }

    pub(super) fn toggle_mute(&mut self) {
        self.state.settings.audio.muted = !self.state.settings.audio.muted;
        self.apply_volume();
    }

    pub(super) fn handle_volume_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> bool {
        if self.binding_capture.is_some()
            || !matches!(code, KeyCode::F7 | KeyCode::F8 | KeyCode::F9)
        {
            return false;
        }
        if pressed {
            match code {
                KeyCode::F7 => self.adjust_volume(-1),
                KeyCode::F8 => self.adjust_volume(1),
                KeyCode::F9 if !repeat => self.toggle_mute(),
                _ => {}
            }
        }
        true
    }

    pub(super) fn volume_indicator(&self, width: f32, height: f32) -> (f32, f32, f32, f32) {
        if self.state.screen == AppScreen::Gameplay {
            (width - 190.0, height - 35.0, 174.0, 28.0)
        } else {
            (width - 198.0, 52.0, 174.0, 20.0)
        }
    }

    pub(super) fn volume_hovered(&self, x: f32, y: f32, width: f32, height: f32) -> bool {
        let (bx, by, bw, bh) = self.volume_indicator(width, height);
        (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
    }

    pub(super) fn handle_volume_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        let (width, height) = self.viewport();
        let (x, y) = self.cursor_position;
        if self.binding_capture.is_some()
            || !(self.modifiers.alt_key() || self.volume_hovered(x, y, width, height))
        {
            self.volume_scroll_remainder = 0.0;
            return false;
        }
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => f64::from(y),
            MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
        };
        if amount.is_finite() {
            self.volume_scroll_remainder += amount;
            let steps = self.volume_scroll_remainder.trunc().clamp(-20.0, 20.0) as i32;
            if steps != 0 {
                self.volume_scroll_remainder = self.volume_scroll_remainder.fract();
                self.adjust_volume(steps);
            }
        }
        true
    }

    pub(super) fn push_volume_indicator(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        let (x, y, w, h) = self.volume_indicator(width, height);
        rects.push(WgpuRect::new(x, y, w, h, rgba(0.07, 0.11, 0.15, 1.0)));
        push_text(
            rects,
            x + 8.0,
            y + 3.0,
            &self.state.settings.audio.volume_label(),
            if self.state.screen == AppScreen::Gameplay {
                2
            } else {
                1
            },
            TEXT,
        );
    }

    pub(super) fn click_volume_button(&mut self, x: f32, y: f32, width: f32, height: f32) -> bool {
        for (index, (bx, by, bw, bh)) in volume_buttons(width, height).into_iter().enumerate() {
            if (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y) {
                match index {
                    0 => self.adjust_volume(-1),
                    1 => self.adjust_volume(1),
                    _ => self.toggle_mute(),
                }
                return true;
            }
        }
        false
    }

    pub(super) fn push_volume_buttons(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        let labels = [
            "F7 QUIETER",
            "F8 LOUDER",
            if self.state.settings.audio.muted {
                "F9 UNMUTE"
            } else {
                "F9 MUTE"
            },
        ];
        for (index, (x, y, w, _)) in volume_buttons(width, height).into_iter().enumerate() {
            push_button(
                rects,
                x,
                y,
                w,
                labels[index],
                index == 2 && self.state.settings.audio.muted,
            );
        }
    }
}

fn volume_buttons(width: f32, height: f32) -> [(f32, f32, f32, f32); 3] {
    std::array::from_fn(|index| {
        (
            width * 0.5 - 270.0 + index as f32 * 190.0,
            height * 0.5 + 148.0,
            170.0,
            36.0,
        )
    })
}

#[cfg(test)]
mod tests;
