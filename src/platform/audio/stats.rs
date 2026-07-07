#[derive(Default)]
pub struct CallbackStats {
    callbacks: usize,
    last_callback_nanos: Option<u128>,
    callback_interval_ms: Vec<f64>,
    predicted_playback_lead_ms: Vec<f64>,
    frames_per_callback: Vec<usize>,
}

impl CallbackStats {
    pub fn record(&mut self, timestamp: cpal::OutputStreamTimestamp, frames: usize) {
        let callback_nanos = timestamp.callback.as_nanos();
        if let Some(last_callback_nanos) = self.last_callback_nanos {
            let interval_ms = (callback_nanos - last_callback_nanos) as f64 / 1_000_000.0;
            self.callback_interval_ms.push(interval_ms);
        }

        let playback_lead_ms = (timestamp.playback - timestamp.callback).as_secs_f64() * 1_000.0;
        self.predicted_playback_lead_ms.push(playback_lead_ms);
        self.frames_per_callback.push(frames);
        self.last_callback_nanos = Some(callback_nanos);
        self.callbacks += 1;
    }

    pub fn print(&self) {
        println!("callbacks={}", self.callbacks);
        print_stats("callback_interval_ms", &self.callback_interval_ms);
        print_stats(
            "predicted_playback_lead_ms",
            &self.predicted_playback_lead_ms,
        );

        if let Some(frames) = self.frames_per_callback.first() {
            println!("frames_per_callback_first={frames}");
        }
    }
}

fn print_stats(label: &str, values: &[f64]) {
    if values.is_empty() {
        println!("{label}=<none>");
        return;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let min = sorted[0];
    let p50 = percentile(&sorted, 0.50);
    let p95 = percentile(&sorted, 0.95);
    let p99 = percentile(&sorted, 0.99);
    let max = sorted[sorted.len() - 1];

    println!("{label}=min {min:.3} p50 {p50:.3} p95 {p95:.3} p99 {p99:.3} max {max:.3}");
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}
