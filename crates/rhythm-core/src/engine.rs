use crate::{
    Chart, GameKey, HitRating, InputEvent, JudgementPhase, JudgementResult, JudgementWindows,
    LaneIndex, NoteKind, ReplayLog,
};

#[derive(Clone, Copy)]
struct ActiveHold {
    head: JudgementResult,
    end_time_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub song_time_seconds: f64,
    pub frame_time_seconds: f64,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

pub struct RhythmEngine {
    chart: Chart,
    windows: JudgementWindows,
    judgements: Vec<Option<JudgementResult>>,
    holds: Vec<Option<ActiveHold>>,
    pressed_lanes: Vec<bool>,
    replay: ReplayLog,
}

impl RhythmEngine {
    pub fn new(chart: Chart, windows: JudgementWindows) -> Self {
        let judgements = vec![None; chart.notes().len()];
        let holds = vec![None; chart.notes().len()];
        let pressed_lanes = vec![false; chart.lane_count() as usize];

        Self {
            chart,
            windows,
            judgements,
            holds,
            pressed_lanes,
            replay: ReplayLog::default(),
        }
    }

    pub fn chart(&self) -> &Chart {
        &self.chart
    }

    pub fn replay(&self) -> &ReplayLog {
        &self.replay
    }

    pub fn submit_input(&mut self, event: InputEvent) -> Option<JudgementResult> {
        self.replay.push_input(event);

        let GameKey::Lane(lane) = event.key else {
            return None;
        };

        if lane.as_usize() >= self.pressed_lanes.len() || !event.time_seconds.is_finite() {
            return None;
        }
        self.pressed_lanes[lane.as_usize()] = event.pressed;

        if !event.pressed {
            let index = self
                .holds
                .iter()
                .position(|hold| hold.is_some_and(|hold| hold.head.lane == lane))?;
            let hold = self.holds[index].unwrap();
            let success =
                event.time_seconds >= hold.end_time_seconds - self.windows.perfect_seconds.max(0.0);
            return Some(self.finish_hold(index, success, Some(event.time_seconds)));
        }
        if self
            .holds
            .iter()
            .any(|hold| hold.is_some_and(|hold| hold.head.lane == lane))
        {
            return None;
        }

        let candidate = self
            .chart
            .notes()
            .iter()
            .enumerate()
            .filter(|(index, note)| {
                self.judgements[*index].is_none()
                    && self.holds[*index].is_none()
                    && note.lane == lane
                    && match note.kind {
                        NoteKind::Tap => true,
                        NoteKind::Hold { end_time_seconds } => {
                            event.time_seconds <= end_time_seconds
                        }
                    }
                    && self
                        .windows
                        .rating_for_delta(event.time_seconds - note.time_seconds)
                        .is_some()
            })
            .min_by(|(_, left), (_, right)| {
                let left_delta = (event.time_seconds - left.time_seconds).abs();
                let right_delta = (event.time_seconds - right.time_seconds).abs();
                left_delta.total_cmp(&right_delta)
            });

        let (index, note) = candidate?;
        let delta_seconds = event.time_seconds - note.time_seconds;
        let rating = self
            .windows
            .rating_for_delta(delta_seconds)
            .expect("candidate was filtered through judgement windows");
        let mut result = JudgementResult::hit(
            note.id,
            note.lane,
            note.time_seconds,
            event.time_seconds,
            delta_seconds,
            rating,
        );

        match note.kind {
            NoteKind::Tap => self.judgements[index] = Some(result),
            NoteKind::Hold { end_time_seconds } => {
                result.phase = JudgementPhase::HoldHead;
                self.holds[index] = Some(ActiveHold {
                    head: result,
                    end_time_seconds,
                });
            }
        }
        self.replay.push_judgement(result);

        Some(result)
    }

    pub fn collect_judgements(&mut self, song_time_seconds: f64, out: &mut Vec<JudgementResult>) {
        if !song_time_seconds.is_finite() {
            return;
        }
        for index in 0..self.chart.notes().len() {
            if self.judgements[index].is_some() {
                continue;
            }

            if let Some(hold) = self.holds[index] {
                if song_time_seconds >= hold.end_time_seconds {
                    let result = self.finish_hold(index, true, None);
                    out.push(result);
                }
                continue;
            }

            let note = self.chart.notes()[index];
            if !self
                .windows
                .is_late_miss(note.time_seconds, song_time_seconds)
            {
                continue;
            }

            let mut result = JudgementResult::miss(note.id, note.lane, note.time_seconds);
            if matches!(note.kind, NoteKind::Hold { .. }) {
                result.phase = JudgementPhase::HoldHead;
            }
            self.judgements[index] = Some(result);
            self.replay.push_judgement(result);
            out.push(result);
        }
    }

    fn finish_hold(
        &mut self,
        index: usize,
        success: bool,
        input_time: Option<f64>,
    ) -> JudgementResult {
        let hold = self.holds[index].take().unwrap();
        let result = JudgementResult {
            phase: JudgementPhase::HoldTail,
            note_id: hold.head.note_id,
            lane: hold.head.lane,
            rating: if success {
                hold.head.rating
            } else {
                HitRating::Miss
            },
            scheduled_time_seconds: hold.end_time_seconds,
            input_time_seconds: input_time,
            delta_seconds: None,
        };
        self.judgements[index] = Some(result);
        self.replay.push_judgement(result);
        result
    }

    pub fn lane_is_pressed(&self, lane: LaneIndex) -> bool {
        self.pressed_lanes
            .get(lane.as_usize())
            .copied()
            .unwrap_or(false)
    }

    pub fn lane_has_active_hold(&self, lane: LaneIndex) -> bool {
        self.holds
            .iter()
            .any(|hold| hold.is_some_and(|hold| hold.head.lane == lane))
    }

    pub fn synchronize_pressed_lanes(&mut self, pressed: &[bool]) {
        for (lane, state) in self.pressed_lanes.iter_mut().enumerate() {
            *state = pressed.get(lane).copied().unwrap_or(false);
        }
    }

    pub fn judged_count(&self) -> usize {
        self.judgements
            .iter()
            .filter(|judgement| judgement.is_some())
            .count()
    }

    pub fn is_complete(&self) -> bool {
        self.judged_count() == self.judgements.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HitRating, LaneIndex, Note, NoteId};

    #[test]
    fn judges_nearest_note_on_lane() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(1), 1.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 1.2));
        let mut engine = RhythmEngine::new(chart, JudgementWindows::default());

        let result = engine.submit_input(InputEvent {
            key: GameKey::Lane(LaneIndex::new(1)),
            pressed: true,
            time_seconds: 1.018,
        });

        assert_eq!(result.unwrap().rating, HitRating::Marvelous);
        assert_eq!(engine.judged_count(), 1);
    }

    #[test]
    fn ignores_release_events_for_tap_judgement() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(1), 1.0));
        let mut engine = RhythmEngine::new(chart, JudgementWindows::default());

        let result = engine.submit_input(InputEvent {
            key: GameKey::Lane(LaneIndex::new(1)),
            pressed: false,
            time_seconds: 1.0,
        });

        assert!(result.is_none());
        assert_eq!(engine.judged_count(), 0);
        assert_eq!(engine.replay().events().len(), 1);
    }

    #[test]
    fn collects_late_misses_into_caller_buffer() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(0), 1.0));
        let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
        let mut misses = Vec::new();

        engine.collect_judgements(1.181, &mut misses);

        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].rating, HitRating::Miss);
        assert!(engine.is_complete());
    }
}

#[cfg(test)]
mod hold_tests;
