use super::persistence::CalibrationDeviceKey;
use crate::play::LiveRunSummary;
use crate::play::metrics::{MetricStats, trimmed_mean};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalibrationHistory {
    device_key: Option<CalibrationDeviceKey>,
    attempts: usize,
    trials: Vec<CalibrationTrial>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationTrial {
    pub input_offset_ms: f64,
    pub hit_count: usize,
    pub mean_hit_delta_ms: f64,
    pub robust_hit_delta_ms: f64,
    pub suggested_total_offset_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationAggregate {
    pub attempts: usize,
    pub trial_count: usize,
    pub hit_count: usize,
    pub suggested_offset_ms: f64,
    pub suggested_offset_stats: MetricStats,
    pub mean_hit_delta_ms: f64,
    pub confidence: CalibrationConfidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationConfidence {
    NoSamples,
    NeedsMoreHits,
    NeedsMoreTrials,
    WideRange,
    Stable,
}

impl CalibrationHistory {
    pub fn record_run(&mut self, summary: &LiveRunSummary) -> bool {
        let key = CalibrationDeviceKey::from_audio(&summary.audio);
        if !self
            .device_key
            .as_ref()
            .is_some_and(|previous| previous.same_stream_as(&key))
        {
            self.clear();
        }
        self.device_key = Some(key);
        self.attempts += 1;

        let Some(trial) = CalibrationTrial::from_run(summary) else {
            return false;
        };

        self.trials.push(trial);
        true
    }

    pub fn clear(&mut self) {
        self.device_key = None;
        self.attempts = 0;
        self.trials.clear();
    }

    pub fn attempts(&self) -> usize {
        self.attempts
    }

    pub fn device_key(&self) -> Option<&CalibrationDeviceKey> {
        self.device_key.as_ref()
    }

    pub fn aggregate(&self) -> Option<CalibrationAggregate> {
        if self.trials.is_empty() {
            return None;
        }

        let suggested_samples = weighted_samples(
            self.trials
                .iter()
                .map(|trial| (trial.suggested_total_offset_ms, trial.hit_count)),
        );
        let delta_samples = weighted_samples(
            self.trials
                .iter()
                .map(|trial| (trial.mean_hit_delta_ms, trial.hit_count)),
        );
        let suggested_offset_stats = MetricStats::from_samples(&suggested_samples)?;
        let mean_hit_delta_ms = MetricStats::from_samples(&delta_samples)
            .map(|stats| stats.mean)
            .unwrap_or_default();
        let hit_count = self.trials.iter().map(|trial| trial.hit_count).sum();
        let confidence =
            calibration_confidence(self.trials.len(), hit_count, suggested_offset_stats);

        Some(CalibrationAggregate {
            attempts: self.attempts,
            trial_count: self.trials.len(),
            hit_count,
            suggested_offset_ms: suggested_offset_stats.mean,
            suggested_offset_stats,
            mean_hit_delta_ms,
            confidence,
        })
    }
}

impl CalibrationTrial {
    fn from_run(summary: &LiveRunSummary) -> Option<Self> {
        let hit_delta_ms = summary.report.hit_delta_ms?;
        let hit_count = summary.report.hits.max(hit_delta_ms.count);
        if hit_count == 0 {
            return None;
        }
        let robust_hit_delta_ms = robust_hit_delta_ms(summary).unwrap_or(hit_delta_ms.mean);

        Some(Self {
            input_offset_ms: summary.input_offset_ms,
            hit_count,
            mean_hit_delta_ms: hit_delta_ms.mean,
            robust_hit_delta_ms,
            suggested_total_offset_ms: summary.input_offset_ms - robust_hit_delta_ms,
        })
    }
}

impl CalibrationConfidence {
    pub fn display_label(self) -> &'static str {
        match self {
            Self::NoSamples => "NO SAMPLES",
            Self::NeedsMoreHits => "MORE HITS",
            Self::NeedsMoreTrials => "MORE TRIALS",
            Self::WideRange => "WIDE RANGE",
            Self::Stable => "STABLE",
        }
    }
}

fn calibration_confidence(
    trial_count: usize,
    hit_count: usize,
    suggested_offset_stats: MetricStats,
) -> CalibrationConfidence {
    if hit_count == 0 {
        return CalibrationConfidence::NoSamples;
    }
    if hit_count < 12 {
        return CalibrationConfidence::NeedsMoreHits;
    }
    if trial_count < 3 {
        return CalibrationConfidence::NeedsMoreTrials;
    }
    if suggested_offset_stats.max - suggested_offset_stats.min > 12.0
        || suggested_offset_stats.stddev > 6.0
    {
        return CalibrationConfidence::WideRange;
    }

    CalibrationConfidence::Stable
}

fn weighted_samples(samples: impl Iterator<Item = (f64, usize)>) -> Vec<f64> {
    let mut weighted = Vec::new();
    for (sample, weight) in samples {
        weighted.extend(std::iter::repeat_n(sample, weight.max(1)));
    }
    weighted
}

fn robust_hit_delta_ms(summary: &LiveRunSummary) -> Option<f64> {
    trimmed_mean(&summary.report.hit_delta_samples_ms, 0.10)
}

#[cfg(test)]
mod tests {
    use super::{CalibrationConfidence, CalibrationHistory};
    use crate::play::metrics::MetricStats;
    use crate::play::{JudgementCounts, LiveAudioSummary, LiveRunSummary, PlayReportSummary};

    #[test]
    fn aggregates_repeated_calibration_trials_by_hits() {
        let mut history = CalibrationHistory::default();

        assert!(history.record_run(&summary(20.0, -5.0, 10)));
        assert!(history.record_run(&summary(24.0, 1.0, 20)));

        let aggregate = history.aggregate().unwrap();
        assert_eq!(aggregate.attempts, 2);
        assert_eq!(aggregate.trial_count, 2);
        assert_eq!(aggregate.hit_count, 30);
        assert!((aggregate.suggested_offset_ms - 23.666).abs() < 0.01);
        assert_eq!(aggregate.confidence, CalibrationConfidence::NeedsMoreTrials);
    }

    #[test]
    fn trial_uses_trimmed_delta_when_samples_are_available() {
        let mut history = CalibrationHistory::default();

        assert!(history.record_run(&summary_with_samples(
            0.0,
            &[-120.0, -4.0, -2.0, 0.0, 2.0, 4.0, 120.0],
        )));

        let aggregate = history.aggregate().unwrap();
        assert!(aggregate.suggested_offset_ms.abs() < 0.001);
    }

    #[test]
    fn ignores_runs_without_hit_samples_but_counts_attempts() {
        let mut history = CalibrationHistory::default();
        let mut summary = summary(0.0, 0.0, 0);
        summary.report.hit_delta_ms = None;

        assert!(!history.record_run(&summary));

        assert_eq!(history.attempts(), 1);
        assert!(history.aggregate().is_none());
    }

    #[test]
    fn different_devices_do_not_pool_trials() {
        let mut history = CalibrationHistory::default();
        let mut first = summary(0.0, -20.0, 12);
        first.audio.device_id = Some("device-a".to_owned());
        let mut second = summary(0.0, -80.0, 12);
        second.audio.device_id = Some("device-b".to_owned());
        history.record_run(&first);
        history.record_run(&second);
        let aggregate = history.aggregate().unwrap();
        assert_eq!(aggregate.suggested_offset_ms, 80.0);
        assert_eq!(aggregate.trial_count, 1);
        assert_eq!(
            history.device_key().unwrap().cpal_device_id.as_deref(),
            Some("device-b")
        );
    }

    #[test]
    fn changed_stream_without_hits_cannot_reuse_previous_trials() {
        let mut history = CalibrationHistory::default();
        let first = summary(0.0, -20.0, 12);
        history.record_run(&first);
        let mut second = summary(0.0, 0.0, 0);
        second.audio.sample_rate = 48_000;
        second.report.hit_delta_ms = None;
        assert!(!history.record_run(&second));
        assert_eq!(history.attempts(), 1);
        assert!(history.aggregate().is_none());
    }

    fn summary(input_offset_ms: f64, mean_delta_ms: f64, hits: usize) -> LiveRunSummary {
        LiveRunSummary {
            title: "Generated Calibration".to_owned(),
            input_offset_ms,
            audio: LiveAudioSummary::default(),
            judged_count: hits,
            complete: false,
            counts: JudgementCounts::default(),
            report: PlayReportSummary {
                hits,
                hit_delta_samples_ms: vec![mean_delta_ms; hits],
                hit_delta_ms: Some(MetricStats {
                    count: hits,
                    mean: mean_delta_ms,
                    stddev: 1.0,
                    min: mean_delta_ms - 1.0,
                    p50: mean_delta_ms,
                    p95: mean_delta_ms + 1.0,
                    p99: mean_delta_ms + 1.0,
                    max: mean_delta_ms + 1.0,
                }),
                ..PlayReportSummary::default()
            },
            event_log_path: None,
        }
    }

    fn summary_with_samples(input_offset_ms: f64, samples: &[f64]) -> LiveRunSummary {
        let stats = MetricStats::from_samples(samples).unwrap();
        LiveRunSummary {
            title: "Generated Calibration".to_owned(),
            input_offset_ms,
            audio: LiveAudioSummary::default(),
            judged_count: samples.len(),
            complete: false,
            counts: JudgementCounts::default(),
            report: PlayReportSummary {
                hits: samples.len(),
                hit_delta_samples_ms: samples.to_vec(),
                hit_delta_ms: Some(stats),
                ..PlayReportSummary::default()
            },
            event_log_path: None,
        }
    }
}
