use super::terminal_highway::HighwaySnapshot;
use crate::platform::input::{NativeInputEvent, SdlTimestampClock, translate_sdl_event};
use crate::render::highway::{
    HighwayNoteSpriteKind, HighwayRenderLayout, build_highway_note_sprites,
};
use sdl3::event::Event;
use sdl3::pixels::Color;
use sdl3::rect::Rect;
use sdl3::render::Canvas;
use sdl3::video::Window;
use std::error::Error;
use std::time::Duration;

const MIN_WIDTH: u32 = 640;
const MIN_HEIGHT: u32 = 480;
const TOP_PAD: i32 = 44;
const BOTTOM_PAD: i32 = 92;
const LANE_GAP: i32 = 8;
const LINE_HEIGHT: u32 = 8;

pub struct SdlPlayWindow {
    _sdl: sdl3::Sdl,
    canvas: Canvas<Window>,
    event_pump: sdl3::EventPump,
    clock: SdlTimestampClock,
    lookahead_seconds: f64,
}

impl SdlPlayWindow {
    pub fn enter(lookahead_seconds: f64) -> Result<Self, Box<dyn Error>> {
        let sdl = sdl3::init()?;
        let video = sdl.video()?;
        let mut window = video
            .window("zeff-rhythm", 960, 720)
            .position_centered()
            .resizable()
            .build()
            .map_err(|error| error.to_string())?;
        let _ = window.raise();
        let mut canvas = window.into_canvas();
        let _ = canvas.window_mut().set_keyboard_grab(true);
        canvas.set_draw_color(Color::RGB(16, 20, 26));
        canvas.clear();
        canvas.present();

        Ok(Self {
            event_pump: sdl.event_pump()?,
            clock: SdlTimestampClock::capture(),
            _sdl: sdl,
            canvas,
            lookahead_seconds,
        })
    }

    pub fn poll_input(&mut self, output: &mut Vec<NativeInputEvent>) -> Result<(), Box<dyn Error>> {
        if let Some(event) = self.event_pump.wait_event_timeout(Duration::from_millis(1)) {
            self.push_input_event(event, output);
        }

        let clock = self.clock;
        for event in self.event_pump.poll_iter() {
            if let Some(input) = translate_sdl_event(clock, event) {
                output.push(input);
            }
        }

        Ok(())
    }

    pub fn has_input_focus(&self) -> bool {
        self.canvas.window().has_input_focus()
    }

    pub fn render(&mut self, snapshot: HighwaySnapshot<'_>) -> Result<(), Box<dyn Error>> {
        let (width, height) = self.canvas.output_size().unwrap_or((960, 720));
        let width = width.max(MIN_WIDTH);
        let height = height.max(MIN_HEIGHT);
        let lane_count = snapshot.chart.lane_count().min(8) as usize;
        let layout = SdlHighwayLayout::new(width, height, lane_count);
        let has_input_focus = self.canvas.window().has_input_focus();

        self.update_title(&snapshot, has_input_focus)?;
        self.canvas.set_draw_color(Color::RGB(16, 20, 26));
        self.canvas.clear();
        self.draw_focus_indicator(width, height, has_input_focus)?;
        self.draw_lanes(&layout, snapshot.active_lanes)?;
        self.draw_notes(&layout, &snapshot)?;
        self.draw_progress(width, height, &snapshot)?;
        self.canvas.present();

        Ok(())
    }

    fn push_input_event(&self, event: Event, output: &mut Vec<NativeInputEvent>) {
        if let Some(input) = translate_sdl_event(self.clock, event) {
            output.push(input);
        }
    }

    fn update_title(
        &mut self,
        snapshot: &HighwaySnapshot<'_>,
        has_input_focus: bool,
    ) -> Result<(), Box<dyn Error>> {
        let latency = snapshot
            .output_latency_seconds
            .map(|seconds| format!("{:.1}ms", seconds * 1_000.0))
            .unwrap_or_else(|| "-".to_owned());
        let countdown = snapshot
            .countdown_seconds
            .map(|seconds| format!(" START {seconds:.1}s"))
            .unwrap_or_default();
        let focus = if has_input_focus {
            "FOCUSED"
        } else {
            "CLICK WINDOW FOR INPUT"
        };
        let title = format!(
            "{} | {focus} | time {:.3}s | judged {}/{} | M:{} P:{} Gt:{} Gd:{} Miss:{} | lead {latency}{countdown}",
            snapshot.chart.metadata().title,
            snapshot.song_time_seconds,
            snapshot.judged_count,
            snapshot.chart.notes().len(),
            snapshot.counts.marvelous,
            snapshot.counts.perfect,
            snapshot.counts.great,
            snapshot.counts.good,
            snapshot.counts.miss,
        );

        self.canvas.window_mut().set_title(&title)?;
        Ok(())
    }

    fn draw_focus_indicator(
        &mut self,
        width: u32,
        height: u32,
        has_input_focus: bool,
    ) -> Result<(), Box<dyn Error>> {
        let color = if has_input_focus {
            Color::RGB(70, 184, 132)
        } else {
            Color::RGB(220, 128, 68)
        };

        self.canvas.set_draw_color(color);
        self.canvas.fill_rect(Rect::new(0, 0, width, 8))?;
        if !has_input_focus {
            self.canvas
                .draw_rect(Rect::new(8, 16, width - 16, height - 32))?;
            self.canvas
                .draw_rect(Rect::new(14, 22, width - 28, height - 44))?;
        }

        Ok(())
    }

    fn draw_lanes(
        &mut self,
        layout: &SdlHighwayLayout,
        active_lanes: [bool; 4],
    ) -> Result<(), Box<dyn Error>> {
        for lane in 0..layout.lane_count {
            let x = layout.lane_x(lane);
            let lane_rect = Rect::new(
                x,
                TOP_PAD,
                layout.lane_width,
                layout.judgement_y.saturating_sub(TOP_PAD) as u32 + 52,
            );

            let active = active_lanes.get(lane).copied().unwrap_or(false);
            let lane_color = if active {
                Color::RGB(48, 60, 62)
            } else {
                Color::RGB(30, 35, 42)
            };
            self.canvas.set_draw_color(lane_color);
            self.canvas.fill_rect(lane_rect)?;

            self.canvas.set_draw_color(Color::RGB(56, 64, 74));
            self.canvas.draw_rect(lane_rect)?;

            self.canvas.set_draw_color(Color::RGB(226, 222, 205));
            self.canvas.fill_rect(Rect::new(
                x + 4,
                layout.judgement_y,
                layout.lane_width.saturating_sub(8),
                LINE_HEIGHT,
            ))?;
        }

        Ok(())
    }

    fn draw_notes(
        &mut self,
        layout: &SdlHighwayLayout,
        snapshot: &HighwaySnapshot<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let render_layout = HighwayRenderLayout::new(
            layout.lane_count,
            self.lookahead_seconds,
            0.180,
            TOP_PAD as f32,
            layout.judgement_y as f32,
        );
        let sprites = build_highway_note_sprites(
            render_layout,
            snapshot.chart,
            snapshot.judged_note_ids,
            snapshot.song_time_seconds,
        );

        for sprite in sprites {
            match sprite.kind {
                HighwayNoteSpriteKind::Tap => self.draw_tap(
                    layout,
                    sprite.lane,
                    sprite.y.round() as i32,
                    sprite.delta_seconds,
                )?,
                HighwayNoteSpriteKind::Hold { end_y } => {
                    self.draw_hold(layout, sprite.lane, sprite.y, end_y, sprite.delta_seconds)?
                }
            }
        }

        Ok(())
    }

    fn draw_tap(
        &mut self,
        layout: &SdlHighwayLayout,
        lane: usize,
        y: i32,
        delta_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.canvas.set_draw_color(note_color(delta_seconds));
        self.canvas.fill_rect(note_rect(layout, lane, y, 14))?;
        Ok(())
    }

    fn draw_hold(
        &mut self,
        layout: &SdlHighwayLayout,
        lane: usize,
        start_y: f32,
        end_y: f32,
        delta_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        let start_y = start_y.round() as i32;
        let end_y = end_y.round() as i32;
        let first_y = end_y
            .min(start_y)
            .clamp(TOP_PAD, layout.height as i32 - BOTTOM_PAD);
        let last_y = end_y
            .max(start_y)
            .clamp(TOP_PAD, layout.height as i32 - BOTTOM_PAD);
        let height = (last_y - first_y).max(8) as u32;
        let x = layout.lane_x(lane) + layout.lane_width as i32 / 2 - 8;

        self.canvas.set_draw_color(Color::RGB(60, 116, 190));
        self.canvas.fill_rect(Rect::new(x, first_y, 16, height))?;
        self.draw_tap(layout, lane, start_y, delta_seconds)?;
        Ok(())
    }

    fn draw_progress(
        &mut self,
        width: u32,
        height: u32,
        snapshot: &HighwaySnapshot<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let progress = if snapshot.end_seconds > 0.0 {
            (snapshot.song_time_seconds / snapshot.end_seconds).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bar_width = ((width - 48) as f64 * progress) as u32;
        let y = height as i32 - 30;

        self.canvas.set_draw_color(Color::RGB(42, 48, 56));
        self.canvas.fill_rect(Rect::new(24, y, width - 48, 8))?;
        self.canvas.set_draw_color(Color::RGB(98, 190, 176));
        self.canvas
            .fill_rect(Rect::new(24, y, bar_width.max(1), 8))?;
        Ok(())
    }
}

struct SdlHighwayLayout {
    height: u32,
    lane_count: usize,
    lane_width: u32,
    lane_start_x: i32,
    judgement_y: i32,
}

impl SdlHighwayLayout {
    fn new(width: u32, height: u32, lane_count: usize) -> Self {
        let lane_count = lane_count.max(1);
        let usable_width = width.saturating_sub(128).max(320);
        let total_gap = LANE_GAP as u32 * lane_count.saturating_sub(1) as u32;
        let lane_width = ((usable_width - total_gap) / lane_count as u32).clamp(52, 110);
        let total_width = lane_width * lane_count as u32 + total_gap;
        let lane_start_x = ((width - total_width) / 2) as i32;
        let judgement_y = height as i32 - BOTTOM_PAD;

        Self {
            height,
            lane_count,
            lane_width,
            lane_start_x,
            judgement_y,
        }
    }

    fn lane_x(&self, lane: usize) -> i32 {
        self.lane_start_x + lane as i32 * (self.lane_width as i32 + LANE_GAP)
    }
}

fn note_rect(layout: &SdlHighwayLayout, lane: usize, y: i32, height: u32) -> Rect {
    let x = layout.lane_x(lane) + 8;
    let width = layout.lane_width.saturating_sub(16);
    Rect::new(x, y - height as i32 / 2, width, height)
}

fn note_color(delta_seconds: f64) -> Color {
    if delta_seconds < -0.050 {
        Color::RGB(196, 74, 74)
    } else if delta_seconds.abs() <= 0.050 {
        Color::RGB(116, 220, 143)
    } else {
        Color::RGB(94, 204, 216)
    }
}
