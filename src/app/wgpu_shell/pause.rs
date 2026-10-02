use super::*;

impl WgpuAppShell {
    pub(super) fn process_winit_focus(&mut self, focused: bool) -> Result<(), Box<dyn Error>> {
        let now = Instant::now();
        if let Some(session) = self.live_session.as_mut() {
            session.process_input(NativeInputEvent {
                kind: if focused {
                    NativeInputEventKind::FocusGained
                } else {
                    NativeInputEventKind::FocusLost
                },
                source: NativeInputSource::Winit,
                timestamp_kind: NativeInputTimestampKind::ReceiptMonotonic,
                event_time: now,
                received_time: now,
                source_timestamp_ns: None,
                queue_age_ms: None,
            })?;
        }
        Ok(())
    }

    pub(super) fn click_pause_button(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), Box<dyn Error>> {
        if self.click_scroll_button(x, y, width, height)
            || self.click_volume_button(x, y, width, height)
        {
            return Ok(());
        }
        let selected = pause_buttons(width, height).iter().position(|rect| {
            (rect.0..rect.0 + rect.2).contains(&x) && (rect.1..rect.1 + rect.3).contains(&y)
        });
        match selected {
            Some(0) => {
                if let Some(session) = self.live_session.as_mut() {
                    session.request_resume()?;
                }
            }
            Some(1) => self.restart_live_session()?,
            Some(2) => self.go_back(),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn push_pause_overlay(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        let Some(session) = self
            .live_session
            .as_ref()
            .filter(|session| session.is_paused())
        else {
            return;
        };
        rects.push(WgpuRect::new(
            0.0,
            66.0,
            width,
            height - 106.0,
            rgba(0.03, 0.05, 0.08, 0.92),
        ));
        let left = (width - 640.0) * 0.5;
        let top = height * 0.5 - 130.0;
        push_text(rects, left + 32.0, top, "PAUSED", 4, TEXT);
        let missing = session.missing_hold_keys();
        let needed = self
            .state
            .settings
            .input
            .lane_bindings
            .iter()
            .enumerate()
            .filter(|(lane, _)| missing.get(*lane).copied().unwrap_or(false))
            .map(|(_, binding)| binding.code.to_uppercase())
            .collect::<Vec<_>>()
            .join(" / ");
        let message = if !session.has_focus() {
            "RETURN TO THE WINDOW, THEN RESUME".to_owned()
        } else if let Some(seconds) = session.pause_countdown_seconds() {
            if seconds > 0.0 {
                format!("RESUMING IN {:.0}", seconds.ceil())
            } else if !needed.is_empty() {
                format!("HOLD {needed} TO CONTINUE")
            } else {
                "RESUMING...".to_owned()
            }
        } else {
            "ENTER / SPACE / ESC TO RESUME".to_owned()
        };
        push_text(rects, left + 32.0, top + 52.0, &message, 2, MUTED_TEXT);
        if !needed.is_empty() {
            push_text(
                rects,
                left + 32.0,
                top + 86.0,
                &format!("RE-HOLD {needed} BEFORE PLAY CONTINUES"),
                2,
                TEXT,
            );
        }
        push_text(
            rects,
            left + 32.0,
            top + 122.0,
            "R RETRY / BACKSPACE END RUN",
            2,
            MUTED_TEXT,
        );
        for (index, (x, y, w, _)) in pause_buttons(width, height).into_iter().enumerate() {
            push_button(
                rects,
                x,
                y,
                w,
                ["RESUME", "RETRY", "END RUN"][index],
                index == 0,
            );
        }
        self.push_scroll_buttons(rects, width, height);
        self.push_volume_buttons(rects, width, height);
    }
}

fn pause_buttons(width: f32, height: f32) -> [(f32, f32, f32, f32); 3] {
    std::array::from_fn(|index| {
        (
            width * 0.5 - 270.0 + index as f32 * 190.0,
            height * 0.5 + 48.0,
            170.0,
            36.0,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_mouse_targets_fit_supported_viewports_and_do_not_overlap() {
        for (width, height) in [(760.0, 520.0), (960.0, 640.0), (1280.0, 720.0)] {
            let buttons = pause_buttons(width, height);
            for (index, (x, y, w, h)) in buttons.into_iter().enumerate() {
                assert!(x >= 0.0 && x + w <= width && y >= 66.0 && y + h < height - 40.0);
                if index > 0 {
                    assert!(buttons[index - 1].0 + buttons[index - 1].2 < x);
                }
            }
        }
    }
}
