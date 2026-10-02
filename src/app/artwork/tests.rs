use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "zeff-artwork-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("chart.osu"), "fixture").unwrap();
        Self(root)
    }

    fn image(&self, name: &str, width: u32, height: u32, color: [u8; 4]) -> PathBuf {
        let path = self.0.join(name);
        image::RgbaImage::from_pixel(width, height, image::Rgba(color))
            .save(&path)
            .unwrap();
        path
    }

    fn request(&self, name: &str) -> ArtworkRequest {
        ArtworkRequest {
            chart_path: self.0.join("chart.osu"),
            background_path: Some(self.0.join(name)),
            banner_path: None,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn pixel(color: [u8; 4]) -> Arc<DecodedArtwork> {
    Arc::new(DecodedArtwork {
        width: 1,
        height: 1,
        rgba: color.to_vec(),
        fit: ArtworkFit::Cover,
    })
}

fn finish(loader: &mut ArtworkLoader) -> Option<Arc<DecodedArtwork>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(update) = loader.poll() {
            return update.image;
        }
        assert!(Instant::now() < deadline, "artwork worker timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn new_selection_keeps_displayed_image_until_replacement_and_rejects_old_completion() {
    let fixture = Fixture::new();
    fixture.image("b.png", 1, 1, [0, 255, 0, 255]);
    let mut loader = ArtworkLoader::default();
    let now = Instant::now();
    loader.request_at(Some(fixture.request("a.png")), now);
    assert!(loader.poll_at(now).is_none());
    let (sender, receiver) = mpsc::channel();
    loader.active = Some(ActiveArtwork {
        generation: loader.generation,
        receiver,
    });
    loader.pending = None;
    loader.request_at(Some(fixture.request("b.png")), now);
    assert!(loader.poll_at(now).is_none());
    sender
        .send((ArtworkCache::default(), Some(pixel([255, 0, 0, 255]))))
        .ok()
        .unwrap();
    assert!(loader.poll_at(now).is_none());
    assert!(loader.is_loading());
    assert!(loader.poll_at(now + DEBOUNCE).is_none());
    let image = finish(&mut loader).unwrap();
    assert_eq!(image.rgba, [0, 255, 0, 255]);
    assert!(!loader.is_loading());
}

#[test]
fn rapid_requests_keep_one_active_decode_and_only_the_latest_pending_selection() {
    let fixture = Fixture::new();
    fixture.image("latest.png", 1, 1, [0, 0, 255, 255]);
    let mut loader = ArtworkLoader::default();
    let (sender, receiver) = mpsc::channel();
    loader.active = Some(ActiveArtwork {
        generation: 0,
        receiver,
    });
    let now = Instant::now();
    for index in 0..100 {
        loader.request_at(Some(fixture.request(&format!("unused-{index}.png"))), now);
        assert!(loader.poll_at(now).is_none());
        assert!(loader.poll_at(now + DEBOUNCE).is_none());
        assert_eq!(loader.active.as_ref().unwrap().generation, 0);
    }
    let latest = fixture.request("latest.png");
    loader.request_at(Some(latest.clone()), now);
    assert_eq!(loader.pending.as_ref().unwrap().request, latest);
    assert!(loader.poll_at(now).is_none());
    sender.send((ArtworkCache::default(), None)).ok().unwrap();
    assert!(loader.poll_at(now + DEBOUNCE).is_none());
    assert_eq!(finish(&mut loader).unwrap().rgba, [0, 0, 255, 255]);
    assert!(!loader.is_loading());
}

#[test]
fn clearing_selection_discards_in_flight_art_and_disconnected_jobs_finish() {
    let mut loader = ArtworkLoader::default();
    let (sender, receiver) = mpsc::channel();
    loader.active = Some(ActiveArtwork {
        generation: 0,
        receiver,
    });
    loader.request(None);
    assert!(loader.poll().unwrap().image.is_none());
    sender
        .send((ArtworkCache::default(), Some(pixel([255; 4]))))
        .ok()
        .unwrap();
    assert!(loader.poll().is_none());
    assert!(!loader.is_loading());
    let (sender, receiver) = mpsc::channel();
    loader.active = Some(ActiveArtwork {
        generation: loader.generation,
        receiver,
    });
    drop(sender);
    assert!(loader.poll().unwrap().image.is_none());
    assert!(!loader.is_loading());
}

#[test]
fn debounce_waits_until_navigation_settles() {
    let fixture = Fixture::new();
    let mut loader = ArtworkLoader::default();
    let now = Instant::now();
    loader.request_at(Some(fixture.request("missing.png")), now);
    assert!(loader.poll_at(now).is_none());
    assert!(
        loader
            .poll_at(now + DEBOUNCE - Duration::from_millis(1))
            .is_none()
    );
    assert!(loader.active.is_none());
    assert!(loader.pending.is_some());
    loader.request_at(None, now);
    assert!(loader.poll_at(now + DEBOUNCE).unwrap().image.is_none());
    assert!(!loader.is_loading());
}

#[test]
fn browsing_past_charts_without_art_keeps_the_background_until_selection_settles() {
    let fixture = Fixture::new();
    let empty = ArtworkRequest {
        chart_path: fixture.0.join("empty.osu"),
        background_path: None,
        banner_path: None,
    };
    let mut loader = ArtworkLoader::default();
    let now = Instant::now();
    let (sender, receiver) = mpsc::channel();
    loader.active = Some(ActiveArtwork {
        generation: 0,
        receiver,
    });
    loader.request_at(Some(empty.clone()), now);
    assert!(loader.poll_at(now + DEBOUNCE / 2).is_none());
    loader.request_at(Some(fixture.request("next.png")), now + DEBOUNCE / 2);
    assert!(loader.poll_at(now + DEBOUNCE).is_none());
    loader.request_at(Some(empty), now + DEBOUNCE);
    assert!(loader.poll_at(now + DEBOUNCE + DEBOUNCE / 2).is_none());
    assert!(loader.poll_at(now + DEBOUNCE * 2).unwrap().image.is_none());
    sender
        .send((ArtworkCache::default(), Some(pixel([255; 4]))))
        .ok()
        .unwrap();
    assert!(loader.poll_at(now + DEBOUNCE * 2).is_none());
    assert!(!loader.is_loading());
}

#[test]
fn missing_corrupt_oversized_and_unsupported_backgrounds_fall_back_to_banner() {
    let fixture = Fixture::new();
    let banner = fixture.image("banner.png", 3, 2, [0, 255, 0, 255]);
    fs::write(fixture.0.join("bad.png"), b"not an image").unwrap();
    fs::write(fixture.0.join("vector.svg"), b"<svg></svg>").unwrap();
    fs::File::create(fixture.0.join("large.png"))
        .unwrap()
        .set_len(MAX_ENCODED_BYTES + 1)
        .unwrap();
    fixture.image("wide.png", MAX_DIMENSION + 1, 1, [255; 4]);
    let mut cache = ArtworkCache::default();
    for name in [
        "missing.png",
        "bad.png",
        "large.png",
        "wide.png",
        "vector.svg",
    ] {
        let mut request = fixture.request(name);
        request.banner_path = Some(banner.clone());
        let image = cache.load(&request).unwrap();
        assert_eq!((image.width, image.height), (3, 2), "{name}");
        assert_eq!(image.fit, ArtworkFit::Contain, "{name}");
        assert_eq!(&image.rgba[..4], &[0, 255, 0, 255], "{name}");
    }
    assert!(cache.load(&fixture.request("bad.png")).is_none());
    let background = fixture.image("valid.png", 2, 3, [255, 0, 0, 255]);
    let image = cache
        .load(&ArtworkRequest {
            chart_path: fixture.0.join("chart.osu"),
            background_path: Some(background),
            banner_path: Some(banner),
        })
        .unwrap();
    assert_eq!((image.width, image.height), (2, 3));
    assert_eq!(image.fit, ArtworkFit::Cover);
}

#[test]
fn decoded_art_is_resized_without_distortion_or_upscaling() {
    let fixture = Fixture::new();
    let mut cache = ArtworkCache::default();
    for (name, width, height, expected) in [
        ("wide.png", 3200, 100, (1600, 50)),
        ("tall.png", 100, 1000, (90, 900)),
        ("small.png", 10, 5, (10, 5)),
    ] {
        fixture.image(name, width, height, [255; 4]);
        let image = cache.load(&fixture.request(name)).unwrap();
        assert_eq!((image.width, image.height), expected);
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
    }
}

#[test]
fn the_same_cached_image_uses_the_requested_background_or_banner_fit() {
    let fixture = Fixture::new();
    let path = fixture.image("banner.png", 256, 80, [255; 4]);
    let mut cache = ArtworkCache::default();
    let mut request = fixture.request("banner.png");
    let background = cache.load(&request).unwrap();
    assert_eq!(background.fit, ArtworkFit::Cover);
    request.background_path = None;
    request.banner_path = Some(path);
    let banner = cache.load(&request).unwrap();
    assert_eq!(banner.fit, ArtworkFit::Contain);
    assert_eq!((banner.width, banner.height), (256, 80));
    assert_eq!(banner.rgba, background.rgba);
    assert!(Arc::ptr_eq(&banner, &cache.load(&request).unwrap()));
}

#[test]
fn cache_revalidates_changed_and_removed_files_and_checks_containment_before_hits() {
    let fixture = Fixture::new();
    let path = fixture.image("art.png", 2, 1, [255, 0, 0, 255]);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(100))
        .unwrap();
    let request = fixture.request("art.png");
    let mut cache = ArtworkCache::default();
    let first = cache.load(&request).unwrap();
    assert!(Arc::ptr_eq(&first, &cache.load(&request).unwrap()));
    let outside = Fixture::new();
    let mut outside_request = request.clone();
    outside_request.chart_path = outside.0.join("chart.osu");
    assert!(cache.load(&outside_request).is_none());
    fixture.image("art.png", 2, 1, [0, 0, 255, 255]);
    let changed = cache.load(&request).unwrap();
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(&changed.rgba[..4], &[0, 0, 255, 255]);
    fs::remove_file(&path).unwrap();
    assert!(cache.load(&request).is_none());
    fs::write(&path, b"corrupt replacement").unwrap();
    assert!(cache.load(&request).is_none());
    assert!(cache.entries.is_empty());
}

#[test]
fn cache_evicts_to_its_retained_allocation_budget() {
    let mut cache = ArtworkCache::default();
    for index in 0..10 {
        let image = Arc::new(DecodedArtwork {
            width: 1600,
            height: 900,
            rgba: vec![0; 1600 * 900 * 4],
            fit: ArtworkFit::Cover,
        });
        cache.insert(
            PathBuf::from(index.to_string()),
            Signature {
                size: 1,
                modified: SystemTime::UNIX_EPOCH,
            },
            image,
        );
        assert!(cache.bytes <= CACHE_BYTES);
        assert_eq!(
            cache.bytes,
            cache
                .entries
                .iter()
                .map(|entry| entry.image.rgba.capacity())
                .sum()
        );
    }
    assert_eq!(cache.entries.len(), 5);
    assert_eq!(cache.entries.front().unwrap().path, Path::new("5"));
    assert_eq!(cache.entries.back().unwrap().path, Path::new("9"));
}
