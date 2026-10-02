use crate::platform::audio::PlaybackClock;
use cpal::traits::StreamTrait;
use rhythm_core::JudgementWindows;
use std::error::Error;
use std::time::{Duration, Instant};

pub(crate) struct ChartClock {
    audio_clock: PlaybackClock,
    chart_start_seconds: f64,
    chart_start_wall: Instant,
    audio_started: bool,
    paused_at: Option<Instant>,
}

impl ChartClock {
    pub(crate) fn new(
        audio_clock: PlaybackClock,
        chart_start_seconds: f64,
        start_delay_seconds: f64,
    ) -> Self {
        Self {
            audio_clock,
            chart_start_seconds,
            chart_start_wall: Instant::now() + Duration::from_secs_f64(start_delay_seconds),
            audio_started: false,
            paused_at: None,
        }
    }

    pub(crate) fn chart_time_seconds(&self) -> f64 {
        if self.audio_started {
            return self.audio_clock.song_time_seconds();
        }

        self.chart_time_at(Instant::now())
    }

    pub(crate) fn chart_time_at(&self, time: Instant) -> f64 {
        if self.audio_started {
            return self.audio_clock.song_time_at(time);
        }

        let time = self.paused_at.map_or(time, |pause| time.min(pause));

        if time < self.chart_start_wall {
            return self.chart_start_seconds;
        }

        self.chart_start_seconds + time.duration_since(self.chart_start_wall).as_secs_f64()
    }

    pub(crate) fn start_audio_if_due(
        &mut self,
        stream: &cpal::Stream,
    ) -> Result<(), Box<dyn Error>> {
        if self.paused_at.is_none() && !self.audio_started && self.chart_time_seconds() >= 0.0 {
            stream.play()?;
            self.audio_started = true;
        }

        Ok(())
    }

    pub(crate) fn countdown_seconds(&self) -> Option<f64> {
        let now = self.paused_at.unwrap_or_else(Instant::now);
        if now < self.chart_start_wall {
            Some(self.chart_start_wall.duration_since(now).as_secs_f64())
        } else {
            None
        }
    }

    pub(crate) fn scheduled_time_seconds(&self) -> f64 {
        self.audio_clock.scheduled_time_seconds()
    }

    pub(crate) fn duration_seconds(&self) -> f64 {
        self.audio_clock.duration_seconds()
    }

    pub(crate) fn output_latency_seconds(&self) -> Option<f64> {
        self.audio_clock.output_latency_seconds()
    }

    pub(crate) fn first_callback_frames(&self) -> Option<usize> {
        self.audio_clock.first_callback_frames()
    }

    pub(crate) fn is_started(&self) -> bool {
        self.audio_started && self.audio_clock.is_started()
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.audio_started && self.audio_clock.is_finished()
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
        self.audio_clock.set_paused(true);
    }

    pub(crate) fn resume(&mut self, now: Instant) {
        if let Some(paused_at) = self.paused_at.take() {
            if !self.audio_started {
                self.chart_start_wall += now.saturating_duration_since(paused_at);
            }
        }
        self.audio_clock.set_paused(false);
    }

    pub(crate) fn is_running(&self) -> bool {
        self.paused_at.is_none() && (!self.audio_started || self.audio_clock.is_running())
    }

    pub(crate) fn accepts_input_at(&self, time: Instant) -> bool {
        self.paused_at.is_none()
            && if self.audio_started {
                self.audio_clock.is_running_at(time)
            } else {
                time >= self.chart_start_wall
            }
    }
}

pub(crate) fn hard_end_seconds(
    chart: &rhythm_core::Chart,
    clip_duration_seconds: f64,
    windows: JudgementWindows,
    input_offset_ms: f64,
) -> f64 {
    chart
        .notes()
        .iter()
        .map(|note| match note.kind {
            rhythm_core::NoteKind::Tap => note.time_seconds,
            rhythm_core::NoteKind::Hold { end_time_seconds } => end_time_seconds,
        })
        .reduce(f64::max)
        .map(|last_time| {
            last_time + windows.miss_seconds + 1.0 + (-input_offset_ms / 1_000.0).max(0.0)
        })
        .unwrap_or(clip_duration_seconds)
        .max(clip_duration_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhythm_core::{Chart, LaneIndex, Note, NoteId};

    #[test]
    fn pause_preserves_pre_roll_and_initial_countdown() {
        let audio = PlaybackClock::new(10.0);
        let mut clock = ChartClock::new(audio, -4.0, 1.5);
        let start = clock.chart_start_wall;
        clock.pause(start - Duration::from_secs(1));
        assert_eq!(clock.chart_time_at(start + Duration::from_secs(30)), -4.0);
        assert!(!clock.accepts_input_at(start + Duration::from_secs(30)));
        clock.resume(start + Duration::from_secs(9));
        assert_eq!(clock.chart_start_wall, start + Duration::from_secs(10));
        assert_eq!(clock.chart_time_at(start + Duration::from_secs(11)), -3.0);
        clock.pause(start + Duration::from_secs(12));
        assert_eq!(clock.chart_time_at(start + Duration::from_secs(50)), -2.0);
        clock.resume(start + Duration::from_secs(22));
        assert_eq!(clock.chart_time_at(start + Duration::from_secs(23)), -1.0);
    }

    #[test]
    fn inputs_from_initial_or_resumed_silence_are_rejected() {
        let base = Instant::now();
        let audio = PlaybackClock::new(10.0);
        let mut clock = ChartClock::new(audio.clone(), 0.0, 0.0);
        clock.audio_started = true;
        audio.begin_callback(base, 0, 100, 1000, Duration::ZERO, 10000);
        assert!(!clock.accepts_input_at(base - Duration::from_millis(1)));
        assert!(clock.accepts_input_at(base + Duration::from_millis(1)));
        clock.pause(base + Duration::from_millis(50));
        audio.begin_callback(
            base + Duration::from_millis(100),
            100,
            100,
            1000,
            Duration::ZERO,
            10000,
        );
        clock.resume(base + Duration::from_secs(3));
        audio.begin_callback(
            base + Duration::from_secs(4),
            100,
            100,
            1000,
            Duration::from_secs(1),
            10000,
        );
        assert!(!clock.accepts_input_at(base + Duration::from_secs(3)));
        assert!(clock.accepts_input_at(base + Duration::from_millis(4001)));
    }

    #[test]
    fn hard_end_keeps_audio_duration_when_chart_is_shorter() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(0), 1.0));

        assert_eq!(
            hard_end_seconds(&chart, 10.0, rhythm_core::JudgementWindows::default(), 0.0),
            10.0
        );
    }

    #[test]
    fn hard_end_reaches_longest_tail_even_when_a_later_head_is_shorter() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 20.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 5.0));
        assert_eq!(
            hard_end_seconds(&chart, 6.0, rhythm_core::JudgementWindows::default(), 0.0),
            21.18
        );
    }

    #[test]
    fn large_negative_offsets_leave_time_for_every_final_judgement() {
        use rhythm_core::{GameKey, InputEvent, RhythmEngine};
        let mut chart = Chart::new(2);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 3.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 1.0));
        for offset_ms in [-2000.0, 0.0, 2000.0] {
            let windows = rhythm_core::JudgementWindows::default();
            let end = hard_end_seconds(&chart, 0.5, windows, offset_ms);
            let mut engine = RhythmEngine::new(chart.clone(), windows);
            engine
                .submit_input(InputEvent {
                    key: GameKey::Lane(LaneIndex::new(0)),
                    pressed: true,
                    time_seconds: 1.0,
                })
                .unwrap();
            let mut results = Vec::new();
            engine.collect_judgements(end + offset_ms / 1_000.0, &mut results);
            assert!(engine.is_complete());
            assert_eq!(results.len(), 2);
        }
    }
}
