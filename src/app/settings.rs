#[derive(Clone, Debug, PartialEq)]
pub struct AppSettings {
    pub audio: AudioSettings,
    pub input: InputSettings,
    pub video: VideoSettings,
    pub gameplay: GameplaySettings,
    pub diagnostics: DiagnosticsSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            audio: AudioSettings::default(),
            input: InputSettings::default(),
            video: VideoSettings::default(),
            gameplay: GameplaySettings::default(),
            diagnostics: DiagnosticsSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioSettings {
    pub host: Option<String>,
    pub device_id: Option<String>,
    pub device_label: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputSettings {
    pub backend: InputBackendPreference,
    pub lane_bindings: [LaneBinding; 4],
    pub input_offset_ms: f64,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            backend: InputBackendPreference::Sdl,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBackendPreference {
    Sdl,
    TerminalDebug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputBindingSource {
    KeyboardScancode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoSettings {
    pub fullscreen: bool,
    pub vsync: bool,
    pub target_frame_rate: Option<u32>,
    pub lookahead_seconds: f64,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
            target_frame_rate: Some(60),
            lookahead_seconds: 4.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
