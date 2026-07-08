#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderTimingSample {
    pub frame_interval_ms: Option<f64>,
    pub input_to_frame_ms: Option<f64>,
    pub snapshot_build_ms: Option<f64>,
    pub command_encode_ms: Option<f64>,
    pub queue_submit_ms: Option<f64>,
    pub acquire_surface_ms: Option<f64>,
    pub present_call_ms: Option<f64>,
    pub gpu_frame_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderTelemetry {
    frame_intervals_ms: Vec<f64>,
    input_to_frame_ms: Vec<f64>,
    snapshot_build_ms: Vec<f64>,
    command_encode_ms: Vec<f64>,
    queue_submit_ms: Vec<f64>,
    acquire_surface_ms: Vec<f64>,
    present_call_ms: Vec<f64>,
    gpu_frame_ms: Vec<f64>,
}

impl RenderTelemetry {
    pub fn record(&mut self, sample: RenderTimingSample) {
        push_if_valid(&mut self.frame_intervals_ms, sample.frame_interval_ms);
        push_if_valid(&mut self.input_to_frame_ms, sample.input_to_frame_ms);
        push_if_valid(&mut self.snapshot_build_ms, sample.snapshot_build_ms);
        push_if_valid(&mut self.command_encode_ms, sample.command_encode_ms);
        push_if_valid(&mut self.queue_submit_ms, sample.queue_submit_ms);
        push_if_valid(&mut self.acquire_surface_ms, sample.acquire_surface_ms);
        push_if_valid(&mut self.present_call_ms, sample.present_call_ms);
        push_if_valid(&mut self.gpu_frame_ms, sample.gpu_frame_ms);
    }

    pub fn frame_sample_count(&self) -> usize {
        self.frame_intervals_ms.len()
    }

    pub fn gpu_sample_count(&self) -> usize {
        self.gpu_frame_ms.len()
    }
}

fn push_if_valid(samples: &mut Vec<f64>, value: Option<f64>) {
    let Some(value) = value else {
        return;
    };

    if value.is_finite() && value >= 0.0 {
        samples.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderTelemetry, RenderTimingSample};

    #[test]
    fn records_only_valid_samples() {
        let mut telemetry = RenderTelemetry::default();

        telemetry.record(RenderTimingSample {
            frame_interval_ms: Some(16.0),
            gpu_frame_ms: Some(f64::NAN),
            ..RenderTimingSample::default()
        });
        telemetry.record(RenderTimingSample {
            frame_interval_ms: Some(-1.0),
            gpu_frame_ms: Some(2.5),
            ..RenderTimingSample::default()
        });

        assert_eq!(telemetry.frame_sample_count(), 1);
        assert_eq!(telemetry.gpu_sample_count(), 1);
    }
}
