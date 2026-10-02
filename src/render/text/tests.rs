use super::*;

#[test]
fn text_cache_reuses_shaped_lines() {
    let first = line("Zeff Rhythm / 1.25x", 2);
    let second = line("Zeff Rhythm / 1.25x", 2);
    assert!(Arc::ptr_eq(&first, &second));
    assert!(!first.rects.is_empty());
    assert_eq!(first.missing_glyphs, 0);
}

#[test]
fn ellipsis_fits_measured_width_without_splitting_graphemes() {
    let text = "Cafe\u{301} / 日本語の長い曲名 / 한글";
    let fitted = fit_text(text, 105.0, 2);
    assert!(fitted.ends_with('…'));
    assert!(line(&fitted, 2).width <= 105.0);
    let prefix = fitted.strip_suffix('…').unwrap();
    assert!(text.starts_with(prefix));
    assert!(
        text.grapheme_indices(true)
            .any(|(index, _)| index == prefix.len())
    );
    assert_eq!(fit_text("abc", 0.0, 2), "");
}

#[test]
fn long_titles_are_fitted_without_rasterizing_the_hidden_text() {
    let mut system = TextSystem::new();
    let title = "鬱".repeat(2048);
    let fitted = system.fit(normalize(&title), 500.0, 2);
    assert!(fitted.ends_with('…'));
    assert!(system.measure(&fitted, 2) <= 500.0);
    assert!(system.cache.is_empty() && system.swash.image_cache.is_empty());
    assert_eq!(system.fit(normalize(&title), 500.0, 2), fitted);
    assert_eq!(system.fits.len(), 1);
    let first = system.line(&fitted, 2);
    assert!(Arc::ptr_eq(&first, &system.line(&fitted, 2)));
    assert!(system.bytes <= CACHE_BYTES && system.fit_bytes <= FIT_CACHE_BYTES);
}

#[test]
fn pathological_graphemes_and_control_text_are_bounded_before_shaping() {
    let combining = format!("a{}", "\u{301}".repeat(100_000));
    assert_eq!(normalize(&combining), "…");
    let large = normalize(&"界".repeat(10_000));
    assert!(large.len() <= MAX_TEXT_BYTES + 3);
    assert!(large.graphemes(true).count() <= MAX_GRAPHEMES + 1);
    assert_eq!(normalize("one\r\ntwo\tthree"), "one two three");
    assert!(fit_text(&combining, f32::INFINITY, 2).len() <= MAX_TEXT_BYTES + 3);
    assert_eq!(fit_text("test", f32::NAN, 2), "");
}

#[cfg(windows)]
#[test]
fn windows_font_fallback_shapes_native_titles_and_other_scripts() {
    for text in [
        "日本語 カタカナ ひらがな",
        "中文标题",
        "한국어",
        "Привет",
        "العربية",
        "हिन्दी",
        "Cafe\u{301}",
    ] {
        let rendered = line(text, 2);
        assert_eq!(rendered.missing_glyphs, 0, "Missing font coverage: {text}");
        assert!(rendered.width > 0.0 && !rendered.rects.is_empty());
    }
}
