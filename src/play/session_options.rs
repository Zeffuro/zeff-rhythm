use super::ChartFormat;
use crate::app::settings::{AppSettings, InputBackendPreference};
use crate::app::state::PlayLaunchRequest;
use crate::platform::audio::{AudioDeviceSelection, AudioStreamOptions};
use crate::platform::input::NativeInputBackendKind;
use rhythm_core::Chart;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static APP_EVENT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct PlaySessionOptions {
    pub chart_path: PathBuf,
    pub format: Option<ChartFormat>,
    pub audio_path: Option<PathBuf>,
    pub input_offset_ms: f64,
    pub max_seconds: Option<f64>,
    pub lookahead_seconds: f64,
    pub lead_in_seconds: Option<f64>,
    pub chart_start_seconds: Option<f64>,
    pub start_delay_seconds: Option<f64>,
    pub display: PlayDisplayMode,
    pub input: NativeInputBackendKind,
    pub event_log_path: Option<PathBuf>,
    pub dry_run: bool,
    pub audio: AudioStreamOptions,
}

impl PlaySessionOptions {
    pub fn from_app_launch(request: &PlayLaunchRequest) -> Result<Self, Box<dyn Error>> {
        let settings = &request.settings;

        Ok(Self {
            chart_path: request.chart.chart_path.clone(),
            format: None,
            audio_path: request.chart.audio_path.clone(),
            input_offset_ms: settings.input.input_offset_ms,
            max_seconds: None,
            lookahead_seconds: settings.video.lookahead_seconds,
            lead_in_seconds: Some(settings.gameplay.lead_in_seconds),
            chart_start_seconds: None,
            start_delay_seconds: None,
            display: PlayDisplayMode::Sdl,
            input: native_input_backend(settings),
            event_log_path: app_event_log_path(settings.diagnostics.event_log_enabled)?,
            dry_run: false,
            audio: audio_options_from_settings(settings),
        })
    }

    pub fn from_app_calibration(settings: &AppSettings) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            chart_path: PathBuf::from("generated-calibration"),
            format: None,
            audio_path: None,
            input_offset_ms: settings.input.input_offset_ms,
            max_seconds: None,
            lookahead_seconds: settings.video.lookahead_seconds.clamp(1.5, 4.0),
            lead_in_seconds: Some(3.0),
            chart_start_seconds: None,
            start_delay_seconds: Some(0.75),
            display: PlayDisplayMode::Sdl,
            input: native_input_backend(settings),
            event_log_path: app_event_log_path(settings.diagnostics.event_log_enabled)?,
            dry_run: false,
            audio: audio_options_from_settings(settings),
        })
    }

    pub fn chart_start_seconds(&self, chart: &Chart) -> f64 {
        if let Some(chart_start_seconds) = self.chart_start_seconds {
            return chart_start_seconds;
        }

        let first_note_seconds = chart
            .notes()
            .first()
            .map(|note| note.time_seconds)
            .unwrap_or_default();

        (first_note_seconds - self.effective_lead_in_seconds()).min(0.0)
    }

    pub fn effective_lead_in_seconds(&self) -> f64 {
        self.lead_in_seconds.unwrap_or(self.lookahead_seconds)
    }

    pub fn effective_start_delay_seconds(&self) -> f64 {
        self.start_delay_seconds.unwrap_or(match self.display {
            PlayDisplayMode::Highway | PlayDisplayMode::Sdl => 1.5,
            PlayDisplayMode::Log => 0.0,
        })
    }

    pub fn effective_input(&self) -> NativeInputBackendKind {
        match self.display {
            PlayDisplayMode::Sdl => NativeInputBackendKind::Sdl,
            PlayDisplayMode::Highway | PlayDisplayMode::Log => self.input,
        }
    }
}

fn native_input_backend(settings: &AppSettings) -> NativeInputBackendKind {
    match settings.input.backend {
        InputBackendPreference::Sdl => NativeInputBackendKind::Sdl,
        InputBackendPreference::TerminalDebug => NativeInputBackendKind::Terminal,
    }
}

fn audio_options_from_settings(settings: &AppSettings) -> AudioStreamOptions {
    AudioStreamOptions {
        selection: AudioDeviceSelection {
            host: settings.audio.host.clone(),
            device: settings.audio.device_id.clone(),
        },
        sample_rate: settings.audio.sample_rate,
        buffer_frames: settings.audio.buffer_frames,
    }
}

fn app_event_log_path(enabled: bool) -> Result<Option<PathBuf>, Box<dyn Error>> {
    if !enabled {
        return Ok(None);
    }

    let timestamp_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let sequence = APP_EVENT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(Some(PathBuf::from(format!(
        ".local_runs/app-session-{timestamp_ms}-{sequence}.csv"
    ))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayDisplayMode {
    Highway,
    Log,
    Sdl,
}

impl PlayDisplayMode {
    pub fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value.to_ascii_lowercase().as_str() {
            "highway" | "view" | "visual" => Ok(Self::Highway),
            "log" | "text" => Ok(Self::Log),
            "sdl" | "window" | "native" => Ok(Self::Sdl),
            _ => Err(format!("unknown display mode: {value}").into()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Highway => "highway",
            Self::Log => "log",
            Self::Sdl => "sdl",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PlayDisplayMode, PlaySessionOptions};
    use crate::app::state::{AppState, ChartSelection};
    use crate::platform::input::NativeInputBackendKind;

    #[test]
    fn builds_session_options_from_app_launch_request() {
        let mut state = AppState::new();
        state.settings.audio.host = Some("WASAPI".to_owned());
        state.settings.audio.device_id = Some("device-id".to_owned());
        state.settings.audio.sample_rate = Some(96_000);
        state.settings.audio.buffer_frames = Some(960);
        state.settings.input.input_offset_ms = 22.0;
        state.settings.video.lookahead_seconds = 3.5;
        state.select_chart(ChartSelection::new("chart.sm"));
        state.request_play().unwrap();
        let launch = state.take_pending_launch().unwrap();

        let options = PlaySessionOptions::from_app_launch(&launch).unwrap();

        assert_eq!(options.chart_path.to_string_lossy(), "chart.sm");
        assert_eq!(options.input_offset_ms, 22.0);
        assert_eq!(options.lookahead_seconds, 3.5);
        assert_eq!(options.lead_in_seconds, Some(4.0));
        assert_eq!(options.display, PlayDisplayMode::Sdl);
        assert_eq!(options.effective_input(), NativeInputBackendKind::Sdl);
        assert!(options.event_log_path.is_some());
        assert_eq!(options.audio.selection.host.as_deref(), Some("WASAPI"));
        assert_eq!(options.audio.selection.device.as_deref(), Some("device-id"));
        assert_eq!(options.audio.sample_rate, Some(96_000));
        assert_eq!(options.audio.buffer_frames, Some(960));
    }

    #[test]
    fn builds_session_options_from_app_calibration_settings() {
        let mut state = AppState::new();
        state.settings.audio.sample_rate = Some(96_000);
        state.settings.input.input_offset_ms = 18.0;

        let options = PlaySessionOptions::from_app_calibration(&state.settings).unwrap();

        assert_eq!(
            options.chart_path.to_string_lossy(),
            "generated-calibration"
        );
        assert_eq!(options.input_offset_ms, 18.0);
        assert_eq!(options.lead_in_seconds, Some(3.0));
        assert_eq!(options.start_delay_seconds, Some(0.75));
        assert!(options.event_log_path.is_some());
        assert_eq!(options.audio.sample_rate, Some(96_000));
    }

    #[test]
    fn app_event_log_paths_are_unique_for_fast_retries() {
        let state = AppState::new();

        let first = PlaySessionOptions::from_app_calibration(&state.settings).unwrap();
        let second = PlaySessionOptions::from_app_calibration(&state.settings).unwrap();

        assert_ne!(first.event_log_path, second.event_log_path);
    }

    #[test]
    fn sdl_display_forces_sdl_input() {
        let options = PlaySessionOptions {
            chart_path: "chart.sm".into(),
            format: None,
            audio_path: None,
            input_offset_ms: 0.0,
            max_seconds: None,
            lookahead_seconds: 4.0,
            lead_in_seconds: None,
            chart_start_seconds: None,
            start_delay_seconds: None,
            display: PlayDisplayMode::Sdl,
            input: NativeInputBackendKind::Terminal,
            event_log_path: None,
            dry_run: false,
            audio: Default::default(),
        };

        assert_eq!(options.effective_input(), NativeInputBackendKind::Sdl);
    }
}
