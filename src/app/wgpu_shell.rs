mod artwork;
mod bindings;
mod controls;
mod interaction;
mod library;
mod loading;
mod navigation;
mod options;
mod pause;
mod previews;
mod rendering;
mod scroll;
mod search_ime;
mod sessions;
mod settings;
mod ui;
mod volume;

use options::WgpuShellOptions;
use settings::*;
use ui::*;

use super::calibration::CalibrationHistory;
use super::library::{AppLibrary, LibraryScanner, visible_range};
use super::persistence::{AppPersistence, CalibrationDeviceKey, SavedCalibrationOffset};
use super::settings::InputBackendPreference;
use super::state::{AppScreen, AppState, SettingsPanel};
use crate::platform::audio::{AudioDeviceSelection, list_output_devices, output_stream_target};
use crate::platform::input::{
    NativeInputEvent, NativeInputEventKind, NativeInputSource, NativeInputTimestampKind,
};
use crate::play::{
    CalibrationPattern, LiveAudioSummary, LivePlaySession, LiveRunSummary, LiveSessionAssets,
    PlaySessionOptions, audio_options_from_settings, build_generated_calibration_assets,
    load_play_session_assets,
};
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
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId};

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
    let lock_path = super::persistence::default_config_path().with_file_name("app.lock");
    let Some(_instance) = super::instance::acquire(&lock_path)? else {
        println!("Zeff Rhythm is already running. Use its existing window.");
        return Ok(());
    };
    let mut app = WgpuAppShell::new_with_persistence(options.clone(), persistence);
    if !options.library_roots.is_empty() {
        app.state.settings.library_roots = options.library_roots.clone();
    }
    app.state.settings.library_roots =
        super::library::absolute_library_roots(&app.state.settings.library_roots)?;
    app.persist_current_settings();
    if let Some(library) = AppLibrary::cached_snapshot(&app.state.settings.library_roots) {
        app.library_index = library.first_available_index().unwrap_or(0);
        app.library = library;
    }
    app.library_scanner
        .request(app.state.settings.library_roots.clone());
    app.apply_saved_calibration_for_current_settings();

    if let Some(path) = app.persistence.path() {
        println!("app_wgpu_config={}", path.display());
    }

    if options.start_calibration {
        app.start_generated_calibration()?;
    } else if options.start_preview {
        app.library = app.library_scanner.wait();
        app.library_index = app.library.first_available_index().unwrap_or(0);
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

struct WgpuAppShell {
    options: WgpuShellOptions,
    state: AppState,
    library: AppLibrary,
    library_scanner: LibraryScanner,
    library_launch_error: Option<String>,
    asset_load: Option<loading::PendingAssetLoad>,
    preview_loader: crate::app::song_preview::PreviewLoader,
    preview_stream: Option<cpal::Stream>,
    preview_volume: crate::platform::audio::PlaybackVolume,
    preview_error: bool,
    search_query: String,
    search_active: bool,
    search_selected_all: bool,
    search_preedit: String,
    search_preedit_cursor: Option<(usize, usize)>,
    modifiers: ModifiersState,
    volume_scroll_remainder: f64,
    ime_enabled: bool,
    ready_only: bool,
    help_visible: bool,
    binding_capture: Option<usize>,
    cursor_position: (f32, f32),
    last_song_click: Option<(usize, Instant)>,
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
    resolved_audio: Option<LiveAudioSummary>,
    audio_resolver: fn(&super::settings::AppSettings) -> Option<LiveAudioSummary>,
    active_lanes: Vec<bool>,
    gpu: Option<WgpuSurfaceState>,
    ui_renderer: Option<WgpuRectRenderer>,
    highway_renderer: Option<WgpuHighwayRenderer>,
    artwork_renderer: Option<crate::render::wgpu_artwork::WgpuArtworkRenderer>,
    artwork_loader: crate::app::artwork::ArtworkLoader,
    artwork_request: Option<crate::app::artwork::ArtworkRequest>,
    start: Option<Instant>,
    last_redraw: Option<Instant>,
    frame_interval_ms: Vec<f64>,
    acquire_surface_ms: Vec<f64>,
    encode_submit_present_ms: Vec<f64>,
    vertices_per_frame: Vec<f64>,
    rendered_frames: u32,
    error: Option<String>,
    result_row_index: usize,
    window_focused: bool,
}

impl WgpuAppShell {
    #[cfg(test)]
    fn new(options: WgpuShellOptions) -> Self {
        let mut shell = Self::new_with_persistence(options, AppPersistence::disabled());
        shell.audio_resolver = |_| None;
        shell
    }

    fn new_with_persistence(options: WgpuShellOptions, persistence: AppPersistence) -> Self {
        let library = AppLibrary::default();
        let library_index = library.first_available_index().unwrap_or(0);
        let mut state = AppState::new();
        state.settings = persistence.settings().clone();
        state.settings.input.backend = InputBackendPreference::Winit;
        bindings::normalize_bindings(&mut state.settings.input);
        state.open_song_select();
        if options.latency_overridden || persistence.path().is_none() {
            state.settings.video.render_latency = options.latency;
            state.settings.video.vsync = !matches!(
                state.settings.video.render_latency.present_mode,
                RenderPresentModePreference::Immediate
            );
        }
        state.settings.input.input_offset_ms =
            persistence.manual_input_offset_ms().clamp(-200.0, 200.0);

        Self {
            options,
            state,
            library,
            library_scanner: LibraryScanner::default(),
            library_launch_error: None,
            asset_load: None,
            preview_loader: Default::default(),
            preview_stream: None,
            preview_volume: Default::default(),
            preview_error: false,
            search_query: String::new(),
            search_active: false,
            search_selected_all: false,
            search_preedit: String::new(),
            search_preedit_cursor: None,
            modifiers: ModifiersState::empty(),
            volume_scroll_remainder: 0.0,
            ime_enabled: false,
            ready_only: true,
            help_visible: false,
            binding_capture: None,
            cursor_position: (0.0, 0.0),
            last_song_click: None,
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
            resolved_audio: None,
            audio_resolver: sessions::resolve_calibration_audio,
            active_lanes: Vec::new(),
            gpu: None,
            ui_renderer: None,
            highway_renderer: None,
            artwork_renderer: None,
            artwork_loader: crate::app::artwork::ArtworkLoader::default(),
            artwork_request: None,
            start: None,
            last_redraw: None,
            frame_interval_ms: Vec::new(),
            acquire_surface_ms: Vec::new(),
            encode_submit_present_ms: Vec::new(),
            vertices_per_frame: Vec::new(),
            rendered_frames: 0,
            error: None,
            result_row_index: 0,
            window_focused: false,
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

fn cycle_index(current: usize, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }

    (current as i32 + direction).rem_euclid(len as i32) as usize
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> [f32; 4] {
    [r, g, b, a]
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod usability_tests;
