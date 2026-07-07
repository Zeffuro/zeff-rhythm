use super::args::parse_optional_f64;
use crate::platform::input::SdlTimestampClock;
use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Scancode};
use sdl3::pixels::Color;
use std::error::Error;
use std::time::{Duration, Instant};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() > 1 {
        return Err("usage: zeff-rhythm sdl-input-probe [seconds]".into());
    }

    let seconds = parse_optional_f64(args.first(), 30.0, "seconds")?;
    if seconds <= 0.0 {
        return Err("seconds must be positive".into());
    }

    let duration = Duration::from_secs_f64(seconds);
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("zeff-rhythm SDL input probe - focus me", 720, 260)
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window.into_canvas();
    let mut event_pump = sdl.event_pump()?;
    let clock = SdlTimestampClock::capture();
    let start = Instant::now();
    let deadline = start + duration;

    println!("sdl_input_probe=text");
    println!("duration={:.3}s", duration.as_secs_f64());
    println!("focus_window=true");
    println!("keys=d/f/j/k lanes=0..3, quit=esc/q");
    println!(
        "fields=event timestamp_ns timestamp_s host_elapsed_s estimated_queue_age_ms keycode scancode repeat lane"
    );

    while Instant::now() < deadline {
        canvas.set_draw_color(Color::RGB(16, 20, 26));
        canvas.clear();
        canvas.present();

        if let Some(event) = event_pump.wait_event_timeout(Duration::from_millis(8)) {
            let should_quit = handle_event(event, start, clock);
            if should_quit {
                break;
            }
        }

        for event in event_pump.poll_iter() {
            let should_quit = handle_event(event, start, clock);
            if should_quit {
                return Ok(());
            }
        }
    }

    Ok(())
}

fn handle_event(event: Event, start: Instant, clock: SdlTimestampClock) -> bool {
    match event {
        Event::Quit { .. } => true,
        Event::KeyDown {
            timestamp,
            keycode,
            scancode,
            repeat,
            ..
        } => {
            print_key_event("down", timestamp, keycode, scancode, repeat, start, clock);
            is_quit_key(keycode, scancode)
        }
        Event::KeyUp {
            timestamp,
            keycode,
            scancode,
            repeat,
            ..
        } => {
            print_key_event("up", timestamp, keycode, scancode, repeat, start, clock);
            false
        }
        _ => false,
    }
}

fn print_key_event(
    event: &str,
    timestamp_ns: u64,
    keycode: Option<Keycode>,
    scancode: Option<Scancode>,
    repeat: bool,
    start: Instant,
    clock: SdlTimestampClock,
) {
    let now = Instant::now();
    let queue_age_ms = clock.signed_age_ms(timestamp_ns, now);
    let lane = lane_for_key(keycode, scancode)
        .map(|lane| lane.to_string())
        .unwrap_or_else(|| "-".to_owned());

    println!(
        "event={event} timestamp_ns={timestamp_ns} timestamp_s={:.9} host_elapsed_s={:.9} estimated_queue_age_ms={queue_age_ms:.3} keycode={} scancode={} repeat={repeat} lane={lane}",
        timestamp_ns as f64 / 1_000_000_000.0,
        now.duration_since(start).as_secs_f64(),
        format_keycode(keycode),
        format_scancode(scancode),
    );
}

fn lane_for_key(keycode: Option<Keycode>, scancode: Option<Scancode>) -> Option<u8> {
    match scancode {
        Some(Scancode::D) => Some(0),
        Some(Scancode::F) => Some(1),
        Some(Scancode::J) => Some(2),
        Some(Scancode::K) => Some(3),
        _ => match keycode {
            Some(Keycode::D) => Some(0),
            Some(Keycode::F) => Some(1),
            Some(Keycode::J) => Some(2),
            Some(Keycode::K) => Some(3),
            _ => None,
        },
    }
}

fn is_quit_key(keycode: Option<Keycode>, scancode: Option<Scancode>) -> bool {
    matches!(
        (keycode, scancode),
        (Some(Keycode::Escape), _)
            | (Some(Keycode::Q), _)
            | (_, Some(Scancode::Escape))
            | (_, Some(Scancode::Q))
    )
}

fn format_keycode(keycode: Option<Keycode>) -> String {
    keycode
        .map(|keycode| keycode.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn format_scancode(scancode: Option<Scancode>) -> String {
    scancode
        .map(|scancode| scancode.to_string())
        .unwrap_or_else(|| "-".to_owned())
}
