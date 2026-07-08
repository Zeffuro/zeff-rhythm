use super::metrics::{MetricStats, print_metric};
use crate::platform::input::NativeInputEvent;
use crate::render::telemetry::{RenderTelemetry, RenderTimingSample};
use rhythm_core::JudgementResult;
use std::error::Error;
use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::Path;

pub struct PlayReport {
    event_log: Option<EventLogWriter>,
    input_presses: usize,
    input_releases: usize,
    initial_focus_gained: usize,
    initial_focus_lost: usize,
    focus_gained: usize,
    focus_lost: usize,
    unmatched_inputs: usize,
    misses: usize,
    hit_delta_ms: Vec<f64>,
    abs_hit_delta_ms: Vec<f64>,
    input_queue_age_ms: Vec<f64>,
    frame_time_ms: Vec<f64>,
    render_cost_ms: Vec<f64>,
    audio_output_latency_ms: Vec<f64>,
    render_telemetry: RenderTelemetry,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayReportSummary {
    pub input_presses: usize,
    pub input_releases: usize,
    pub initial_focus_gained: usize,
    pub initial_focus_lost: usize,
    pub focus_gained: usize,
    pub focus_lost: usize,
    pub unmatched_inputs: usize,
    pub hits: usize,
    pub misses: usize,
    pub hit_delta_ms: Option<MetricStats>,
    pub hit_delta_samples_ms: Vec<f64>,
    pub abs_hit_delta_ms: Option<MetricStats>,
    pub input_queue_age_ms: Option<MetricStats>,
    pub frame_time_ms: Option<MetricStats>,
    pub render_cost_ms: Option<MetricStats>,
    pub audio_output_latency_ms: Option<MetricStats>,
    pub render_frame_samples: usize,
    pub render_gpu_samples: usize,
}

impl PlayReport {
    pub fn new(event_log_path: Option<&Path>) -> Result<Self, Box<dyn Error>> {
        let event_log = match event_log_path {
            Some(path) => Some(EventLogWriter::new(path)?),
            None => None,
        };

        Ok(Self {
            event_log,
            input_presses: 0,
            input_releases: 0,
            initial_focus_gained: 0,
            initial_focus_lost: 0,
            focus_gained: 0,
            focus_lost: 0,
            unmatched_inputs: 0,
            misses: 0,
            hit_delta_ms: Vec::new(),
            abs_hit_delta_ms: Vec::new(),
            input_queue_age_ms: Vec::new(),
            frame_time_ms: Vec::new(),
            render_cost_ms: Vec::new(),
            audio_output_latency_ms: Vec::new(),
            render_telemetry: RenderTelemetry::default(),
        })
    }

    pub fn record_input_press(
        &mut self,
        lane: u8,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.input_presses += 1;
        self.record_input_queue_age(input);
        self.write_input_event("input_press", lane, input, chart_time_seconds)
    }

    pub fn record_input_release(
        &mut self,
        lane: u8,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.input_releases += 1;
        self.record_input_queue_age(input);
        self.write_input_event("input_release", lane, input, chart_time_seconds)
    }

    pub fn record_unmatched_input(
        &mut self,
        lane: u8,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.unmatched_inputs += 1;
        self.write_input_event("unmatched_input", lane, input, chart_time_seconds)
    }

    pub fn record_focus_gained(
        &mut self,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.focus_gained += 1;
        self.write_system_event("focus_gained", input, chart_time_seconds)
    }

    pub fn record_initial_focus(
        &mut self,
        has_focus: bool,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        if has_focus {
            self.initial_focus_gained += 1;
            self.write_focus_state_event("focus_initial_gained", chart_time_seconds)
        } else {
            self.initial_focus_lost += 1;
            self.write_focus_state_event("focus_initial_lost", chart_time_seconds)
        }
    }

    pub fn record_focus_lost(
        &mut self,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.focus_lost += 1;
        self.write_system_event("focus_lost", input, chart_time_seconds)
    }

    pub fn record_hit(
        &mut self,
        result: JudgementResult,
        input: NativeInputEvent,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(delta_seconds) = result.delta_seconds {
            let delta_ms = delta_seconds * 1_000.0;
            self.hit_delta_ms.push(delta_ms);
            self.abs_hit_delta_ms.push(delta_ms.abs());
        }

        if let Some(event_log) = self.event_log.as_mut() {
            event_log.write_row(EventLogRow {
                event: "hit",
                chart_time_seconds: result.input_time_seconds,
                lane: Some(result.lane.as_u8()),
                note_id: Some(result.note_id.as_u32()),
                rating: Some(format!("{:?}", result.rating)),
                delta_ms: result.delta_seconds.map(|seconds| seconds * 1_000.0),
                input_queue_age_ms: input.queue_age_ms,
                input_timestamp_ns: input.source_timestamp_ns,
                input_source: Some(input.source.as_str()),
                input_timestamp_kind: Some(input.timestamp_kind.as_str()),
                scheduled_time_seconds: Some(result.scheduled_time_seconds),
            })?;
        }

        Ok(())
    }

    pub fn record_miss(
        &mut self,
        result: JudgementResult,
        observed_chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        self.misses += 1;

        if let Some(event_log) = self.event_log.as_mut() {
            event_log.write_row(EventLogRow {
                event: "miss",
                chart_time_seconds: Some(observed_chart_time_seconds),
                lane: Some(result.lane.as_u8()),
                note_id: Some(result.note_id.as_u32()),
                rating: Some(format!("{:?}", result.rating)),
                delta_ms: None,
                input_queue_age_ms: None,
                input_timestamp_ns: None,
                input_source: None,
                input_timestamp_kind: None,
                scheduled_time_seconds: Some(result.scheduled_time_seconds),
            })?;
        }

        Ok(())
    }

    pub fn record_render_sample(
        &mut self,
        frame_time_ms: f64,
        render_cost_ms: f64,
        audio_output_latency_seconds: Option<f64>,
    ) {
        if frame_time_ms.is_finite() && frame_time_ms >= 0.0 {
            self.frame_time_ms.push(frame_time_ms);
        }
        if render_cost_ms.is_finite() && render_cost_ms >= 0.0 {
            self.render_cost_ms.push(render_cost_ms);
        }
        self.render_telemetry.record(RenderTimingSample {
            frame_interval_ms: Some(frame_time_ms),
            command_encode_ms: Some(render_cost_ms),
            ..RenderTimingSample::default()
        });
        self.record_audio_output_latency_seconds(audio_output_latency_seconds);
    }

    pub fn record_audio_output_latency_seconds(
        &mut self,
        audio_output_latency_seconds: Option<f64>,
    ) {
        let Some(seconds) = audio_output_latency_seconds else {
            return;
        };

        let milliseconds = seconds * 1_000.0;
        if milliseconds.is_finite() && milliseconds >= 0.0 {
            self.audio_output_latency_ms.push(milliseconds);
        }
    }

    pub fn print_summary(&self) {
        println!(
            "run_summary input_presses={} input_releases={} focus_initial_gained={} focus_initial_lost={} focus_gained={} focus_lost={} unmatched_inputs={} hits={} misses={}",
            self.input_presses,
            self.input_releases,
            self.initial_focus_gained,
            self.initial_focus_lost,
            self.focus_gained,
            self.focus_lost,
            self.unmatched_inputs,
            self.hit_delta_ms.len(),
            self.misses
        );
        print_metric("hit_delta_ms", &self.hit_delta_ms);
        print_metric("abs_hit_delta_ms", &self.abs_hit_delta_ms);
        print_metric("input_queue_age_ms", &self.input_queue_age_ms);
        print_metric("frame_time_ms", &self.frame_time_ms);
        print_metric("render_cost_ms", &self.render_cost_ms);
        print_metric("audio_output_latency_ms", &self.audio_output_latency_ms);
        println!(
            "render_telemetry frame_samples={} gpu_samples={}",
            self.render_telemetry.frame_sample_count(),
            self.render_telemetry.gpu_sample_count()
        );
    }

    pub fn summary(&self) -> PlayReportSummary {
        PlayReportSummary {
            input_presses: self.input_presses,
            input_releases: self.input_releases,
            initial_focus_gained: self.initial_focus_gained,
            initial_focus_lost: self.initial_focus_lost,
            focus_gained: self.focus_gained,
            focus_lost: self.focus_lost,
            unmatched_inputs: self.unmatched_inputs,
            hits: self.hit_delta_ms.len(),
            misses: self.misses,
            hit_delta_ms: MetricStats::from_samples(&self.hit_delta_ms),
            hit_delta_samples_ms: self.hit_delta_ms.clone(),
            abs_hit_delta_ms: MetricStats::from_samples(&self.abs_hit_delta_ms),
            input_queue_age_ms: MetricStats::from_samples(&self.input_queue_age_ms),
            frame_time_ms: MetricStats::from_samples(&self.frame_time_ms),
            render_cost_ms: MetricStats::from_samples(&self.render_cost_ms),
            audio_output_latency_ms: MetricStats::from_samples(&self.audio_output_latency_ms),
            render_frame_samples: self.render_telemetry.frame_sample_count(),
            render_gpu_samples: self.render_telemetry.gpu_sample_count(),
        }
    }

    pub fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(event_log) = self.event_log.as_mut() {
            event_log.flush()?;
        }

        Ok(())
    }

    fn record_input_queue_age(&mut self, input: NativeInputEvent) {
        if let Some(queue_age_ms) = input.queue_age_ms {
            self.input_queue_age_ms.push(queue_age_ms);
        }
    }

    fn write_input_event(
        &mut self,
        event: &'static str,
        lane: u8,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(event_log) = self.event_log.as_mut() {
            event_log.write_row(EventLogRow {
                event,
                chart_time_seconds: Some(chart_time_seconds),
                lane: Some(lane),
                note_id: None,
                rating: None,
                delta_ms: None,
                input_queue_age_ms: input.queue_age_ms,
                input_timestamp_ns: input.source_timestamp_ns,
                input_source: Some(input.source.as_str()),
                input_timestamp_kind: Some(input.timestamp_kind.as_str()),
                scheduled_time_seconds: None,
            })?;
        }

        Ok(())
    }

    fn write_system_event(
        &mut self,
        event: &'static str,
        input: NativeInputEvent,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(event_log) = self.event_log.as_mut() {
            event_log.write_row(EventLogRow {
                event,
                chart_time_seconds: Some(chart_time_seconds),
                lane: None,
                note_id: None,
                rating: None,
                delta_ms: None,
                input_queue_age_ms: input.queue_age_ms,
                input_timestamp_ns: input.source_timestamp_ns,
                input_source: Some(input.source.as_str()),
                input_timestamp_kind: Some(input.timestamp_kind.as_str()),
                scheduled_time_seconds: None,
            })?;
        }

        Ok(())
    }

    fn write_focus_state_event(
        &mut self,
        event: &'static str,
        chart_time_seconds: f64,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(event_log) = self.event_log.as_mut() {
            event_log.write_row(EventLogRow {
                event,
                chart_time_seconds: Some(chart_time_seconds),
                lane: None,
                note_id: None,
                rating: None,
                delta_ms: None,
                input_queue_age_ms: None,
                input_timestamp_ns: None,
                input_source: None,
                input_timestamp_kind: None,
                scheduled_time_seconds: None,
            })?;
        }

        Ok(())
    }
}

struct EventLogWriter {
    writer: BufWriter<File>,
}

impl EventLogWriter {
    fn new(path: &Path) -> Result<Self, Box<dyn Error>> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            create_dir_all(parent)?;
        }

        let mut writer = BufWriter::new(File::create(path)?);
        writeln!(
            writer,
            "event,chart_time_seconds,lane,note_id,rating,delta_ms,input_queue_age_ms,input_timestamp_ns,input_source,input_timestamp_kind,scheduled_time_seconds"
        )?;

        Ok(Self { writer })
    }

    fn write_row(&mut self, row: EventLogRow) -> Result<(), Box<dyn Error>> {
        writeln!(
            self.writer,
            "{},{},{},{},{},{},{},{},{},{},{}",
            row.event,
            format_optional_f64(row.chart_time_seconds),
            format_optional_u8(row.lane),
            format_optional_u32(row.note_id),
            row.rating.unwrap_or_default(),
            format_optional_f64(row.delta_ms),
            format_optional_f64(row.input_queue_age_ms),
            format_optional_u64(row.input_timestamp_ns),
            row.input_source.unwrap_or_default(),
            row.input_timestamp_kind.unwrap_or_default(),
            format_optional_f64(row.scheduled_time_seconds),
        )?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        self.writer.flush()?;
        Ok(())
    }
}

struct EventLogRow {
    event: &'static str,
    chart_time_seconds: Option<f64>,
    lane: Option<u8>,
    note_id: Option<u32>,
    rating: Option<String>,
    delta_ms: Option<f64>,
    input_queue_age_ms: Option<f64>,
    input_timestamp_ns: Option<u64>,
    input_source: Option<&'static str>,
    input_timestamp_kind: Option<&'static str>,
    scheduled_time_seconds: Option<f64>,
}

fn format_optional_f64(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.6}")).unwrap_or_default()
}

fn format_optional_u8(value: Option<u8>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn format_optional_u32(value: Option<u32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn format_optional_u64(value: Option<u64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}
