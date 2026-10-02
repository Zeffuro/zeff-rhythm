use serde::{Deserialize, Serialize};

use crate::render::settings::{RenderLatencySettings, RenderPresentModePreference};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_library_roots")]
    pub library_roots: Vec<std::path::PathBuf>,
    pub audio: AudioSettings,
    pub input: InputSettings,
    pub video: VideoSettings,
    pub gameplay: GameplaySettings,
    pub diagnostics: DiagnosticsSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            library_roots: default_library_roots(),
            audio: AudioSettings::default(),
            input: InputSettings::default(),
            video: VideoSettings::default(),
            gameplay: GameplaySettings::default(),
            diagnostics: DiagnosticsSettings::default(),
        }
    }
}

fn default_library_roots() -> Vec<std::path::PathBuf> {
    vec![std::path::PathBuf::from(".local_assets")]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSettings {
    pub host: Option<String>,
    pub device_id: Option<String>,
    pub device_label: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
    #[serde(default = "default_song_previews")]
    pub song_previews: bool,
}

fn default_song_previews() -> bool {
    true
}

fn default_volume_percent() -> u8 {
    100
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            host: None,
            device_id: None,
            device_label: None,
            sample_rate: None,
            buffer_frames: None,
            volume_percent: default_volume_percent(),
            muted: false,
            song_previews: true,
        }
    }
}

impl AudioSettings {
    pub fn gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            f32::from(self.volume_percent.min(100)) / 100.0
        }
    }

    pub fn adjust_volume(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        self.volume_percent =
            (i32::from(self.volume_percent.min(100)) + direction.signum() * 5).clamp(0, 100) as u8;
        self.muted = false;
    }

    pub fn volume_label(&self) -> String {
        format!(
            "{} {}%",
            if self.muted { "MUTED" } else { "VOLUME" },
            self.volume_percent.min(100)
        )
    }
}

#[cfg(test)]
mod volume_tests;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputSettings {
    pub backend: InputBackendPreference,
    pub lane_bindings: [LaneBinding; 4],
    pub input_offset_ms: f64,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            backend: InputBackendPreference::Winit,
            lane_bindings: [
                LaneBinding::keyboard_scancode(0, "D"),
                LaneBinding::keyboard_scancode(1, "F"),
                LaneBinding::keyboard_scancode(2, "J"),
                LaneBinding::keyboard_scancode(3, "K"),
            ],
            input_offset_ms: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputBackendPreference {
    Winit,
    Sdl,
    TerminalDebug,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneBinding {
    pub lane: u8,
    pub source: InputBindingSource,
    pub code: String,
}

impl LaneBinding {
    pub fn keyboard_scancode(lane: u8, code: &str) -> Self {
        Self {
            lane,
            source: InputBindingSource::KeyboardScancode,
            code: code.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputBindingSource {
    KeyboardScancode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoSettings {
    pub fullscreen: bool,
    pub vsync: bool,
    pub target_frame_rate: Option<u32>,
    pub render_latency: RenderLatencySettings,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
            target_frame_rate: Some(60),
            render_latency: RenderLatencySettings {
                present_mode: RenderPresentModePreference::Fifo,
                desired_maximum_frame_latency: 1,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameplaySettings {
    pub lead_in_seconds: f64,
    pub scroll_speed: f64,
    pub judgement_preset: String,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            lead_in_seconds: 4.0,
            scroll_speed: 1.0,
            judgement_preset: "default".to_owned(),
        }
    }
}

impl GameplaySettings {
    pub fn effective_scroll_speed(&self) -> f64 {
        if self.scroll_speed.is_finite() {
            self.scroll_speed.clamp(0.25, 4.0)
        } else {
            1.0
        }
    }

    pub fn scroll_time_seconds(&self) -> f64 {
        // 1x uses one second of travel, independent of song timing.
        1.0 / self.effective_scroll_speed()
    }

    pub fn adjust_scroll_speed(&mut self, direction: i32) {
        let steps = self.effective_scroll_speed() * 10.0;
        let next = match direction.cmp(&0) {
            std::cmp::Ordering::Less => ((steps - 1e-8).ceil() - 1.0) / 10.0,
            std::cmp::Ordering::Greater => ((steps + 1e-8).floor() + 1.0) / 10.0,
            std::cmp::Ordering::Equal => self.effective_scroll_speed(),
        };
        self.scroll_speed = next.clamp(0.25, 4.0);
    }

    pub fn scroll_label(&self) -> String {
        format!(
            "SCROLL {:.2}X / {:.0} MS",
            self.effective_scroll_speed(),
            self.scroll_time_seconds() * 1000.0
        )
    }
}

#[cfg(test)]
mod scroll_tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSettings {
    pub event_log_enabled: bool,
    pub runtime_metrics_enabled: bool,
    pub overlay_enabled: bool,
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            event_log_enabled: true,
            runtime_metrics_enabled: true,
            overlay_enabled: true,
        }
    }
}
