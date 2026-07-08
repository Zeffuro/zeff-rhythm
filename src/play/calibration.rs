use super::live_session::LiveSessionAssets;
use crate::platform::audio::{AudioClip, AudioStreamOptions, output_stream_target};
use rhythm_core::{Beat, Chart, ChartMetadata, LaneIndex, Note, NoteId, TimingPoint};
use std::error::Error;
use std::f64::consts::TAU;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationPattern {
    pub lane_count: u8,
    pub lane: u8,
    pub first_note_seconds: f64,
    pub interval_seconds: f64,
    pub note_count: u32,
    pub tail_seconds: f64,
    pub click_seconds: f64,
    pub click_frequency_hz: f64,
}

impl CalibrationPattern {
    fn note_time_seconds(self, index: u32) -> f64 {
        self.first_note_seconds + index as f64 * self.interval_seconds
    }

    fn duration_seconds(self) -> f64 {
        self.note_time_seconds(self.note_count.saturating_sub(1)) + self.tail_seconds
    }
}

impl Default for CalibrationPattern {
    fn default() -> Self {
        Self {
            lane_count: 1,
            lane: 0,
            first_note_seconds: 2.5,
            interval_seconds: 0.75,
            note_count: 24,
            tail_seconds: 1.5,
            click_seconds: 0.045,
            click_frequency_hz: 1_200.0,
        }
    }
}

pub(crate) fn build_generated_calibration_assets(
    audio: &AudioStreamOptions,
    pattern: CalibrationPattern,
) -> Result<LiveSessionAssets, Box<dyn Error>> {
    let target = output_stream_target(audio)?;
    let sample_rate = target.config.sample_rate;

    Ok(LiveSessionAssets {
        chart: generate_calibration_chart(pattern),
        audio_label: "generated-calibration-clicks".to_owned(),
        clip: Arc::new(generate_calibration_clip(pattern, sample_rate)),
        target,
    })
}

pub fn generate_calibration_chart(pattern: CalibrationPattern) -> Chart {
    let lane_count = pattern.lane_count.max(1);
    let lane = LaneIndex::new(pattern.lane.min(lane_count - 1));
    let note_count = pattern.note_count.max(1);
    let mut chart = Chart::new(lane_count);
    *chart.metadata_mut() = ChartMetadata {
        title: "Generated Calibration".to_owned(),
        artist: "zeff-rhythm".to_owned(),
        source: Some("generated".to_owned()),
        audio_filename: None,
    };
    chart.set_timing_points(vec![TimingPoint::new(
        Beat::new(0.0),
        0.0,
        pattern.interval_seconds.max(0.1),
    )]);

    for index in 0..note_count {
        chart.push_note(Note::tap(
            NoteId::new(index + 1),
            lane,
            pattern.note_time_seconds(index),
        ));
    }

    chart
}

pub fn generate_calibration_clip(pattern: CalibrationPattern, sample_rate: u32) -> AudioClip {
    let sample_rate = sample_rate.max(1);
    let channels = 2;
    let frame_count = (pattern.duration_seconds().max(0.1) * sample_rate as f64).ceil() as usize;
    let mut samples = vec![0.0; frame_count * channels];

    for index in 0..pattern.note_count.max(1) {
        add_click(&mut samples, channels, sample_rate, pattern, index);
    }

    AudioClip {
        samples,
        channels,
        sample_rate,
    }
}

fn add_click(
    samples: &mut [f32],
    channels: usize,
    sample_rate: u32,
    pattern: CalibrationPattern,
    note_index: u32,
) {
    let start_frame = (pattern.note_time_seconds(note_index) * sample_rate as f64).round() as usize;
    let click_frames = (pattern.click_seconds.max(0.001) * sample_rate as f64).round() as usize;
    let frequency = pattern.click_frequency_hz.max(20.0);

    for offset in 0..click_frames {
        let frame = start_frame + offset;
        let sample_index = frame * channels;
        if sample_index >= samples.len() {
            break;
        }

        let progress = offset as f64 / click_frames.max(1) as f64;
        let envelope = (1.0 - progress).powi(2);
        let seconds = offset as f64 / sample_rate as f64;
        let value = ((TAU * frequency * seconds).sin() * envelope * 0.42) as f32;

        for channel in 0..channels {
            samples[sample_index + channel] =
                (samples[sample_index + channel] + value).clamp(-1.0_f32, 1.0_f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CalibrationPattern, generate_calibration_chart, generate_calibration_clip};

    #[test]
    fn generated_chart_is_single_lane_and_predictable() {
        let pattern = CalibrationPattern {
            note_count: 4,
            ..CalibrationPattern::default()
        };
        let chart = generate_calibration_chart(pattern);

        assert_eq!(chart.lane_count(), 1);
        assert_eq!(chart.notes().len(), 4);
        assert_eq!(chart.notes()[0].time_seconds, 2.5);
        assert_eq!(chart.notes()[3].time_seconds, 4.75);
        assert_eq!(chart.metadata().title, "Generated Calibration");
    }

    #[test]
    fn generated_clip_contains_click_energy_at_note_times() {
        let pattern = CalibrationPattern {
            note_count: 1,
            ..CalibrationPattern::default()
        };
        let clip = generate_calibration_clip(pattern, 48_000);
        let note_frame = (pattern.first_note_seconds * clip.sample_rate as f64) as usize;
        let window = &clip.samples[note_frame * clip.channels..(note_frame + 96) * clip.channels];

        assert!(window.iter().any(|sample| sample.abs() > 0.1));
        assert_eq!(clip.channels, 2);
        assert_eq!(clip.sample_rate, 48_000);
    }
}
