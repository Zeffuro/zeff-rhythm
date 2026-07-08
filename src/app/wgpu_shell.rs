use super::calibration::CalibrationHistory;
use super::library::AppLibrary;
use super::persistence::{AppPersistence, CalibrationDeviceKey, SavedCalibrationOffset};
use super::settings::InputBackendPreference;
use super::state::{AppScreen, AppState, SettingsPanel};
use crate::platform::audio::{AudioDeviceSelection, list_output_devices};
use crate::platform::input::{
    NativeInputEvent, NativeInputEventKind, NativeInputSource, NativeInputTimestampKind,
};
use crate::play::{
    CalibrationPattern, LivePlaySession, LiveRunSummary, PlaySessionOptions,
    build_generated_calibration_assets,
};
use crate::render::bitmap_font::{GLYPH_SPACING, GLYPH_WIDTH, glyph_rows};
use crate::render::highway::{HighwayNoteSprite, build_highway_note_sprites};
use crate::render::settings::{
    RenderLatencySettings, RenderPresentModePreference, clamp_desired_frame_latency,
};
use crate::render::wgpu_highway::{
    WgpuHighwayFrame, WgpuHighwayRenderer, wgpu_highway_render_layout,
};
use crate::render::wgpu_rects::{WgpuRect, WgpuRectFrame, WgpuRectRenderer};
use crate::render::wgpu_surface::{WgpuFrameError, WgpuSurfaceState, format_present_modes};
use pollster::block_on;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const DEFAULT_WIDTH: u32 = 960;
const DEFAULT_HEIGHT: u32 = 640;
const MENU_ITEMS: [WgpuMenuItem; 5] = [
    WgpuMenuItem::SongSelect,
    WgpuMenuItem::Settings,
    WgpuMenuItem::Calibration,
    WgpuMenuItem::Diagnostics,
    WgpuMenuItem::Quit,
];
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.020,
    g: 0.024,
    b: 0.030,
    a: 1.0,
};
const TEXT: [f32; 4] = [0.91, 0.89, 0.81, 1.0];
const MUTED_TEXT: [f32; 4] = [0.58, 0.62, 0.67, 1.0];

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let options = WgpuShellOptions::parse(args)?;
    let event_loop = EventLoop::new()?;
    let persistence = match AppPersistence::load_default() {
        Ok(persistence) => persistence,
        Err(error) => {
            println!("app_wgpu_config_load_error={error}");
            AppPersistence::disabled()
        }
    };
    let mut app = WgpuAppShell::new_with_persistence(options, persistence);

    if let Some(path) = app.persistence.path() {
        println!("app_wgpu_config={}", path.display());
    }

    if options.start_calibration {
        app.start_generated_calibration()?;
    } else if options.start_preview {
        app.prepare_selected_library_entry()?;
    }

    event_loop.run_app(&mut app)?;

    let finish_result = app.finish_live_session();

    if let Some(error) = app.error {
        return Err(error.into());
    }
    finish_result?;

    app.print_summary();
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct WgpuShellOptions {
    width: u32,
    height: u32,
    max_seconds: Option<f64>,
    start_preview: bool,
    start_calibration: bool,
    latency: RenderLatencySettings,
    latency_overridden: bool,
    power_preference: wgpu::PowerPreference,
}

impl Default for WgpuShellOptions {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            max_seconds: None,
            start_preview: false,
            start_calibration: false,
            latency: RenderLatencySettings::default(),
            latency_overridden: false,
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }
}

impl WgpuShellOptions {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self::default();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--width" => {
                    options.width = parse_u32_value(args, &mut index, "--width")?.max(1);
                }
                "--height" => {
                    options.height = parse_u32_value(args, &mut index, "--height")?.max(1);
                }
                "--max-seconds" => {
                    options.max_seconds =
                        Some(parse_f64_value(args, &mut index, "--max-seconds")?.max(0.1));
                }
                "--preview" => {
                    options.start_preview = true;
                }
                "--calibration" => {
                    options.start_calibration = true;
                }
                "--present" => {
                    let value = parse_string_value(args, &mut index, "--present")?;
                    options.latency.present_mode = parse_present_mode(&value)?;
                    options.latency_overridden = true;
                }
                "--frame-latency" => {
                    options.latency.desired_maximum_frame_latency = clamp_desired_frame_latency(
                        parse_u32_value(args, &mut index, "--frame-latency")?,
                    );
                    options.latency_overridden = true;
                }
                "--power" => {
                    let value = parse_string_value(args, &mut index, "--power")?;
                    options.power_preference = parse_power_preference(&value)?;
                }
                unknown => {
                    return Err(format!(
                        "unknown option: {unknown}. usage: zeff-rhythm app-wgpu [--preview|--calibration] [--max-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]"
                    )
                    .into());
                }
            }

            index += 1;
        }

        Ok(options)
    }
}

struct WgpuAppShell {
    options: WgpuShellOptions,
    state: AppState,
    library: AppLibrary,
    menu_index: usize,
    library_index: usize,
    settings_panel: SettingsPanel,
    settings_row_index: usize,
    pending_session_options: Option<PlaySessionOptions>,
    pending_session_source: Option<PendingSessionSource>,
    live_session: Option<LivePlaySession>,
    latest_run: Option<LiveRunSummary>,
    latest_run_source: Option<PendingSessionSource>,
    calibration_history: CalibrationHistory,
    persistence: AppPersistence,
    active_lanes: Vec<bool>,
    gpu: Option<WgpuSurfaceState>,
    ui_renderer: Option<WgpuRectRenderer>,
    highway_renderer: Option<WgpuHighwayRenderer>,
    start: Option<Instant>,
    last_redraw: Option<Instant>,
    frame_interval_ms: Vec<f64>,
    acquire_surface_ms: Vec<f64>,
    encode_submit_present_ms: Vec<f64>,
    vertices_per_frame: Vec<f64>,
    rendered_frames: u32,
    error: Option<String>,
    result_row_index: usize,
}

impl WgpuAppShell {
    #[cfg(test)]
    fn new(options: WgpuShellOptions) -> Self {
        Self::new_with_persistence(options, AppPersistence::disabled())
    }

    fn new_with_persistence(options: WgpuShellOptions, persistence: AppPersistence) -> Self {
        let library = AppLibrary::local_defaults();
        let library_index = library.first_available_index().unwrap_or(0);
        let mut state = AppState::new();
        state.settings = persistence.settings().clone();
        if options.latency_overridden || persistence.path().is_none() {
            state.settings.video.render_latency = options.latency;
            state.settings.video.vsync = !matches!(
                state.settings.video.render_latency.present_mode,
                RenderPresentModePreference::Immediate
            );
        }

        Self {
            options,
            state,
            library,
            menu_index: 0,
            library_index,
            settings_panel: SettingsPanel::Audio,
            settings_row_index: 0,
            pending_session_options: None,
            pending_session_source: None,
            live_session: None,
            latest_run: None,
            latest_run_source: None,
            calibration_history: CalibrationHistory::default(),
            persistence,
            active_lanes: Vec::new(),
            gpu: None,
            ui_renderer: None,
            highway_renderer: None,
            start: None,
            last_redraw: None,
            frame_interval_ms: Vec::new(),
            acquire_surface_ms: Vec::new(),
            encode_submit_present_ms: Vec::new(),
            vertices_per_frame: Vec::new(),
            rendered_frames: 0,
            error: None,
            result_row_index: 0,
        }
    }

    fn handle_key(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
    ) -> Result<(), Box<dyn Error>> {
        if self.state.screen == AppScreen::Gameplay {
            if let Some(lane) = key_lane(code) {
                if !repeat {
                    self.process_winit_lane_input(lane as u8, pressed)?;
                }
                self.request_redraw();
                return Ok(());
            }
        }

        if !pressed || repeat {
            return Ok(());
        }

        match code {
            KeyCode::KeyQ => {
                self.finish_live_session()?;
                event_loop.exit();
            }
            KeyCode::Escape | KeyCode::Backspace => self.go_back(),
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.move_selection(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust_or_move_panel(-1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust_or_move_panel(1),
            KeyCode::Enter | KeyCode::Space => self.confirm(event_loop)?,
            _ => {}
        }

        self.request_redraw();
        Ok(())
    }

    fn move_selection(&mut self, direction: i32) {
        match self.state.screen {
            AppScreen::MainMenu => {
                self.menu_index = cycle_index(self.menu_index, MENU_ITEMS.len(), direction);
            }
            AppScreen::SongSelect => {
                self.library_index =
                    cycle_index(self.library_index, self.library.entries().len(), direction);
            }
            AppScreen::Settings(_) => {
                self.settings_row_index = cycle_index(
                    self.settings_row_index,
                    settings_row_count(self.settings_panel) + 1,
                    direction,
                );
            }
            AppScreen::Results => {
                self.result_row_index = cycle_index(
                    self.result_row_index,
                    self.result_actions().len(),
                    direction,
                );
            }
            AppScreen::Calibration | AppScreen::Gameplay => {}
        }
    }

    fn adjust_or_move_panel(&mut self, direction: i32) {
        if let AppScreen::Settings(_) = self.state.screen {
            self.adjust_selected_setting(direction);
        }
    }

    fn confirm(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        match self.state.screen {
            AppScreen::MainMenu => match MENU_ITEMS[self.menu_index] {
                WgpuMenuItem::SongSelect => self.state.open_song_select(),
                WgpuMenuItem::Settings => {
                    self.settings_row_index = 0;
                    self.state.open_settings(self.settings_panel);
                }
                WgpuMenuItem::Calibration => self.state.open_calibration(),
                WgpuMenuItem::Diagnostics => {
                    self.settings_panel = SettingsPanel::Diagnostics;
                    self.settings_row_index = 0;
                    self.state.open_settings(self.settings_panel);
                }
                WgpuMenuItem::Quit => event_loop.exit(),
            },
            AppScreen::SongSelect => self.prepare_selected_library_entry()?,
            AppScreen::Settings(_) => self.adjust_selected_setting(1),
            AppScreen::Calibration => self.start_generated_calibration()?,
            AppScreen::Gameplay => {
                self.restart_live_session()?;
            }
            AppScreen::Results => self.confirm_results()?,
        }

        Ok(())
    }

    fn go_back(&mut self) {
        match self.state.screen {
            AppScreen::MainMenu => {}
            AppScreen::Gameplay => {
                if let Err(error) = self.finish_live_session() {
                    println!("app_wgpu_live_finish_error={error}");
                }
                self.state.screen = AppScreen::Results;
            }
            AppScreen::Results
            | AppScreen::SongSelect
            | AppScreen::Settings(_)
            | AppScreen::Calibration => self.state.open_main_menu(),
        }
    }

    fn prepare_selected_library_entry(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(entry) = self.library.get(self.library_index) else {
            return Ok(());
        };

        if !entry.is_available() {
            println!("app_wgpu_song_unavailable={}", entry.chart_path().display());
            return Ok(());
        }

        self.state.select_chart(entry.chart_selection());
        self.finish_live_session()?;
        self.state.request_play()?;
        self.start_live_session_from_pending_launch()
    }

    fn start_live_session_from_pending_launch(&mut self) -> Result<(), Box<dyn Error>> {
        let launch = self
            .state
            .take_pending_launch()
            .ok_or("app state did not create a play launch request")?;
        let session_options = PlaySessionOptions::from_app_launch(&launch)?;
        self.start_live_session(session_options, PendingSessionSource::Chart)
    }

    fn start_generated_calibration(&mut self) -> Result<(), Box<dyn Error>> {
        self.finish_live_session()?;
        let session_options = PlaySessionOptions::from_app_calibration(&self.state.settings)?;
        self.start_live_session(session_options, PendingSessionSource::Calibration)
    }

    fn start_live_session(
        &mut self,
        session_options: PlaySessionOptions,
        source: PendingSessionSource,
    ) -> Result<(), Box<dyn Error>> {
        let live_session = match source {
            PendingSessionSource::Chart => LivePlaySession::start(session_options.clone())?,
            PendingSessionSource::Calibration => {
                let assets = build_generated_calibration_assets(
                    &session_options.audio,
                    CalibrationPattern::default(),
                )?;
                LivePlaySession::start_with_assets(session_options.clone(), assets)?
            }
        };

        self.latest_run = None;
        self.latest_run_source = None;
        self.result_row_index = 0;
        self.pending_session_options = Some(session_options);
        self.pending_session_source = Some(source);
        self.live_session = Some(live_session);
        self.last_redraw = None;
        self.state.screen = AppScreen::Gameplay;
        Ok(())
    }

    fn restart_live_session(&mut self) -> Result<(), Box<dyn Error>> {
        match self.pending_session_source {
            Some(PendingSessionSource::Chart) | None if self.state.selected_chart.is_some() => {
                self.finish_live_session()?;
                self.state.request_play()?;
                self.start_live_session_from_pending_launch()
            }
            Some(PendingSessionSource::Calibration) => self.start_generated_calibration(),
            Some(source) => {
                let Some(options) = self.pending_session_options.clone() else {
                    return Ok(());
                };
                self.finish_live_session()?;
                self.start_live_session(options, source)
            }
            None => Ok(()),
        }
    }

    fn finish_live_session(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(mut live_session) = self.live_session.take() else {
            return Ok(());
        };

        let source = self.pending_session_source;
        let summary = live_session.summary();
        if source == Some(PendingSessionSource::Calibration) {
            self.calibration_history.record_run(&summary);
        }

        self.latest_run = Some(summary);
        self.latest_run_source = source;
        self.result_row_index = 0;
        live_session.print_summary()
    }

    fn confirm_results(&mut self) -> Result<(), Box<dyn Error>> {
        let action = self
            .result_actions()
            .get(self.result_row_index)
            .copied()
            .unwrap_or(ResultAction::MainMenu);

        match action {
            ResultAction::Retry => self.restart_live_session()?,
            ResultAction::ApplyOffset => {
                if let Some(total_offset_ms) = self.suggested_results_offset_ms() {
                    let total_offset_ms = total_offset_ms.clamp(-200.0, 200.0);
                    self.state.settings.input.input_offset_ms = total_offset_ms;
                    if let Some(options) = self.pending_session_options.as_mut() {
                        options.input_offset_ms = total_offset_ms;
                    }
                    self.persist_applied_offset(total_offset_ms);
                }
            }
            ResultAction::ClearCalibration => {
                self.calibration_history.clear();
                self.result_row_index = 0;
            }
            ResultAction::SongSelect => self.state.open_song_select(),
            ResultAction::MainMenu => self.state.open_main_menu(),
        }

        Ok(())
    }

    fn persist_current_settings(&mut self) {
        if let Err(error) = self.persistence.save_settings(&self.state.settings) {
            println!("app_wgpu_settings_save_error={error}");
        }
    }

    fn persist_applied_offset(&mut self, total_offset_ms: f64) {
        self.persistence.set_settings(&self.state.settings);

        if self.latest_run_source == Some(PendingSessionSource::Calibration)
            && let (Some(summary), Some(aggregate)) = (
                self.latest_run.as_ref(),
                self.calibration_history.aggregate(),
            )
        {
            let saved_offset = SavedCalibrationOffset::new(
                CalibrationDeviceKey::from_audio(&summary.audio),
                total_offset_ms,
                aggregate.hit_count,
                aggregate.trial_count,
                aggregate.confidence.display_label(),
            );

            if let Err(error) = self.persistence.upsert_calibration_offset(saved_offset) {
                println!("app_wgpu_calibration_save_error={error}");
            }
            return;
        }

        if let Err(error) = self.persistence.save() {
            println!("app_wgpu_settings_save_error={error}");
        }
    }

    fn apply_saved_calibration_for_current_settings(&mut self) {
        let key = CalibrationDeviceKey::from_settings(&self.state.settings);
        let Some(saved_offset) = self.persistence.calibration_for_key(&key) else {
            return;
        };

        self.state.settings.input.input_offset_ms =
            saved_offset.input_offset_ms.clamp(-200.0, 200.0);
    }

    fn process_winit_lane_input(&mut self, lane: u8, pressed: bool) -> Result<(), Box<dyn Error>> {
        let now = Instant::now();
        let kind = if pressed {
            NativeInputEventKind::LanePress(lane)
        } else {
            NativeInputEventKind::LaneRelease(lane)
        };
        let input = NativeInputEvent {
            kind,
            source: NativeInputSource::Winit,
            timestamp_kind: NativeInputTimestampKind::ReceiptTime,
            event_time: now,
            received_time: now,
            source_timestamp_ns: None,
            queue_age_ms: None,
        };

        if let Some(live_session) = self.live_session.as_mut() {
            live_session.process_input(input)?;
        } else {
            if lane as usize >= self.active_lanes.len() {
                self.active_lanes.resize(lane as usize + 1, false);
            }
            self.active_lanes[lane as usize] = pressed;
        }

        Ok(())
    }

    fn adjust_selected_setting(&mut self, direction: i32) {
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
            (SettingsPanel::Input, 1) => {
                self.state.settings.input.backend = match self.state.settings.input.backend {
                    InputBackendPreference::Sdl => InputBackendPreference::TerminalDebug,
                    InputBackendPreference::TerminalDebug => InputBackendPreference::Sdl,
                };
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
            (SettingsPanel::Video, 1) => {
                self.state.settings.video.fullscreen = !self.state.settings.video.fullscreen;
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
            (SettingsPanel::Video, 5) => {
                self.state.settings.video.lookahead_seconds =
                    (self.state.settings.video.lookahead_seconds + direction as f64 * 0.25)
                        .clamp(1.0, 10.0);
            }
            (SettingsPanel::Gameplay, 1) => {
                self.state.settings.gameplay.lead_in_seconds =
                    (self.state.settings.gameplay.lead_in_seconds + direction as f64 * 0.5)
                        .clamp(0.0, 10.0);
            }
            (SettingsPanel::Gameplay, 2) => {
                self.state.settings.gameplay.scroll_speed =
                    (self.state.settings.gameplay.scroll_speed + direction as f64 * 0.1)
                        .clamp(0.5, 4.0);
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

        if audio_settings_changed {
            self.apply_saved_calibration_for_current_settings();
        }
        self.persist_current_settings();
    }

    fn apply_render_latency(&mut self) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.apply_latency(self.state.settings.video.render_latency);
        }
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(live_session) = self.live_session.as_mut() {
            if let Err(error) = live_session.update() {
                self.error = Some(error.to_string());
                event_loop.exit();
                return;
            }
            if live_session.is_finished() {
                if let Err(error) = self.finish_live_session() {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                    return;
                }
                self.state.screen = AppScreen::Results;
                self.result_row_index = 0;
            }
        }

        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let width = gpu.config.width;
        let height = gpu.config.height;
        gpu.window.set_title(&self.window_title());

        let now = Instant::now();
        let mut frame_interval_ms = None;
        if let Some(previous) = self.last_redraw.replace(now) {
            let milliseconds = previous.elapsed().as_secs_f64() * 1_000.0;
            self.frame_interval_ms.push(milliseconds);
            frame_interval_ms = Some(milliseconds);
        }

        let payload = self.render_payload(width, height);
        let sample = match payload {
            WgpuAppRenderPayload::Rects(rects) => {
                let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.ui_renderer.as_mut())
                else {
                    return;
                };
                renderer
                    .render(
                        gpu,
                        WgpuRectFrame {
                            rects: &rects,
                            clear_color: CLEAR_COLOR,
                        },
                    )
                    .map(|sample| WgpuAppFrameSample {
                        acquire_surface_ms: sample.acquire_surface_ms,
                        encode_submit_present_ms: sample.encode_submit_present_ms,
                        vertex_count: sample.vertex_count,
                    })
            }
            WgpuAppRenderPayload::Highway {
                sprites,
                active_lanes,
                lane_count,
                song_time_seconds,
                end_seconds,
            } => {
                let (Some(gpu), Some(renderer)) =
                    (self.gpu.as_mut(), self.highway_renderer.as_mut())
                else {
                    return;
                };
                renderer
                    .render(
                        gpu,
                        WgpuHighwayFrame {
                            sprites: &sprites,
                            lane_count,
                            active_lanes: &active_lanes,
                            song_time_seconds,
                            end_seconds,
                        },
                    )
                    .map(|sample| WgpuAppFrameSample {
                        acquire_surface_ms: sample.acquire_surface_ms,
                        encode_submit_present_ms: sample.encode_submit_present_ms,
                        vertex_count: sample.vertex_count,
                    })
            }
        };

        match sample {
            Ok(sample) => {
                self.acquire_surface_ms.push(sample.acquire_surface_ms);
                self.encode_submit_present_ms
                    .push(sample.encode_submit_present_ms);
                self.vertices_per_frame.push(sample.vertex_count as f64);
                self.rendered_frames += 1;
                if let Some(live_session) = self.live_session.as_mut() {
                    live_session.record_render_sample(
                        frame_interval_ms.unwrap_or_default(),
                        sample.acquire_surface_ms + sample.encode_submit_present_ms,
                    );
                }
            }
            Err(WgpuFrameError::Recoverable(reason)) => {
                println!("app_wgpu_frame_skipped={reason}");
            }
            Err(WgpuFrameError::Fatal(reason)) => {
                self.error = Some(reason);
                event_loop.exit();
                return;
            }
        }

        if self.should_exit_after_frame() {
            event_loop.exit();
        } else if self.should_continue_redrawing() {
            self.request_redraw();
        }
    }

    fn render_payload(&self, width: u32, height: u32) -> WgpuAppRenderPayload {
        if self.state.screen == AppScreen::Gameplay
            && let Some(live_session) = self.live_session.as_ref()
        {
            let snapshot = live_session.snapshot();
            let layout = wgpu_highway_render_layout(
                width,
                height,
                snapshot.chart.lane_count() as usize,
                self.state.settings.video.lookahead_seconds,
            );
            let sprites = build_highway_note_sprites(
                layout,
                snapshot.chart,
                snapshot.judged_note_ids,
                snapshot.song_time_seconds,
            );
            return WgpuAppRenderPayload::Highway {
                sprites,
                active_lanes: snapshot.active_lanes.to_vec(),
                lane_count: snapshot.chart.lane_count() as usize,
                song_time_seconds: snapshot.song_time_seconds,
                end_seconds: snapshot.end_seconds,
            };
        }

        WgpuAppRenderPayload::Rects(self.build_ui_rects(width as f32, height as f32))
    }

    fn build_ui_rects(&self, width: f32, height: f32) -> Vec<WgpuRect> {
        let mut rects = Vec::new();
        rects.push(WgpuRect::new(
            0.0,
            0.0,
            width,
            72.0,
            rgba(0.11, 0.13, 0.16, 1.0),
        ));
        rects.push(WgpuRect::new(
            40.0,
            26.0,
            width * 0.34,
            12.0,
            screen_color(self.state.screen),
        ));
        rects.push(WgpuRect::new(
            40.0,
            46.0,
            width * 0.20,
            8.0,
            rgba(0.34, 0.38, 0.44, 1.0),
        ));
        push_text(&mut rects, 40.0, 20.0, "ZEFF RHYTHM", 3, TEXT);
        push_text(
            &mut rects,
            40.0,
            52.0,
            screen_header(self.state.screen),
            2,
            MUTED_TEXT,
        );
        rects.push(WgpuRect::new(
            0.0,
            height - 64.0,
            width,
            64.0,
            rgba(0.11, 0.13, 0.16, 1.0),
        ));
        push_text(
            &mut rects,
            40.0,
            height - 52.0,
            &format!(
                "OFFSET {:.1} MS   APP-WGPU",
                self.state.settings.input.input_offset_ms
            ),
            2,
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
            AppScreen::SongSelect => {
                let mut labels = self
                    .library
                    .entries()
                    .iter()
                    .map(|entry| {
                        if entry.is_available() {
                            entry.title.clone()
                        } else {
                            format!("{} MISSING", entry.title)
                        }
                    })
                    .collect::<Vec<_>>();
                if labels.is_empty() {
                    labels.push("NO CHARTS".to_owned());
                }
                self.push_rows(
                    &mut rects,
                    width,
                    132.0,
                    &labels,
                    self.library_index,
                    screen_color(self.state.screen),
                );
                if let Some(entry) = self.library.get(self.library_index) {
                    push_text(
                        &mut rects,
                        112.0,
                        300.0,
                        &compact_text(&entry.chart_path().display().to_string(), 46),
                        2,
                        MUTED_TEXT,
                    );
                }
            }
            AppScreen::Settings(panel) => {
                self.push_panel_tabs(&mut rects, width, panel);
                let labels = settings_rows(&self.state, panel);
                self.push_rows(
                    &mut rects,
                    width,
                    264.0,
                    &labels,
                    self.settings_row_index,
                    screen_color(self.state.screen),
                );
            }
            AppScreen::Calibration => {
                push_text(&mut rects, 124.0, 156.0, "CALIBRATION", 3, TEXT);
                let labels = vec![
                    "ENTER STARTS CLICK TEST".to_owned(),
                    "PRESS D ON EACH CLICK".to_owned(),
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

        rects
    }

    fn push_results_view(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        rects.push(WgpuRect::new(
            72.0,
            112.0,
            width - 144.0,
            height - 216.0,
            rgba(0.13, 0.16, 0.20, 1.0),
        ));
        push_text(rects, 96.0, 138.0, "RESULTS", 3, TEXT);

        let mut lines = Vec::new();
        if let Some(summary) = self.latest_run.as_ref() {
            lines.push(compact_text(&summary.title, 42));
            lines.push(format!(
                "JUDGED {}  HITS {}  MISSES {}",
                summary.judged_count, summary.report.hits, summary.report.misses
            ));
            lines.push(format!(
                "M {}  P {}  G {}  GOOD {}",
                summary.counts.marvelous,
                summary.counts.perfect,
                summary.counts.great,
                summary.counts.good
            ));
            if let Some(stats) = summary.report.hit_delta_ms {
                lines.push(format!(
                    "DELTA MEAN {:+.1}MS  P50 {:+.1}MS",
                    stats.mean, stats.p50
                ));
            }
            if let Some(stats) = summary.report.abs_hit_delta_ms {
                lines.push(format!(
                    "ABS MEAN {:.1}MS  P95 {:.1}MS",
                    stats.mean, stats.p95
                ));
            }
            if let Some(stats) = summary.report.input_queue_age_ms {
                lines.push(format!("INPUT QUEUE P95 {:.1}MS", stats.p95));
            } else {
                lines.push("INPUT TIMESTAMP WINIT RECEIPT".to_owned());
            }
            if let Some(stats) = summary.report.audio_output_latency_ms {
                lines.push(format!("AUDIO LEAD P50 {:.1}MS", stats.p50));
            }
            if let Some(stats) = summary.report.frame_time_ms {
                lines.push(format!("FRAME P95 {:.1}MS", stats.p95));
            }
            if let Some(total_offset_ms) = self.suggested_results_offset_ms() {
                lines.push(format!(
                    "SUGGEST OFFSET {:+.1}MS",
                    total_offset_ms.clamp(-200.0, 200.0)
                ));
            }
            if self.latest_run_source == Some(PendingSessionSource::Calibration) {
                if let Some(aggregate) = self.calibration_history.aggregate() {
                    lines.push(format!(
                        "CAL TRIALS {}  HITS {}",
                        aggregate.trial_count, aggregate.hit_count
                    ));
                    lines.push(format!(
                        "CAL OFFSET {:+.1}MS  RANGE {:.1}MS",
                        aggregate.suggested_offset_ms.clamp(-200.0, 200.0),
                        aggregate.suggested_offset_stats.max - aggregate.suggested_offset_stats.min
                    ));
                    lines.push(format!("CAL CONF {}", aggregate.confidence.display_label()));
                } else {
                    lines.push(format!(
                        "CAL ATTEMPTS {}  NO HIT SAMPLES",
                        self.calibration_history.attempts()
                    ));
                }
            }
            if let Some(path) = summary.event_log_path.as_ref() {
                lines.push(format!(
                    "LOG {}",
                    compact_text(&path.display().to_string(), 38)
                ));
            }
        } else {
            lines.push("NO RUN SUMMARY YET".to_owned());
        }

        for (index, line) in lines.iter().take(10).enumerate() {
            push_text(
                rects,
                96.0,
                204.0 + index as f32 * 30.0,
                line,
                2,
                if index == 0 { TEXT } else { MUTED_TEXT },
            );
        }

        let action_labels = self
            .result_actions()
            .iter()
            .map(|action| self.result_action_label(*action))
            .collect::<Vec<_>>();
        self.push_action_rows(
            rects,
            width,
            height,
            &action_labels,
            self.result_row_index,
            screen_color(AppScreen::Results),
        );
    }

    fn push_action_rows(
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

    fn result_actions(&self) -> Vec<ResultAction> {
        let mut actions = Vec::new();
        if self.pending_session_options.is_some() || self.state.selected_chart.is_some() {
            actions.push(ResultAction::Retry);
        }
        if self.suggested_results_offset_ms().is_some() {
            actions.push(ResultAction::ApplyOffset);
        }
        if self.latest_run_source == Some(PendingSessionSource::Calibration)
            && self.calibration_history.attempts() > 0
        {
            actions.push(ResultAction::ClearCalibration);
        }
        actions.push(ResultAction::SongSelect);
        actions.push(ResultAction::MainMenu);
        actions
    }

    fn result_action_label(&self, action: ResultAction) -> String {
        match action {
            ResultAction::Retry => "RETRY RUN".to_owned(),
            ResultAction::ApplyOffset => self
                .suggested_results_offset_ms()
                .map(|offset_ms| format!("APPLY {:+.1} MS", offset_ms.clamp(-200.0, 200.0)))
                .unwrap_or_else(|| "APPLY OFFSET".to_owned()),
            ResultAction::ClearCalibration => "CLEAR TRIALS".to_owned(),
            ResultAction::SongSelect => "SONG SELECT".to_owned(),
            ResultAction::MainMenu => "MAIN MENU".to_owned(),
        }
    }

    fn suggested_results_offset_ms(&self) -> Option<f64> {
        if self.latest_run_source == Some(PendingSessionSource::Calibration)
            && let Some(aggregate) = self.calibration_history.aggregate()
        {
            return Some(aggregate.suggested_offset_ms);
        }

        self.latest_run
            .as_ref()
            .and_then(LiveRunSummary::suggested_total_input_offset_ms)
    }

    fn push_rows(
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
        for (index, label) in labels.iter().enumerate() {
            let y = start_y + index as f32 * 72.0;
            let selected = index == selected_index;
            let base = if selected {
                rgba(0.22, 0.26, 0.31, 1.0)
            } else {
                rgba(0.13, 0.16, 0.20, 1.0)
            };
            rects.push(WgpuRect::new(x, y, row_width, 52.0, base));
            rects.push(WgpuRect::new(x, y, 10.0, 52.0, accent));
            let fill_width = if selected {
                row_width - 64.0
            } else {
                row_width * 0.52
            };
            rects.push(WgpuRect::new(
                x + 34.0,
                y + 21.0,
                fill_width.max(12.0),
                10.0,
                if selected {
                    rgba(0.86, 0.84, 0.76, 1.0)
                } else {
                    rgba(0.34, 0.38, 0.44, 1.0)
                },
            ));
            push_text(
                rects,
                x + 34.0,
                y + 16.0,
                &compact_text(label, 28),
                2,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }

    fn push_panel_tabs(&self, rects: &mut Vec<WgpuRect>, width: f32, panel: SettingsPanel) {
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
                120.0,
                panel_width - 12.0,
                72.0,
                if selected {
                    panel_color(candidate)
                } else {
                    rgba(0.14, 0.17, 0.21, 1.0)
                },
            ));
            push_text(
                rects,
                60.0 + index as f32 * panel_width,
                146.0,
                panel_display_label(candidate),
                2,
                if selected { TEXT } else { MUTED_TEXT },
            );
        }
    }

    fn window_title(&self) -> String {
        format!(
            "zeff-rhythm wgpu app | {} | {}",
            screen_label(self.state.screen),
            selected_label(self)
        )
    }

    fn request_redraw(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    fn should_continue_redrawing(&self) -> bool {
        self.options.max_seconds.is_some() || self.state.screen == AppScreen::Gameplay
    }

    fn should_exit_after_frame(&self) -> bool {
        self.options
            .max_seconds
            .zip(self.start)
            .is_some_and(|(seconds, start)| start.elapsed().as_secs_f64() >= seconds)
    }

    fn print_summary(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            println!(
                "app_wgpu_adapter name=\"{}\" backend={:?} device_type={:?}",
                gpu.adapter_info.name, gpu.adapter_info.backend, gpu.adapter_info.device_type
            );
            println!(
                "app_wgpu_surface size={}x{} format={:?}",
                gpu.config.width, gpu.config.height, gpu.config.format
            );
            println!(
                "app_wgpu_present requested={} selected={:?} supported={}",
                self.state
                    .settings
                    .video
                    .render_latency
                    .present_mode
                    .as_str(),
                gpu.config.present_mode,
                format_present_modes(&gpu.supported_present_modes)
            );
        }

        println!(
            "app_wgpu frames={} screen={}",
            self.rendered_frames,
            screen_label(self.state.screen)
        );
        crate::play::metrics::print_metric("app_wgpu_frame_interval_ms", &self.frame_interval_ms);
        crate::play::metrics::print_metric("app_wgpu_acquire_surface_ms", &self.acquire_surface_ms);
        crate::play::metrics::print_metric(
            "app_wgpu_encode_submit_present_ms",
            &self.encode_submit_present_ms,
        );
        crate::play::metrics::print_metric("app_wgpu_vertices", &self.vertices_per_frame);
    }
}

impl ApplicationHandler for WgpuAppShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("zeff-rhythm wgpu app")
            .with_inner_size(PhysicalSize::new(self.options.width, self.options.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(format!("failed to create winit window: {error}"));
                event_loop.exit();
                return;
            }
        };
        let gpu = match block_on(WgpuSurfaceState::new(
            event_loop,
            window,
            self.state.settings.video.render_latency,
            self.options.power_preference,
        )) {
            Ok(gpu) => gpu,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };
        let ui_renderer = WgpuRectRenderer::new(&gpu.device, gpu.config.format);
        let highway_renderer = WgpuHighwayRenderer::new(&gpu.device, gpu.config.format);

        println!(
            "app_wgpu=started screen={} present={} frame_latency={}",
            screen_label(self.state.screen),
            self.state
                .settings
                .video
                .render_latency
                .present_mode
                .as_str(),
            self.state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency
        );
        gpu.window.request_redraw();
        self.start = Some(Instant::now());
        self.ui_renderer = Some(ui_renderer);
        self.highway_renderer = Some(highway_renderer);
        self.gpu = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if let Err(error) = self.handle_key(event_loop, code, pressed, event.repeat) {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.render_frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.should_continue_redrawing() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

struct WgpuAppFrameSample {
    acquire_surface_ms: f64,
    encode_submit_present_ms: f64,
    vertex_count: usize,
}

enum WgpuAppRenderPayload {
    Rects(Vec<WgpuRect>),
    Highway {
        sprites: Vec<HighwayNoteSprite>,
        active_lanes: Vec<bool>,
        lane_count: usize,
        song_time_seconds: f64,
        end_seconds: f64,
    },
}

#[derive(Clone, Copy)]
enum WgpuMenuItem {
    SongSelect,
    Settings,
    Calibration,
    Diagnostics,
    Quit,
}

impl WgpuMenuItem {
    fn display_label(self) -> &'static str {
        match self {
            Self::SongSelect => "SONG SELECT",
            Self::Settings => "SETTINGS",
            Self::Calibration => "CALIBRATION",
            Self::Diagnostics => "DIAGNOSTICS",
            Self::Quit => "QUIT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingSessionSource {
    Chart,
    Calibration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultAction {
    Retry,
    ApplyOffset,
    ClearCalibration,
    SongSelect,
    MainMenu,
}

fn push_text(rects: &mut Vec<WgpuRect>, x: f32, y: f32, text: &str, scale: u32, color: [f32; 4]) {
    let scale = scale.max(1);
    let scale_f32 = scale as f32;
    let advance = (GLYPH_WIDTH + GLYPH_SPACING) as f32 * scale_f32;

    for (index, character) in text.chars().enumerate() {
        let glyph_x = x + index as f32 * advance;
        let rows = glyph_rows(character);
        for (row_index, row) in rows.iter().copied().enumerate() {
            for column in 0..GLYPH_WIDTH {
                let mask = 1_u8 << ((GLYPH_WIDTH - 1 - column) as u32);
                if row & mask == 0 {
                    continue;
                }

                rects.push(WgpuRect::new(
                    glyph_x + column as f32 * scale_f32,
                    y + row_index as f32 * scale_f32,
                    scale_f32,
                    scale_f32,
                    color,
                ));
            }
        }
    }
}

fn compact_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
}

fn settings_rows(state: &AppState, panel: SettingsPanel) -> Vec<String> {
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
        ]),
        SettingsPanel::Input => rows.extend([
            format!("BACKEND {:?}", state.settings.input.backend),
            format!("OFFSET {:.1} MS", state.settings.input.input_offset_ms),
            "OFFSET STEP 10 MS".to_owned(),
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
            format!("LOOKAHEAD {:.1} S", state.settings.video.lookahead_seconds),
        ]),
        SettingsPanel::Gameplay => rows.extend([
            format!("LEAD IN {:.1} S", state.settings.gameplay.lead_in_seconds),
            format!("SCROLL {:.1}X", state.settings.gameplay.scroll_speed),
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

fn optional_u32_text(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "AUTO".to_owned())
}

fn on_off(value: bool) -> &'static str {
    if value { "ON" } else { "OFF" }
}

fn cycle_audio_device(state: &mut AppState, direction: i32) {
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

struct AudioDeviceChoice {
    id: Option<String>,
    label: Option<String>,
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

fn key_lane(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::KeyD => Some(0),
        KeyCode::KeyF => Some(1),
        KeyCode::KeyJ => Some(2),
        KeyCode::KeyK => Some(3),
        _ => None,
    }
}

fn cycle_index(current: usize, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }

    (current as i32 + direction).rem_euclid(len as i32) as usize
}

fn settings_row_count(panel: SettingsPanel) -> usize {
    match panel {
        SettingsPanel::Audio => 4,
        SettingsPanel::Video => 5,
        SettingsPanel::Input | SettingsPanel::Gameplay | SettingsPanel::Diagnostics => 3,
    }
}

fn screen_label(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "main_menu",
        AppScreen::SongSelect => "song_select",
        AppScreen::Settings(SettingsPanel::Audio) => "settings.audio",
        AppScreen::Settings(SettingsPanel::Input) => "settings.input",
        AppScreen::Settings(SettingsPanel::Video) => "settings.video",
        AppScreen::Settings(SettingsPanel::Gameplay) => "settings.gameplay",
        AppScreen::Settings(SettingsPanel::Diagnostics) => "settings.diagnostics",
        AppScreen::Calibration => "calibration",
        AppScreen::Gameplay => "gameplay",
        AppScreen::Results => "results",
    }
}

fn screen_header(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "MAIN MENU",
        AppScreen::SongSelect => "SONG SELECT",
        AppScreen::Settings(_) => "SETTINGS",
        AppScreen::Calibration => "CALIBRATION",
        AppScreen::Gameplay => "GAMEPLAY",
        AppScreen::Results => "RESULTS",
    }
}

fn panel_display_label(panel: SettingsPanel) -> &'static str {
    match panel {
        SettingsPanel::Audio => "AUDIO",
        SettingsPanel::Input => "INPUT",
        SettingsPanel::Video => "VIDEO",
        SettingsPanel::Gameplay => "GAME",
        SettingsPanel::Diagnostics => "DIAG",
    }
}

fn selected_label(shell: &WgpuAppShell) -> String {
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
        AppScreen::Gameplay => shell
            .live_session
            .as_ref()
            .map(|live_session| live_session.snapshot().chart.metadata().title.clone())
            .unwrap_or_else(|| "preview_pending".to_owned()),
        AppScreen::Results => shell
            .latest_run
            .as_ref()
            .map(|summary| summary.title.clone())
            .unwrap_or_else(|| "latest_run_pending".to_owned()),
    }
}

fn screen_color(screen: AppScreen) -> [f32; 4] {
    match screen {
        AppScreen::MainMenu => rgba(0.38, 0.57, 0.78, 1.0),
        AppScreen::SongSelect => rgba(0.35, 0.64, 0.84, 1.0),
        AppScreen::Settings(panel) => panel_color(panel),
        AppScreen::Calibration => rgba(0.88, 0.61, 0.20, 1.0),
        AppScreen::Gameplay => rgba(0.43, 0.80, 0.62, 1.0),
        AppScreen::Results => rgba(0.68, 0.50, 0.82, 1.0),
    }
}

fn panel_color(panel: SettingsPanel) -> [f32; 4] {
    match panel {
        SettingsPanel::Audio => rgba(0.35, 0.64, 0.84, 1.0),
        SettingsPanel::Input => rgba(0.43, 0.80, 0.62, 1.0),
        SettingsPanel::Video => rgba(0.88, 0.61, 0.20, 1.0),
        SettingsPanel::Gameplay => rgba(0.68, 0.50, 0.82, 1.0),
        SettingsPanel::Diagnostics => rgba(0.80, 0.38, 0.45, 1.0),
    }
}

fn previous_panel(panel: SettingsPanel) -> SettingsPanel {
    match panel {
        SettingsPanel::Audio => SettingsPanel::Diagnostics,
        SettingsPanel::Input => SettingsPanel::Audio,
        SettingsPanel::Video => SettingsPanel::Input,
        SettingsPanel::Gameplay => SettingsPanel::Video,
        SettingsPanel::Diagnostics => SettingsPanel::Gameplay,
    }
}

fn next_panel(panel: SettingsPanel) -> SettingsPanel {
    match panel {
        SettingsPanel::Audio => SettingsPanel::Input,
        SettingsPanel::Input => SettingsPanel::Video,
        SettingsPanel::Video => SettingsPanel::Gameplay,
        SettingsPanel::Gameplay => SettingsPanel::Diagnostics,
        SettingsPanel::Diagnostics => SettingsPanel::Audio,
    }
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> [f32; 4] {
    [r, g, b, a]
}

fn parse_u32_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<u32, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    value
        .parse::<u32>()
        .map_err(|error| format!("invalid {name} value `{value}`: {error}").into())
}

fn parse_f64_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<f64, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    value
        .parse::<f64>()
        .map_err(|error| format!("invalid {name} value `{value}`: {error}").into())
}

fn parse_string_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<String, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    Ok(value.clone())
}

fn parse_present_mode(value: &str) -> Result<RenderPresentModePreference, Box<dyn Error>> {
    match value {
        "fifo" => Ok(RenderPresentModePreference::Fifo),
        "mailbox" => Ok(RenderPresentModePreference::Mailbox),
        "immediate" => Ok(RenderPresentModePreference::Immediate),
        _ => {
            Err(format!("invalid --present `{value}`; expected fifo, mailbox, or immediate").into())
        }
    }
}

fn parse_power_preference(value: &str) -> Result<wgpu::PowerPreference, Box<dyn Error>> {
    match value {
        "high" => Ok(wgpu::PowerPreference::HighPerformance),
        "low" => Ok(wgpu::PowerPreference::LowPower),
        "none" => Ok(wgpu::PowerPreference::None),
        _ => Err(format!("invalid --power `{value}`; expected high, low, or none").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingSessionSource, ResultAction, WgpuAppShell, WgpuShellOptions};
    use crate::app::settings::InputBackendPreference;
    use crate::app::state::{AppScreen, SettingsPanel};
    use crate::play::metrics::MetricStats;
    use crate::play::{JudgementCounts, LiveAudioSummary, LiveRunSummary, PlayReportSummary};
    use crate::render::settings::RenderPresentModePreference;

    #[test]
    fn cycles_audio_settings_without_device_enumeration() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.settings_panel = SettingsPanel::Audio;

        shell.settings_row_index = 1;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.audio.host.as_deref(), Some("WASAPI"));

        shell.state.settings.audio.device_id = Some("device-id".to_owned());
        shell.state.settings.audio.device_label = Some("Device".to_owned());
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.audio.host, None);
        assert_eq!(shell.state.settings.audio.device_id, None);
        assert_eq!(shell.state.settings.audio.device_label, None);

        shell.settings_row_index = 3;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.audio.sample_rate, Some(44_100));

        shell.settings_row_index = 4;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.audio.buffer_frames, Some(240));
    }

    #[test]
    fn cycles_input_video_gameplay_and_diagnostic_settings() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());

        shell.settings_panel = SettingsPanel::Input;
        shell.settings_row_index = 1;
        shell.adjust_selected_setting(1);
        assert_eq!(
            shell.state.settings.input.backend,
            InputBackendPreference::TerminalDebug
        );
        shell.settings_row_index = 2;
        shell.adjust_selected_setting(1);
        shell.settings_row_index = 3;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.input.input_offset_ms, 11.0);

        shell.settings_panel = SettingsPanel::Video;
        shell.settings_row_index = 2;
        shell.adjust_selected_setting(1);
        assert_eq!(
            shell.state.settings.video.render_latency.present_mode,
            RenderPresentModePreference::Mailbox
        );
        shell.adjust_selected_setting(1);
        assert_eq!(
            shell.state.settings.video.render_latency.present_mode,
            RenderPresentModePreference::Immediate
        );
        assert!(!shell.state.settings.video.vsync);

        shell.settings_row_index = 3;
        shell.adjust_selected_setting(1);
        assert_eq!(
            shell
                .state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency,
            2
        );
        shell.settings_row_index = 4;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.video.target_frame_rate, Some(120));

        shell.settings_panel = SettingsPanel::Gameplay;
        shell.settings_row_index = 3;
        shell.adjust_selected_setting(1);
        assert_eq!(shell.state.settings.gameplay.judgement_preset, "strict");

        shell.settings_panel = SettingsPanel::Diagnostics;
        shell.settings_row_index = 1;
        shell.adjust_selected_setting(1);
        assert!(!shell.state.settings.diagnostics.event_log_enabled);
    }

    #[test]
    fn result_apply_offset_updates_current_setting() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.state.screen = AppScreen::Results;
        shell.latest_run = Some(run_summary(22.0, -8.0, 4));

        shell.confirm_results().unwrap();

        assert_eq!(shell.state.settings.input.input_offset_ms, 30.0);
    }

    #[test]
    fn calibration_apply_offset_uses_repeated_trial_average() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.state.screen = AppScreen::Results;
        shell.latest_run_source = Some(PendingSessionSource::Calibration);
        let first = run_summary(20.0, -5.0, 10);
        let second = run_summary(24.0, 1.0, 20);
        shell.calibration_history.record_run(&first);
        shell.calibration_history.record_run(&second);
        shell.latest_run = Some(second);

        shell.confirm_results().unwrap();

        assert!((shell.state.settings.input.input_offset_ms - 23.666).abs() < 0.01);
    }

    #[test]
    fn clear_calibration_action_resets_history() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.state.screen = AppScreen::Results;
        shell.latest_run_source = Some(PendingSessionSource::Calibration);
        shell.latest_run = Some(run_summary(20.0, -5.0, 10));
        shell
            .calibration_history
            .record_run(shell.latest_run.as_ref().unwrap());
        shell.result_row_index = shell
            .result_actions()
            .iter()
            .position(|action| *action == ResultAction::ClearCalibration)
            .unwrap();

        shell.confirm_results().unwrap();

        assert_eq!(shell.calibration_history.attempts(), 0);
        assert!(shell.calibration_history.aggregate().is_none());
    }

    fn run_summary(input_offset_ms: f64, mean_delta_ms: f64, hits: usize) -> LiveRunSummary {
        LiveRunSummary {
            title: "test".to_owned(),
            input_offset_ms,
            audio: LiveAudioSummary::default(),
            judged_count: hits,
            complete: false,
            counts: JudgementCounts::default(),
            report: PlayReportSummary {
                hits,
                hit_delta_ms: Some(MetricStats {
                    count: hits,
                    mean: mean_delta_ms,
                    stddev: 1.0,
                    min: mean_delta_ms - 1.0,
                    p50: mean_delta_ms,
                    p95: mean_delta_ms + 1.0,
                    p99: mean_delta_ms + 1.0,
                    max: mean_delta_ms + 1.0,
                }),
                ..PlayReportSummary::default()
            },
            event_log_path: None,
        }
    }
}
