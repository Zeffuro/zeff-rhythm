use super::settings::AppSettings;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub screen: AppScreen,
    pub settings: AppSettings,
    pub selected_chart: Option<ChartSelection>,
    pending_launch: Option<PlayLaunchRequest>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen: AppScreen::MainMenu,
            settings: AppSettings::default(),
            selected_chart: None,
            pending_launch: None,
        }
    }

    pub fn open_main_menu(&mut self) {
        self.screen = AppScreen::MainMenu;
    }

    pub fn open_song_select(&mut self) {
        self.screen = AppScreen::SongSelect;
    }

    pub fn open_settings(&mut self, panel: SettingsPanel) {
        self.screen = AppScreen::Settings(panel);
    }

    pub fn open_calibration(&mut self) {
        self.screen = AppScreen::Calibration;
    }

    pub fn select_chart(&mut self, selection: ChartSelection) {
        self.selected_chart = Some(selection);
    }

    pub fn request_play(&mut self) -> Result<(), AppStateError> {
        let chart = self
            .selected_chart
            .clone()
            .ok_or(AppStateError::NoChartSelected)?;
        let request = PlayLaunchRequest {
            chart,
            settings: self.settings.clone(),
        };

        self.pending_launch = Some(request);
        self.screen = AppScreen::Gameplay;
        Ok(())
    }

    pub fn take_pending_launch(&mut self) -> Option<PlayLaunchRequest> {
        self.pending_launch.take()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    MainMenu,
    SongSelect,
    Settings(SettingsPanel),
    Calibration,
    Gameplay,
    Results,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPanel {
    Audio,
    Input,
    Video,
    Gameplay,
    Diagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartSelection {
    pub chart_path: PathBuf,
    pub audio_path: Option<PathBuf>,
}

impl ChartSelection {
    pub fn new(chart_path: impl Into<PathBuf>) -> Self {
        Self {
            chart_path: chart_path.into(),
            audio_path: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayLaunchRequest {
    pub chart: ChartSelection,
    pub settings: AppSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppStateError {
    NoChartSelected,
}

impl std::fmt::Display for AppStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoChartSelected => formatter.write_str("no chart selected"),
        }
    }
}

impl std::error::Error for AppStateError {}

#[cfg(test)]
mod tests {
    use super::{AppScreen, AppState, AppStateError, ChartSelection, SettingsPanel};

    #[test]
    fn starts_at_main_menu() {
        let state = AppState::new();

        assert_eq!(state.screen, AppScreen::MainMenu);
        assert!(state.selected_chart.is_none());
        assert!(state.pending_launch.is_none());
    }

    #[test]
    fn creates_play_launch_from_selected_chart_and_settings() {
        let mut state = AppState::new();
        state.settings.input.input_offset_ms = 22.0;
        state.select_chart(ChartSelection::new("song.sm"));

        state.request_play().unwrap();
        let request = state.take_pending_launch().unwrap();

        assert_eq!(state.screen, AppScreen::Gameplay);
        assert_eq!(request.chart.chart_path.to_string_lossy(), "song.sm");
        assert_eq!(request.settings.input.input_offset_ms, 22.0);
        assert!(state.take_pending_launch().is_none());
    }

    #[test]
    fn play_requires_selected_chart() {
        let mut state = AppState::new();

        assert_eq!(state.request_play(), Err(AppStateError::NoChartSelected));
    }

    #[test]
    fn settings_screen_tracks_active_panel() {
        let mut state = AppState::new();
        state.open_settings(SettingsPanel::Audio);

        assert_eq!(state.screen, AppScreen::Settings(SettingsPanel::Audio));
    }
}
