mod sdl;
mod terminal;

pub use sdl::{SdlInputBackend, SdlTimestampClock, translate_sdl_event};
pub use terminal::TerminalInputBackend;

use std::error::Error;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeInputBackendKind {
    Terminal,
    Sdl,
}

impl NativeInputBackendKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "terminal" | "term" | "console" => Ok(Self::Terminal),
            "sdl" | "sdl3" => Ok(Self::Sdl),
            _ => Err(format!("unknown input backend: {value}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Sdl => "sdl",
        }
    }
}

pub enum NativeInputBackend {
    Terminal(TerminalInputBackend),
    Sdl(SdlInputBackend),
}

impl NativeInputBackend {
    pub fn new(kind: NativeInputBackendKind) -> Result<Self, Box<dyn Error>> {
        match kind {
            NativeInputBackendKind::Terminal => Ok(Self::Terminal(TerminalInputBackend::new()?)),
            NativeInputBackendKind::Sdl => Ok(Self::Sdl(SdlInputBackend::new()?)),
        }
    }

    pub fn poll(&mut self, output: &mut Vec<NativeInputEvent>) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Terminal(backend) => backend.poll(output),
            Self::Sdl(backend) => backend.poll(output),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeInputEvent {
    pub kind: NativeInputEventKind,
    pub source: NativeInputSource,
    pub timestamp_kind: NativeInputTimestampKind,
    pub event_time: Instant,
    pub received_time: Instant,
    pub source_timestamp_ns: Option<u64>,
    pub queue_age_ms: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeInputEventKind {
    LanePress(u8),
    LaneRelease(u8),
    FocusGained,
    FocusLost,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeInputSource {
    Terminal,
    Sdl,
    Winit,
}

impl NativeInputSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Sdl => "sdl",
            Self::Winit => "winit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeInputTimestampKind {
    ReceiptTime,
    SourceEventTime,
}

impl NativeInputTimestampKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReceiptTime => "receipt_time",
            Self::SourceEventTime => "source_event_time",
        }
    }
}
