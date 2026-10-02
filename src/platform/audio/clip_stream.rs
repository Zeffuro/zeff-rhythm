use super::clip::AudioClip;
use super::devices::OutputStreamTarget;
use super::playback_clock::PlaybackClock;
use super::volume::{GainRamp, PlaybackVolume};
use cpal::traits::DeviceTrait;
use cpal::{FromSample, I24, OutputCallbackInfo, SampleFormat, SizedSample, StreamConfig};
use std::sync::Arc;
use std::time::Instant;

pub fn build_clip_stream(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    clip: Arc<AudioClip>,
) -> Result<(cpal::Stream, PlaybackClock), cpal::Error> {
    build_clip_stream_with_gain(
        device,
        config,
        sample_format,
        clip,
        PlaybackVolume::default(),
    )
}

pub fn build_clip_stream_with_volume(
    target: &OutputStreamTarget,
    clip: Arc<AudioClip>,
    volume: PlaybackVolume,
) -> Result<(cpal::Stream, PlaybackClock), cpal::Error> {
    build_clip_stream_with_gain(
        &target.device,
        target.config.clone(),
        target.sample_format,
        clip,
        volume,
    )
}

fn build_clip_stream_with_gain(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    clip: Arc<AudioClip>,
    volume: PlaybackVolume,
) -> Result<(cpal::Stream, PlaybackClock), cpal::Error> {
    let clock = PlaybackClock::new(clip.duration_seconds());
    let stream = match sample_format {
        SampleFormat::I8 => {
            build_clip_stream_for_format::<i8>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::I16 => {
            build_clip_stream_for_format::<i16>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::I24 => {
            build_clip_stream_for_format::<I24>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::I32 => {
            build_clip_stream_for_format::<i32>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::I64 => {
            build_clip_stream_for_format::<i64>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::U8 => {
            build_clip_stream_for_format::<u8>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::U16 => {
            build_clip_stream_for_format::<u16>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::U32 => {
            build_clip_stream_for_format::<u32>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::U64 => {
            build_clip_stream_for_format::<u64>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::F32 => {
            build_clip_stream_for_format::<f32>(device, config, clip, clock.clone(), volume)
        }
        SampleFormat::F64 => {
            build_clip_stream_for_format::<f64>(device, config, clip, clock.clone(), volume)
        }
        _ => unreachable!("unsupported sample format reported by CPAL"),
    }?;

    Ok((stream, clock))
}

fn build_clip_stream_for_format<T>(
    device: &cpal::Device,
    config: StreamConfig,
    clip: Arc<AudioClip>,
    clock: PlaybackClock,
    volume: PlaybackVolume,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let output_channels = config.channels as usize;
    let output_sample_rate = config.sample_rate;
    let total_output_frames = (clip.frame_count() as f64 * output_sample_rate as f64
        / clip.sample_rate as f64)
        .ceil() as u64;
    let mut output_frame_position = 0_u64;
    let mut gain_ramp = GainRamp::new(volume.gain(), output_sample_rate);

    device.build_output_stream(
        config,
        move |data: &mut [T], info: &OutputCallbackInfo| {
            let playback_lead = info.timestamp().playback - info.timestamp().callback;
            let first_frame_at = Instant::now() + playback_lead;
            render_callback(
                data,
                &clip,
                &clock,
                &volume,
                &mut gain_ramp,
                &mut output_frame_position,
                output_channels,
                output_sample_rate,
                total_output_frames,
                first_frame_at,
                playback_lead,
            );
        },
        |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn render_callback<T: SizedSample + FromSample<f32>>(
    data: &mut [T],
    clip: &AudioClip,
    clock: &PlaybackClock,
    volume: &PlaybackVolume,
    gain_ramp: &mut GainRamp,
    output_frame_position: &mut u64,
    output_channels: usize,
    output_sample_rate: u32,
    total_output_frames: u64,
    first_frame_at: Instant,
    playback_lead: std::time::Duration,
) {
    gain_ramp.set_target(volume.gain());
    if !clock.begin_callback(
        first_frame_at,
        *output_frame_position,
        data.len() / output_channels,
        output_sample_rate,
        playback_lead,
        total_output_frames,
    ) {
        data.fill(T::from_sample(0.0));
        for _ in data.chunks(output_channels) {
            gain_ramp.next_gain();
        }
        return;
    }

    for frame in data.chunks_mut(output_channels) {
        let gain = gain_ramp.next_gain();
        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = T::from_sample(
                output_sample_value(
                    clip,
                    *output_frame_position,
                    output_sample_rate,
                    total_output_frames,
                    channel,
                ) * gain,
            );
        }
        *output_frame_position += 1;
    }
}

fn output_sample_value(
    clip: &AudioClip,
    output_frame_position: u64,
    output_sample_rate: u32,
    total_output_frames: u64,
    output_channel: usize,
) -> f32 {
    if output_frame_position >= total_output_frames {
        return 0.0;
    }

    let source_frame_position =
        output_frame_position as f64 * clip.sample_rate as f64 / output_sample_rate as f64;
    let source_channel = output_channel % clip.channels;

    clip.sample_interpolated(source_frame_position, source_channel)
}

#[cfg(test)]
mod tests;
