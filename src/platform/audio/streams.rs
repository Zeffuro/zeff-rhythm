use super::stats::CallbackStats;
use cpal::traits::DeviceTrait;
use cpal::{FromSample, I24, OutputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn build_silence_stream(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    stats: Arc<Mutex<CallbackStats>>,
) -> Result<cpal::Stream, cpal::Error> {
    match sample_format {
        SampleFormat::I8 => build_silence_stream_for_format::<i8>(device, config, stats),
        SampleFormat::I16 => build_silence_stream_for_format::<i16>(device, config, stats),
        SampleFormat::I24 => build_silence_stream_for_format::<I24>(device, config, stats),
        SampleFormat::I32 => build_silence_stream_for_format::<i32>(device, config, stats),
        SampleFormat::I64 => build_silence_stream_for_format::<i64>(device, config, stats),
        SampleFormat::U8 => build_silence_stream_for_format::<u8>(device, config, stats),
        SampleFormat::U16 => build_silence_stream_for_format::<u16>(device, config, stats),
        SampleFormat::U32 => build_silence_stream_for_format::<u32>(device, config, stats),
        SampleFormat::U64 => build_silence_stream_for_format::<u64>(device, config, stats),
        SampleFormat::F32 => build_silence_stream_for_format::<f32>(device, config, stats),
        SampleFormat::F64 => build_silence_stream_for_format::<f64>(device, config, stats),
        _ => unreachable!("unsupported sample format reported by CPAL"),
    }
}

pub fn build_click_stream(
    device: &cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    start_at: Instant,
    interval: Duration,
) -> Result<cpal::Stream, cpal::Error> {
    match sample_format {
        SampleFormat::I8 => build_click_stream_for_format::<i8>(device, config, start_at, interval),
        SampleFormat::I16 => {
            build_click_stream_for_format::<i16>(device, config, start_at, interval)
        }
        SampleFormat::I24 => {
            build_click_stream_for_format::<I24>(device, config, start_at, interval)
        }
        SampleFormat::I32 => {
            build_click_stream_for_format::<i32>(device, config, start_at, interval)
        }
        SampleFormat::I64 => {
            build_click_stream_for_format::<i64>(device, config, start_at, interval)
        }
        SampleFormat::U8 => build_click_stream_for_format::<u8>(device, config, start_at, interval),
        SampleFormat::U16 => {
            build_click_stream_for_format::<u16>(device, config, start_at, interval)
        }
        SampleFormat::U32 => {
            build_click_stream_for_format::<u32>(device, config, start_at, interval)
        }
        SampleFormat::U64 => {
            build_click_stream_for_format::<u64>(device, config, start_at, interval)
        }
        SampleFormat::F32 => {
            build_click_stream_for_format::<f32>(device, config, start_at, interval)
        }
        SampleFormat::F64 => {
            build_click_stream_for_format::<f64>(device, config, start_at, interval)
        }
        _ => unreachable!("unsupported sample format reported by CPAL"),
    }
}

fn build_silence_stream_for_format<T>(
    device: &cpal::Device,
    config: StreamConfig,
    stats: Arc<Mutex<CallbackStats>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], info: &OutputCallbackInfo| {
            if let Ok(mut stats) = stats.try_lock() {
                stats.record(info.timestamp(), data.len() / channels);
            }
            fill_value(data, 0.0);
        },
        |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn build_click_stream_for_format<T>(
    device: &cpal::Device,
    config: StreamConfig,
    start_at: Instant,
    interval: Duration,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate as f64;
    let channels = config.channels as usize;
    let interval_seconds = interval.as_secs_f64();

    device.build_output_stream(
        config,
        move |data: &mut [T], info: &OutputCallbackInfo| {
            let playback_lead = info.timestamp().playback - info.timestamp().callback;
            let first_frame_at = Instant::now() + playback_lead;

            for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
                let frame_at =
                    first_frame_at + Duration::from_secs_f64(frame_index as f64 / sample_rate);
                let value = click_value(frame_at, start_at, interval_seconds);
                for sample in frame {
                    *sample = T::from_sample(value);
                }
            }
        },
        |error| eprintln!("audio stream error: {error}"),
        None,
    )
}

fn click_value(frame_at: Instant, start_at: Instant, interval_seconds: f64) -> f32 {
    if frame_at < start_at {
        return 0.0;
    }

    let since_start = frame_at.duration_since(start_at).as_secs_f64();
    let phase = since_start % interval_seconds;
    let click_seconds = 0.030;
    if phase >= click_seconds {
        return 0.0;
    }

    let envelope = 1.0 - phase / click_seconds;
    let wave = (phase * 1_000.0 * std::f64::consts::TAU).sin();
    (wave * envelope * 0.35) as f32
}

fn fill_value<T>(data: &mut [T], value: f32)
where
    T: Sample + FromSample<f32>,
{
    let value = T::from_sample(value);
    for sample in data {
        *sample = value;
    }
}
