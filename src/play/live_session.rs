use super::assets::LoadedPlaySession;
use super::{
    HighwaySnapshot, JudgementCounts, PlayReport, PlaySessionOptions, judgement_message,
    load_play_session_assets, push_message,
};
use crate::platform::audio::{AudioClip, OutputStreamTarget, PlaybackClock, build_clip_stream};
use crate::platform::input::{NativeInputEvent, NativeInputEventKind};
use cpal::BufferSize;
use cpal::traits::StreamTrait;
use rhythm_core::{
    Chart, GameKey, InputEvent as CoreInputEvent, JudgementResult, JudgementWindows, LaneIndex,
    RhythmEngine,
};
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::PlayReportSummary;

pub struct LivePlaySession {
    options: PlaySessionOptions,
    stream: cpal::Stream,
    chart_clock: LiveChartClock,
    engine: RhythmEngine,
    counts: JudgementCounts,
    judged_note_ids: HashSet<u32>,
    messages: VecDeque<String>,
    misses: Vec<JudgementResult>,
    input_flashes: [Option<Instant>; 4],
    report: PlayReport,
    audio: LiveAudioSummary,
    hard_end_seconds: f64,
    finished: bool,
    summary_printed: bool,
}

pub(crate) struct LiveSessionAssets {
    pub chart: Chart,
    pub audio_label: String,
    pub clip: Arc<AudioClip>,
    pub target: OutputStreamTarget,
}

impl From<LoadedPlaySession> for LiveSessionAssets {
    fn from(assets: LoadedPlaySession) -> Self {
        Self {
            chart: assets.chart,
            audio_label: assets.audio_path.display().to_string(),
            clip: assets.clip,
            target: assets.target,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveRunSummary {
    pub title: String,
    pub input_offset_ms: f64,
    pub audio: LiveAudioSummary,
    pub judged_count: usize,
    pub complete: bool,
    pub counts: JudgementCounts,
    pub report: PlayReportSummary,
    pub event_log_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveAudioSummary {
    pub host_name: String,
    pub device_label: String,
    pub device_id: Option<String>,
    pub sample_rate: u32,
    pub buffer_frames: Option<u32>,
}

impl LiveRunSummary {
    pub fn suggested_input_offset_adjustment_ms(&self) -> Option<f64> {
        self.report.hit_delta_ms.map(|stats| -stats.mean)
    }

    pub fn suggested_total_input_offset_ms(&self) -> Option<f64> {
        self.suggested_input_offset_adjustment_ms()
            .map(|adjustment| self.input_offset_ms + adjustment)
    }
}

impl LiveAudioSummary {
    fn from_target(target: &OutputStreamTarget) -> Self {
        Self {
            host_name: target.host_name.clone(),
            device_label: target.device.to_string(),
            device_id: target.device_id.clone(),
            sample_rate: target.config.sample_rate,
            buffer_frames: match target.config.buffer_size {
                BufferSize::Fixed(frames) => Some(frames),
                _ => None,
            },
        }
    }
}

impl LivePlaySession {
    pub fn start(options: PlaySessionOptions) -> Result<Self, Box<dyn Error>> {
        let assets = LiveSessionAssets::from(load_play_session_assets(&options)?);
        Self::start_with_assets(options, assets)
    }

    pub(crate) fn start_with_assets(
        options: PlaySessionOptions,
        assets: LiveSessionAssets,
    ) -> Result<Self, Box<dyn Error>> {
        let (stream, clock) = build_clip_stream(
            &assets.target.device,
            assets.target.config.clone(),
            assets.target.sample_format,
            Arc::clone(&assets.clip),
        )?;
        let windows = JudgementWindows::default();
        let chart_start_seconds = options.chart_start_seconds(&assets.chart);
        let start_delay_seconds = options.effective_start_delay_seconds();
        let hard_end_seconds = match options.max_seconds {
            Some(seconds) => seconds,
            None => hard_end_seconds(&assets.chart, assets.clip.duration_seconds(), windows),
        };
        let report = PlayReport::new(options.event_log_path.as_deref())?;
        let mut messages = VecDeque::new();
        push_message(
            &mut messages,
            format!(
                "lookahead {:.1}s; hit notes at the === line",
                options.lookahead_seconds
            ),
        );

        println!(
            "live_session_start chart={} title={} audio={} device={} input_offset_ms={:.3} lookahead_seconds={:.3} chart_start_seconds={:.3}",
            options.chart_path.display(),
            assets.chart.metadata().title,
            assets.audio_label,
            assets.target.device,
            options.input_offset_ms,
            options.lookahead_seconds,
            chart_start_seconds
        );

        let audio = LiveAudioSummary::from_target(&assets.target);

        Ok(Self {
            options,
            stream,
            chart_clock: LiveChartClock::new(clock, chart_start_seconds, start_delay_seconds),
            engine: RhythmEngine::new(assets.chart, windows),
            counts: JudgementCounts::default(),
            judged_note_ids: HashSet::new(),
            messages,
            misses: Vec::new(),
            input_flashes: [None; 4],
            report,
            audio,
            hard_end_seconds,
            finished: false,
            summary_printed: false,
        })
    }

    pub fn update(&mut self) -> Result<(), Box<dyn Error>> {
        if self.finished {
            return Ok(());
        }

        self.chart_clock.start_audio_if_due(&self.stream)?;
        let countdown_active = self.chart_clock.countdown_seconds().is_some();
        let chart_time_seconds = self.chart_clock.chart_time_seconds();

        if !countdown_active {
            self.misses.clear();
            self.engine
                .collect_misses(chart_time_seconds, &mut self.misses);
            let misses = self.misses.drain(..).collect::<Vec<_>>();
            for miss in misses {
                self.report.record_miss(miss, chart_time_seconds)?;
                self.record_judgement("miss", miss);
            }
        }

        if (self.engine.is_complete() && self.chart_clock.is_finished())
            || chart_time_seconds >= self.hard_end_seconds
        {
            self.finished = true;
        }

        Ok(())
    }

    pub fn process_input(&mut self, input: NativeInputEvent) -> Result<bool, Box<dyn Error>> {
        match input.kind {
            NativeInputEventKind::Quit => {
                self.finished = true;
                Ok(true)
            }
            NativeInputEventKind::LanePress(lane) => {
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report
                    .record_input_press(lane, input, input_chart_time)?;
                if lane as usize >= self.engine.chart().lane_count() as usize {
                    return Ok(false);
                }
                if lane as usize >= self.input_flashes.len() {
                    return Ok(false);
                }
                self.input_flashes[lane as usize] = Some(input.received_time);

                if self.chart_clock.countdown_seconds().is_some() {
                    return Ok(false);
                }

                self.handle_lane_press(lane, input)?;
                Ok(false)
            }
            NativeInputEventKind::LaneRelease(lane) => {
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report
                    .record_input_release(lane, input, input_chart_time)?;
                Ok(false)
            }
            NativeInputEventKind::FocusGained => {
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report.record_focus_gained(input, input_chart_time)?;
                push_message(&mut self.messages, "focus gained".to_owned());
                Ok(false)
            }
            NativeInputEventKind::FocusLost => {
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report.record_focus_lost(input, input_chart_time)?;
                push_message(&mut self.messages, "focus lost".to_owned());
                Ok(false)
            }
        }
    }

    pub fn record_render_sample(&mut self, frame_time_ms: f64, render_cost_ms: f64) {
        self.report.record_render_sample(
            frame_time_ms,
            render_cost_ms,
            self.chart_clock.output_latency_seconds(),
        );
    }

    pub fn snapshot(&self) -> HighwaySnapshot<'_> {
        HighwaySnapshot {
            chart: self.engine.chart(),
            judged_note_ids: &self.judged_note_ids,
            counts: &self.counts,
            messages: &self.messages,
            active_lanes: active_lanes(self.input_flashes),
            song_time_seconds: self.chart_clock.chart_time_seconds(),
            scheduled_time_seconds: self.chart_clock.scheduled_time_seconds(),
            audio_duration_seconds: self.chart_clock.duration_seconds(),
            end_seconds: self.hard_end_seconds,
            judged_count: self.engine.judged_count(),
            output_latency_seconds: self.chart_clock.output_latency_seconds(),
            countdown_seconds: self.chart_clock.countdown_seconds(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn print_summary(&mut self) -> Result<(), Box<dyn Error>> {
        if self.summary_printed {
            return Ok(());
        }

        self.summary_printed = true;
        println!("judged={}", self.engine.judged_count());
        println!("complete={}", self.engine.is_complete());
        println!("marvelous={}", self.counts.marvelous);
        println!("perfect={}", self.counts.perfect);
        println!("great={}", self.counts.great);
        println!("good={}", self.counts.good);
        println!("miss={}", self.counts.miss);
        self.report.print_summary();
        self.report.flush()?;
        if let Some(path) = self.options.event_log_path.as_ref() {
            println!("event_log_written={}", path.display());
        }

        Ok(())
    }

    pub fn summary(&self) -> LiveRunSummary {
        LiveRunSummary {
            title: self.engine.chart().metadata().title.clone(),
            input_offset_ms: self.options.input_offset_ms,
            audio: self.audio.clone(),
            judged_count: self.engine.judged_count(),
            complete: self.engine.is_complete(),
            counts: self.counts,
            report: self.report.summary(),
            event_log_path: self.options.event_log_path.clone(),
        }
    }

    fn handle_lane_press(
        &mut self,
        lane_number: u8,
        input: NativeInputEvent,
    ) -> Result<(), Box<dyn Error>> {
        let lane = LaneIndex::new(lane_number);
        let input_time_seconds = self.chart_clock.chart_time_at(input.event_time)
            + self.options.input_offset_ms / 1_000.0;
        let result = self.engine.submit_input(CoreInputEvent {
            key: GameKey::Lane(lane),
            pressed: true,
            time_seconds: input_time_seconds,
        });

        match result {
            Some(result) => {
                self.report.record_hit(result, input)?;
                self.record_judgement("hit", result);
            }
            None => {
                self.report
                    .record_unmatched_input(lane.as_u8(), input, input_time_seconds)?;
                push_message(
                    &mut self.messages,
                    format!(
                        "input lane={} song_time={input_time_seconds:.6}s unmatched",
                        lane.as_u8()
                    ),
                );
            }
        }

        Ok(())
    }

    fn record_judgement(&mut self, kind: &str, result: JudgementResult) {
        self.counts.add(result);
        self.judged_note_ids.insert(result.note_id.as_u32());
        push_message(&mut self.messages, judgement_message(kind, result));
    }
}

struct LiveChartClock {
    audio_clock: PlaybackClock,
    chart_start_seconds: f64,
    chart_start_wall: Instant,
    audio_started: bool,
}

impl LiveChartClock {
    fn new(audio_clock: PlaybackClock, chart_start_seconds: f64, start_delay_seconds: f64) -> Self {
        Self {
            audio_clock,
            chart_start_seconds,
            chart_start_wall: Instant::now() + Duration::from_secs_f64(start_delay_seconds),
            audio_started: false,
        }
    }

    fn chart_time_seconds(&self) -> f64 {
        if self.audio_started {
            return self.audio_clock.song_time_seconds();
        }

        self.chart_time_at(Instant::now())
    }

    fn chart_time_at(&self, time: Instant) -> f64 {
        if self.audio_started {
            return self.audio_clock.song_time_at(time);
        }

        if time < self.chart_start_wall {
            return self.chart_start_seconds;
        }

        self.chart_start_seconds + time.duration_since(self.chart_start_wall).as_secs_f64()
    }

    fn start_audio_if_due(&mut self, stream: &cpal::Stream) -> Result<(), Box<dyn Error>> {
        if !self.audio_started && self.chart_time_seconds() >= 0.0 {
            stream.play()?;
            self.audio_started = true;
        }

        Ok(())
    }

    fn countdown_seconds(&self) -> Option<f64> {
        let now = Instant::now();
        if now < self.chart_start_wall {
            Some(self.chart_start_wall.duration_since(now).as_secs_f64())
        } else {
            None
        }
    }

    fn scheduled_time_seconds(&self) -> f64 {
        self.audio_clock.scheduled_time_seconds()
    }

    fn duration_seconds(&self) -> f64 {
        self.audio_clock.duration_seconds()
    }

    fn output_latency_seconds(&self) -> Option<f64> {
        self.audio_clock.output_latency_seconds()
    }

    fn is_finished(&self) -> bool {
        self.audio_started && self.audio_clock.is_finished()
    }
}

fn active_lanes(input_flashes: [Option<Instant>; 4]) -> [bool; 4] {
    let now = Instant::now();
    input_flashes.map(|instant| {
        instant
            .map(|instant| now.duration_since(instant).as_millis() < 90)
            .unwrap_or(false)
    })
}

fn hard_end_seconds(
    chart: &rhythm_core::Chart,
    clip_duration_seconds: f64,
    windows: JudgementWindows,
) -> f64 {
    chart
        .notes()
        .last()
        .map(|note| note.time_seconds + windows.miss_seconds + 1.0)
        .unwrap_or(clip_duration_seconds)
        .max(clip_duration_seconds)
}

#[cfg(test)]
mod tests {
    use super::hard_end_seconds;
    use rhythm_core::{Chart, LaneIndex, Note, NoteId};

    #[test]
    fn hard_end_keeps_audio_duration_when_chart_is_shorter() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(0), 1.0));

        assert_eq!(
            hard_end_seconds(&chart, 10.0, rhythm_core::JudgementWindows::default()),
            10.0
        );
    }
}
