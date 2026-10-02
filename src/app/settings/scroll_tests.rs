use super::*;

#[test]
fn speed_controls_only_note_travel_and_has_finite_bounds() {
    let mut settings = GameplaySettings::default();
    assert_eq!(settings.scroll_time_seconds(), 1.0);
    settings.scroll_speed = 2.0;
    assert_eq!(settings.scroll_time_seconds(), 0.5);
    settings.scroll_speed = 0.5;
    assert_eq!(settings.scroll_time_seconds(), 2.0);
    for speed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        settings.scroll_speed = speed;
        assert_eq!(settings.scroll_time_seconds(), 1.0);
    }
    settings.scroll_speed = -2.0;
    assert_eq!(settings.scroll_time_seconds(), 4.0);
    settings.scroll_speed = 80.0;
    assert_eq!(settings.scroll_time_seconds(), 0.25);
}

#[test]
fn speed_steps_leave_the_lower_bound_on_a_consistent_tenth_lattice() {
    let mut settings = GameplaySettings::default();
    for _ in 0..50 {
        settings.adjust_scroll_speed(-1);
    }
    assert_eq!(settings.scroll_speed, 0.25);
    assert!(settings.scroll_label().contains("0.25X / 4000 MS"));
    settings.adjust_scroll_speed(1);
    assert_eq!(settings.scroll_speed, 0.3);
    settings.adjust_scroll_speed(1);
    assert_eq!(settings.scroll_speed, 0.4);
    for _ in 0..50 {
        settings.adjust_scroll_speed(1);
    }
    assert_eq!(settings.scroll_speed, 4.0);
    settings.adjust_scroll_speed(-1);
    assert_eq!(settings.scroll_speed, 3.9);
}

#[test]
fn legacy_lookahead_is_ignored_and_existing_scroll_setting_takes_precedence() {
    let mut settings = AppSettings::default();
    settings.gameplay.scroll_speed = 2.0;
    settings.input.input_offset_ms = 23.0;
    let source = toml::to_string(&settings)
        .unwrap()
        .replace("[video]", "[video]\nlookahead_seconds = 2.0");
    let loaded: AppSettings = toml::from_str(&source).unwrap();
    assert_eq!(loaded, settings);
    assert_eq!(loaded.gameplay.scroll_time_seconds(), 0.5);
    let saved = toml::to_string(&loaded).unwrap();
    assert!(!saved.contains("lookahead_seconds"));
    assert_eq!(toml::from_str::<AppSettings>(&saved).unwrap(), settings);
}
