use super::*;

pub(in super::super) fn push_button(
    rects: &mut Vec<WgpuRect>,
    x: f32,
    y: f32,
    width: f32,
    label: &str,
    active: bool,
) {
    rects.push(WgpuRect::new(
        x,
        y,
        width,
        36.0,
        if active {
            rgba(0.13, 0.25, 0.31, 1.0)
        } else {
            rgba(0.08, 0.12, 0.16, 1.0)
        },
    ));
    push_text(
        rects,
        x + 12.0,
        y + 12.0,
        label,
        2,
        if active { TEXT } else { MUTED_TEXT },
    );
}

impl WgpuAppShell {
    pub(in super::super) fn context_hint(&self) -> &'static str {
        match self.state.screen {
            AppScreen::SongSelect => {
                "ARROWS OR MOUSE SELECT / F2 RANDOM / SLASH SEARCH / CTRL+A SELECT ALL / TAB ALL"
            }
            AppScreen::Gameplay => "HOLD UNTIL TAIL / ESC PAUSE / R RETRY / BACKSPACE RESULTS",
            AppScreen::Settings(_) => {
                "ARROWS SELECT AND ADJUST / ENTER CHANGE / F3 LANE KEYS / ESC LIBRARY / Q QUIT"
            }
            AppScreen::Results => "ARROWS SELECT / ENTER OR CLICK / ESC LIBRARY / Q QUIT",
            _ => "ARROWS SELECT / ENTER OPEN / F2 SETTINGS / ESC BACK / Q QUIT",
        }
    }

    pub(in super::super) fn push_navigation(&self, rects: &mut Vec<WgpuRect>, width: f32) {
        for (index, label) in ["LIBRARY", "SETTINGS", "CALIBRATE", "F1 HELP"]
            .iter()
            .enumerate()
        {
            let selected = match index {
                0 => self.state.screen == AppScreen::SongSelect,
                1 => matches!(self.state.screen, AppScreen::Settings(_)),
                2 => self.state.screen == AppScreen::Calibration,
                _ => self.help_visible,
            };
            let x = width - 432.0 + index as f32 * 104.0;
            rects.push(WgpuRect::new(
                x,
                18.0,
                96.0,
                34.0,
                if selected {
                    rgba(0.13, 0.25, 0.31, 1.0)
                } else {
                    rgba(0.08, 0.12, 0.16, 1.0)
                },
            ));
            push_text(
                rects,
                x + 6.0,
                31.0,
                label,
                1,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }

    pub(in super::super) fn push_help(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        rects.push(WgpuRect::new(
            24.0,
            84.0,
            width - 48.0,
            height - 164.0,
            rgba(0.05, 0.08, 0.12, 0.98),
        ));
        push_text(rects, 48.0, 102.0, "HOW TO PLAY / F1 CLOSE", 2, TEXT);
        let lines = [
            format!("LANES LEFT TO RIGHT: {}", self.bindings_text()),
            "TAP AT THE LINE. HOLD LONG NOTES UNTIL THEIR TAIL.".to_owned(),
            "F3 OPENS BINDINGS. ENTER A LANE TO CHANGE ITS KEY.".to_owned(),
            "LIBRARY: ARROWS, WHEEL OR CLICK. ENTER / DOUBLE CLICK PLAY.".to_owned(),
            "SLASH SEARCH / CTRL+A SELECT ALL / F2 RANDOM / R RESCAN.".to_owned(),
            "DROP A SONGS FOLDER TO ADD IT. FOLDERS ARE REMEMBERED.".to_owned(),
            "PLAY: ESC PAUSE. ENTER RESUME. R RETRY. BACKSPACE END RUN.".to_owned(),
            "F5 SLOWER / F6 FASTER. SPEED CHANGES NOTE SPACING.".to_owned(),
            "VOLUME: F7/F8 OR ALT+WHEEL. F9 / CLICK VOLUME TO MUTE.".to_owned(),
            "F10 TOGGLES SONG PREVIEWS. LEAVING THE LIBRARY STOPS THEM.".to_owned(),
            "MENUS: ESC BACK. Q QUIT. EVERYTHING STAYS IN THIS WINDOW.".to_owned(),
        ];
        let scale = if width >= 860.0 && height >= 520.0 {
            2
        } else {
            1
        };
        let step = ((height - 236.0) / lines.len() as f32).clamp(18.0, 34.0);
        for (index, line) in lines.iter().enumerate() {
            push_text(
                rects,
                48.0,
                146.0 + index as f32 * step,
                &compact_text(line, ((width - 100.0) / (6 * scale) as f32) as usize),
                scale,
                MUTED_TEXT,
            );
        }
    }

    pub(in super::super) fn gameplay_overlay(&self, width: f32, height: f32) -> Vec<WgpuRect> {
        let mut rects = Vec::new();
        rects.push(WgpuRect::new(
            0.0,
            0.0,
            width,
            66.0,
            rgba(0.04, 0.07, 0.10, 0.98),
        ));
        if let Some(session) = &self.live_session {
            let snapshot = session.snapshot();
            push_text(
                &mut rects,
                24.0,
                14.0,
                &compact_text(
                    snapshot.chart.metadata().display_title(),
                    ((width - 390.0) / 12.0).max(1.0) as usize,
                ),
                2,
                TEXT,
            );
            push_text(
                &mut rects,
                width - 344.0,
                14.0,
                &self.state.settings.gameplay.scroll_label(),
                2,
                TEXT,
            );
            let status = if let Some(countdown) = snapshot.countdown_seconds {
                format!("START IN {:.1}S", countdown)
            } else {
                format!(
                    "HITS {} / MISSES {} / {:.0}S",
                    snapshot.counts.marvelous
                        + snapshot.counts.perfect
                        + snapshot.counts.great
                        + snapshot.counts.good,
                    snapshot.counts.miss,
                    snapshot.song_time_seconds.max(0.0)
                )
            };
            push_text(&mut rects, 24.0, 42.0, &status, 2, MUTED_TEXT);
            if let Some(result) = session.recent_judgement() {
                let label = match (result.phase, result.rating) {
                    (rhythm_core::JudgementPhase::HoldTail, rhythm_core::HitRating::Miss) => {
                        "HOLD BREAK".to_owned()
                    }
                    (rhythm_core::JudgementPhase::HoldTail, _) => "HOLD OK".to_owned(),
                    (rhythm_core::JudgementPhase::HoldHead, rating)
                        if rating != rhythm_core::HitRating::Miss =>
                    {
                        format!("HOLD {rating:?}")
                    }
                    (_, rating) => format!("{rating:?}"),
                };
                push_text(
                    &mut rects,
                    width - 260.0,
                    42.0,
                    &label,
                    2,
                    if result.rating == rhythm_core::HitRating::Miss {
                        rgba(0.95, 0.40, 0.40, 1.0)
                    } else {
                        rgba(0.45, 0.90, 0.70, 1.0)
                    },
                );
            }
        }
        rects.push(WgpuRect::new(
            0.0,
            height - 40.0,
            width,
            40.0,
            rgba(0.04, 0.07, 0.10, 0.98),
        ));
        push_text(
            &mut rects,
            24.0,
            height - 28.0,
            &compact_text(
                &format!("{} / F5 SLOWER F6 FASTER / ESC PAUSE", self.bindings_text()),
                ((width - 230.0) / 12.0) as usize,
            ),
            2,
            TEXT,
        );
        self.push_pause_overlay(&mut rects, width, height);
        self.push_volume_indicator(&mut rects, width, height);
        if self.help_visible {
            self.push_help(&mut rects, width, height);
        }
        rects
    }
}
