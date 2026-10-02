use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Running,
    Paused,
    Countdown(Instant),
    AwaitingAudio,
}

pub(super) struct SessionPause {
    mode: Mode,
    pub(super) physical_lanes: [bool; 4],
    required_lanes: [bool; 4],
    focused: bool,
}

impl Default for SessionPause {
    fn default() -> Self {
        Self {
            mode: Mode::Running,
            physical_lanes: [false; 4],
            required_lanes: [false; 4],
            focused: true,
        }
    }
}

impl SessionPause {
    pub(super) fn is_paused(&self) -> bool {
        self.mode != Mode::Running
    }

    pub(super) fn pause(&mut self, required_lanes: [bool; 4]) -> bool {
        let changed = self.mode != Mode::Paused;
        self.required_lanes = required_lanes;
        self.mode = Mode::Paused;
        changed
    }

    pub(super) fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.physical_lanes.fill(false);
        }
    }

    pub(super) fn set_lane(&mut self, lane: u8, pressed: bool) {
        if let Some(state) = self.physical_lanes.get_mut(lane as usize) {
            *state = pressed;
        }
    }

    pub(super) fn request_resume(&mut self, now: Instant) -> bool {
        if self.focused && self.mode == Mode::Paused {
            self.mode = Mode::Countdown(now + Duration::from_secs(3));
            return true;
        }
        false
    }

    pub(super) fn missing_hold_keys(&self) -> [bool; 4] {
        std::array::from_fn(|lane| self.required_lanes[lane] && !self.physical_lanes[lane])
    }

    pub(super) fn ready_to_resume(&self, now: Instant) -> bool {
        self.focused
            && matches!(self.mode, Mode::Countdown(deadline) if now >= deadline)
            && !self.missing_hold_keys().contains(&true)
    }

    pub(super) fn awaiting_audio(&mut self) {
        self.mode = Mode::AwaitingAudio;
    }

    pub(super) fn audio_is_running(&mut self) -> bool {
        if self.mode == Mode::AwaitingAudio {
            self.mode = Mode::Running;
            return true;
        }
        false
    }

    pub(super) fn resume_was_interrupted(&self) -> bool {
        self.mode == Mode::AwaitingAudio && self.missing_hold_keys().contains(&true)
    }

    pub(super) fn countdown_seconds(&self, now: Instant) -> Option<f64> {
        match self.mode {
            Mode::Countdown(deadline) => {
                Some(deadline.saturating_duration_since(now).as_secs_f64())
            }
            _ => None,
        }
    }

    pub(super) fn is_focused(&self) -> bool {
        self.focused
    }

    #[cfg(test)]
    pub(super) fn mode_is_awaiting_audio(&self) -> bool {
        self.mode == Mode::AwaitingAudio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhythm_core::{
        Chart, GameKey, InputEvent, JudgementWindows, LaneIndex, Note, NoteId, RhythmEngine,
    };

    #[test]
    fn resume_requires_focus_explicit_action_and_three_seconds() {
        let now = Instant::now();
        let mut pause = SessionPause::default();
        pause.pause([false; 4]);
        pause.set_focus(false);
        assert!(!pause.request_resume(now));
        pause.set_focus(true);
        assert!(pause.is_paused());
        assert!(!pause.ready_to_resume(now + Duration::from_secs(10)));
        assert!(pause.request_resume(now));
        assert!(!pause.request_resume(now));
        assert!(!pause.ready_to_resume(now + Duration::from_millis(2999)));
        assert!(pause.ready_to_resume(now + Duration::from_secs(3)));
        pause.awaiting_audio();
        assert!(pause.is_paused());
        assert!(pause.audio_is_running());
        assert!(!pause.is_paused());
    }

    #[test]
    fn focus_loss_cancels_countdown_and_clears_stale_physical_keys() {
        let now = Instant::now();
        let mut pause = SessionPause::default();
        pause.set_lane(0, true);
        pause.pause([true, false, false, false]);
        pause.request_resume(now);
        pause.set_focus(false);
        pause.pause([true, false, false, false]);
        pause.set_focus(true);
        assert!(!pause.ready_to_resume(now + Duration::from_secs(5)));
        assert_eq!(pause.physical_lanes, [false; 4]);
        assert!(pause.request_resume(now));
        assert!(!pause.ready_to_resume(now + Duration::from_secs(5)));
        pause.set_lane(0, true);
        assert!(pause.ready_to_resume(now + Duration::from_secs(5)));
        pause.awaiting_audio();
        pause.set_lane(0, false);
        assert!(pause.resume_was_interrupted());
    }

    #[test]
    fn paused_hold_can_be_regrabbed_without_scoring_another_head() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 5.0));
        let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
        engine
            .submit_input(InputEvent {
                key: GameKey::Lane(LaneIndex::new(0)),
                pressed: true,
                time_seconds: 1.0,
            })
            .unwrap();
        let mut pause = SessionPause::default();
        let now = Instant::now();
        pause.pause(std::array::from_fn(|lane| {
            engine.lane_has_active_hold(LaneIndex::new(lane as u8))
        }));
        pause.set_focus(false);
        pause.set_focus(true);
        pause.request_resume(now);
        pause.set_lane(0, true);
        assert!(pause.ready_to_resume(now + Duration::from_secs(3)));
        let replay_len = engine.replay().events().len();
        engine.synchronize_pressed_lanes(&pause.physical_lanes);
        assert_eq!(engine.replay().events().len(), replay_len);
        assert_eq!(engine.judged_count(), 0);
        let mut results = Vec::new();
        engine.collect_judgements(5.0, &mut results);
        assert_eq!(results.len(), 1);
        assert!(engine.is_complete());
        assert_ne!(results[0].rating, rhythm_core::HitRating::Miss);
    }
}
