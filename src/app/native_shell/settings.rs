use super::*;

fn panel_label(panel: SettingsPanel) -> &'static str {
    match panel {
        SettingsPanel::Audio => "audio",
        SettingsPanel::Input => "input",
        SettingsPanel::Video => "video",
        SettingsPanel::Gameplay => "gameplay",
        SettingsPanel::Diagnostics => "diagnostics",
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

pub(super) fn settings_selected_label(panel: SettingsPanel, row_index: usize) -> String {
    if row_index == 0 {
        return format!("{} PANEL", panel_label(panel));
    }

    format!(
        "{} {}",
        panel_label(panel),
        settings_row_label(panel, row_index)
    )
}

fn settings_row_label(panel: SettingsPanel, row_index: usize) -> &'static str {
    match (panel, row_index) {
        (SettingsPanel::Audio, 1) => "HOST",
        (SettingsPanel::Audio, 2) => "DEVICE",
        (SettingsPanel::Audio, 3) => "RATE",
        (SettingsPanel::Audio, 4) => "BUFFER",
        (SettingsPanel::Input, 1) => "BACKEND",
        (SettingsPanel::Input, 2) => "OFFSET",
        (SettingsPanel::Input, 3) => "OFFSET COARSE",
        (SettingsPanel::Video, 1) => "MODE",
        (SettingsPanel::Video, 2) => "PRESENT",
        (SettingsPanel::Video, 3) => "LATENCY",
        (SettingsPanel::Video, 4) => "FPS",
        (SettingsPanel::Gameplay, 1) => "LEAD IN",
        (SettingsPanel::Gameplay, 2) => "SCROLL",
        (SettingsPanel::Gameplay, 3) => "JUDGEMENT",
        (SettingsPanel::Diagnostics, 1) => "EVENT LOG",
        (SettingsPanel::Diagnostics, 2) => "METRICS",
        (SettingsPanel::Diagnostics, 3) => "OVERLAY",
        _ => "PANEL",
    }
}

pub(super) fn settings_row_count(panel: SettingsPanel) -> usize {
    match panel {
        SettingsPanel::Audio => 4,
        SettingsPanel::Video => 4,
        SettingsPanel::Input | SettingsPanel::Gameplay | SettingsPanel::Diagnostics => 3,
    }
}

pub(super) fn draw_settings_body(
    canvas: &mut Canvas<Window>,
    state: &AppState,
    panel: SettingsPanel,
    width: u32,
    selected_row_index: usize,
) -> Result<(), Box<dyn Error>> {
    let x = 84;
    draw_text(canvas, x, 260, panel_display_label(panel), 3, TEXT)?;
    let rows: Vec<String> = match panel {
        SettingsPanel::Audio => vec![
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
                    .map(compact_label)
                    .as_deref()
                    .unwrap_or("DEFAULT")
            ),
            format!(
                "RATE {}",
                optional_u32_text(state.settings.audio.sample_rate)
            ),
            format!(
                "BUFFER {}",
                optional_u32_text(state.settings.audio.buffer_frames)
            ),
        ],
        SettingsPanel::Input => vec![
            format!("BACKEND {:?}", state.settings.input.backend),
            format!("OFFSET {:.3} MS", state.settings.input.input_offset_ms),
            "OFFSET STEP 10 MS".to_owned(),
        ],
        SettingsPanel::Video => vec![
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
        ],
        SettingsPanel::Gameplay => vec![
            format!("LEAD IN {:.1} S", state.settings.gameplay.lead_in_seconds),
            state.settings.gameplay.scroll_label(),
            format!("JUDGEMENT {}", state.settings.gameplay.judgement_preset),
        ],
        SettingsPanel::Diagnostics => vec![
            format!(
                "EVENT LOG {}",
                if state.settings.diagnostics.event_log_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            format!(
                "METRICS {}",
                if state.settings.diagnostics.runtime_metrics_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            ),
            format!(
                "OVERLAY {}",
                if state.settings.diagnostics.overlay_enabled {
                    "ON"
                } else {
                    "OFF"
                }
            ),
        ],
    };

    let row_width = width.saturating_sub(168);
    for (index, row) in rows.iter().enumerate() {
        let selected = selected_row_index == index + 1;
        let y = 320 + index as i32 * 44;
        canvas.set_draw_color(if selected {
            Color::RGB(58, 66, 78)
        } else {
            Color::RGB(42, 49, 58)
        });
        canvas.fill_rect(Rect::new(x, y - 8, row_width, 30))?;
        if selected {
            canvas.set_draw_color(panel_color(panel));
            canvas.fill_rect(Rect::new(x, y - 8, 8, 30))?;
        }
        draw_text(
            canvas,
            x + 20,
            y,
            row,
            2,
            if selected { TEXT } else { MUTED_TEXT },
        )?;
    }

    Ok(())
}

fn compact_label(value: &str) -> String {
    const MAX_CHARS: usize = 34;
    if value.chars().count() <= MAX_CHARS {
        return value.to_owned();
    }

    value.chars().take(MAX_CHARS - 3).collect::<String>() + "..."
}

pub(super) fn adjust_setting(
    state: &mut AppState,
    panel: SettingsPanel,
    row_index: usize,
    direction: i32,
) {
    match (panel, row_index) {
        (SettingsPanel::Audio, 1) => {
            state.settings.audio.host = cycle_option_string(
                state.settings.audio.host.as_deref(),
                &[None, Some("WASAPI")],
                direction,
            );
            state.settings.audio.device_id = None;
            state.settings.audio.device_label = None;
        }
        (SettingsPanel::Audio, 2) => cycle_audio_device(state, direction),
        (SettingsPanel::Audio, 3) => {
            state.settings.audio.sample_rate = cycle_option_u32(
                state.settings.audio.sample_rate,
                &[None, Some(44_100), Some(48_000), Some(96_000)],
                direction,
            );
        }
        (SettingsPanel::Audio, 4) => {
            state.settings.audio.buffer_frames = cycle_option_u32(
                state.settings.audio.buffer_frames,
                &[None, Some(240), Some(480), Some(960), Some(1_920)],
                direction,
            );
        }
        (SettingsPanel::Input, 1) => {
            state.settings.input.backend = match state.settings.input.backend {
                super::super::settings::InputBackendPreference::Winit => {
                    super::super::settings::InputBackendPreference::Sdl
                }
                super::super::settings::InputBackendPreference::Sdl => {
                    super::super::settings::InputBackendPreference::TerminalDebug
                }
                super::super::settings::InputBackendPreference::TerminalDebug => {
                    super::super::settings::InputBackendPreference::Winit
                }
            };
        }
        (SettingsPanel::Input, 2) => {
            state.settings.input.input_offset_ms =
                (state.settings.input.input_offset_ms + direction as f64).clamp(-200.0, 200.0);
        }
        (SettingsPanel::Input, 3) => {
            state.settings.input.input_offset_ms = (state.settings.input.input_offset_ms
                + direction as f64 * 10.0)
                .clamp(-200.0, 200.0);
        }
        (SettingsPanel::Video, 1) => {
            state.settings.video.fullscreen = !state.settings.video.fullscreen;
        }
        (SettingsPanel::Video, 2) => {
            state.settings.video.render_latency.present_mode = if direction < 0 {
                state.settings.video.render_latency.present_mode.previous()
            } else {
                state.settings.video.render_latency.present_mode.next()
            };
            state.settings.video.vsync = !matches!(
                state.settings.video.render_latency.present_mode,
                crate::render::settings::RenderPresentModePreference::Immediate
            );
        }
        (SettingsPanel::Video, 3) => {
            let current = state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency;
            let next = (current as i32 - 1 + direction).rem_euclid(3) as u32 + 1;
            state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency = clamp_desired_frame_latency(next);
        }
        (SettingsPanel::Video, 4) => {
            state.settings.video.target_frame_rate = cycle_option_u32(
                state.settings.video.target_frame_rate,
                &[None, Some(60), Some(120), Some(144), Some(240)],
                direction,
            );
        }
        (SettingsPanel::Gameplay, 1) => {
            state.settings.gameplay.lead_in_seconds =
                (state.settings.gameplay.lead_in_seconds + direction as f64 * 0.5).clamp(0.0, 10.0);
        }
        (SettingsPanel::Gameplay, 2) => {
            state.settings.gameplay.adjust_scroll_speed(direction);
        }
        (SettingsPanel::Gameplay, 3) => {
            state.settings.gameplay.judgement_preset = cycle_string(
                &state.settings.gameplay.judgement_preset,
                &["default", "strict"],
                direction,
            );
        }
        (SettingsPanel::Diagnostics, 1) => {
            state.settings.diagnostics.event_log_enabled =
                !state.settings.diagnostics.event_log_enabled;
        }
        (SettingsPanel::Diagnostics, 2) => {
            state.settings.diagnostics.runtime_metrics_enabled =
                !state.settings.diagnostics.runtime_metrics_enabled;
        }
        (SettingsPanel::Diagnostics, 3) => {
            state.settings.diagnostics.overlay_enabled =
                !state.settings.diagnostics.overlay_enabled;
        }
        _ => {}
    }
}

fn cycle_audio_device(state: &mut AppState, direction: i32) {
    let selection = AudioDeviceSelection {
        host: state.settings.audio.host.clone(),
        device: None,
    };
    let devices = match list_output_devices(&selection) {
        Ok(devices) => devices,
        Err(error) => {
            println!("app_audio_device_list_error={error}");
            return;
        }
    };

    if devices.is_empty() {
        state.settings.audio.device_id = None;
        state.settings.audio.device_label = None;
        return;
    }

    let mut choices = Vec::with_capacity(devices.len() + 1);
    choices.push(AudioDeviceChoice {
        id: None,
        label: None,
    });
    for device in devices {
        choices.push(AudioDeviceChoice {
            id: Some(device.id.unwrap_or_else(|| device.name.clone())),
            label: Some(if device.is_default {
                format!("{} (DEFAULT)", device.name)
            } else {
                device.name
            }),
        });
    }

    let current = state.settings.audio.device_id.as_deref();
    let next = cycle_position(
        choices
            .iter()
            .position(|choice| choice.id.as_deref() == current),
        choices.len(),
        direction,
    );

    state.settings.audio.device_id = choices[next].id.clone();
    state.settings.audio.device_label = choices[next].label.clone();
}

struct AudioDeviceChoice {
    id: Option<String>,
    label: Option<String>,
}

fn optional_u32_text(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "AUTO".to_owned())
}

fn cycle_option_u32(current: Option<u32>, values: &[Option<u32>], direction: i32) -> Option<u32> {
    values[cycle_position(
        values.iter().position(|value| *value == current),
        values.len(),
        direction,
    )]
}

fn cycle_option_string(
    current: Option<&str>,
    values: &[Option<&str>],
    direction: i32,
) -> Option<String> {
    values[cycle_position(
        values.iter().position(|value| *value == current),
        values.len(),
        direction,
    )]
    .map(str::to_owned)
}

fn cycle_string(current: &str, values: &[&str], direction: i32) -> String {
    values[cycle_position(
        values.iter().position(|value| *value == current),
        values.len(),
        direction,
    )]
    .to_owned()
}

fn cycle_position(position: Option<usize>, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }

    (position.unwrap_or_default() as i32 + direction).rem_euclid(len as i32) as usize
}

pub(super) fn previous_panel(panel: SettingsPanel) -> SettingsPanel {
    match panel {
        SettingsPanel::Audio => SettingsPanel::Diagnostics,
        SettingsPanel::Input => SettingsPanel::Audio,
        SettingsPanel::Video => SettingsPanel::Input,
        SettingsPanel::Gameplay => SettingsPanel::Video,
        SettingsPanel::Diagnostics => SettingsPanel::Gameplay,
    }
}

pub(super) fn next_panel(panel: SettingsPanel) -> SettingsPanel {
    match panel {
        SettingsPanel::Audio => SettingsPanel::Input,
        SettingsPanel::Input => SettingsPanel::Video,
        SettingsPanel::Video => SettingsPanel::Gameplay,
        SettingsPanel::Gameplay => SettingsPanel::Diagnostics,
        SettingsPanel::Diagnostics => SettingsPanel::Audio,
    }
}
