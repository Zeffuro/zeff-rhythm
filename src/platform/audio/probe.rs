use super::devices::{AudioStreamOptions, output_stream_target, print_target_summary};
use super::stats::CallbackStats;
use super::streams::build_silence_stream;
use cpal::traits::StreamTrait;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn run_audio_callback_probe(
    duration: Duration,
    options: &AudioStreamOptions,
) -> Result<(), Box<dyn Error>> {
    let target = output_stream_target(options)?;
    let stats = Arc::new(Mutex::new(CallbackStats::default()));

    println!("audio callback probe");
    print_target_summary(&target);
    println!("duration={:.3}s", duration.as_secs_f64());

    let stream = build_silence_stream(
        &target.device,
        target.config,
        target.sample_format,
        Arc::clone(&stats),
    )?;
    stream.play()?;
    thread::sleep(duration);
    drop(stream);

    let stats = stats.lock().expect("callback stats lock poisoned");
    stats.print();

    Ok(())
}
