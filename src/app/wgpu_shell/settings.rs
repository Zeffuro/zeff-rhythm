use super::*;

impl WgpuAppShell {
    pub(super) fn adjust_selected_setting(&mut self, direction: i32) {
        if self.settings_row_index == 0 {
            self.settings_panel = if direction < 0 {
                previous_panel(self.settings_panel)
            } else {
                next_panel(self.settings_panel)
            };
            self.state.open_settings(self.settings_panel);
            return;
        }

        let mut audio_settings_changed = false;

        match (self.settings_panel, self.settings_row_index) {
            (SettingsPanel::Audio, 1) => {
                self.state.settings.audio.host = cycle_option_string(
                    self.state.settings.audio.host.as_deref(),
                    &[None, Some("WASAPI")],
                    direction,
                );
                self.state.settings.audio.device_id = None;
                self.state.settings.audio.device_label = None;
                audio_settings_changed = true;
            }
            (SettingsPanel::Audio, 2) => {
                cycle_audio_device(&mut self.state, direction);
                audio_settings_changed = true;
            }
            (SettingsPanel::Audio, 3) => {
                self.state.settings.audio.sample_rate = cycle_option_u32(
                    self.state.settings.audio.sample_rate,
                    &[None, Some(44_100), Some(48_000), Some(96_000)],
                    direction,
                );
                audio_settings_changed = true;
            }
            (SettingsPanel::Audio, 4) => {
                self.state.settings.audio.buffer_frames = cycle_option_u32(
                    self.state.settings.audio.buffer_frames,
                    &[None, Some(240), Some(480), Some(960), Some(1_920)],
                    direction,
                );
                audio_settings_changed = true;
            }
            (SettingsPanel::Audio, 5) => {
                self.adjust_volume(direction);
                return;
            }
            (SettingsPanel::Audio, 6) => {
                self.toggle_mute();
                return;
            }
            (SettingsPanel::Audio, 7) => {
                self.toggle_song_previews();
                return;
            }
            (SettingsPanel::Input, 1) => {
                self.state.settings.input.backend = InputBackendPreference::Winit;
            }
            (SettingsPanel::Input, 2) => {
                self.state.settings.input.input_offset_ms =
                    (self.state.settings.input.input_offset_ms + direction as f64)
                        .clamp(-200.0, 200.0);
            }
            (SettingsPanel::Input, 3) => {
                self.state.settings.input.input_offset_ms =
                    (self.state.settings.input.input_offset_ms + direction as f64 * 10.0)
                        .clamp(-200.0, 200.0);
            }
            (SettingsPanel::Input, 4..=7) => {
                self.binding_capture = Some(self.settings_row_index - 4);
            }
            (SettingsPanel::Input, 8) => {
                self.state.settings.input.lane_bindings =
                    super::super::settings::InputSettings::default().lane_bindings;
            }
            (SettingsPanel::Video, 1) => {
                self.state.settings.video.fullscreen = !self.state.settings.video.fullscreen;
                if let Some(gpu) = &self.gpu {
                    gpu.window.set_fullscreen(
                        self.state
                            .settings
                            .video
                            .fullscreen
                            .then_some(winit::window::Fullscreen::Borderless(None)),
                    );
                }
            }
            (SettingsPanel::Video, 2) => {
                self.state.settings.video.render_latency.present_mode = if direction < 0 {
                    self.state
                        .settings
                        .video
                        .render_latency
                        .present_mode
                        .previous()
                } else {
                    self.state.settings.video.render_latency.present_mode.next()
                };
                self.state.settings.video.vsync = !matches!(
                    self.state.settings.video.render_latency.present_mode,
                    RenderPresentModePreference::Immediate
                );
                self.apply_render_latency();
            }
            (SettingsPanel::Video, 3) => {
                let current = self
                    .state
                    .settings
                    .video
                    .render_latency
                    .desired_maximum_frame_latency;
                let next = (current as i32 - 1 + direction).rem_euclid(3) as u32 + 1;
                self.state
                    .settings
                    .video
                    .render_latency
                    .desired_maximum_frame_latency = clamp_desired_frame_latency(next);
                self.apply_render_latency();
            }
            (SettingsPanel::Video, 4) => {
                self.state.settings.video.target_frame_rate = cycle_option_u32(
                    self.state.settings.video.target_frame_rate,
                    &[None, Some(60), Some(120), Some(144), Some(240)],
                    direction,
                );
            }
            (SettingsPanel::Gameplay, 1) => {
                self.state.settings.gameplay.lead_in_seconds =
                    (self.state.settings.gameplay.lead_in_seconds + direction as f64 * 0.5)
                        .clamp(0.0, 10.0);
            }
            (SettingsPanel::Gameplay, 2) => {
                self.adjust_scroll_speed(direction);
                return;
            }
            (SettingsPanel::Gameplay, 3) => {
                self.state.settings.gameplay.judgement_preset = cycle_string(
                    &self.state.settings.gameplay.judgement_preset,
                    &["default", "strict"],
                    direction,
                );
            }
            (SettingsPanel::Diagnostics, 1) => {
                self.state.settings.diagnostics.event_log_enabled =
                    !self.state.settings.diagnostics.event_log_enabled;
            }
            (SettingsPanel::Diagnostics, 2) => {
                self.state.settings.diagnostics.runtime_metrics_enabled =
                    !self.state.settings.diagnostics.runtime_metrics_enabled;
            }
            (SettingsPanel::Diagnostics, 3) => {
                self.state.settings.diagnostics.overlay_enabled =
                    !self.state.settings.diagnostics.overlay_enabled;
            }
            _ => {}
        }

        if self.settings_panel == SettingsPanel::Input && matches!(self.settings_row_index, 2 | 3) {
            self.persistence
                .set_manual_input_offset_ms(self.state.settings.input.input_offset_ms);
        }
        if audio_settings_changed {
            self.apply_saved_calibration_for_current_settings();
        }
        self.persist_current_settings();
    }

    pub(super) fn apply_render_latency(&mut self) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.apply_latency(self.state.settings.video.render_latency);
        }
    }
}

pub(super) fn cycle_audio_device(state: &mut AppState, direction: i32) {
    let selection = AudioDeviceSelection {
        host: state.settings.audio.host.clone(),
        device: None,
    };
    let devices = match list_output_devices(&selection) {
        Ok(devices) => devices,
        Err(error) => {
            println!("app_wgpu_audio_device_list_error={error}");
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

pub(super) struct AudioDeviceChoice {
    id: Option<String>,
    label: Option<String>,
}

pub(super) fn cycle_option_u32(
    current: Option<u32>,
    values: &[Option<u32>],
    direction: i32,
) -> Option<u32> {
    values[cycle_position(
        values.iter().position(|value| *value == current),
        values.len(),
        direction,
    )]
}

pub(super) fn cycle_option_string(
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

pub(super) fn cycle_string(current: &str, values: &[&str], direction: i32) -> String {
    values[cycle_position(
        values.iter().position(|value| *value == current),
        values.len(),
        direction,
    )]
    .to_owned()
}

pub(super) fn cycle_position(position: Option<usize>, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }

    (position.unwrap_or_default() as i32 + direction).rem_euclid(len as i32) as usize
}

pub(super) fn settings_row_count(panel: SettingsPanel) -> usize {
    if panel == SettingsPanel::Input {
        return 8;
    }
    match panel {
        SettingsPanel::Audio => 7,
        SettingsPanel::Video => 4,
        SettingsPanel::Input | SettingsPanel::Gameplay | SettingsPanel::Diagnostics => 3,
    }
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
