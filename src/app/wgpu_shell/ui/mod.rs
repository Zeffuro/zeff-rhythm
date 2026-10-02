use super::*;

mod help;
mod results;
mod screens;
pub(super) use help::push_button;

impl WgpuAppShell {
    pub(super) fn push_action_rows(
        &self,
        rects: &mut Vec<WgpuRect>,
        width: f32,
        height: f32,
        labels: &[String],
        selected_index: usize,
        accent: [f32; 4],
    ) {
        let row_width = 280.0_f32.min(width - 144.0);
        let x = (width - row_width - 72.0).max(96.0);
        let start_y = (height - 116.0 - labels.len() as f32 * 56.0).max(168.0);
        for (index, label) in labels.iter().enumerate() {
            let y = start_y + index as f32 * 56.0;
            let selected = index == selected_index.min(labels.len().saturating_sub(1));
            rects.push(WgpuRect::new(
                x,
                y,
                row_width,
                42.0,
                if selected {
                    rgba(0.22, 0.26, 0.31, 1.0)
                } else {
                    rgba(0.16, 0.19, 0.23, 1.0)
                },
            ));
            rects.push(WgpuRect::new(x, y, 8.0, 42.0, accent));
            push_text(
                rects,
                x + 24.0,
                y + 12.0,
                &compact_text(label, 20),
                2,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }

    pub(super) fn push_rows(
        &self,
        rects: &mut Vec<WgpuRect>,
        width: f32,
        start_y: f32,
        labels: &[String],
        selected_index: usize,
        accent: [f32; 4],
    ) {
        let row_width = (width - 160.0).min(640.0);
        let x = (width - row_width) * 0.5;
        let (_, height) = self.viewport();
        let range = visible_range(
            selected_index,
            labels.len(),
            super::interaction::menu_rows(height, start_y),
        );
        for (row, index) in range.enumerate() {
            let label = &labels[index];
            let y = start_y + row as f32 * 48.0;
            let selected = index == selected_index;
            let base = if selected {
                rgba(0.22, 0.26, 0.31, 1.0)
            } else {
                rgba(0.13, 0.16, 0.20, 1.0)
            };
            rects.push(WgpuRect::new(x, y, row_width, 40.0, base));
            rects.push(WgpuRect::new(x, y, 4.0, 40.0, accent));
            push_text(
                rects,
                x + 34.0,
                y + 13.0,
                &compact_text(label, ((row_width - 48.0) / 12.0) as usize),
                2,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }

    pub(super) fn push_panel_tabs(
        &self,
        rects: &mut Vec<WgpuRect>,
        width: f32,
        panel: SettingsPanel,
    ) {
        let panels = [
            SettingsPanel::Audio,
            SettingsPanel::Input,
            SettingsPanel::Video,
            SettingsPanel::Gameplay,
            SettingsPanel::Diagnostics,
        ];
        let panel_width = ((width - 96.0) / panels.len() as f32).max(80.0);
        for (index, candidate) in panels.iter().copied().enumerate() {
            let selected = candidate == panel;
            rects.push(WgpuRect::new(
                48.0 + index as f32 * panel_width,
                100.0,
                panel_width - 12.0,
                46.0,
                if selected {
                    panel_color(candidate)
                } else {
                    rgba(0.14, 0.17, 0.21, 1.0)
                },
            ));
            push_text(
                rects,
                60.0 + index as f32 * panel_width,
                116.0,
                panel_display_label(candidate),
                2,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }
}

pub(super) fn push_text(
    rects: &mut Vec<WgpuRect>,
    x: f32,
    y: f32,
    text: &str,
    scale: u32,
    color: [f32; 4],
) {
    for ink in &crate::render::text::line(text, scale).rects {
        rects.push(WgpuRect::new(
            x + ink.x as f32,
            y + ink.y as f32,
            ink.width as f32,
            ink.height as f32,
            crate::render::text::ink_color(ink.color, color),
        ));
    }
}

pub(super) fn compact_text(value: &str, max_chars: usize) -> String {
    crate::render::text::fit_text(value, max_chars as f32 * 12.0, 2)
}

pub(super) fn settings_rows(state: &AppState, panel: SettingsPanel) -> Vec<String> {
    let mut rows = vec![format!("PANEL {}", panel_display_label(panel))];

    match panel {
        SettingsPanel::Audio => rows.extend([
            format!(
                "HOST {}",
                state.settings.audio.host.as_deref().unwrap_or("DEFAULT")
            ),
            format!(
                "DEVICE {}",
                state
                    .settings
                    .audio
                    .device_label
                    .as_deref()
                    .or(state.settings.audio.device_id.as_deref())
                    .map(|value| compact_text(value, 22))
                    .unwrap_or_else(|| "DEFAULT".to_owned())
            ),
            format!(
                "RATE {}",
                optional_u32_text(state.settings.audio.sample_rate)
            ),
            format!(
                "BUFFER {}",
                optional_u32_text(state.settings.audio.buffer_frames)
            ),
            state.settings.audio.volume_label(),
            format!("MUTE {}", on_off(state.settings.audio.muted)),
            format!(
                "SONG PREVIEWS {}",
                on_off(state.settings.audio.song_previews)
            ),
        ]),
        SettingsPanel::Input => rows.extend([
            format!("BACKEND {:?}", state.settings.input.backend),
            format!("OFFSET {:.1} MS", state.settings.input.input_offset_ms),
            "OFFSET STEP 10 MS".to_owned(),
            format!(
                "LANE 1 KEY {} / ENTER TO REBIND",
                state.settings.input.lane_bindings[0].code
            ),
            format!(
                "LANE 2 KEY {} / ENTER TO REBIND",
                state.settings.input.lane_bindings[1].code
            ),
            format!(
                "LANE 3 KEY {} / ENTER TO REBIND",
                state.settings.input.lane_bindings[2].code
            ),
            format!(
                "LANE 4 KEY {} / ENTER TO REBIND",
                state.settings.input.lane_bindings[3].code
            ),
            "RESET KEYS TO D F J K".to_owned(),
        ]),
        SettingsPanel::Video => rows.extend([
            format!(
                "MODE {}",
                if state.settings.video.fullscreen {
                    "FULLSCREEN"
                } else {
                    "WINDOWED"
                }
            ),
            format!(
                "PRESENT {}",
                state
                    .settings
                    .video
                    .render_latency
                    .present_mode
                    .display_label()
            ),
            format!(
                "FRAME LATENCY {}",
                state
                    .settings
                    .video
                    .render_latency
                    .desired_maximum_frame_latency
            ),
            format!(
                "TARGET FPS {}",
                optional_u32_text(state.settings.video.target_frame_rate)
            ),
        ]),
        SettingsPanel::Gameplay => rows.extend([
            format!("LEAD IN {:.1} S", state.settings.gameplay.lead_in_seconds),
            state.settings.gameplay.scroll_label(),
            format!("JUDGEMENT {}", state.settings.gameplay.judgement_preset),
        ]),
        SettingsPanel::Diagnostics => rows.extend([
            format!(
                "EVENT LOG {}",
                on_off(state.settings.diagnostics.event_log_enabled)
            ),
            format!(
                "METRICS {}",
                on_off(state.settings.diagnostics.runtime_metrics_enabled)
            ),
            format!(
                "OVERLAY {}",
                on_off(state.settings.diagnostics.overlay_enabled)
            ),
        ]),
    }

    rows
}

pub(super) fn optional_u32_text(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "AUTO".to_owned())
}

pub(super) fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

pub(super) fn screen_label(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "main_menu",
        AppScreen::SongSelect => "song_select",
        AppScreen::Settings(SettingsPanel::Audio) => "settings.audio",
        AppScreen::Settings(SettingsPanel::Input) => "settings.input",
        AppScreen::Settings(SettingsPanel::Video) => "settings.video",
        AppScreen::Settings(SettingsPanel::Gameplay) => "settings.gameplay",
        AppScreen::Settings(SettingsPanel::Diagnostics) => "settings.diagnostics",
        AppScreen::Calibration => "calibration",
        AppScreen::Diagnostics => "diagnostics",
        AppScreen::Gameplay => "gameplay",
        AppScreen::Results => "results",
    }
}

pub(super) fn screen_header(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "MAIN MENU",
        AppScreen::SongSelect => "SONG SELECT",
        AppScreen::Settings(_) => "SETTINGS",
        AppScreen::Calibration => "CALIBRATION",
        AppScreen::Diagnostics => "DIAGNOSTICS",
        AppScreen::Gameplay => "GAMEPLAY",
        AppScreen::Results => "RESULTS",
    }
}

pub(super) fn panel_display_label(panel: SettingsPanel) -> &'static str {
    match panel {
        SettingsPanel::Audio => "AUDIO",
        SettingsPanel::Input => "INPUT",
        SettingsPanel::Video => "VIDEO",
        SettingsPanel::Gameplay => "GAME",
        SettingsPanel::Diagnostics => "DIAG",
    }
}

pub(super) fn selected_label(shell: &WgpuAppShell) -> String {
    match shell.state.screen {
        AppScreen::MainMenu => match MENU_ITEMS[shell.menu_index] {
            WgpuMenuItem::SongSelect => "song_select".to_owned(),
            WgpuMenuItem::Settings => "settings".to_owned(),
            WgpuMenuItem::Calibration => "calibration".to_owned(),
            WgpuMenuItem::Diagnostics => "diagnostics".to_owned(),
            WgpuMenuItem::Quit => "quit".to_owned(),
        },
        AppScreen::SongSelect => shell
            .library
            .get(shell.library_index)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "none".to_owned()),
        AppScreen::Settings(panel) => format!(
            "{} row {}",
            screen_label(AppScreen::Settings(panel)).replace("settings.", ""),
            shell.settings_row_index
        ),
        AppScreen::Calibration => "generated_click_test".to_owned(),
        AppScreen::Diagnostics => "runtime_summary".to_owned(),
        AppScreen::Gameplay => shell
            .live_session
            .as_ref()
            .map(|live_session| {
                live_session
                    .snapshot()
                    .chart
                    .metadata()
                    .display_title()
                    .to_owned()
            })
            .unwrap_or_else(|| "preview_pending".to_owned()),
        AppScreen::Results => shell
            .latest_run
            .as_ref()
            .map(|summary| summary.title.clone())
            .unwrap_or_else(|| "latest_run_pending".to_owned()),
    }
}

pub(super) fn screen_color(screen: AppScreen) -> [f32; 4] {
    match screen {
        AppScreen::MainMenu => rgba(0.38, 0.57, 0.78, 1.0),
        AppScreen::SongSelect => rgba(0.35, 0.64, 0.84, 1.0),
        AppScreen::Settings(panel) => panel_color(panel),
        AppScreen::Calibration => rgba(0.88, 0.61, 0.20, 1.0),
        AppScreen::Diagnostics => rgba(0.80, 0.38, 0.45, 1.0),
        AppScreen::Gameplay => rgba(0.43, 0.80, 0.62, 1.0),
        AppScreen::Results => rgba(0.68, 0.50, 0.82, 1.0),
    }
}

pub(super) fn panel_color(panel: SettingsPanel) -> [f32; 4] {
    match panel {
        SettingsPanel::Audio => rgba(0.35, 0.64, 0.84, 1.0),
        SettingsPanel::Input => rgba(0.43, 0.80, 0.62, 1.0),
        SettingsPanel::Video => rgba(0.88, 0.61, 0.20, 1.0),
        SettingsPanel::Gameplay => rgba(0.68, 0.50, 0.82, 1.0),
        SettingsPanel::Diagnostics => rgba(0.80, 0.38, 0.45, 1.0),
    }
}
