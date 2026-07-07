use crate::{Chart, GameKey, InputEvent, JudgementResult, JudgementWindows, ReplayLog};

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
    replay: ReplayLog,
}

impl RhythmEngine {
    pub fn new(chart: Chart, windows: JudgementWindows) -> Self {
        let judgements = vec![None; chart.notes().len()];

        Self {
            chart,
            windows,
            judgements,
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

        if !event.pressed {
            return None;
        }

        let candidate = self
            .chart
            .notes()
            .iter()
            .enumerate()
            .filter(|(index, note)| {
                self.judgements[*index].is_none()
                    && note.lane == lane
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
        let result = JudgementResult::hit(
            note.id,
            note.lane,
            note.time_seconds,
            event.time_seconds,
            delta_seconds,
            rating,
        );

        self.judgements[index] = Some(result);
        self.replay.push_judgement(result);

        Some(result)
    }

    pub fn collect_misses(&mut self, song_time_seconds: f64, out: &mut Vec<JudgementResult>) {
        for index in 0..self.chart.notes().len() {
            if self.judgements[index].is_some() {
                continue;
            }

            let note = self.chart.notes()[index];
            if !self
                .windows
                .is_late_miss(note.time_seconds, song_time_seconds)
            {
                continue;
            }

            let result = JudgementResult::miss(note.id, note.lane, note.time_seconds);
            self.judgements[index] = Some(result);
            self.replay.push_judgement(result);
            out.push(result);
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

        engine.collect_misses(1.181, &mut misses);

        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].rating, HitRating::Miss);
        assert!(engine.is_complete());
    }
}
