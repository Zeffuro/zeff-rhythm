use super::clip::AudioClip;
use super::playback_clock::PlaybackClock;
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
    let clock = PlaybackClock::new(clip.duration_seconds());
    let stream = match sample_format {
        SampleFormat::I8 => build_clip_stream_for_format::<i8>(device, config, clip, clock.clone()),
        SampleFormat::I16 => {
            build_clip_stream_for_format::<i16>(device, config, clip, clock.clone())
        }
        SampleFormat::I24 => {
            build_clip_stream_for_format::<I24>(device, config, clip, clock.clone())
        }
        SampleFormat::I32 => {
            build_clip_stream_for_format::<i32>(device, config, clip, clock.clone())
        }
        SampleFormat::I64 => {
            build_clip_stream_for_format::<i64>(device, config, clip, clock.clone())
        }
        SampleFormat::U8 => build_clip_stream_for_format::<u8>(device, config, clip, clock.clone()),
        SampleFormat::U16 => {
            build_clip_stream_for_format::<u16>(device, config, clip, clock.clone())
        }
        SampleFormat::U32 => {
            build_clip_stream_for_format::<u32>(device, config, clip, clock.clone())
        }
        SampleFormat::U64 => {
            build_clip_stream_for_format::<u64>(device, config, clip, clock.clone())
        }
        SampleFormat::F32 => {
            build_clip_stream_for_format::<f32>(device, config, clip, clock.clone())
        }
        SampleFormat::F64 => {
            build_clip_stream_for_format::<f64>(device, config, clip, clock.clone())
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

    device.build_output_stream(
        config,
        move |data: &mut [T], info: &OutputCallbackInfo| {
            let playback_lead = info.timestamp().playback - info.timestamp().callback;
            let first_frame_at = Instant::now() + playback_lead;
            let first_frame_index = output_frame_position;

            for frame in data.chunks_mut(output_channels) {
                for (channel, sample) in frame.iter_mut().enumerate() {
                    let value = output_sample_value(
                        &clip,
                        output_frame_position,
                        output_sample_rate,
                        total_output_frames,
                        channel,
                    );
                    *sample = T::from_sample(value);
                }

                output_frame_position += 1;
            }

            clock.record_callback(
                first_frame_at,
                first_frame_index,
                data.len() / output_channels,
                output_sample_rate,
                playback_lead,
                output_frame_position >= total_output_frames,
            );
        },
        |error| eprintln!("audio stream error: {error}"),
        None,
    )
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
