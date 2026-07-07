#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioTimeSnapshot {
    pub now_seconds: f64,
    pub start_time_seconds: f64,
    pub output_latency_seconds: Option<f64>,
}

pub trait AudioClock {
    /// Current estimated song/audio time in seconds.
    fn now(&self) -> f64;

    /// Time at which playback began on this clock.
    fn start_time(&self) -> f64;

    /// Estimated output latency in seconds if the platform can report it.
    fn output_latency(&self) -> Option<f64>;

    fn snapshot(&self) -> AudioTimeSnapshot {
        AudioTimeSnapshot {
            now_seconds: self.now(),
            start_time_seconds: self.start_time(),
            output_latency_seconds: self.output_latency(),
        }
    }
}
