use crate::app::settings::InputBackendPreference;
use crate::app::state::{AppScreen, AppState, ChartSelection, SettingsPanel};
use crate::play::PlaySessionOptions;
use std::error::Error;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if !args.is_empty() {
        return Err("usage: zeff-rhythm app-smoke".into());
    }

    let mut state = AppState::new();
    println!("app_smoke=state");
    println!("screen={}", screen_name(state.screen));
    println!(
        "audio host={} device={} device_label={} sample_rate={} buffer={}",
        optional_text(state.settings.audio.host.as_deref()),
        optional_text(state.settings.audio.device_id.as_deref()),
        optional_text(state.settings.audio.device_label.as_deref()),
        optional_u32(state.settings.audio.sample_rate),
        optional_u32(state.settings.audio.buffer_frames)
    );
    println!(
        "input backend={:?} offset_ms={:.3} lanes={}",
        state.settings.input.backend,
        state.settings.input.input_offset_ms,
        lane_bindings(&state)
    );
    println!(
        "video fullscreen={} vsync={} present={} frame_latency={} target_fps={} lookahead_seconds={:.3}",
        state.settings.video.fullscreen,
        state.settings.video.vsync,
        state.settings.video.render_latency.present_mode.as_str(),
        state
            .settings
            .video
            .render_latency
            .desired_maximum_frame_latency,
        optional_u32(state.settings.video.target_frame_rate),
        state.settings.video.lookahead_seconds
    );
    println!(
        "diagnostics event_log={} runtime_metrics={} overlay={}",
        state.settings.diagnostics.event_log_enabled,
        state.settings.diagnostics.runtime_metrics_enabled,
        state.settings.diagnostics.overlay_enabled
    );
    println!("screens={}", screen_list());
    println!("settings_panels={}", settings_panel_list());
    println!("input_backends={}", input_backend_list());

    state.open_song_select();
    println!("open_song_select={}", screen_name(state.screen));
    state.open_settings(SettingsPanel::Audio);
    println!("open_settings={}", screen_name(state.screen));
    state.open_calibration();
    println!("open_calibration={}", screen_name(state.screen));
    state.open_main_menu();
    println!("open_main_menu={}", screen_name(state.screen));
    state.select_chart(ChartSelection::new(
        ".local_assets/stepmania/speedcore/Speedcore.sm",
    ));
    state.request_play()?;

    let request = state
        .take_pending_launch()
        .ok_or("app state did not create a play launch request")?;
    let session_options = PlaySessionOptions::from_app_launch(&request)?;
    println!("launch_chart={}", request.chart.chart_path.display());
    println!(
        "launch_input_offset_ms={:.3}",
        request.settings.input.input_offset_ms
    );
    println!("session_display={}", session_options.display.as_str());
    println!(
        "session_input={}",
        session_options.effective_input().as_str()
    );
    println!(
        "session_lookahead_seconds={:.3}",
        session_options.lookahead_seconds
    );
    println!(
        "session_event_log={}",
        session_options
            .event_log_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned())
    );

    Ok(())
}

fn screen_name(screen: AppScreen) -> &'static str {
    match screen {
        AppScreen::MainMenu => "main_menu",
        AppScreen::SongSelect => "song_select",
        AppScreen::Settings(SettingsPanel::Audio) => "settings.audio",
        AppScreen::Settings(SettingsPanel::Input) => "settings.input",
        AppScreen::Settings(SettingsPanel::Video) => "settings.video",
        AppScreen::Settings(SettingsPanel::Gameplay) => "settings.gameplay",
        AppScreen::Settings(SettingsPanel::Diagnostics) => "settings.diagnostics",
        AppScreen::Calibration => "calibration",
        AppScreen::Gameplay => "gameplay",
        AppScreen::Results => "results",
    }
}

fn optional_text(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn screen_list() -> String {
    [
        AppScreen::MainMenu,
        AppScreen::SongSelect,
        AppScreen::Settings(SettingsPanel::Audio),
        AppScreen::Calibration,
        AppScreen::Gameplay,
        AppScreen::Results,
    ]
    .map(screen_name)
    .join(",")
}

fn settings_panel_list() -> String {
    [
        SettingsPanel::Audio,
        SettingsPanel::Input,
        SettingsPanel::Video,
        SettingsPanel::Gameplay,
        SettingsPanel::Diagnostics,
    ]
    .map(|panel| screen_name(AppScreen::Settings(panel)).replace("settings.", ""))
    .join(",")
}

fn input_backend_list() -> String {
    [
        InputBackendPreference::Sdl,
        InputBackendPreference::TerminalDebug,
    ]
    .map(|backend| format!("{backend:?}"))
    .join(",")
}

fn lane_bindings(state: &AppState) -> String {
    state
        .settings
        .input
        .lane_bindings
        .iter()
        .map(|binding| format!("{}:{}", binding.lane, binding.code))
        .collect::<Vec<_>>()
        .join(",")
}
