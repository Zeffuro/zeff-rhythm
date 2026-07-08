#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricStats {
    pub count: usize,
    pub mean: f64,
    pub stddev: f64,
    pub min: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl MetricStats {
    pub fn from_samples(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        let mut sorted = samples.to_vec();
        sorted.sort_by(|left, right| left.total_cmp(right));
        let count = sorted.len();
        let mean = sorted.iter().sum::<f64>() / count as f64;
        let variance = sorted
            .iter()
            .map(|sample| {
                let difference = sample - mean;
                difference * difference
            })
            .sum::<f64>()
            / count as f64;

        Some(Self {
            count,
            mean,
            stddev: variance.sqrt(),
            min: sorted[0],
            p50: percentile(&sorted, 0.50),
            p95: percentile(&sorted, 0.95),
            p99: percentile(&sorted, 0.99),
            max: sorted[count - 1],
        })
    }
}

pub fn trimmed_mean(samples: &[f64], trim_fraction: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }

    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let trim_each_side = ((sorted.len() as f64 * trim_fraction.clamp(0.0, 0.45)).floor() as usize)
        .min((sorted.len() - 1) / 2);
    let trimmed = &sorted[trim_each_side..sorted.len() - trim_each_side];

    Some(trimmed.iter().sum::<f64>() / trimmed.len() as f64)
}

pub fn print_metric(name: &str, samples: &[f64]) {
    let Some(stats) = MetricStats::from_samples(samples) else {
        println!("metric {name} count=0");
        return;
    };

    println!(
        "metric {name} count={} mean={:.3} stddev={:.3} min={:.3} p50={:.3} p95={:.3} p99={:.3} max={:.3}",
        stats.count,
        stats.mean,
        stats.stddev,
        stats.min,
        stats.p50,
        stats.p95,
        stats.p99,
        stats.max
    );
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::MetricStats;

    #[test]
    fn calculates_metric_stats() {
        let stats = MetricStats::from_samples(&[-10.0, 0.0, 10.0, 20.0]).unwrap();

        assert_eq!(stats.count, 4);
        assert_eq!(stats.mean, 5.0);
        assert_eq!(stats.min, -10.0);
        assert_eq!(stats.p50, 10.0);
        assert_eq!(stats.max, 20.0);
    }

    #[test]
    fn calculates_trimmed_mean() {
        let mean = super::trimmed_mean(&[-100.0, 0.0, 10.0, 20.0, 200.0], 0.2).unwrap();

        assert_eq!(mean, 10.0);
    }
}
