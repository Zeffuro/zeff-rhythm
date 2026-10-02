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
    pub chart_index: usize,
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
    pub volume: f32,
    pub audio: AudioStreamOptions,
}

impl PlaySessionOptions {
    pub fn from_app_launch(request: &PlayLaunchRequest) -> Result<Self, Box<dyn Error>> {
        let settings = &request.settings;

        Ok(Self {
            chart_index: request.chart.chart_index,
            chart_path: request.chart.chart_path.clone(),
            format: None,
            audio_path: request.chart.audio_path.clone(),
            input_offset_ms: settings.input.input_offset_ms,
            max_seconds: None,
            lookahead_seconds: settings.gameplay.scroll_time_seconds(),
            lead_in_seconds: Some(settings.gameplay.lead_in_seconds),
            chart_start_seconds: None,
            start_delay_seconds: None,
            display: PlayDisplayMode::AppWgpu,
            input: native_input_backend(settings),
            event_log_path: app_event_log_path(settings.diagnostics.event_log_enabled)?,
            dry_run: false,
            volume: settings.audio.gain(),
            audio: audio_options_from_settings(settings),
        })
    }

    pub fn from_app_calibration(settings: &AppSettings) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            chart_index: 0,
            chart_path: PathBuf::from("generated-calibration"),
            format: None,
            audio_path: None,
            input_offset_ms: settings.input.input_offset_ms,
            max_seconds: None,
            lookahead_seconds: settings.gameplay.scroll_time_seconds(),
            lead_in_seconds: Some(3.0),
            chart_start_seconds: None,
            start_delay_seconds: Some(0.75),
            display: PlayDisplayMode::AppWgpu,
            input: native_input_backend(settings),
            event_log_path: app_event_log_path(settings.diagnostics.event_log_enabled)?,
            dry_run: false,
            volume: settings.audio.gain(),
            audio: audio_options_from_settings(settings),
        })
    }

    pub fn from_app_launch_for_sdl_harness(
        request: &PlayLaunchRequest,
    ) -> Result<Self, Box<dyn Error>> {
        let mut options = Self::from_app_launch(request)?;
        options.display = PlayDisplayMode::Sdl;
        options.input = NativeInputBackendKind::Sdl;
        Ok(options)
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
            PlayDisplayMode::Highway | PlayDisplayMode::Sdl | PlayDisplayMode::AppWgpu => 1.5,
            PlayDisplayMode::Log => 0.0,
        })
    }

    pub fn effective_input(&self) -> NativeInputBackendKind {
        match self.display {
            PlayDisplayMode::Sdl => NativeInputBackendKind::Sdl,
            PlayDisplayMode::AppWgpu | PlayDisplayMode::Highway | PlayDisplayMode::Log => {
                self.input
            }
        }
    }

    pub(crate) fn judgement_time_seconds(&self, chart_time_seconds: f64) -> f64 {
        // Expiry and key events must advance on the same offset-adjusted timeline.
        chart_time_seconds + self.input_offset_ms / 1_000.0
    }
}

fn native_input_backend(settings: &AppSettings) -> NativeInputBackendKind {
    match settings.input.backend {
        InputBackendPreference::Winit => NativeInputBackendKind::Winit,
        InputBackendPreference::Sdl => NativeInputBackendKind::Sdl,
        InputBackendPreference::TerminalDebug => NativeInputBackendKind::Terminal,
    }
}

pub(crate) fn audio_options_from_settings(settings: &AppSettings) -> AudioStreamOptions {
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
    let config = crate::app::persistence::default_config_path();
    let directory = config
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("runs");
    Ok(Some(directory.join(format!(
        "app-session-{timestamp_ms}-{sequence}.csv"
    ))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayDisplayMode {
    AppWgpu,
    Highway,
    Log,
    Sdl,
}

impl PlayDisplayMode {
    pub fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value.to_ascii_lowercase().as_str() {
            "app_wgpu" | "app-wgpu" | "wgpu" => Ok(Self::AppWgpu),
            "highway" | "view" | "visual" => Ok(Self::Highway),
            "log" | "text" => Ok(Self::Log),
            "sdl" | "window" | "native" => Ok(Self::Sdl),
            _ => Err(format!("unknown display mode: {value}").into()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppWgpu => "app_wgpu",
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
        state.settings.gameplay.scroll_speed = 2.0;
        state.select_chart(ChartSelection::new("chart.sm"));
        state.request_play().unwrap();
        let launch = state.take_pending_launch().unwrap();

        let options = PlaySessionOptions::from_app_launch(&launch).unwrap();

        assert_eq!(options.chart_path.to_string_lossy(), "chart.sm");
        assert_eq!(options.input_offset_ms, 22.0);
        assert_eq!(options.lookahead_seconds, 0.5);
        assert_eq!(options.lead_in_seconds, Some(4.0));
        assert_eq!(options.display, PlayDisplayMode::AppWgpu);
        assert_eq!(options.effective_input(), NativeInputBackendKind::Winit);
        assert!(options.event_log_path.is_some());
        assert_eq!(options.audio.selection.host.as_deref(), Some("WASAPI"));
        assert_eq!(options.audio.selection.device.as_deref(), Some("device-id"));
        assert_eq!(options.audio.sample_rate, Some(96_000));
        assert_eq!(options.audio.buffer_frames, Some(960));
    }

    #[test]
    fn builds_sdl_harness_session_options_from_app_launch_request() {
        let mut state = AppState::new();
        state.select_chart(ChartSelection::new("chart.sm"));
        state.request_play().unwrap();
        let launch = state.take_pending_launch().unwrap();

        let options = PlaySessionOptions::from_app_launch_for_sdl_harness(&launch).unwrap();

        assert_eq!(options.display, PlayDisplayMode::Sdl);
        assert_eq!(options.effective_input(), NativeInputBackendKind::Sdl);
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
        assert_eq!(options.display, PlayDisplayMode::AppWgpu);
        assert_eq!(options.effective_input(), NativeInputBackendKind::Winit);
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
        assert!(first.event_log_path.as_ref().unwrap().is_absolute());
    }

    #[test]
    fn offset_adjusted_hold_expiry_is_independent_of_frame_cadence() {
        use crate::app::settings::AppSettings;
        use rhythm_core::{
            Chart, GameKey, HitRating, InputEvent, JudgementWindows, LaneIndex, Note, NoteId,
            RhythmEngine,
        };
        for offset_ms in [-200.0, 0.0, 200.0] {
            for render_frames in [false, true] {
                let mut options =
                    PlaySessionOptions::from_app_calibration(&AppSettings::default()).unwrap();
                options.input_offset_ms = offset_ms;
                let offset = offset_ms / 1_000.0;
                let mut chart = Chart::new(1);
                chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
                let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
                engine
                    .submit_input(InputEvent {
                        key: GameKey::Lane(LaneIndex::new(0)),
                        pressed: true,
                        time_seconds: options.judgement_time_seconds(1.0 - offset),
                    })
                    .unwrap();
                let mut results = Vec::new();
                if render_frames {
                    for raw_time in [1.5 - offset, 1.8 - offset] {
                        engine.collect_judgements(
                            options.judgement_time_seconds(raw_time),
                            &mut results,
                        );
                    }
                }
                assert!(results.is_empty());
                let release = engine
                    .submit_input(InputEvent {
                        key: GameKey::Lane(LaneIndex::new(0)),
                        pressed: false,
                        time_seconds: options.judgement_time_seconds(1.85 - offset),
                    })
                    .unwrap();
                assert_eq!(release.rating, HitRating::Miss);
                assert_eq!(engine.judged_count(), 1);
                engine.collect_judgements(options.judgement_time_seconds(3.0), &mut results);
                assert!(results.is_empty());
            }
        }
    }

    #[test]
    fn sdl_display_forces_sdl_input() {
        let options = PlaySessionOptions {
            chart_index: 0,
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
            volume: 1.0,
        };

        assert_eq!(options.effective_input(), NativeInputBackendKind::Sdl);
    }
}
