use super::devices::{AudioStreamOptions, output_stream_target, print_target_summary};
use super::streams::build_click_stream;
use cpal::traits::StreamTrait;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::error::Error;
use std::time::{Duration, Instant};

pub fn run_tap_probe(
    duration: Duration,
    bpm: f64,
    options: &AudioStreamOptions,
) -> Result<(), Box<dyn Error>> {
    if bpm <= 0.0 {
        return Err("bpm must be positive".into());
    }

    let target = output_stream_target(options)?;
    let start_at = Instant::now() + Duration::from_millis(1000);
    let interval = Duration::from_secs_f64(60.0 / bpm);

    println!("tap probe");
    print_target_summary(&target);
    println!("duration={:.3}s bpm={bpm:.3}", duration.as_secs_f64());
    println!("keys=d/f/j/k/space, quit=esc/q");

    let stream = build_click_stream(
        &target.device,
        target.config,
        target.sample_format,
        start_at,
        interval,
    )?;
    stream.play()?;

    let _raw_mode = RawModeGuard::new()?;
    let deadline = Instant::now() + duration;

    while Instant::now() < deadline {
        if !event::poll(Duration::from_millis(10))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')
        ) {
            break;
        }

        let Some(label) = key_label(key.code) else {
            continue;
        };

        let now = Instant::now();
        print_key_delta(label, now, start_at, interval);
    }

    drop(stream);
    Ok(())
}

fn key_label(code: KeyCode) -> Option<&'static str> {
    match code {
        KeyCode::Char('d') | KeyCode::Char('D') => Some("D"),
        KeyCode::Char('f') | KeyCode::Char('F') => Some("F"),
        KeyCode::Char('j') | KeyCode::Char('J') => Some("J"),
        KeyCode::Char('k') | KeyCode::Char('K') => Some("K"),
        KeyCode::Char(' ') => Some("SPACE"),
        _ => None,
    }
}

fn print_key_delta(label: &str, now: Instant, start_at: Instant, interval: Duration) {
    if now < start_at {
        println!("key={label} before_start");
        return;
    }

    let interval_seconds = interval.as_secs_f64();
    let elapsed_seconds = now.duration_since(start_at).as_secs_f64();
    let nearest_click = (elapsed_seconds / interval_seconds).round();
    let nearest_click_at = start_at + Duration::from_secs_f64(nearest_click * interval_seconds);
    let delta_ms = signed_duration_ms(now, nearest_click_at);

    println!(
        "key={label} elapsed={elapsed_seconds:.6}s click={} delta={delta_ms:.3}ms",
        nearest_click as i64
    );
}

fn signed_duration_ms(left: Instant, right: Instant) -> f64 {
    if left >= right {
        left.duration_since(right).as_secs_f64() * 1_000.0
    } else {
        -right.duration_since(left).as_secs_f64() * 1_000.0
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self, Box<dyn Error>> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
