use super::args::{parse_f64_arg, parse_u32_arg};
use super::charts::parse_format_option;
use crate::platform::audio::AudioStreamOptions;
use crate::platform::input::NativeInputBackendKind;
use crate::play::{PlayDisplayMode as PlayDisplay, PlaySessionOptions, run_play_session};
use std::error::Error;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let options = parse_play_map_options(args)?;
    run_play_session(options)
}

fn parse_play_map_options(args: &[String]) -> Result<PlaySessionOptions, Box<dyn Error>> {
    let mut chart_path = None;
    let mut chart_index = 0;
    let mut format = None;
    let mut audio_path = None;
    let mut input_offset_ms = 0.0;
    let mut max_seconds = None;
    let mut lookahead_seconds = 4.0;
    let mut lead_in_seconds = None;
    let mut chart_start_seconds = None;
    let mut start_delay_seconds = None;
    let mut display = PlayDisplay::Highway;
    let mut input = NativeInputBackendKind::Terminal;
    let mut event_log_path = None;
    let mut dry_run = false;
    let mut audio = AudioStreamOptions::default();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--chart-index" => {
                chart_index = parse_u32_arg(
                    &next_value(args, &mut index, "--chart-index")?,
                    "chart index",
                )? as usize;
            }
            "--format" => {
                let value = next_value(args, &mut index, "--format")?;
                format = parse_format_option(&value)?;
            }
            "--audio" => {
                audio_path = Some(PathBuf::from(next_value(args, &mut index, "--audio")?));
            }
            "--offset-ms" | "--input-offset-ms" => {
                let value = next_value(args, &mut index, arg)?;
                input_offset_ms = parse_f64_arg(&value, "input offset milliseconds")?;
            }
            "--max-seconds" => {
                let value = next_value(args, &mut index, "--max-seconds")?;
                max_seconds = Some(parse_positive_f64(&value, "max seconds")?);
            }
            "--lookahead-seconds" => {
                let value = next_value(args, &mut index, "--lookahead-seconds")?;
                lookahead_seconds = parse_positive_f64(&value, "lookahead seconds")?;
            }
            "--lead-in-seconds" => {
                let value = next_value(args, &mut index, "--lead-in-seconds")?;
                lead_in_seconds = Some(parse_nonnegative_f64(&value, "lead-in seconds")?);
            }
            "--chart-start-seconds" => {
                let value = next_value(args, &mut index, "--chart-start-seconds")?;
                chart_start_seconds = Some(parse_f64_arg(&value, "chart start seconds")?);
            }
            "--start-delay-seconds" => {
                let value = next_value(args, &mut index, "--start-delay-seconds")?;
                start_delay_seconds = Some(parse_nonnegative_f64(&value, "start delay seconds")?);
            }
            "--display" => {
                let value = next_value(args, &mut index, "--display")?;
                display = PlayDisplay::parse(&value)?;
            }
            "--input" => {
                let value = next_value(args, &mut index, "--input")?;
                input = NativeInputBackendKind::parse(&value)?;
            }
            "--event-log" => {
                event_log_path = Some(PathBuf::from(next_value(args, &mut index, "--event-log")?));
            }
            "--log" | "--no-view" => {
                display = PlayDisplay::Log;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--host" => {
                audio.selection.host = Some(next_value(args, &mut index, "--host")?);
            }
            "--device" => {
                audio.selection.device = Some(next_value(args, &mut index, "--device")?);
            }
            "--sample-rate" => {
                let value = next_value(args, &mut index, "--sample-rate")?;
                audio.sample_rate = Some(parse_u32_arg(&value, "sample rate")?);
            }
            "--buffer" | "--buffer-frames" => {
                let value = next_value(args, &mut index, arg)?;
                audio.buffer_frames = Some(parse_u32_arg(&value, "buffer frames")?);
            }
            _ if arg.starts_with("--format=") => {
                format = parse_format_option(&arg["--format=".len()..])?;
            }
            _ if arg.starts_with("--audio=") => {
                audio_path = Some(PathBuf::from(&arg["--audio=".len()..]));
            }
            _ if arg.starts_with("--offset-ms=") => {
                input_offset_ms =
                    parse_f64_arg(&arg["--offset-ms=".len()..], "input offset milliseconds")?;
            }
            _ if arg.starts_with("--input-offset-ms=") => {
                input_offset_ms = parse_f64_arg(
                    &arg["--input-offset-ms=".len()..],
                    "input offset milliseconds",
                )?;
            }
            _ if arg.starts_with("--max-seconds=") => {
                max_seconds = Some(parse_positive_f64(
                    &arg["--max-seconds=".len()..],
                    "max seconds",
                )?);
            }
            _ if arg.starts_with("--lookahead-seconds=") => {
                lookahead_seconds =
                    parse_positive_f64(&arg["--lookahead-seconds=".len()..], "lookahead seconds")?;
            }
            _ if arg.starts_with("--lead-in-seconds=") => {
                lead_in_seconds = Some(parse_nonnegative_f64(
                    &arg["--lead-in-seconds=".len()..],
                    "lead-in seconds",
                )?);
            }
            _ if arg.starts_with("--chart-start-seconds=") => {
                chart_start_seconds = Some(parse_f64_arg(
                    &arg["--chart-start-seconds=".len()..],
                    "chart start seconds",
                )?);
            }
            _ if arg.starts_with("--start-delay-seconds=") => {
                start_delay_seconds = Some(parse_nonnegative_f64(
                    &arg["--start-delay-seconds=".len()..],
                    "start delay seconds",
                )?);
            }
            _ if arg.starts_with("--display=") => {
                display = PlayDisplay::parse(&arg["--display=".len()..])?;
            }
            _ if arg.starts_with("--input=") => {
                input = NativeInputBackendKind::parse(&arg["--input=".len()..])?;
            }
            _ if arg.starts_with("--event-log=") => {
                event_log_path = Some(PathBuf::from(&arg["--event-log=".len()..]));
            }
            _ if arg.starts_with("--host=") => {
                audio.selection.host = Some(arg["--host=".len()..].to_owned());
            }
            _ if arg.starts_with("--device=") => {
                audio.selection.device = Some(arg["--device=".len()..].to_owned());
            }
            _ if arg.starts_with("--sample-rate=") => {
                audio.sample_rate = Some(parse_u32_arg(
                    &arg["--sample-rate=".len()..],
                    "sample rate",
                )?);
            }
            _ if arg.starts_with("--buffer=") => {
                audio.buffer_frames =
                    Some(parse_u32_arg(&arg["--buffer=".len()..], "buffer frames")?);
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}").into()),
            _ => {
                if chart_path.replace(PathBuf::from(arg)).is_some() {
                    return Err(usage().into());
                }
            }
        }

        index += 1;
    }

    let Some(chart_path) = chart_path else {
        return Err(usage().into());
    };

    Ok(PlaySessionOptions {
        chart_index,
        chart_path,
        format,
        audio_path,
        input_offset_ms,
        max_seconds,
        lookahead_seconds,
        lead_in_seconds,
        chart_start_seconds,
        start_delay_seconds,
        display,
        input,
        event_log_path,
        dry_run,
        volume: 1.0,
        audio,
    })
}

fn usage() -> &'static str {
    "usage: zeff-rhythm play-map <chart-path> [--format auto|osu|sm] [--chart-index N] [--audio PATH] [--input terminal|sdl] [--event-log PATH] [--input-offset-ms MS] [--max-seconds SECONDS] [--display highway|sdl|log] [--lookahead-seconds SECONDS] [--lead-in-seconds SECONDS] [--chart-start-seconds SECONDS] [--start-delay-seconds SECONDS] [--dry-run] [--host HOST] [--device NAME_OR_ID] [--sample-rate HZ] [--buffer FRAMES]"
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String, Box<dyn Error>> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("missing value for {option}").into())
}

fn parse_positive_f64(value: &str, name: &str) -> Result<f64, Box<dyn Error>> {
    let parsed = parse_f64_arg(value, name)?;
    if parsed <= 0.0 {
        return Err(format!("{name} must be positive").into());
    }

    Ok(parsed)
}

fn parse_nonnegative_f64(value: &str, name: &str) -> Result<f64, Box<dyn Error>> {
    let parsed = parse_f64_arg(value, name)?;
    if parsed < 0.0 {
        return Err(format!("{name} must be non-negative").into());
    }

    Ok(parsed)
}
