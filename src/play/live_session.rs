mod pause;
mod transport;

use super::assets::LoadedPlaySession;
use super::chart_clock::{ChartClock, hard_end_seconds};
use super::{
    HighwaySnapshot, JudgementCounts, PlayReport, PlaySessionOptions, judgement_message,
    push_message,
};
use crate::platform::audio::{
    AudioClip, OutputStreamTarget, PlaybackVolume, build_clip_stream_with_volume,
};
use crate::platform::input::{NativeInputEvent, NativeInputEventKind};
use cpal::BufferSize;
use rhythm_core::{
    Chart, GameKey, InputEvent as CoreInputEvent, JudgementPhase, JudgementResult,
    JudgementWindows, LaneIndex, RhythmEngine,
};
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::PlayReportSummary;

pub struct LivePlaySession {
    options: PlaySessionOptions,
    stream: cpal::Stream,
    volume: PlaybackVolume,
    chart_clock: ChartClock,
    engine: RhythmEngine,
    counts: JudgementCounts,
    judged_note_ids: HashSet<u32>,
    messages: VecDeque<String>,
    misses: Vec<JudgementResult>,
    report: PlayReport,
    audio: LiveAudioSummary,
    hard_end_seconds: f64,
    finished: bool,
    summary_printed: bool,
    recent_judgement: Option<(JudgementResult, Instant)>,
    pause: pause::SessionPause,
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
    pub requested_buffer_frames: Option<u32>,
    pub first_callback_frames: Option<usize>,
    pub channel_count: u16,
    pub sample_format: String,
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
    pub(crate) fn from_target(target: &OutputStreamTarget) -> Self {
        Self {
            host_name: target.host_name.clone(),
            device_label: target.device.to_string(),
            device_id: target.device_id.clone(),
            sample_rate: target.config.sample_rate,
            requested_buffer_frames: match target.config.buffer_size {
                BufferSize::Fixed(frames) => Some(frames),
                _ => None,
            },
            first_callback_frames: None,
            channel_count: target.config.channels,
            sample_format: format!("{:?}", target.sample_format),
        }
    }

    fn with_first_callback_frames(mut self, first_callback_frames: Option<usize>) -> Self {
        self.first_callback_frames = first_callback_frames;
        self
    }
}

impl LivePlaySession {
    pub(crate) fn start_with_assets(
        options: PlaySessionOptions,
        assets: LiveSessionAssets,
    ) -> Result<Self, Box<dyn Error>> {
        let volume = PlaybackVolume::new(options.volume);
        let (stream, clock) =
            build_clip_stream_with_volume(&assets.target, Arc::clone(&assets.clip), volume.clone())
                .map_err(|error| format!("Could not open audio stream: {error}"))?;
        let windows = JudgementWindows::default();
        let chart_start_seconds = options.chart_start_seconds(&assets.chart);
        let start_delay_seconds = options.effective_start_delay_seconds();
        let hard_end_seconds = match options.max_seconds {
            Some(seconds) => seconds,
            None => hard_end_seconds(
                &assets.chart,
                assets.clip.duration_seconds(),
                windows,
                options.input_offset_ms,
            ),
        };
        let report = PlayReport::new(options.event_log_path.as_deref())
            .map_err(|error| format!("Could not write play log: {error}"))?;
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
            volume,
            chart_clock: ChartClock::new(clock, chart_start_seconds, start_delay_seconds),
            engine: RhythmEngine::new(assets.chart, windows),
            counts: JudgementCounts::default(),
            judged_note_ids: HashSet::new(),
            messages,
            misses: Vec::new(),
            report,
            audio,
            hard_end_seconds,
            finished: false,
            summary_printed: false,
            recent_judgement: None,
            pause: pause::SessionPause::default(),
        })
    }

    pub fn set_volume(&mut self, gain: f32) {
        self.volume.set_gain(gain);
        self.options.volume = self.volume.gain();
    }

    pub fn update(&mut self) -> Result<(), Box<dyn Error>> {
        if self.finished {
            return Ok(());
        }

        self.update_transport()?;
        if self.is_paused() {
            return Ok(());
        }

        self.chart_clock.start_audio_if_due(&self.stream)?;
        if !self.chart_clock.is_running() {
            return Ok(());
        }
        let countdown_active = self.chart_clock.countdown_seconds().is_some();
        let chart_time_seconds = self.chart_clock.chart_time_seconds();

        if !countdown_active {
            self.misses.clear();
            self.engine.collect_judgements(
                self.options.judgement_time_seconds(chart_time_seconds),
                &mut self.misses,
            );
            let results = self.misses.drain(..).collect::<Vec<_>>();
            for result in results {
                if result.phase == JudgementPhase::HoldTail {
                    self.report
                        .record_hold_tail(result, None, chart_time_seconds)?;
                    self.record_judgement("hold complete", result);
                } else {
                    self.report.record_miss(result, chart_time_seconds)?;
                    self.record_judgement("miss", result);
                }
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
        self.update_transport()?;
        match input.kind {
            NativeInputEventKind::Quit => {
                self.finished = true;
                Ok(true)
            }
            NativeInputEventKind::LanePress(lane) => {
                self.pause.set_lane(lane, true);
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                if self.is_paused() || !self.chart_clock.accepts_input_at(input.event_time) {
                    self.report
                        .record_paused_input(lane, true, input, input_chart_time)?;
                    return Ok(false);
                }
                self.report
                    .record_input_press(lane, input, input_chart_time)?;
                if lane as usize >= self.engine.chart().lane_count() as usize {
                    return Ok(false);
                }
                if lane >= 4 {
                    return Ok(false);
                }

                if self.chart_clock.countdown_seconds().is_some() {
                    return Ok(false);
                }

                self.handle_lane_press(lane, input)?;
                Ok(false)
            }
            NativeInputEventKind::LaneRelease(lane) => {
                self.pause.set_lane(lane, false);
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                if self.is_paused() || !self.chart_clock.accepts_input_at(input.event_time) {
                    self.report
                        .record_paused_input(lane, false, input, input_chart_time)?;
                    if self.pause.resume_was_interrupted()
                        || (!self.is_paused()
                            && self.engine.lane_has_active_hold(LaneIndex::new(lane)))
                    {
                        self.pause()?;
                    }
                    return Ok(false);
                }
                self.report
                    .record_input_release(lane, input, input_chart_time)?;
                self.handle_lane_release(lane, input, input_chart_time)?;
                Ok(false)
            }
            NativeInputEventKind::FocusGained => {
                self.pause.set_focus(true);
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report.record_focus_gained(input, input_chart_time)?;
                push_message(&mut self.messages, "focus gained".to_owned());
                Ok(false)
            }
            NativeInputEventKind::FocusLost => {
                self.pause.set_focus(false);
                let input_chart_time = self.chart_clock.chart_time_at(input.event_time);
                self.report.record_focus_lost(input, input_chart_time)?;
                self.pause()?;
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
            active_lanes: self.pause.physical_lanes,
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

    pub fn recent_judgement(&self) -> Option<JudgementResult> {
        self.recent_judgement
            .filter(|(_, time)| time.elapsed().as_millis() < 900)
            .map(|(result, _)| result)
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
            title: self.engine.chart().metadata().display_title().to_owned(),
            input_offset_ms: self.options.input_offset_ms,
            audio: self
                .audio
                .clone()
                .with_first_callback_frames(self.chart_clock.first_callback_frames()),
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
        let input_time_seconds = self
            .options
            .judgement_time_seconds(self.chart_clock.chart_time_at(input.event_time));
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
        self.recent_judgement = Some((result, Instant::now()));
        self.counts.add(result);
        if result.is_final() {
            self.judged_note_ids.insert(result.note_id.as_u32());
        }
        push_message(&mut self.messages, judgement_message(kind, result));
    }

    fn handle_lane_release(
        &mut self,
        lane: u8,
        input: NativeInputEvent,
        chart_time: f64,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(result) = self.engine.submit_input(CoreInputEvent {
            key: GameKey::Lane(LaneIndex::new(lane)),
            pressed: false,
            time_seconds: self.options.judgement_time_seconds(chart_time),
        }) {
            self.report
                .record_hold_tail(result, Some(input), chart_time)?;
            self.record_judgement("hold end", result);
        }
        Ok(())
    }
}

#[cfg(test)]
mod native_tests;
