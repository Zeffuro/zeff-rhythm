use super::library::AppLibrary;
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

pub fn run() -> Result<(), Box<dyn Error>> {
    let mut shell = NativeAppShell::new();

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
    let window = video
        .window("zeff-rhythm", WIDTH, HEIGHT)
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window.into_canvas();
    let mut event_pump = sdl.event_pump()?;

    println!("app=window");
    println!(
        "controls=up/down or w/s to select, enter/space to open, esc/backspace to go back, q to quit"
    );

    loop {
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
    fn new() -> Self {
        let library = AppLibrary::local_defaults();
        let library_index = library.first_available_index().unwrap_or(0);

        Self {
            state: AppState::new(),
            library,
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
            AppScreen::Calibration | AppScreen::Results => {}
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
        let session_options = PlaySessionOptions::from_app_launch(&launch)?;
        let preview = match load_play_session_preview(&session_options) {
            Ok(preview) => preview,
            Err(error) => {
                println!(
                    "app_preview_error chart={} error={error}",
                    session_options.chart_path.display()
                );
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
            MenuItem::Diagnostics => {
                self.settings_panel = SettingsPanel::Diagnostics;
                self.settings_row_index = self
                    .settings_row_index
                    .min(settings_row_count(self.settings_panel));
                self.state.open_settings(self.settings_panel);
            }
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
            | AppScreen::Calibration => self.state.open_main_menu(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas<Window>) -> Result<(), Box<dyn Error>> {
        let (width, height) = canvas.output_size().unwrap_or((WIDTH, HEIGHT));
        let width = width.max(640);
        let height = height.max(420);

        self.update_title(canvas)?;
        canvas.set_draw_color(Color::RGB(15, 18, 24));
        canvas.clear();

        match self.state.screen {
            AppScreen::MainMenu => self.draw_main_menu(canvas, width, height)?,
            AppScreen::Settings(panel) => self.draw_settings(canvas, width, height, panel)?,
            AppScreen::SongSelect => self.draw_song_select(canvas, width, height)?,
            AppScreen::Calibration => self.draw_calibration(canvas, width, height)?,
            AppScreen::Gameplay => self.draw_gameplay_placeholder(canvas, width, height)?,
            AppScreen::Results => self.draw_results(canvas, width, height)?,
        }

        canvas.present();
        Ok(())
    }

    fn update_title(&mut self, canvas: &mut Canvas<Window>) -> Result<(), Box<dyn Error>> {
        if self.last_title_update.elapsed() < Duration::from_millis(250) {
            return Ok(());
        }

        let title = format!(
            "zeff-rhythm | {} | selected {}",
            screen_label(self.state.screen),
            selected_label(self),
        );
        canvas.window_mut().set_title(&title)?;
        self.last_title_update = Instant::now();
        Ok(())
    }

    fn draw_main_menu(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 0)?;
        let item_height = ((height - 180) / MENU_ITEMS.len() as u32).clamp(44, 78);
        let start_y = 120_i32;
        let item_width = (width - 160).min(640);
        let x = ((width - item_width) / 2) as i32;

        for (index, item) in MENU_ITEMS.iter().enumerate() {
            let y = start_y + index as i32 * (item_height as i32 + 14);
            let selected = index == self.menu_index;
            draw_menu_row(
                canvas,
                x,
                y,
                item_width,
                item_height,
                selected,
                item.color(),
                item.display_label(),
            )?;
        }

        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_settings(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
        panel: SettingsPanel,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 1)?;
        let panels = [
            SettingsPanel::Audio,
            SettingsPanel::Input,
            SettingsPanel::Video,
            SettingsPanel::Gameplay,
            SettingsPanel::Diagnostics,
        ];
        let panel_width = ((width - 96) / panels.len() as u32).max(80);

        for (index, candidate) in panels.iter().copied().enumerate() {
            let selected = candidate == panel;
            let x = 48 + index as i32 * panel_width as i32;
            draw_menu_row(
                canvas,
                x,
                120,
                panel_width.saturating_sub(12),
                74,
                selected,
                panel_color(candidate),
                panel_display_label(candidate),
            )?;
        }

        let body_height = height.saturating_sub(270).max(80);
        canvas.set_draw_color(Color::RGB(34, 39, 48));
        canvas.fill_rect(Rect::new(64, 240, width - 128, body_height))?;
        draw_settings_body(canvas, &self.state, panel, width, self.settings_row_index)?;
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_song_select(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 2)?;
        let row_width = width - 192;
        for (index, entry) in self.library.entries().iter().enumerate() {
            let available = entry.is_available();
            let color = if available {
                Color::RGB(90, 166, 221)
            } else {
                Color::RGB(98, 102, 112)
            };
            draw_menu_row(
                canvas,
                96,
                130 + index as i32 * 104,
                row_width,
                86,
                index == self.library_index,
                color,
                &entry.title,
            )?;
            draw_text(
                canvas,
                130,
                130 + index as i32 * 104 + 56,
                &entry.subtitle,
                2,
                MUTED_TEXT,
            )?;
        }

        if let Some(entry) = self.library.get(self.library_index) {
            let status = if entry.is_available() {
                "READY"
            } else {
                "MISSING"
            };
            draw_text(
                canvas,
                112,
                270,
                status,
                2,
                panel_color(SettingsPanel::Input),
            )?;
            draw_text(
                canvas,
                112,
                312,
                &entry.chart_path().display().to_string(),
                2,
                MUTED_TEXT,
            )?;
        }
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_calibration(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 3)?;
        canvas.set_draw_color(Color::RGB(40, 44, 53));
        canvas.fill_rect(Rect::new(96, 130, width - 192, height - 250))?;
        draw_text(canvas, 124, 156, "CALIBRATION", 3, TEXT)?;
        draw_text(
            canvas,
            124,
            210,
            "GENERATED TIMING TEST PENDING",
            2,
            MUTED_TEXT,
        )?;
        canvas.set_draw_color(Color::RGB(238, 181, 81));
        for index in 0..4 {
            let x = 130 + index * 92;
            canvas.fill_rect(Rect::new(x, height as i32 / 2, 56, 56))?;
        }
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_gameplay_placeholder(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 4)?;
        if self.pending_session_options.is_some() {
            canvas.set_draw_color(Color::RGB(126, 206, 170));
            canvas.fill_rect(Rect::new(96, 92, width - 192, 12))?;
            draw_text(canvas, 96, 84, "SESSION READY", 2, TEXT)?;
            if let Some(summary) = self.pending_session_summary.as_ref() {
                draw_text(canvas, 96, 118, &summary.title, 3, TEXT)?;
                draw_text(
                    canvas,
                    96,
                    154,
                    &format!(
                        "{} LANES  {} NOTES  {} HOLDS",
                        summary.lane_count, summary.note_count, summary.hold_count
                    ),
                    2,
                    MUTED_TEXT,
                )?;
                draw_text(
                    canvas,
                    96,
                    184,
                    &format!("AUDIO {:.3} S", summary.audio_duration_seconds),
                    2,
                    MUTED_TEXT,
                )?;
            }
            if let Some(preview) = self.pending_session_preview.as_ref() {
                let lookahead_seconds = self
                    .pending_session_options
                    .as_ref()
                    .map(|options| options.lookahead_seconds)
                    .unwrap_or(4.0);
                draw_gameplay_preview(canvas, width, height, preview, lookahead_seconds)?;
            } else {
                draw_empty_lanes(canvas, width, height, 4)?;
            }
            draw_text(canvas, 96, 222, "WGPU CHART PREVIEW READY", 2, MUTED_TEXT)?;
            draw_text(canvas, 96, 252, "P LIVE SDL HARNESS", 2, MUTED_TEXT)?;
        } else {
            draw_empty_lanes(canvas, width, height, 4)?;
            draw_text(canvas, 96, 84, "NO SESSION", 2, MUTED_TEXT)?;
        }
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_results(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 5)?;
        canvas.set_draw_color(Color::RGB(45, 55, 67));
        canvas.fill_rect(Rect::new(96, 140, width - 192, 64))?;
        canvas.fill_rect(Rect::new(96, 230, width - 300, 42))?;
        canvas.fill_rect(Rect::new(96, 296, width - 380, 42))?;
        draw_text(canvas, 124, 160, "RESULTS", 3, TEXT)?;
        draw_text(canvas, 124, 244, "LATEST RUN PENDING", 2, MUTED_TEXT)?;
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }
}

fn draw_gameplay_preview(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    preview: &PlaySessionPreview,
    lookahead_seconds: f64,
) -> Result<(), Box<dyn Error>> {
    let layout = AppHighwayLayout::new(width, height, preview.lane_count as usize);
    draw_lanes(canvas, &layout)?;

    let render_layout = HighwayRenderLayout::new(
        layout.lane_count,
        lookahead_seconds,
        0.180,
        layout.top_y as f32,
        layout.judgement_y as f32,
    );
    let judged = HashSet::new();
    let sprites = build_highway_note_sprites(
        render_layout,
        &preview.chart,
        &judged,
        preview.chart_start_seconds,
    );

    for sprite in sprites {
        match sprite.kind {
            HighwayNoteSpriteKind::Tap => {
                canvas.set_draw_color(preview_note_color(sprite.delta_seconds));
                canvas.fill_rect(preview_note_rect(
                    &layout,
                    sprite.lane,
                    sprite.y.round() as i32,
                    14,
                ))?;
            }
            HighwayNoteSpriteKind::Hold { end_y } => {
                let start_y = sprite.y.round() as i32;
                let end_y = end_y.round() as i32;
                let first_y = end_y
                    .min(start_y)
                    .clamp(layout.top_y, layout.height as i32 - 120);
                let last_y = end_y
                    .max(start_y)
                    .clamp(layout.top_y, layout.height as i32 - 120);
                let height = (last_y - first_y).max(8) as u32;
                let x = layout.lane_x(sprite.lane) + layout.lane_width as i32 / 2 - 8;

                canvas.set_draw_color(Color::RGB(60, 116, 190));
                canvas.fill_rect(Rect::new(x, first_y, 16, height))?;
                canvas.set_draw_color(preview_note_color(sprite.delta_seconds));
                canvas.fill_rect(preview_note_rect(&layout, sprite.lane, start_y, 14))?;
            }
        }
    }

    Ok(())
}

fn draw_empty_lanes(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    lane_count: usize,
) -> Result<(), Box<dyn Error>> {
    let layout = AppHighwayLayout::new(width, height, lane_count);
    draw_lanes(canvas, &layout)
}

fn draw_lanes(
    canvas: &mut Canvas<Window>,
    layout: &AppHighwayLayout,
) -> Result<(), Box<dyn Error>> {
    for lane in 0..layout.lane_count {
        let x = layout.lane_x(lane);
        canvas.set_draw_color(Color::RGB(26, 31, 38));
        canvas.fill_rect(Rect::new(
            x,
            layout.top_y,
            layout.lane_width,
            layout.judgement_y.saturating_sub(layout.top_y) as u32 + 44,
        ))?;
        canvas.set_draw_color(Color::RGB(56, 64, 74));
        canvas.draw_rect(Rect::new(
            x,
            layout.top_y,
            layout.lane_width,
            layout.judgement_y.saturating_sub(layout.top_y) as u32 + 44,
        ))?;
        canvas.set_draw_color(Color::RGB(230, 226, 210));
        canvas.fill_rect(Rect::new(
            x + 8,
            layout.judgement_y,
            layout.lane_width.saturating_sub(16),
            8,
        ))?;
    }

    Ok(())
}

struct AppHighwayLayout {
    height: u32,
    lane_count: usize,
    lane_width: u32,
    lane_start_x: i32,
    top_y: i32,
    judgement_y: i32,
}

impl AppHighwayLayout {
    fn new(width: u32, height: u32, lane_count: usize) -> Self {
        let lane_count = lane_count.clamp(1, 8);
        let lane_gap = 8;
        let usable_width = width.saturating_sub(240).max(320);
        let total_gap = lane_gap as u32 * lane_count.saturating_sub(1) as u32;
        let lane_width = ((usable_width - total_gap) / lane_count as u32).clamp(52, 110);
        let total_width = lane_width * lane_count as u32 + total_gap;
        let lane_start_x = ((width - total_width) / 2) as i32;

        Self {
            height,
            lane_count,
            lane_width,
            lane_start_x,
            top_y: 250,
            judgement_y: height as i32 - 130,
        }
    }

    fn lane_x(&self, lane: usize) -> i32 {
        self.lane_start_x + lane as i32 * (self.lane_width as i32 + 8)
    }
}

fn preview_note_rect(layout: &AppHighwayLayout, lane: usize, y: i32, height: u32) -> Rect {
    let x = layout.lane_x(lane) + 8;
    let width = layout.lane_width.saturating_sub(16);
    Rect::new(x, y - height as i32 / 2, width, height)
}

fn preview_note_color(delta_seconds: f64) -> Color {
    if delta_seconds < -0.050 {
        Color::RGB(196, 74, 74)
    } else if delta_seconds.abs() <= 0.050 {
        Color::RGB(116, 220, 143)
    } else {
        Color::RGB(94, 204, 216)
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

fn draw_header(canvas: &mut Canvas<Window>, width: u32, section: u8) -> Result<(), Box<dyn Error>> {
    canvas.set_draw_color(Color::RGB(29, 34, 43));
    canvas.fill_rect(Rect::new(0, 0, width, 72))?;
    canvas.set_draw_color(section_color(section));
    canvas.fill_rect(Rect::new(40, 26, width / 3, 12))?;
    canvas.fill_rect(Rect::new(40, 46, width / 5, 8))?;
    draw_text(canvas, 40, 22, "ZEFF RHYTHM", 3, TEXT)?;
    draw_text(canvas, 40, 52, screen_header(section), 2, MUTED_TEXT)?;
    Ok(())
}

fn draw_footer(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    input_offset_ms: f64,
) -> Result<(), Box<dyn Error>> {
    canvas.set_draw_color(Color::RGB(29, 34, 43));
    canvas.fill_rect(Rect::new(0, height as i32 - 64, width, 64))?;
    canvas.set_draw_color(Color::RGB(76, 143, 191));
    let offset_width = (input_offset_ms.abs().round() as u32).clamp(4, width / 3);
    canvas.fill_rect(Rect::new(40, height as i32 - 38, offset_width, 10))?;
    draw_text(
        canvas,
        40,
        height as i32 - 52,
        &format!("INPUT OFFSET {:.3} MS", input_offset_ms),
        2,
        MUTED_TEXT,
    )?;
    Ok(())
}

fn draw_menu_row(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    selected: bool,
    accent: Color,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    let background = if selected {
        Color::RGB(54, 61, 73)
    } else {
        Color::RGB(33, 39, 48)
    };
    canvas.set_draw_color(background);
    canvas.fill_rect(Rect::new(x, y, width, height))?;
    canvas.set_draw_color(accent);
    canvas.fill_rect(Rect::new(x, y, 10, height))?;
    let inner_width = width.saturating_sub(60);
    canvas.fill_rect(Rect::new(
        x + 34,
        y + height as i32 / 2 - 6,
        inner_width,
        12,
    ))?;
    let scale = if height >= 70 { 3 } else { 2 };
    draw_text(
        canvas,
        x + 34,
        y + (height as i32 - text_height(scale) as i32) / 2,
        label,
        scale,
        TEXT,
    )?;
    if selected {
        canvas.set_draw_color(Color::RGB(232, 228, 212));
        canvas.draw_rect(Rect::new(x - 6, y - 6, width + 12, height + 12))?;
    }
    Ok(())
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

fn selected_label(shell: &NativeAppShell) -> String {
    match shell.state.screen {
        AppScreen::MainMenu => MENU_ITEMS[shell.menu_index].label().to_owned(),
        AppScreen::Settings(panel) => settings_selected_label(panel, shell.settings_row_index),
        AppScreen::SongSelect => shell
            .library
            .get(shell.library_index)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "none".to_owned()),
        AppScreen::Calibration => "generated_test_pending".to_owned(),
        AppScreen::Gameplay => {
            if shell.pending_session_options.is_some() {
                shell
                    .pending_session_summary
                    .as_ref()
                    .map(|summary| summary.title.clone())
                    .unwrap_or_else(|| "play_session_ready".to_owned())
            } else {
                "play_session_pending".to_owned()
            }
        }
        AppScreen::Results => "latest_run_pending".to_owned(),
    }
}

fn panel_label(panel: SettingsPanel) -> &'static str {
    match panel {
        SettingsPanel::Audio => "audio",
        SettingsPanel::Input => "input",
        SettingsPanel::Video => "video",
        SettingsPanel::Gameplay => "gameplay",
        SettingsPanel::Diagnostics => "diagnostics",
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

fn settings_selected_label(panel: SettingsPanel, row_index: usize) -> String {
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
        (SettingsPanel::Video, 5) => "LOOKAHEAD",
        (SettingsPanel::Gameplay, 1) => "LEAD IN",
        (SettingsPanel::Gameplay, 2) => "SCROLL",
        (SettingsPanel::Gameplay, 3) => "JUDGEMENT",
        (SettingsPanel::Diagnostics, 1) => "EVENT LOG",
        (SettingsPanel::Diagnostics, 2) => "METRICS",
        (SettingsPanel::Diagnostics, 3) => "OVERLAY",
        _ => "PANEL",
    }
}

fn settings_row_count(panel: SettingsPanel) -> usize {
    match panel {
        SettingsPanel::Audio => 4,
        SettingsPanel::Video => 5,
        SettingsPanel::Input | SettingsPanel::Gameplay | SettingsPanel::Diagnostics => 3,
    }
}

fn draw_settings_body(
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
            format!("LOOKAHEAD {:.1} S", state.settings.video.lookahead_seconds),
        ],
        SettingsPanel::Gameplay => vec![
            format!("LEAD IN {:.1} S", state.settings.gameplay.lead_in_seconds),
            format!("SCROLL {:.1}X", state.settings.gameplay.scroll_speed),
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

fn adjust_setting(state: &mut AppState, panel: SettingsPanel, row_index: usize, direction: i32) {
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
                super::settings::InputBackendPreference::Sdl => {
                    super::settings::InputBackendPreference::TerminalDebug
                }
                super::settings::InputBackendPreference::TerminalDebug => {
                    super::settings::InputBackendPreference::Sdl
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
        (SettingsPanel::Video, 5) => {
            state.settings.video.lookahead_seconds =
                (state.settings.video.lookahead_seconds + direction as f64 * 0.25).clamp(1.0, 10.0);
        }
        (SettingsPanel::Gameplay, 1) => {
            state.settings.gameplay.lead_in_seconds =
                (state.settings.gameplay.lead_in_seconds + direction as f64 * 0.5).clamp(0.0, 10.0);
        }
        (SettingsPanel::Gameplay, 2) => {
            state.settings.gameplay.scroll_speed =
                (state.settings.gameplay.scroll_speed + direction as f64 * 0.1).clamp(0.5, 4.0);
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

fn screen_header(section: u8) -> &'static str {
    match section {
        1 => "SETTINGS",
        2 => "SONG SELECT",
        3 => "CALIBRATION",
        4 => "GAMEPLAY",
        5 => "RESULTS",
        _ => "MAIN MENU",
    }
}

fn section_color(section: u8) -> Color {
    match section {
        1 => Color::RGB(112, 194, 145),
        2 => Color::RGB(90, 166, 221),
        3 => Color::RGB(238, 181, 81),
        4 => Color::RGB(126, 206, 170),
        5 => Color::RGB(178, 132, 214),
        _ => Color::RGB(96, 151, 213),
    }
}

fn panel_color(panel: SettingsPanel) -> Color {
    match panel {
        SettingsPanel::Audio => Color::RGB(90, 166, 221),
        SettingsPanel::Input => Color::RGB(112, 194, 145),
        SettingsPanel::Video => Color::RGB(238, 181, 81),
        SettingsPanel::Gameplay => Color::RGB(178, 132, 214),
        SettingsPanel::Diagnostics => Color::RGB(210, 112, 128),
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
