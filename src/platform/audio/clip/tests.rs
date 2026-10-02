use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeff-audio-decode-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn wav(&self, channels: u16, samples: &[i16]) -> PathBuf {
        let path = self.0.join("song.wav");
        let data_bytes = (samples.len() * size_of::<i16>()) as u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&44100u32.to_le_bytes());
        wav.extend_from_slice(&(44100 * u32::from(channels) * 2).to_le_bytes());
        wav.extend_from_slice(&(channels * 2).to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(&path, wav).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn preserves_samples_and_channels_across_packet_boundaries_without_growth_slack() {
    for channels in [1, 2] {
        let fixture = Fixture::new();
        let samples: Vec<i16> = (0..5003 * usize::from(channels))
            .map(|index| (index as i32 * 17 - 32768) as i16)
            .collect();
        let clip = load_audio_clip(&fixture.wav(channels, &samples)).unwrap();

        assert_eq!(clip.channels, usize::from(channels));
        assert_eq!(clip.sample_rate, 44100);
        assert_eq!(clip.frame_count(), 5003);
        assert_eq!(clip.samples.capacity(), clip.samples.len());
        for (actual, expected) in clip.samples.iter().zip(&samples) {
            assert_eq!(*actual, f32::from(*expected) / 32768.0);
        }
    }
}

#[test]
fn ignores_missing_oversized_and_overflowing_frame_hints() {
    let max_samples = MAX_INITIAL_ALLOCATION_BYTES / size_of::<f32>();
    assert_eq!(initial_sample_capacity(None, 2), 0);
    assert_eq!(initial_sample_capacity(Some(u64::MAX), 2), 0);
    assert_eq!(initial_sample_capacity(Some(2), usize::MAX), 0);
    assert_eq!(initial_sample_capacity(Some(max_samples as u64), 2), 0);
    assert_eq!(
        initial_sample_capacity(Some((max_samples / 2) as u64), 2),
        max_samples
    );
    assert_eq!(initial_sample_capacity(Some(5003), 2), 10006);
}

#[test]
fn rejects_empty_audio_and_malformed_headers() {
    let fixture = Fixture::new();
    let path = fixture.wav(2, &[]);
    assert!(load_audio_clip(&path).is_err());

    let path = fixture.wav(2, &[0; 4096]);
    let mut wav = fs::read(&path).unwrap();
    wav.truncate(20);
    fs::write(&path, wav).unwrap();
    assert!(load_audio_clip(&path).is_err());

    fs::write(&path, b"invalid audio").unwrap();
    assert!(load_audio_clip(&path).is_err());
}
