use super::args::{parse_audio_args, parse_optional_f64};
use crate::platform::audio;
use std::error::Error;
use std::time::Duration;

pub fn latency_probe(args: &[String]) -> Result<(), Box<dyn Error>> {
    let parsed = parse_audio_args(args)?;
    if !parsed.positionals.is_empty() {
        return Err("usage: zeff-rhythm latency-probe [--host HOST] [--device NAME_OR_ID]".into());
    }

    audio::print_latency_probe(&parsed.options.selection)
}

pub fn audio_callback_probe(args: &[String]) -> Result<(), Box<dyn Error>> {
    let parsed = parse_audio_args(args)?;
    if parsed.positionals.len() > 1 {
        return Err("usage: zeff-rhythm audio-callback-probe [seconds] [audio options]".into());
    }

    let seconds = parse_optional_f64(parsed.positionals.first(), 3.0, "seconds")?;
    audio::run_audio_callback_probe(Duration::from_secs_f64(seconds), &parsed.options)
}

pub fn tap_probe(args: &[String]) -> Result<(), Box<dyn Error>> {
    let parsed = parse_audio_args(args)?;
    if parsed.positionals.len() > 2 {
        return Err("usage: zeff-rhythm tap-probe [seconds] [bpm] [audio options]".into());
    }

    let seconds = parse_optional_f64(parsed.positionals.first(), 20.0, "seconds")?;
    let bpm = parse_optional_f64(parsed.positionals.get(1), 120.0, "bpm")?;
    audio::run_tap_probe(Duration::from_secs_f64(seconds), bpm, &parsed.options)
}
