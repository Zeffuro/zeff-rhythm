use super::{NativeInputEvent, NativeInputEventKind, NativeInputSource, NativeInputTimestampKind};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::{Keycode, Scancode};
use sdl3::pixels::Color;
use sdl3::render::Canvas;
use sdl3::video::Window;
use std::error::Error;
use std::time::{Duration, Instant};

pub struct SdlInputBackend {
    _sdl: sdl3::Sdl,
    canvas: Canvas<Window>,
    event_pump: sdl3::EventPump,
    clock: SdlTimestampClock,
}

impl SdlInputBackend {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let sdl = sdl3::init()?;
        let video = sdl.video()?;
        let mut window = video
            .window("zeff-rhythm input - focus for D/F/J/K", 520, 140)
            .position_centered()
            .build()
            .map_err(|error| error.to_string())?;
        let _ = window.raise();
        let mut canvas = window.into_canvas();
        let _ = canvas.window_mut().set_keyboard_grab(true);
        let event_pump = sdl.event_pump()?;
        let clock = SdlTimestampClock::capture();

        Ok(Self {
            _sdl: sdl,
            canvas,
            event_pump,
            clock,
        })
    }

    pub fn poll(&mut self, output: &mut Vec<NativeInputEvent>) -> Result<(), Box<dyn Error>> {
        self.canvas.set_draw_color(Color::RGB(16, 20, 26));
        self.canvas.clear();
        self.canvas.present();

        if let Some(event) = self.event_pump.wait_event_timeout(Duration::from_millis(1)) {
            if let Some(input) = translate_sdl_event(self.clock, event) {
                output.push(input);
            }
        }

        let clock = self.clock;
        for event in self.event_pump.poll_iter() {
            if let Some(input) = translate_sdl_event(clock, event) {
                output.push(input);
            }
        }

        Ok(())
    }
}

pub fn translate_sdl_event(clock: SdlTimestampClock, event: Event) -> Option<NativeInputEvent> {
    let (timestamp, kind) = match event {
        Event::Quit { timestamp }
        | Event::Window {
            timestamp,
            win_event: WindowEvent::CloseRequested,
            ..
        } => (timestamp, NativeInputEventKind::Quit),
        Event::Window {
            timestamp,
            win_event: WindowEvent::FocusGained,
            ..
        } => (timestamp, NativeInputEventKind::FocusGained),
        Event::Window {
            timestamp,
            win_event: WindowEvent::FocusLost,
            ..
        } => (timestamp, NativeInputEventKind::FocusLost),
        Event::KeyDown {
            timestamp,
            keycode,
            scancode,
            repeat,
            ..
        } => {
            if repeat {
                return None;
            }
            if is_quit_key(keycode, scancode) {
                (timestamp, NativeInputEventKind::Quit)
            } else if let Some(lane) = lane_for_key(keycode, scancode) {
                (timestamp, NativeInputEventKind::LanePress(lane))
            } else {
                return None;
            }
        }
        Event::KeyUp {
            timestamp,
            keycode,
            scancode,
            repeat,
            ..
        } => {
            if repeat {
                return None;
            }
            if let Some(lane) = lane_for_key(keycode, scancode) {
                (timestamp, NativeInputEventKind::LaneRelease(lane))
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let received_time = Instant::now();
    let event_time = clock.host_time_for(timestamp);

    Some(NativeInputEvent {
        kind,
        source: NativeInputSource::Sdl,
        timestamp_kind: NativeInputTimestampKind::SourceEventTime,
        event_time,
        received_time,
        source_timestamp_ns: Some(timestamp),
        queue_age_ms: Some(clock.signed_age_ms(timestamp, received_time)),
    })
}

#[derive(Clone, Copy, Debug)]
pub struct SdlTimestampClock {
    sdl_timestamp_ns: u64,
    host_time: Instant,
}

impl SdlTimestampClock {
    pub fn capture() -> Self {
        let before = Instant::now();
        let sdl_timestamp_ns = unsafe { sdl3::sys::timer::SDL_GetTicksNS() };
        let after = Instant::now();
        let host_time = before + after.duration_since(before).div_f64(2.0);

        Self {
            sdl_timestamp_ns,
            host_time,
        }
    }

    pub fn host_time_for(&self, timestamp_ns: u64) -> Instant {
        if timestamp_ns >= self.sdl_timestamp_ns {
            self.host_time + Duration::from_nanos(timestamp_ns - self.sdl_timestamp_ns)
        } else {
            self.host_time
                .checked_sub(Duration::from_nanos(self.sdl_timestamp_ns - timestamp_ns))
                .unwrap_or(self.host_time)
        }
    }

    pub fn signed_age_ms(&self, timestamp_ns: u64, received_time: Instant) -> f64 {
        signed_duration_ms(received_time, self.host_time_for(timestamp_ns))
    }
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

fn signed_duration_ms(left: Instant, right: Instant) -> f64 {
    if left >= right {
        left.duration_since(right).as_secs_f64() * 1_000.0
    } else {
        -right.duration_since(left).as_secs_f64() * 1_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::SdlTimestampClock;
    use std::time::{Duration, Instant};

    #[test]
    fn maps_sdl_nanoseconds_to_host_instants() {
        let host_time = Instant::now();
        let clock = SdlTimestampClock {
            sdl_timestamp_ns: 1_000_000,
            host_time,
        };

        assert_eq!(
            clock.host_time_for(4_000_000),
            host_time + Duration::from_millis(3)
        );
        assert_eq!(
            clock.host_time_for(500_000),
            host_time - Duration::from_micros(500)
        );
    }
}
