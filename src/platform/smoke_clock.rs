use rhythm_core::AudioClock;
use std::time::Instant;

/// Temporary native shell clock. Real gameplay should use the audio backend clock.
pub struct SmokeAudioClock {
    started_at: Instant,
}

impl SmokeAudioClock {
    pub fn started_now() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl AudioClock for SmokeAudioClock {
    fn now(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    fn start_time(&self) -> f64 {
        0.0
    }

    fn output_latency(&self) -> Option<f64> {
        None
    }
}
