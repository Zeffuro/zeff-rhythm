use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct PlaybackClock {
    inner: Arc<Mutex<PlaybackClockState>>,
    duration_seconds: f64,
}

#[derive(Clone, Debug, Default)]
struct PlaybackClockState {
    start_wall: Option<Instant>,
    output_latency: Option<Duration>,
    frames_submitted: u64,
    sample_rate: u32,
    samples_exhausted: bool,
}

impl PlaybackClock {
    pub fn new(duration_seconds: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlaybackClockState::default())),
            duration_seconds,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        self.duration_seconds
    }

    pub fn song_time_seconds(&self) -> f64 {
        let now = Instant::now();
        let state = self.inner.lock().expect("playback clock lock poisoned");
        song_time_from_state(&state, now)
    }

    pub fn song_time_at(&self, time: Instant) -> f64 {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        song_time_from_state(&state, time)
    }

    pub fn scheduled_time_seconds(&self) -> f64 {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        if state.sample_rate == 0 {
            return 0.0;
        }

        state.frames_submitted as f64 / state.sample_rate as f64
    }

    pub fn output_latency_seconds(&self) -> Option<f64> {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        state.output_latency.map(|duration| duration.as_secs_f64())
    }

    pub fn is_started(&self) -> bool {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        state.start_wall.is_some()
    }

    pub fn is_finished(&self) -> bool {
        let now = Instant::now();
        let state = self.inner.lock().expect("playback clock lock poisoned");
        state.samples_exhausted && song_time_from_state(&state, now) >= self.duration_seconds
    }

    pub(crate) fn record_callback(
        &self,
        first_frame_at: Instant,
        first_frame_index: u64,
        frames_written: usize,
        sample_rate: u32,
        output_latency: Duration,
        samples_exhausted: bool,
    ) {
        let first_frame_time =
            Duration::from_secs_f64(first_frame_index as f64 / sample_rate as f64);
        let start_wall = first_frame_at
            .checked_sub(first_frame_time)
            .unwrap_or(first_frame_at);
        let mut state = self.inner.lock().expect("playback clock lock poisoned");

        state.start_wall = Some(start_wall);
        state.output_latency = Some(output_latency);
        state.frames_submitted = first_frame_index + frames_written as u64;
        state.sample_rate = sample_rate;
        state.samples_exhausted = samples_exhausted;
    }
}

fn song_time_from_state(state: &PlaybackClockState, now: Instant) -> f64 {
    let Some(start_wall) = state.start_wall else {
        return 0.0;
    };

    if now >= start_wall {
        now.duration_since(start_wall).as_secs_f64()
    } else {
        -start_wall.duration_since(now).as_secs_f64()
    }
}
