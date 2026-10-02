use super::args::{parse_f64_arg, parse_u32_arg};
use crate::platform::audio::AudioStreamOptions;
use crate::platform::input::NativeInputBackendKind;
use crate::play::{
    CalibrationPattern, PlayDisplayMode as PlayDisplay, PlaySessionOptions,
    run_generated_calibration_session,
};
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static CALIBRATION_EVENT_LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (options, pattern) = parse_options(args)?;
    run_generated_calibration_session(options, pattern)
}

fn parse_options(
    args: &[String],
) -> Result<(PlaySessionOptions, CalibrationPattern), Box<dyn Error>> {
    let mut pattern = CalibrationPattern::default();
    let mut input_offset_ms = 0.0;
    let mut max_seconds = None;
    let mut lookahead_seconds = 4.0;
    let mut lead_in_seconds = Some(3.0);
    let mut chart_start_seconds = None;
    let mut start_delay_seconds = Some(0.75);
    let mut display = PlayDisplay::Sdl;
    let mut input = NativeInputBackendKind::Terminal;
    let mut event_log_path = None;
    let mut event_log_enabled = true;
    let mut dry_run = false;
    let mut audio = AudioStreamOptions::default();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
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
                event_log_enabled = true;
            }
            "--no-event-log" => {
                event_log_path = None;
                event_log_enabled = false;
            }
            "--notes" | "--note-count" => {
                let value = next_value(args, &mut index, arg)?;
                pattern.note_count = parse_positive_u32(&value, "note count")?;
            }
            "--lane-count" => {
                let value = next_value(args, &mut index, "--lane-count")?;
                pattern.lane_count = parse_u8_arg(&value, "lane count")?;
            }
            "--lane" => {
                let value = next_value(args, &mut index, "--lane")?;
                pattern.lane = parse_u8_arg(&value, "lane")?;
            }
            "--first-note-seconds" => {
                let value = next_value(args, &mut index, "--first-note-seconds")?;
                pattern.first_note_seconds = parse_nonnegative_f64(&value, "first note seconds")?;
            }
            "--interval-seconds" => {
                let value = next_value(args, &mut index, "--interval-seconds")?;
                pattern.interval_seconds = parse_positive_f64(&value, "interval seconds")?;
            }
            "--tail-seconds" => {
                let value = next_value(args, &mut index, "--tail-seconds")?;
                pattern.tail_seconds = parse_nonnegative_f64(&value, "tail seconds")?;
            }
            "--click-seconds" => {
                let value = next_value(args, &mut index, "--click-seconds")?;
                pattern.click_seconds = parse_positive_f64(&value, "click seconds")?;
            }
            "--click-frequency-hz" => {
                let value = next_value(args, &mut index, "--click-frequency-hz")?;
                pattern.click_frequency_hz = parse_positive_f64(&value, "click frequency hz")?;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--log" | "--no-view" => {
                display = PlayDisplay::Log;
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
                event_log_enabled = true;
            }
            _ if arg.starts_with("--notes=") => {
                pattern.note_count = parse_positive_u32(&arg["--notes=".len()..], "note count")?;
            }
            _ if arg.starts_with("--note-count=") => {
                pattern.note_count =
                    parse_positive_u32(&arg["--note-count=".len()..], "note count")?;
            }
            _ if arg.starts_with("--lane-count=") => {
                pattern.lane_count = parse_u8_arg(&arg["--lane-count=".len()..], "lane count")?;
            }
            _ if arg.starts_with("--lane=") => {
                pattern.lane = parse_u8_arg(&arg["--lane=".len()..], "lane")?;
            }
            _ if arg.starts_with("--first-note-seconds=") => {
                pattern.first_note_seconds = parse_nonnegative_f64(
                    &arg["--first-note-seconds=".len()..],
                    "first note seconds",
                )?;
            }
            _ if arg.starts_with("--interval-seconds=") => {
                pattern.interval_seconds =
                    parse_positive_f64(&arg["--interval-seconds=".len()..], "interval seconds")?;
            }
            _ if arg.starts_with("--tail-seconds=") => {
                pattern.tail_seconds =
                    parse_nonnegative_f64(&arg["--tail-seconds=".len()..], "tail seconds")?;
            }
            _ if arg.starts_with("--click-seconds=") => {
                pattern.click_seconds =
                    parse_positive_f64(&arg["--click-seconds=".len()..], "click seconds")?;
            }
            _ if arg.starts_with("--click-frequency-hz=") => {
                pattern.click_frequency_hz = parse_positive_f64(
                    &arg["--click-frequency-hz=".len()..],
                    "click frequency hz",
                )?;
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
            _ => return Err(usage().into()),
        }

        index += 1;
    }

    if event_log_enabled && event_log_path.is_none() && !dry_run {
        event_log_path = Some(default_event_log_path(display)?);
    }

    Ok((
        PlaySessionOptions {
            chart_index: 0,
            chart_path: PathBuf::from("generated-calibration"),
            format: None,
            audio_path: None,
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
        },
        pattern,
    ))
}

fn usage() -> &'static str {
    "usage: zeff-rhythm play-calibration [--display sdl|highway|log] [--input terminal|sdl] [--event-log PATH|--no-event-log] [--input-offset-ms MS] [--max-seconds SECONDS] [--notes N] [--interval-seconds SECONDS] [--first-note-seconds SECONDS] [--dry-run] [--host HOST] [--device NAME_OR_ID] [--sample-rate HZ] [--buffer FRAMES]"
}

fn default_event_log_path(display: PlayDisplay) -> Result<PathBuf, Box<dyn Error>> {
    let timestamp_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let sequence = CALIBRATION_EVENT_LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(PathBuf::from(format!(
        ".local_runs/calibration-{}-{timestamp_ms}-{sequence}.csv",
        display.as_str()
    )))
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String, Box<dyn Error>> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("missing value for {option}").into())
}

fn parse_positive_u32(value: &str, name: &str) -> Result<u32, Box<dyn Error>> {
    let parsed = parse_u32_arg(value, name)?;
    if parsed == 0 {
        return Err(format!("{name} must be positive").into());
    }

    Ok(parsed)
}

fn parse_u8_arg(value: &str, name: &str) -> Result<u8, Box<dyn Error>> {
    let parsed = parse_u32_arg(value, name)?;
    u8::try_from(parsed).map_err(|_| format!("{name} must be 0..255").into())
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

#[cfg(test)]
mod tests {
    use super::parse_options;

    #[test]
    fn enables_default_event_log_for_real_calibration_runs() {
        let (options, _) = parse_options(&[]).unwrap();
        let path = options.event_log_path.unwrap();

        assert!(
            path.to_string_lossy()
                .starts_with(".local_runs/calibration-sdl-")
        );
    }

    #[test]
    fn dry_run_does_not_allocate_default_event_log() {
        let args = vec!["--dry-run".to_owned()];
        let (options, _) = parse_options(&args).unwrap();

        assert_eq!(options.event_log_path, None);
    }

    #[test]
    fn no_event_log_disables_default_event_log() {
        let args = vec!["--no-event-log".to_owned()];
        let (options, _) = parse_options(&args).unwrap();

        assert_eq!(options.event_log_path, None);
    }
}
