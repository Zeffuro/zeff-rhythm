use super::*;
use crate::app::settings::AppSettings;
use crate::play::{PlaySessionOptions, load_chart_audio_assets};
use std::fs::{File, FileTimes};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeff-clip-cache-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn wav(&self, name: &str, samples: &[i16], revision: u64) -> PathBuf {
        let path = self.0.join(name);
        let data_bytes = (samples.len() * 2) as u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(&path, wav).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                FileTimes::new()
                    .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000 + revision)),
            )
            .unwrap();
        path
    }

    fn options(&self) -> PlaySessionOptions {
        fs::write(
            self.0.join("chart.sm"),
            concat!(
                "#TITLE:Cache test;\n#MUSIC:song.wav;\n#BPMS:0=120;\n",
                "#NOTES:dance-single::Easy:1::\n1000\n0000\n0000\n0000;\n",
                "#NOTES:dance-single::Hard:7::\n0100\n0000\n0000\n0000;\n",
            ),
        )
        .unwrap();
        let mut options =
            PlaySessionOptions::from_app_calibration(&AppSettings::default()).unwrap();
        options.chart_path = self.0.join("chart.sm");
        options.audio.selection.device = Some("must not resolve output during asset load".into());
        options
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn isolated_load(path: &Path, cache: &Mutex<ClipCache>) -> Arc<AudioClip> {
    load_with(path, cache, load_audio_clip).unwrap()
}

#[test]
fn reuses_audio_across_exact_difficulties_and_reparses_chart() {
    let fixture = Fixture::new();
    fixture.wav("song.wav", &[0, 16000], 0);
    let mut options = fixture.options();
    let preview = crate::play::load_cached_audio(&fixture.0.join("song.wav")).unwrap();
    let easy = load_chart_audio_assets(&options).unwrap();
    assert!(Arc::ptr_eq(&preview, &easy.clip));
    options.chart_index = 1;
    let source = fs::read_to_string(&options.chart_path).unwrap();
    fs::write(
        &options.chart_path,
        source.replace("Cache test", "New title"),
    )
    .unwrap();
    let hard = load_chart_audio_assets(&options).unwrap();
    assert!(Arc::ptr_eq(&easy.clip, &hard.clip));
    assert_eq!(
        easy.chart.metadata().difficulty.as_deref(),
        Some("Easy (1)")
    );
    assert_eq!(
        hard.chart.metadata().difficulty.as_deref(),
        Some("Hard (7)")
    );
    assert_eq!(hard.chart.metadata().title, "New title");
    assert_eq!(hard.clip.samples.len(), 2);
    options.chart_index = 2;
    assert!(load_chart_audio_assets(&options).is_err());
}

#[test]
fn canonical_alias_reuses_clip_and_nanosecond_revision_invalidates_it() {
    let fixture = Fixture::new();
    let path = fixture.wav("song.wav", &[0, 16000], 0);
    let cache = Mutex::new(ClipCache::new(MAX_BYTES, 8));
    let first = isolated_load(&path, &cache);
    let alias = fixture.0.join(".").join("song.wav");
    assert!(Arc::ptr_eq(&first, &isolated_load(&alias, &cache)));
    fixture.wav("song.wav", &[0, -16000], 0);
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(
            UNIX_EPOCH + Duration::from_secs(1_700_000_000) + Duration::from_nanos(100_000),
        ))
        .unwrap();
    let revised = isolated_load(&path, &cache);
    assert!(!Arc::ptr_eq(&first, &revised));
    assert!(first.samples[1] > 0.0 && revised.samples[1] < 0.0);
}

#[test]
fn deletion_corruption_and_failed_decode_never_return_cached_success() {
    let fixture = Fixture::new();
    let path = fixture.wav("song.wav", &[0], 0);
    let cache = Mutex::new(ClipCache::new(MAX_BYTES, 8));
    let first = isolated_load(&path, &cache);
    fs::remove_file(&path).unwrap();
    assert!(load_with(&path, &cache, load_audio_clip).is_err());
    fs::write(&path, b"invalid audio").unwrap();
    assert!(load_with(&path, &cache, load_audio_clip).is_err());
    assert!(cache.lock().unwrap().entries.is_empty());
    fixture.wav("song.wav", &[12000, 0], 1);
    let recovered = isolated_load(&path, &cache);
    assert!(!Arc::ptr_eq(&first, &recovered));
    assert_eq!(recovered.samples.len(), 2);
}

#[test]
fn lru_entry_and_allocation_limits_evict_and_oversized_clips_are_not_retained() {
    let fixture = Fixture::new();
    let a = fixture.wav("a.wav", &[0], 0);
    let b = fixture.wav("b.wav", &[0], 0);
    let c = fixture.wav("c.wav", &[0], 0);
    let cache = Mutex::new(ClipCache::new(MAX_BYTES, 2));
    let first_a = isolated_load(&a, &cache);
    let first_b = isolated_load(&b, &cache);
    assert!(Arc::ptr_eq(&first_a, &isolated_load(&a, &cache)));
    isolated_load(&c, &cache);
    assert!(Arc::ptr_eq(&first_a, &isolated_load(&a, &cache)));
    assert!(!Arc::ptr_eq(&first_b, &isolated_load(&b, &cache)));

    let allocation_bytes = first_a.samples.capacity() * size_of::<f32>();
    let bounded = Mutex::new(ClipCache::new(allocation_bytes, 8));
    let first = isolated_load(&a, &bounded);
    isolated_load(&b, &bounded);
    assert_eq!(bounded.lock().unwrap().entries.len(), 1);
    assert!(!Arc::ptr_eq(&first, &isolated_load(&a, &bounded)));
    let tiny = Mutex::new(ClipCache::new(allocation_bytes - 1, 8));
    let oversized = isolated_load(&a, &tiny);
    assert!(!Arc::ptr_eq(&oversized, &isolated_load(&a, &tiny)));
    assert!(tiny.lock().unwrap().entries.is_empty());
    assert_eq!(tiny.lock().unwrap().bytes, 0);
}

#[test]
fn changing_file_during_decode_is_rejected_without_holding_cache_lock() {
    let fixture = Fixture::new();
    let path = fixture.wav("song.wav", &[0], 0);
    let cache = Mutex::new(ClipCache::new(MAX_BYTES, 8));
    let result = load_with(&path, &cache, |decode_path| {
        assert!(cache.try_lock().is_ok());
        let clip = load_audio_clip(decode_path)?;
        fixture.wav("song.wav", &[16000, 0], 1);
        Ok(clip)
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("changed while decoding")
    );
    assert!(cache.lock().unwrap().entries.is_empty());
    assert_eq!(isolated_load(&path, &cache).samples.len(), 2);
}

#[test]
fn concurrent_decodes_admit_one_shared_clip() {
    let fixture = Fixture::new();
    let path = fixture.wav("song.wav", &[0, 16000], 0);
    let cache = Mutex::new(ClipCache::new(MAX_BYTES, 8));
    let decoded = std::sync::Barrier::new(2);
    let (first, second) = std::thread::scope(|scope| {
        let load = || {
            load_with(&path, &cache, |path| {
                let clip = load_audio_clip(path)?;
                decoded.wait();
                Ok(clip)
            })
            .unwrap()
        };
        let first = scope.spawn(load);
        let second = scope.spawn(load);
        (first.join().unwrap(), second.join().unwrap())
    });
    assert!(Arc::ptr_eq(&first, &second));
    let cache = cache.lock().unwrap();
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.bytes, first.samples.capacity() * size_of::<f32>());
}
