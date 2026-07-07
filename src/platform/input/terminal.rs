use super::{NativeInputEvent, NativeInputEventKind};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::error::Error;
use std::time::{Duration, Instant};

pub struct TerminalInputBackend {
    _raw_mode: RawModeGuard,
}

impl TerminalInputBackend {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            _raw_mode: RawModeGuard::new()?,
        })
    }

    pub fn poll(&mut self, output: &mut Vec<NativeInputEvent>) -> Result<(), Box<dyn Error>> {
        if !event::poll(Duration::from_millis(4))? {
            return Ok(());
        }

        loop {
            let received_time = Instant::now();
            let Event::Key(key) = event::read()? else {
                if !event::poll(Duration::ZERO)? {
                    break;
                }
                continue;
            };

            if let Some(kind) = key_event_kind(key.code, key.kind) {
                output.push(NativeInputEvent {
                    kind,
                    event_time: received_time,
                    received_time,
                    source_timestamp_ns: None,
                    queue_age_ms: None,
                });
            }

            if !event::poll(Duration::ZERO)? {
                break;
            }
        }

        Ok(())
    }
}

fn key_event_kind(code: KeyCode, kind: KeyEventKind) -> Option<NativeInputEventKind> {
    if is_quit_key(code) && kind == KeyEventKind::Press {
        return Some(NativeInputEventKind::Quit);
    }

    let lane = lane_for_key(code)?;
    match kind {
        KeyEventKind::Press => Some(NativeInputEventKind::LanePress(lane)),
        KeyEventKind::Release => Some(NativeInputEventKind::LaneRelease(lane)),
        KeyEventKind::Repeat => None,
    }
}

fn lane_for_key(code: KeyCode) -> Option<u8> {
    match code {
        KeyCode::Char('d') | KeyCode::Char('D') => Some(0),
        KeyCode::Char('f') | KeyCode::Char('F') => Some(1),
        KeyCode::Char('j') | KeyCode::Char('J') => Some(2),
        KeyCode::Char('k') | KeyCode::Char('K') => Some(3),
        _ => None,
    }
}

fn is_quit_key(code: KeyCode) -> bool {
    matches!(code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q'))
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
