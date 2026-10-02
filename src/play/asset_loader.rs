use super::{PlaySessionOptions, PreparedChartAssets, load_chart_audio_assets};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

pub struct AssetLoadJob {
    receiver: Option<Receiver<Result<PreparedChartAssets, String>>>,
    launch_error: Option<String>,
}

impl AssetLoadJob {
    pub fn start(options: PlaySessionOptions) -> Self {
        Self::start_task(move || {
            load_chart_audio_assets(&options).map_err(|error| {
                format!(
                    "Could not load {} (chart index {}): {error}",
                    options.chart_path.display(),
                    options.chart_index
                )
            })
        })
    }

    fn start_task(
        task: impl FnOnce() -> Result<PreparedChartAssets, String> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let launch_error = thread::Builder::new()
            .name("chart-audio-loader".into())
            .spawn(move || {
                // Dropping the job discards the result; this worker owns no native devices.
                let _ = sender.send(task());
            })
            .err()
            .map(|error| format!("Could not start chart loader: {error}"));
        Self {
            receiver: Some(receiver),
            launch_error,
        }
    }

    pub fn poll(&mut self) -> Option<Result<PreparedChartAssets, String>> {
        if let Some(error) = self.launch_error.take() {
            self.receiver = None;
            return Some(Err(error));
        }
        let receiver = self.receiver.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("Chart loader stopped before completing".into()),
        };
        self.receiver = None;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::settings::AppSettings;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "zeff-assets-{}-{}",
                std::process::id(),
                FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn options(&self) -> PlaySessionOptions {
            let mut options =
                PlaySessionOptions::from_app_calibration(&AppSettings::default()).unwrap();
            options.chart_path = self.0.join("chart.sm");
            options
        }

        fn write_assets(&self) {
            fs::write(
                self.0.join("chart.sm"),
                concat!(
                    "#TITLE:Loader test;\n#MUSIC:song.wav;\n#BPMS:0=120;\n",
                    "#NOTES:dance-single::Easy:1::\n1000\n0000\n0000\n0000;\n",
                    "#NOTES:dance-single::Hard:7::\n0100\n0000\n0000\n0000;\n",
                ),
            )
            .unwrap();
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&38u32.to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&8000u32.to_le_bytes());
            wav.extend_from_slice(&16000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&2u32.to_le_bytes());
            wav.extend_from_slice(&0i16.to_le_bytes());
            fs::write(self.0.join("song.wav"), wav).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn finish(job: &mut AssetLoadJob) -> Result<PreparedChartAssets, String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = job.poll() {
                assert!(job.poll().is_none());
                return result;
            }
            assert!(Instant::now() < deadline, "asset loader timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn prepares_exact_stepmania_difficulty_without_resolving_output() {
        let fixture = Fixture::new();
        fixture.write_assets();
        let mut options = fixture.options();
        options.chart_index = 1;
        options.audio.selection.device = Some("nonexistent output must never be resolved".into());
        let assets = finish(&mut AssetLoadJob::start(options)).unwrap();
        assert_eq!(
            assets.chart.metadata().difficulty.as_deref(),
            Some("Hard (7)")
        );
        assert_eq!(assets.audio_path, fixture.0.join("song.wav"));
        assert_eq!(assets.clip.sample_rate, 8000);
        assert_eq!(assets.clip.frame_count(), 1);
    }

    #[test]
    fn reports_missing_chart_and_audio_failures() {
        let fixture = Fixture::new();
        assert!(finish(&mut AssetLoadJob::start(fixture.options())).is_err());
        fixture.write_assets();
        fs::remove_file(fixture.0.join("song.wav")).unwrap();
        assert!(finish(&mut AssetLoadJob::start(fixture.options())).is_err());
    }

    #[test]
    fn rejects_invalid_difficulty_and_undecodable_audio() {
        let fixture = Fixture::new();
        fixture.write_assets();
        let mut options = fixture.options();
        options.chart_index = 2;
        assert!(finish(&mut AssetLoadJob::start(options)).is_err());
        fs::write(fixture.0.join("song.wav"), b"invalid audio").unwrap();
        assert!(finish(&mut AssetLoadJob::start(fixture.options())).is_err());
    }

    #[test]
    fn dropped_job_does_not_wait_for_worker_or_consume_its_result() {
        let (release, blocked) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let job = AssetLoadJob::start_task(move || {
            blocked.recv().unwrap();
            done.send(()).unwrap();
            Err("discarded result".into())
        });
        drop(job);
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn reports_worker_termination_once() {
        let mut job = AssetLoadJob::start_task(|| panic!("simulated worker failure"));
        assert!(
            finish(&mut job)
                .unwrap_err()
                .contains("stopped before completing")
        );
    }
}
