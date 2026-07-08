use rhythm_core::{HitRating, JudgementResult};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JudgementCounts {
    pub marvelous: usize,
    pub perfect: usize,
    pub great: usize,
    pub good: usize,
    pub miss: usize,
}

impl JudgementCounts {
    pub fn add(&mut self, result: JudgementResult) {
        match result.rating {
            HitRating::Marvelous => self.marvelous += 1,
            HitRating::Perfect => self.perfect += 1,
            HitRating::Great => self.great += 1,
            HitRating::Good => self.good += 1,
            HitRating::Miss => self.miss += 1,
        }
    }
}
