use crate::LaneIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameKey {
    Lane(LaneIndex),
    Start,
    Back,
    MenuUp,
    MenuDown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputEvent {
    pub key: GameKey,
    pub pressed: bool,
    pub time_seconds: f64,
}
