use crate::{InputEvent, JudgementResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplayEvent {
    Input(InputEvent),
    Judgement(JudgementResult),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayLog {
    events: Vec<ReplayEvent>,
}

impl ReplayLog {
    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }

    pub fn push_input(&mut self, event: InputEvent) {
        self.events.push(ReplayEvent::Input(event));
    }

    pub fn push_judgement(&mut self, result: JudgementResult) {
        self.events.push(ReplayEvent::Judgement(result));
    }
}
