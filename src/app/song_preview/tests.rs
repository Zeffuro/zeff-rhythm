use super::*;

fn request(name: &str) -> Option<PreviewRequest> {
    Some(PreviewRequest {
        path: name.into(),
        start_seconds: Some(5.0),
        duration_seconds: None,
    })
}

fn clip() -> Arc<AudioClip> {
    Arc::new(AudioClip {
        samples: vec![0.0; 32],
        channels: 1,
        sample_rate: 8000,
    })
}

#[test]
fn rapid_changes_debounce_and_only_latest_decodes() {
    let now = Instant::now();
    let mut loader = PreviewLoader::default();
    loader.request(request("first"), now);
    assert!(
        loader
            .poll_with(now + DEBOUNCE / 2, |_| panic!("too early"))
            .is_none()
    );
    loader.request(request("last"), now + DEBOUNCE / 2);
    assert!(
        loader
            .poll_with(now + DEBOUNCE, |_| panic!("debounce must restart"))
            .is_none()
    );
    let (sender, receiver) = mpsc::channel();
    loader.poll_with(now + DEBOUNCE * 2, |req| {
        assert_eq!(req.path, PathBuf::from("last"));
        Ok(receiver)
    });
    sender.send(Ok(clip())).unwrap();
    let (req, _) = loader
        .poll_with(now + DEBOUNCE * 2, |_| panic!("duplicate decode"))
        .unwrap()
        .unwrap();
    assert_eq!(req.path, PathBuf::from("last"));
    assert!(!loader.is_loading());
    assert!(!loader.request(request("last"), now + DEBOUNCE * 3));
    assert!(
        loader
            .poll_with(now + DEBOUNCE * 4, |_| panic!("same audio restarted"))
            .is_none()
    );
}

#[test]
fn stale_decode_is_discarded_and_workers_never_overlap() {
    let now = Instant::now();
    let mut loader = PreviewLoader::default();
    loader.request(request("first"), now);
    let (first, rx) = mpsc::channel();
    loader.poll_with(now + DEBOUNCE, |_| Ok(rx));
    loader.request(request("last"), now + DEBOUNCE);
    loader.poll_with(now + DEBOUNCE * 2, |_| panic!("already decoding"));
    first.send(Ok(clip())).unwrap();
    let (last, rx) = mpsc::channel();
    assert!(loader.poll_with(now + DEBOUNCE * 3, |_| Ok(rx)).is_none());
    loader.request(None, now + DEBOUNCE * 3);
    last.send(Ok(clip())).unwrap();
    assert!(
        loader
            .poll_with(now + DEBOUNCE * 4, |_| panic!("cancelled"))
            .is_none()
    );
    assert!(!loader.is_loading());
}

#[test]
fn failures_are_reported_once_and_selection_change_can_retry() {
    let now = Instant::now();
    let mut loader = PreviewLoader::default();
    loader.request(request("missing"), now);
    assert!(
        loader
            .poll_with(now + DEBOUNCE, |_| Err("spawn failed".into()))
            .unwrap()
            .is_err()
    );
    assert!(
        loader
            .poll_with(now + DEBOUNCE * 2, |_| panic!("failure retried"))
            .is_none()
    );
    loader.request(None, now);
    loader.request(request("missing"), now);
    let (sender, rx) = mpsc::channel();
    loader.poll_with(now + DEBOUNCE, |_| Ok(rx));
    drop(sender);
    assert!(
        loader
            .poll_with(now + DEBOUNCE * 2, |_| panic!())
            .unwrap()
            .is_err()
    );
    assert!(!loader.is_loading());
}
