use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Debug)]
pub struct PlaybackVolume {
    gain_bits: Arc<AtomicU32>,
}

impl PlaybackVolume {
    pub fn new(gain: f32) -> Self {
        Self {
            gain_bits: Arc::new(AtomicU32::new(sanitize_gain(gain).to_bits())),
        }
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain_bits
            .store(sanitize_gain(gain).to_bits(), Ordering::Relaxed);
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }
}

impl Default for PlaybackVolume {
    fn default() -> Self {
        Self::new(1.0)
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(super) struct GainRamp {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    frames: u32,
}

impl GainRamp {
    pub(super) fn new(gain: f32, sample_rate: u32) -> Self {
        Self {
            current: gain,
            target: gain,
            step: 0.0,
            remaining: 0,
            frames: (sample_rate / 100).max(1),
        }
    }

    pub(super) fn set_target(&mut self, target: f32) {
        if self.target != target {
            self.target = target;
            self.remaining = self.frames;
            self.step = (target - self.current) / self.frames as f32;
        }
    }

    pub(super) fn next_gain(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.current = if self.remaining == 0 {
                self.target
            } else {
                (self.current + self.step).clamp(0.0, 1.0)
            };
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_is_bounded_and_shared() {
        let volume = PlaybackVolume::default();
        let control = volume.clone();
        assert_eq!(volume.gain(), 1.0);
        for (input, expected) in [
            (-1.0, 0.0),
            (2.0, 1.0),
            (0.375, 0.375),
            (f32::NAN, 0.0),
            (f32::INFINITY, 0.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            control.set_gain(input);
            assert_eq!(volume.gain(), expected);
            assert_eq!(PlaybackVolume::new(input).gain(), expected);
        }
    }

    #[test]
    fn ramp_starts_at_requested_gain_and_reaches_exact_endpoints() {
        for sample_rate in [100, 44_100, 48_000, 192_000] {
            let frames = (sample_rate / 100).max(1);
            let mut ramp = GainRamp::new(0.0, sample_rate);
            assert_eq!(ramp.next_gain(), 0.0);
            for target in [1.0, 0.0] {
                ramp.set_target(target);
                let mut previous = ramp.current;
                for _ in 0..frames {
                    let gain = ramp.next_gain();
                    assert!((0.0..=1.0).contains(&gain));
                    assert!((gain - previous).abs() <= 1.0 / frames as f32 + 0.00002);
                    previous = gain;
                }
                assert_eq!(ramp.next_gain(), target);
            }
        }
    }

    #[test]
    fn repeated_callback_targets_do_not_restart_ramp() {
        let mut ramp = GainRamp::new(1.0, 48_000);
        for _ in 0..480 {
            ramp.set_target(0.0);
            ramp.next_gain();
        }
        assert_eq!(ramp.current, 0.0);
        ramp.set_target(1.0);
        for _ in 0..100 {
            ramp.next_gain();
        }
        let start = ramp.current;
        ramp.set_target(0.0);
        assert!((ramp.next_gain() - start).abs() <= 1.0 / 480.0);
        for _ in 1..480 {
            ramp.next_gain();
        }
        assert_eq!(ramp.current, 0.0);
    }
}
