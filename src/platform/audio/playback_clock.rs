use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct PlaybackClock {
    inner: Arc<Mutex<PlaybackClockState>>,
    duration_seconds: f64,
}

#[derive(Debug)]
struct ClockSegment {
    begins_at: Instant,
    anchor_at: Instant,
    media_seconds: f64,
    paused: bool,
}

#[derive(Debug)]
struct PlaybackClockState {
    segments: Vec<ClockSegment>,
    pause_requested: bool,
    callback_paused: bool,
    output_latency: Option<Duration>,
    frames_submitted: u64,
    first_callback_frames: Option<usize>,
    sample_rate: u32,
    samples_exhausted: bool,
    started: bool,
}

impl PlaybackClock {
    pub fn new(duration_seconds: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlaybackClockState {
                segments: Vec::with_capacity(8),
                pause_requested: false,
                callback_paused: false,
                output_latency: None,
                frames_submitted: 0,
                first_callback_frames: None,
                sample_rate: 0,
                samples_exhausted: false,
                started: false,
            })),
            duration_seconds,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        self.duration_seconds
    }

    /// Applies at the next callback boundary; already queued audio drains normally.
    pub fn set_paused(&self, paused: bool) {
        let mut state = self.inner.lock().expect("playback clock lock poisoned");
        if state.pause_requested != paused {
            // Reserve on the control thread so callback transitions never allocate.
            state.segments.reserve(2);
            state.pause_requested = paused;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.inner
            .lock()
            .expect("playback clock lock poisoned")
            .pause_requested
    }

    /// Remains true through the playback lead of the first resumed callback.
    pub fn is_pause_applied(&self) -> bool {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        pause_applied_at(&state, Instant::now())
    }

    /// True once a media callback is audible and no pause is requested or latched.
    pub fn is_running(&self) -> bool {
        self.is_running_at(Instant::now())
    }

    /// Rejects timestamps before the latest audible resume boundary.
    pub fn is_running_at(&self, time: Instant) -> bool {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        running_at(&state, time)
    }

    pub fn song_time_seconds(&self) -> f64 {
        self.song_time_at(Instant::now())
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

    pub fn first_callback_frames(&self) -> Option<usize> {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        state.first_callback_frames
    }

    pub fn is_started(&self) -> bool {
        self.inner
            .lock()
            .expect("playback clock lock poisoned")
            .started
    }

    pub fn is_finished(&self) -> bool {
        let state = self.inner.lock().expect("playback clock lock poisoned");
        state.samples_exhausted
            && song_time_from_state(&state, Instant::now()) >= self.duration_seconds
    }

    pub(crate) fn begin_callback(
        &self,
        first_frame_at: Instant,
        first_frame_index: u64,
        frames: usize,
        sample_rate: u32,
        output_latency: Duration,
        total_output_frames: u64,
    ) -> bool {
        let mut state = self.inner.lock().expect("playback clock lock poisoned");
        let paused = state.pause_requested;
        let media_seconds = first_frame_index as f64 / sample_rate as f64;
        state.first_callback_frames.get_or_insert(frames);
        state.output_latency = Some(output_latency);
        state.sample_rate = sample_rate;

        if state.segments.is_empty() || state.callback_paused != paused {
            state.segments.push(ClockSegment {
                begins_at: first_frame_at,
                anchor_at: first_frame_at,
                media_seconds,
                paused,
            });
        } else if !paused {
            let segment = state.segments.last_mut().unwrap();
            segment.anchor_at = first_frame_at;
            segment.media_seconds = media_seconds;
        }
        state.callback_paused = paused;
        if !paused {
            state.started = true;
            state.frames_submitted = first_frame_index + frames as u64;
            state.samples_exhausted = state.frames_submitted >= total_output_frames;
        }
        !paused
    }
}

fn segment_index_at(state: &PlaybackClockState, now: Instant) -> Option<usize> {
    if state.segments.is_empty() {
        return None;
    }
    Some(
        state
            .segments
            .partition_point(|segment| segment.begins_at <= now)
            .saturating_sub(1),
    )
}

fn pause_applied_at(state: &PlaybackClockState, now: Instant) -> bool {
    state.callback_paused
        || segment_index_at(state, now).is_some_and(|index| state.segments[index].paused)
        || (state.segments.len() > 1
            && state
                .segments
                .last()
                .is_some_and(|segment| !segment.paused && now < segment.begins_at))
}

fn running_at(state: &PlaybackClockState, now: Instant) -> bool {
    !state.pause_requested
        && !state.callback_paused
        && state
            .segments
            .last()
            .is_some_and(|segment| !segment.paused && now >= segment.begins_at)
}

fn song_time_from_state(state: &PlaybackClockState, now: Instant) -> f64 {
    let Some(index) = segment_index_at(state, now) else {
        return 0.0;
    };
    let segment = &state.segments[index];
    if segment.paused {
        return segment.media_seconds;
    }
    let elapsed = if now >= segment.anchor_at {
        now.duration_since(segment.anchor_at).as_secs_f64()
    } else {
        -segment.anchor_at.duration_since(now).as_secs_f64()
    };
    let time = segment.media_seconds + elapsed;
    state
        .segments
        .get(index + 1)
        .filter(|next| next.paused)
        .map_or(time, |next| time.min(next.media_seconds))
}

#[cfg(test)]
mod tests;
