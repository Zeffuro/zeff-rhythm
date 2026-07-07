use crate::{LaneIndex, NoteId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitRating {
    Marvelous,
    Perfect,
    Great,
    Good,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JudgementWindows {
    pub marvelous_seconds: f64,
    pub perfect_seconds: f64,
    pub great_seconds: f64,
    pub good_seconds: f64,
    pub miss_seconds: f64,
}

impl Default for JudgementWindows {
    fn default() -> Self {
        Self {
            marvelous_seconds: 0.0225,
            perfect_seconds: 0.045,
            great_seconds: 0.090,
            good_seconds: 0.135,
            miss_seconds: 0.180,
        }
    }
}

impl JudgementWindows {
    pub fn rating_for_delta(self, delta_seconds: f64) -> Option<HitRating> {
        let abs_delta = delta_seconds.abs();

        if abs_delta <= self.marvelous_seconds {
            Some(HitRating::Marvelous)
        } else if abs_delta <= self.perfect_seconds {
            Some(HitRating::Perfect)
        } else if abs_delta <= self.great_seconds {
            Some(HitRating::Great)
        } else if abs_delta <= self.good_seconds {
            Some(HitRating::Good)
        } else {
            None
        }
    }

    pub fn is_late_miss(self, note_time_seconds: f64, song_time_seconds: f64) -> bool {
        song_time_seconds - note_time_seconds > self.miss_seconds
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JudgementResult {
    pub note_id: NoteId,
    pub lane: LaneIndex,
    pub rating: HitRating,
    pub scheduled_time_seconds: f64,
    pub input_time_seconds: Option<f64>,
    pub delta_seconds: Option<f64>,
}

impl JudgementResult {
    pub const fn hit(
        note_id: NoteId,
        lane: LaneIndex,
        scheduled_time_seconds: f64,
        input_time_seconds: f64,
        delta_seconds: f64,
        rating: HitRating,
    ) -> Self {
        Self {
            note_id,
            lane,
            rating,
            scheduled_time_seconds,
            input_time_seconds: Some(input_time_seconds),
            delta_seconds: Some(delta_seconds),
        }
    }

    pub const fn miss(note_id: NoteId, lane: LaneIndex, scheduled_time_seconds: f64) -> Self {
        Self {
            note_id,
            lane,
            rating: HitRating::Miss,
            scheduled_time_seconds,
            input_time_seconds: None,
            delta_seconds: None,
        }
    }
}
