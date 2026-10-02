use super::clip::AudioClip;
use super::devices::OutputStreamTarget;
use super::volume::{GainRamp, PlaybackVolume};
use cpal::traits::DeviceTrait;
use cpal::{FromSample, I24, SampleFormat, SizedSample};
use std::error::Error;
use std::sync::Arc;

pub fn build_preview_stream(
    target: &OutputStreamTarget,
    clip: Arc<AudioClip>,
    start_seconds: Option<f64>,
    duration_seconds: Option<f64>,
    volume: PlaybackVolume,
) -> Result<cpal::Stream, Box<dyn Error>> {
    if target.config.channels == 0 {
        return Err("preview output has no channels".into());
    }
    let segment = PreviewSegment::new(
        &clip,
        target.config.sample_rate,
        start_seconds,
        duration_seconds,
    )?;
    match target.sample_format {
        SampleFormat::I8 => build_for_format::<i8>(target, clip, segment, volume),
        SampleFormat::I16 => build_for_format::<i16>(target, clip, segment, volume),
        SampleFormat::I24 => build_for_format::<I24>(target, clip, segment, volume),
        SampleFormat::I32 => build_for_format::<i32>(target, clip, segment, volume),
        SampleFormat::I64 => build_for_format::<i64>(target, clip, segment, volume),
        SampleFormat::U8 => build_for_format::<u8>(target, clip, segment, volume),
        SampleFormat::U16 => build_for_format::<u16>(target, clip, segment, volume),
        SampleFormat::U32 => build_for_format::<u32>(target, clip, segment, volume),
        SampleFormat::U64 => build_for_format::<u64>(target, clip, segment, volume),
        SampleFormat::F32 => build_for_format::<f32>(target, clip, segment, volume),
        SampleFormat::F64 => build_for_format::<f64>(target, clip, segment, volume),
        _ => Err("unsupported preview sample format".into()),
    }
}

struct PreviewSegment {
    first_source_frame: usize,
    end_source_frame: usize,
    source_step: f64,
    output_frames: u64,
    fade_frames: u64,
}

impl PreviewSegment {
    fn new(
        clip: &AudioClip,
        output_sample_rate: u32,
        start_seconds: Option<f64>,
        duration_seconds: Option<f64>,
    ) -> Result<Self, Box<dyn Error>> {
        if clip.channels == 0
            || clip.sample_rate == 0
            || clip.samples.is_empty()
            || !clip.samples.len().is_multiple_of(clip.channels)
            || output_sample_rate == 0
        {
            return Err("invalid preview audio clip or sample rate".into());
        }
        let track_seconds = clip.duration_seconds();
        let start = start_seconds
            .filter(|start| start.is_finite() && *start >= 0.0 && *start < track_seconds)
            .unwrap_or(track_seconds * 0.4);
        let duration = duration_seconds
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .unwrap_or(12.0)
            .clamp(1.0, 30.0);
        let first_source_frame =
            ((start * clip.sample_rate as f64).floor() as usize).min(clip.frame_count() - 1);
        let source_frames = ((duration * clip.sample_rate as f64).ceil() as usize)
            .min(clip.frame_count() - first_source_frame);
        let output_frames = (source_frames as f64 * output_sample_rate as f64
            / clip.sample_rate as f64)
            .ceil() as u64;
        Ok(Self {
            first_source_frame,
            end_source_frame: first_source_frame + source_frames,
            source_step: clip.sample_rate as f64 / output_sample_rate as f64,
            output_frames,
            fade_frames: ((output_sample_rate as f64 * 0.04).round() as u64)
                .max(1)
                .min(output_frames / 2),
        })
    }

    fn sample(&self, clip: &AudioClip, cursor: u64, output_channel: usize) -> f32 {
        let source_position = (self.first_source_frame as f64 + cursor as f64 * self.source_step)
            .min((self.end_source_frame - 1) as f64);
        let sample = clip.sample_interpolated(source_position, output_channel % clip.channels);
        if sample.is_finite() { sample } else { 0.0 }
    }

    fn envelope(&self, cursor: u64) -> f32 {
        if self.fade_frames == 0 {
            return 0.0;
        }
        let distance = cursor.min(self.output_frames - 1 - cursor);
        if distance >= self.fade_frames {
            return 1.0;
        }
        let phase = distance as f64 / self.fade_frames as f64;
        (0.5 - 0.5 * (std::f64::consts::PI * phase).cos()) as f32
    }
}

struct PreviewRenderer {
    segment: PreviewSegment,
    cursor: u64,
    gain: GainRamp,
}

impl PreviewRenderer {
    fn render<T: SizedSample + FromSample<f32>>(
        &mut self,
        data: &mut [T],
        clip: &AudioClip,
        output_channels: usize,
        volume: &PlaybackVolume,
    ) {
        self.gain.set_target(volume.gain());
        for frame in data.chunks_mut(output_channels) {
            let gain = self.gain.next_gain() * self.segment.envelope(self.cursor);
            for (channel, sample) in frame.iter_mut().enumerate() {
                *sample = T::from_sample(self.segment.sample(clip, self.cursor, channel) * gain);
            }
            self.cursor += 1;
            if self.cursor == self.segment.output_frames {
                self.cursor = 0;
            }
        }
    }
}

fn build_for_format<T: SizedSample + FromSample<f32>>(
    target: &OutputStreamTarget,
    clip: Arc<AudioClip>,
    segment: PreviewSegment,
    volume: PlaybackVolume,
) -> Result<cpal::Stream, Box<dyn Error>> {
    let output_channels = target.config.channels as usize;
    let mut renderer = PreviewRenderer {
        segment,
        cursor: 0,
        gain: GainRamp::new(volume.gain(), target.config.sample_rate),
    };
    Ok(target.device.build_output_stream(
        target.config.clone(),
        move |data: &mut [T], _| renderer.render(data, &clip, output_channels, &volume),
        |_| {},
        None,
    )?)
}

#[cfg(test)]
mod tests;
