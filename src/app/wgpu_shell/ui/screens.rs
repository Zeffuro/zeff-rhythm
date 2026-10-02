use super::*;

impl WgpuAppShell {
    pub(in super::super) fn build_ui_rects(&self, width: f32, height: f32) -> Vec<WgpuRect> {
        let mut rects = Vec::new();
        rects.push(WgpuRect::new(
            0.0,
            0.0,
            width,
            72.0,
            rgba(0.035, 0.055, 0.075, 1.0),
        ));
        push_text(&mut rects, 32.0, 16.0, "ZEFF RHYTHM", 2, TEXT);
        push_text(
            &mut rects,
            32.0,
            44.0,
            screen_header(self.state.screen),
            2,
            MUTED_TEXT,
        );
        self.push_navigation(&mut rects, width);
        self.push_volume_indicator(&mut rects, width, height);
        rects.push(WgpuRect::new(
            0.0,
            height - 64.0,
            width,
            64.0,
            rgba(0.035, 0.055, 0.075, 1.0),
        ));
        push_text(
            &mut rects,
            40.0,
            height - 52.0,
            &compact_text(
                &format!("KEYS {} / F1 HELP / F3 BINDINGS", self.bindings_text()),
                ((width - 80.0) / 12.0) as usize,
            ),
            2,
            MUTED_TEXT,
        );
        push_text(
            &mut rects,
            40.0,
            height - 28.0,
            &compact_text(self.context_hint(), ((width - 80.0) / 12.0) as usize),
            1,
            MUTED_TEXT,
        );

        match self.state.screen {
            AppScreen::MainMenu => {
                let labels = MENU_ITEMS
                    .iter()
                    .map(|item| item.display_label().to_owned())
                    .collect::<Vec<_>>();
                self.push_rows(
                    &mut rects,
                    width,
                    122.0,
                    &labels,
                    self.menu_index,
                    screen_color(self.state.screen),
                );
            }
            AppScreen::SongSelect => self.push_song_select(&mut rects, width, height),
            AppScreen::Settings(panel) => {
                self.push_panel_tabs(&mut rects, width, panel);
                let labels = settings_rows(&self.state, panel);
                self.push_rows(
                    &mut rects,
                    width,
                    208.0,
                    &labels,
                    self.settings_row_index,
                    screen_color(self.state.screen),
                );
                push_text(
                    &mut rects,
                    80.0,
                    174.0,
                    if self.binding_capture.is_some() {
                        "PRESS A LETTER OR NUMBER / ESC CANCEL"
                    } else {
                        "ARROWS ADJUST / ENTER OR CLICK / CHANGES SAVE"
                    },
                    1,
                    TEXT,
                );
            }
            AppScreen::Calibration => {
                push_text(&mut rects, 124.0, 156.0, "CALIBRATION", 3, TEXT);
                let labels = vec![
                    "ENTER STARTS CLICK TEST".to_owned(),
                    format!(
                        "PRESS {} ON EACH CLICK",
                        self.state.settings.input.lane_bindings[0].code
                    ),
                    "RESULTS SUGGEST OFFSET".to_owned(),
                    "ESC MAIN MENU".to_owned(),
                ];
                self.push_rows(
                    &mut rects,
                    width,
                    218.0,
                    &labels,
                    0,
                    screen_color(self.state.screen),
                );
                for index in 0..1 {
                    rects.push(WgpuRect::new(
                        width * 0.5 - 28.0 + index as f32 * 92.0,
                        height * 0.50,
                        56.0,
                        56.0,
                        rgba(0.88, 0.61, 0.20, 1.0),
                    ));
                }
                self.push_calibration_status(&mut rects, width * 0.58, 156.0, 2);
            }
            AppScreen::Diagnostics => {
                self.push_diagnostics_view(&mut rects, width, height);
            }
            AppScreen::Gameplay => {
                push_text(&mut rects, 96.0, 116.0, "NO PREVIEW LOADED", 3, TEXT);
                let labels = vec![
                    "SONG SELECT THEN ENTER".to_owned(),
                    "APP-WGPU --PREVIEW".to_owned(),
                    "D F J K LANE TEST".to_owned(),
                    "ESC RESULTS".to_owned(),
                ];
                self.push_rows(
                    &mut rects,
                    width,
                    132.0,
                    &labels,
                    0,
                    screen_color(self.state.screen),
                );
            }
            AppScreen::Results => {
                self.push_results_view(&mut rects, width, height);
            }
        }

        self.push_loading(&mut rects, width, height);
        if self.help_visible {
            self.push_help(&mut rects, width, height);
        }
        rects
    }
}
