mod library;
mod rendering;
mod settings;
mod ui;

use settings::*;
use ui::*;

use super::library::{AppLibrary, LibraryScanner, library_roots_from_args, visible_range};
use super::persistence::AppPersistence;
use super::state::{AppScreen, AppState, SettingsPanel};
use super::ui_text::{draw_text, text_height};
use crate::platform::audio::{AudioDeviceSelection, list_output_devices};
use crate::play::{
    PlaySessionOptions, PlaySessionPreview, PreparedPlaySessionSummary, WgpuPreviewRunOptions,
    load_play_session_preview, prepare_play_session, run_play_session, run_wgpu_preview,
};
use crate::render::highway::{
    HighwayNoteSpriteKind, HighwayRenderLayout, build_highway_note_sprites,
};
use crate::render::settings::clamp_desired_frame_latency;
use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Scancode};
use sdl3::pixels::Color;
use sdl3::rect::Rect;
use sdl3::render::Canvas;
use sdl3::video::Window;
use std::collections::HashSet;
use std::error::Error;
use std::time::{Duration, Instant};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 640;
const TEXT: Color = Color::RGB(232, 228, 212);
const MUTED_TEXT: Color = Color::RGB(146, 154, 166);
const MENU_ITEMS: [MenuItem; 5] = [
    MenuItem::SongSelect,
    MenuItem::Settings,
    MenuItem::Calibration,
    MenuItem::Diagnostics,
    MenuItem::Quit,
];

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let mut persistence = AppPersistence::load_default()?;
    let roots = library_roots_from_args(args)?;
    let roots = if roots.is_empty() {
        persistence.settings().library_roots.clone()
    } else {
        let mut settings = persistence.settings().clone();
        settings.library_roots = roots.clone();
        persistence.save_settings(&settings)?;
        roots
    };
    let mut shell = NativeAppShell::new(roots);

    loop {
        match run_menu_window(&mut shell)? {
            ShellExit::Quit => return Ok(()),
            ShellExit::Play(options) => {
                println!(
                    "app_play_start chart={} event_log={}",
                    options.chart_path.display(),
                    options
                        .event_log_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_owned())
                );

                match run_play_session(options) {
                    Ok(()) => {
                        println!("app_play_finished=true");
                        shell.state.screen = AppScreen::Results;
                    }
                    Err(error) => {
                        println!("app_play_error={error}");
                        shell.state.open_song_select();
                    }
                }
            }
            ShellExit::Preview(options) => {
                let latency = shell.state.settings.video.render_latency;
                let chart_path = options.chart_path.clone();
                let run_options = WgpuPreviewRunOptions::new(options).with_latency(latency);

                println!(
                    "app_wgpu_preview_start chart={} present={} frame_latency={}",
                    chart_path.display(),
                    latency.present_mode.as_str(),
                    latency.desired_maximum_frame_latency
                );

                match run_wgpu_preview(run_options) {
                    Ok(()) => {
                        println!("app_wgpu_preview_finished=true");
                        shell.state.screen = AppScreen::Results;
                    }
                    Err(error) => {
                        println!("app_wgpu_preview_error={error}");
                        shell.state.open_song_select();
                    }
                }
            }
        }
    }
}

fn run_menu_window(shell: &mut NativeAppShell) -> Result<ShellExit, Box<dyn Error>> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window = video
        .window("zeff-rhythm", WIDTH, HEIGHT)
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    window.set_minimum_size(640, 400)?;
    let mut canvas = window.into_canvas();
    let mut event_pump = sdl.event_pump()?;

    println!("app=window");
    println!(
        "controls=up/down or w/s to select, enter/space to open, esc/backspace to go back, q to quit"
    );

    loop {
        shell.refresh_library();
        for event in event_pump.poll_iter() {
            if let Some(exit) = shell.handle_event(event)? {
                return Ok(exit);
            }
        }

        shell.render(&mut canvas)?;
        std::thread::sleep(Duration::from_millis(16));
    }
}

struct NativeAppShell {
    state: AppState,
    library: AppLibrary,
    library_scanner: LibraryScanner,
    library_launch_error: Option<String>,
    menu_index: usize,
    library_index: usize,
    settings_panel: SettingsPanel,
    settings_row_index: usize,
    pending_session_options: Option<PlaySessionOptions>,
    pending_session_summary: Option<PreparedPlaySessionSummary>,
    pending_session_preview: Option<PlaySessionPreview>,
    last_title_update: Instant,
}

enum ShellExit {
    Quit,
    Play(PlaySessionOptions),
    Preview(PlaySessionOptions),
}

impl NativeAppShell {
    fn new(roots: Vec<std::path::PathBuf>) -> Self {
        let library = AppLibrary::default();
        let library_index = library.first_available_index().unwrap_or(0);
        let mut state = AppState::new();
        state.settings.library_roots = roots.clone();
        let mut library_scanner = LibraryScanner::default();
        library_scanner.request(roots);

        Self {
            state,
            library,
            library_scanner,
            library_launch_error: None,
            menu_index: 0,
            library_index,
            settings_panel: SettingsPanel::Audio,
            settings_row_index: 0,
            pending_session_options: None,
            pending_session_summary: None,
            pending_session_preview: None,
            last_title_update: Instant::now() - Duration::from_secs(1),
        }
    }

    fn handle_event(&mut self, event: Event) -> Result<Option<ShellExit>, Box<dyn Error>> {
        match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: sdl3::event::WindowEvent::CloseRequested,
                ..
            } => return Ok(Some(ShellExit::Quit)),
            Event::KeyDown {
                keycode,
                scancode,
                repeat,
                ..
            } if !repeat => return self.handle_key(keycode, scancode),
            Event::DropFile { filename, .. } => {
                let path = std::path::PathBuf::from(filename);
                if !self.state.settings.library_roots.contains(&path) {
                    self.state.settings.library_roots.push(path);
                }
                self.library_scanner
                    .request(self.state.settings.library_roots.clone());
                let mut persistence = AppPersistence::load_default()?;
                let mut settings = persistence.settings().clone();
                settings.library_roots = self.state.settings.library_roots.clone();
                persistence.save_settings(&settings)?;
                if self.state.screen != AppScreen::Gameplay {
                    self.state.open_song_select();
                }
            }
            _ => {}
        }

        Ok(None)
    }

    fn handle_key(
        &mut self,
        keycode: Option<Keycode>,
        scancode: Option<Scancode>,
    ) -> Result<Option<ShellExit>, Box<dyn Error>> {
        if matches_key(keycode, scancode, KeyIntent::Quit) {
            return Ok(Some(ShellExit::Quit));
        }

        if matches_key(keycode, scancode, KeyIntent::Back) {
            self.go_back();
            return Ok(None);
        }

        match self.state.screen {
            AppScreen::MainMenu => {
                if matches_key(keycode, scancode, KeyIntent::Up) {
                    self.menu_index = self.menu_index.saturating_sub(1);
                } else if matches_key(keycode, scancode, KeyIntent::Down) {
                    self.menu_index = (self.menu_index + 1).min(MENU_ITEMS.len() - 1);
                } else if matches_key(keycode, scancode, KeyIntent::Confirm) {
                    return self.activate_menu_item();
                }
            }
            AppScreen::Settings(_) => {
                if matches_key(keycode, scancode, KeyIntent::Up) {
                    self.settings_row_index = self.settings_row_index.saturating_sub(1);
                } else if matches_key(keycode, scancode, KeyIntent::Down) {
                    self.settings_row_index =
                        (self.settings_row_index + 1).min(settings_row_count(self.settings_panel));
                } else if matches_key(keycode, scancode, KeyIntent::Left) {
                    self.adjust_selected_setting(-1);
                } else if matches_key(keycode, scancode, KeyIntent::Right)
                    || matches_key(keycode, scancode, KeyIntent::Confirm)
                {
                    self.adjust_selected_setting(1);
                }
            }
            AppScreen::SongSelect => {
                self.library_launch_error = None;
                match scancode {
                    Some(Scancode::R) => {
                        self.library_scanner
                            .request(self.state.settings.library_roots.clone());
                        return Ok(None);
                    }
                    Some(Scancode::Home) => {
                        self.library_index = 0;
                        return Ok(None);
                    }
                    Some(Scancode::End) => {
                        self.library_index = self.library.entries().len().saturating_sub(1);
                        return Ok(None);
                    }
                    Some(Scancode::PageUp) => {
                        self.library_index = self.library_index.saturating_sub(5);
                        return Ok(None);
                    }
                    Some(Scancode::PageDown) => {
                        self.library_index = (self.library_index + 5)
                            .min(self.library.entries().len().saturating_sub(1));
                        return Ok(None);
                    }
                    _ => {}
                }
                if matches_key(keycode, scancode, KeyIntent::Up) {
                    self.library_index = self.library_index.saturating_sub(1);
                } else if matches_key(keycode, scancode, KeyIntent::Down) {
                    self.library_index = (self.library_index + 1)
                        .min(self.library.entries().len().saturating_sub(1));
                } else if matches_key(keycode, scancode, KeyIntent::Confirm) {
                    self.prepare_selected_library_entry()?;
                }
            }
            AppScreen::Gameplay => {
                if matches_key(keycode, scancode, KeyIntent::Confirm)
                    && let Some(options) = self.pending_session_options.clone()
                {
                    return Ok(Some(ShellExit::Preview(options)));
                }

                if matches_key(keycode, scancode, KeyIntent::PlayLive)
                    && let Some(options) = self.pending_session_options.clone()
                {
                    return Ok(Some(ShellExit::Play(options)));
                }
            }
            AppScreen::Calibration | AppScreen::Diagnostics | AppScreen::Results => {}
        }

        Ok(None)
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

        adjust_setting(
            &mut self.state,
            self.settings_panel,
            self.settings_row_index,
            direction,
        );
    }

    fn prepare_selected_library_entry(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(entry) = self.library.get(self.library_index) else {
            return Ok(());
        };

        if !entry.is_available() {
            println!("app_song_unavailable={}", entry.chart_path().display());
            return Ok(());
        }

        self.state.select_chart(entry.chart_selection());
        self.state.request_play()?;
        let launch = self
            .state
            .take_pending_launch()
            .ok_or("app state did not create a play launch request")?;
        let session_options = PlaySessionOptions::from_app_launch_for_sdl_harness(&launch)?;
        let preview = match load_play_session_preview(&session_options) {
            Ok(preview) => preview,
            Err(error) => {
                println!(
                    "app_preview_error chart={} error={error}",
                    session_options.chart_path.display()
                );
                self.library_launch_error = Some(error.to_string());
                self.state.open_song_select();
                return Ok(());
            }
        };
        let summary = match prepare_play_session(&session_options) {
            Ok(summary) => summary,
            Err(error) => {
                println!(
                    "app_launch_error chart={} error={error}",
                    session_options.chart_path.display()
                );
                self.library_launch_error = Some(error.to_string());
                self.state.open_song_select();
                return Ok(());
            }
        };

        println!(
            "app_launch chart={} title={} display={} input={} lookahead_seconds={:.3}",
            session_options.chart_path.display(),
            summary.title.as_str(),
            session_options.display.as_str(),
            session_options.effective_input().as_str(),
            session_options.lookahead_seconds
        );
        self.pending_session_options = Some(session_options);
        self.pending_session_summary = Some(summary);
        self.pending_session_preview = Some(preview);
        Ok(())
    }

    fn activate_menu_item(&mut self) -> Result<Option<ShellExit>, Box<dyn Error>> {
        match MENU_ITEMS[self.menu_index] {
            MenuItem::SongSelect => self.state.open_song_select(),
            MenuItem::Settings => {
                self.settings_row_index = self
                    .settings_row_index
                    .min(settings_row_count(self.settings_panel));
                self.state.open_settings(self.settings_panel);
            }
            MenuItem::Calibration => self.state.open_calibration(),
            MenuItem::Diagnostics => self.state.open_diagnostics(),
            MenuItem::Quit => return Ok(Some(ShellExit::Quit)),
        }

        Ok(None)
    }

    fn go_back(&mut self) {
        match self.state.screen {
            AppScreen::MainMenu => {}
            AppScreen::Gameplay => self.state.screen = AppScreen::Results,
            AppScreen::Results
            | AppScreen::SongSelect
            | AppScreen::Settings(_)
            | AppScreen::Calibration
            | AppScreen::Diagnostics => self.state.open_main_menu(),
        }
    }
}

#[derive(Clone, Copy)]
enum MenuItem {
    SongSelect,
    Settings,
    Calibration,
    Diagnostics,
    Quit,
}

impl MenuItem {
    fn label(self) -> &'static str {
        match self {
            Self::SongSelect => "song_select",
            Self::Settings => "settings",
            Self::Calibration => "calibration",
            Self::Diagnostics => "diagnostics",
            Self::Quit => "quit",
        }
    }

    fn display_label(self) -> &'static str {
        match self {
            Self::SongSelect => "SONG SELECT",
            Self::Settings => "SETTINGS",
            Self::Calibration => "CALIBRATION",
            Self::Diagnostics => "DIAGNOSTICS",
            Self::Quit => "QUIT",
        }
    }

    fn color(self) -> Color {
        match self {
            Self::SongSelect => Color::RGB(90, 166, 221),
            Self::Settings => Color::RGB(112, 194, 145),
            Self::Calibration => Color::RGB(238, 181, 81),
            Self::Diagnostics => Color::RGB(178, 132, 214),
            Self::Quit => Color::RGB(210, 92, 92),
        }
    }
}

#[derive(Clone, Copy)]
enum KeyIntent {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    PlayLive,
    Back,
    Quit,
}

fn matches_key(keycode: Option<Keycode>, scancode: Option<Scancode>, intent: KeyIntent) -> bool {
    match intent {
        KeyIntent::Up => matches!(
            (keycode, scancode),
            (Some(Keycode::Up), _) | (_, Some(Scancode::Up)) | (_, Some(Scancode::W))
        ),
        KeyIntent::Down => matches!(
            (keycode, scancode),
            (Some(Keycode::Down), _) | (_, Some(Scancode::Down)) | (_, Some(Scancode::S))
        ),
        KeyIntent::Left => matches!(
            (keycode, scancode),
            (Some(Keycode::Left), _) | (_, Some(Scancode::Left)) | (_, Some(Scancode::A))
        ),
        KeyIntent::Right => matches!(
            (keycode, scancode),
            (Some(Keycode::Right), _) | (_, Some(Scancode::Right)) | (_, Some(Scancode::D))
        ),
        KeyIntent::Confirm => matches!(
            (keycode, scancode),
            (Some(Keycode::Return), _)
                | (Some(Keycode::Space), _)
                | (_, Some(Scancode::Return))
                | (_, Some(Scancode::Space))
        ),
        KeyIntent::PlayLive => matches!(
            (keycode, scancode),
            (Some(Keycode::P), _) | (_, Some(Scancode::P))
        ),
        KeyIntent::Back => matches!(
            (keycode, scancode),
            (Some(Keycode::Escape), _)
                | (Some(Keycode::Backspace), _)
                | (_, Some(Scancode::Escape))
                | (_, Some(Scancode::Backspace))
        ),
        KeyIntent::Quit => matches!(
            (keycode, scancode),
            (Some(Keycode::Q), _) | (_, Some(Scancode::Q))
        ),
    }
}
