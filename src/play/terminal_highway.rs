use super::judgement_counts::JudgementCounts;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use rhythm_core::{Chart, NoteKind};
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::io::{Stdout, Write, stdout};

const LANE_WIDTH: u16 = 5;
const BASE_X: u16 = 2;
const TOP_ROW: u16 = 4;

pub struct HighwaySnapshot<'a> {
    pub chart: &'a Chart,
    pub judged_note_ids: &'a HashSet<u32>,
    pub counts: &'a JudgementCounts,
    pub messages: &'a VecDeque<String>,
    pub active_lanes: [bool; 4],
    pub song_time_seconds: f64,
    pub scheduled_time_seconds: f64,
    pub audio_duration_seconds: f64,
    pub end_seconds: f64,
    pub judged_count: usize,
    pub output_latency_seconds: Option<f64>,
    pub countdown_seconds: Option<f64>,
}

pub struct TerminalHighway {
    stdout: Stdout,
    lookahead_seconds: f64,
}

impl TerminalHighway {
    pub fn enter(lookahead_seconds: f64) -> Result<Self, Box<dyn Error>> {
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

        Ok(Self {
            stdout,
            lookahead_seconds,
        })
    }

    pub fn render(&mut self, snapshot: HighwaySnapshot<'_>) -> Result<(), Box<dyn Error>> {
        let (width, height) = terminal::size().unwrap_or((100, 32));
        let judgement_row = height.saturating_sub(7).max(TOP_ROW + 8);
        let lane_count = snapshot.chart.lane_count().min(8) as usize;

        queue!(self.stdout, MoveTo(0, 0), Clear(ClearType::All))?;
        self.render_header(width, &snapshot)?;
        self.render_lanes(lane_count, judgement_row)?;
        self.render_notes(lane_count, judgement_row, &snapshot)?;
        self.render_lane_labels(lane_count, judgement_row + 1, snapshot.active_lanes)?;
        self.render_messages(width, judgement_row + 3, &snapshot)?;
        self.stdout.flush()?;

        Ok(())
    }

    fn render_header(
        &mut self,
        width: u16,
        snapshot: &HighwaySnapshot<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let title = snapshot.chart.metadata().title.as_str();
        let latency_ms = snapshot
            .output_latency_seconds
            .map(|seconds| format!("{:.1}ms", seconds * 1_000.0))
            .unwrap_or_else(|| "-".to_owned());

        let countdown = snapshot
            .countdown_seconds
            .map(|seconds| format!("  START {seconds:.1}"))
            .unwrap_or_default();

        queue!(
            self.stdout,
            MoveTo(0, 0),
            SetForegroundColor(Color::White),
            Print(truncate(
                &format!(
                    "{title}  time {:.3}/{:.3}s  submitted {:.3}s  lead {latency_ms}{countdown}",
                    snapshot.song_time_seconds,
                    snapshot.audio_duration_seconds,
                    snapshot.scheduled_time_seconds,
                ),
                width
            )),
            ResetColor,
            MoveTo(0, 1),
            Print(truncate(
                &format!(
                    "judged {}/{}  M:{} P:{} Gt:{} Gd:{} Miss:{}  end {:.3}s",
                    snapshot.judged_count,
                    snapshot.chart.notes().len(),
                    snapshot.counts.marvelous,
                    snapshot.counts.perfect,
                    snapshot.counts.great,
                    snapshot.counts.good,
                    snapshot.counts.miss,
                    snapshot.end_seconds,
                ),
                width
            )),
            MoveTo(0, 2),
            Print(truncate("D/F/J/K = lanes 0/1/2/3    Esc/Q = quit", width)),
        )?;

        Ok(())
    }

    fn render_lanes(
        &mut self,
        lane_count: usize,
        judgement_row: u16,
    ) -> Result<(), Box<dyn Error>> {
        let right_edge = BASE_X + lane_count as u16 * LANE_WIDTH;

        for row in TOP_ROW..=judgement_row {
            for lane in 0..lane_count {
                let x = BASE_X + lane as u16 * LANE_WIDTH;
                queue!(self.stdout, MoveTo(x, row), Print("|    "))?;
            }
            queue!(self.stdout, MoveTo(right_edge, row), Print("|"))?;
        }

        queue!(self.stdout, SetForegroundColor(Color::DarkGrey))?;
        for lane in 0..lane_count {
            let x = BASE_X + lane as u16 * LANE_WIDTH + 1;
            queue!(self.stdout, MoveTo(x, judgement_row), Print("==="))?;
        }
        queue!(self.stdout, ResetColor)?;

        Ok(())
    }

    fn render_notes(
        &mut self,
        lane_count: usize,
        judgement_row: u16,
        snapshot: &HighwaySnapshot<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let visible_start = snapshot.song_time_seconds - 0.180;
        let visible_end = snapshot.song_time_seconds + self.lookahead_seconds;

        for note in snapshot.chart.notes() {
            if snapshot.judged_note_ids.contains(&note.id.as_u32()) {
                continue;
            }

            let lane = note.lane.as_usize();
            if lane >= lane_count {
                continue;
            }

            match note.kind {
                NoteKind::Tap => {
                    if note.time_seconds < visible_start || note.time_seconds > visible_end {
                        continue;
                    }

                    if let Some(row) = self.row_for_time(
                        note.time_seconds,
                        snapshot.song_time_seconds,
                        judgement_row,
                    ) {
                        self.draw_note(
                            lane,
                            row,
                            note.time_seconds - snapshot.song_time_seconds,
                            " O ",
                        )?;
                    }
                }
                NoteKind::Hold { end_time_seconds } => {
                    if end_time_seconds < visible_start || note.time_seconds > visible_end {
                        continue;
                    }

                    self.draw_hold(
                        lane,
                        note.time_seconds,
                        end_time_seconds,
                        snapshot.song_time_seconds,
                        judgement_row,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn render_lane_labels(
        &mut self,
        lane_count: usize,
        row: u16,
        active_lanes: [bool; 4],
    ) -> Result<(), Box<dyn Error>> {
        for lane in 0..lane_count {
            let x = BASE_X + lane as u16 * LANE_WIDTH + 2;
            let active = lane < active_lanes.len() && active_lanes[lane];
            let color = if active { Color::Yellow } else { Color::White };
            queue!(
                self.stdout,
                MoveTo(x, row),
                SetForegroundColor(color),
                Print(lane_label(lane)),
                ResetColor
            )?;
        }

        Ok(())
    }

    fn render_messages(
        &mut self,
        width: u16,
        first_row: u16,
        snapshot: &HighwaySnapshot<'_>,
    ) -> Result<(), Box<dyn Error>> {
        let max_messages = 3;
        let start = snapshot.messages.len().saturating_sub(max_messages);

        for (index, message) in snapshot.messages.iter().skip(start).enumerate() {
            queue!(
                self.stdout,
                MoveTo(0, first_row + index as u16),
                Print(truncate(message, width))
            )?;
        }

        Ok(())
    }

    fn draw_hold(
        &mut self,
        lane: usize,
        start_time: f64,
        end_time: f64,
        song_time: f64,
        judgement_row: u16,
    ) -> Result<(), Box<dyn Error>> {
        let start_row = self
            .row_for_time(start_time, song_time, judgement_row)
            .unwrap_or(judgement_row);
        let end_row = self
            .row_for_time(end_time, song_time, judgement_row)
            .unwrap_or(TOP_ROW);
        let first = end_row.min(start_row);
        let last = end_row.max(start_row);

        queue!(self.stdout, SetForegroundColor(Color::Blue))?;
        for row in first..=last {
            self.draw_lane_text(lane, row, " | ")?;
        }
        queue!(self.stdout, ResetColor)?;

        self.draw_note(lane, start_row, start_time - song_time, " H ")?;

        Ok(())
    }

    fn draw_note(
        &mut self,
        lane: usize,
        row: u16,
        delta_seconds: f64,
        text: &str,
    ) -> Result<(), Box<dyn Error>> {
        let color = note_color(delta_seconds);
        queue!(self.stdout, SetForegroundColor(color))?;
        self.draw_lane_text(lane, row, text)?;
        queue!(self.stdout, ResetColor)?;
        Ok(())
    }

    fn draw_lane_text(&mut self, lane: usize, row: u16, text: &str) -> Result<(), Box<dyn Error>> {
        let x = BASE_X + lane as u16 * LANE_WIDTH + 1;
        queue!(self.stdout, MoveTo(x, row), Print(text))?;
        Ok(())
    }

    fn row_for_time(
        &self,
        time_seconds: f64,
        song_time_seconds: f64,
        judgement_row: u16,
    ) -> Option<u16> {
        let delta = time_seconds - song_time_seconds;
        if delta < -0.180 || delta > self.lookahead_seconds {
            return None;
        }

        let play_rows = judgement_row.saturating_sub(TOP_ROW).max(1);
        let normalized = (self.lookahead_seconds - delta) / self.lookahead_seconds;
        let row = TOP_ROW as f64 + normalized * play_rows as f64;
        Some((row.round() as u16).clamp(TOP_ROW, judgement_row))
    }
}

impl Drop for TerminalHighway {
    fn drop(&mut self) {
        let _ = execute!(self.stdout, Show, LeaveAlternateScreen);
    }
}

fn lane_label(lane: usize) -> &'static str {
    match lane {
        0 => "D",
        1 => "F",
        2 => "J",
        3 => "K",
        _ => ".",
    }
}

fn note_color(delta_seconds: f64) -> Color {
    if delta_seconds < -0.050 {
        Color::DarkRed
    } else if delta_seconds.abs() <= 0.050 {
        Color::Green
    } else {
        Color::Cyan
    }
}

pub fn judgement_message(kind: &str, result: rhythm_core::JudgementResult) -> String {
    match result.delta_seconds {
        Some(delta) => format!(
            "{kind} note={} lane={} {:?} delta={:.1}ms",
            result.note_id.as_u32(),
            result.lane.as_u8(),
            result.rating,
            delta * 1_000.0
        ),
        None => format!(
            "{kind} note={} lane={} {:?}",
            result.note_id.as_u32(),
            result.lane.as_u8(),
            result.rating
        ),
    }
}

pub fn push_message(messages: &mut VecDeque<String>, message: String) {
    const MAX_MESSAGES: usize = 8;
    if messages.len() == MAX_MESSAGES {
        messages.pop_front();
    }
    messages.push_back(message);
}

pub fn active_lanes(input_flashes: [Option<std::time::Instant>; 4]) -> [bool; 4] {
    let now = std::time::Instant::now();
    input_flashes.map(|instant| {
        instant
            .map(|instant| now.duration_since(instant).as_millis() < 90)
            .unwrap_or(false)
    })
}

fn truncate(value: &str, width: u16) -> String {
    let width = width as usize;
    if value.len() <= width {
        return value.to_owned();
    }

    value.chars().take(width.saturating_sub(1)).collect()
}
