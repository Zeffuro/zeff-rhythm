use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("zeff-ui-{nonce}"));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("song.wav"), b"audio fixture").unwrap();
        std::fs::write(root.join("chart.sm"), "#TITLE:Searchable song;#ARTIST:Someone;#MUSIC:song.wav;#BPMS:0=120;#NOTES:dance-single:Author:Easy:1:0:\n1000\n;#NOTES:dance-single:Author:Hard:9:0:\n1000\n0100\n;").unwrap();
        std::fs::write(root.join("bad.osu"), "[General]\nMode:3\n").unwrap();
        Self(root)
    }
    fn shell(&self) -> WgpuAppShell {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.window_focused = true;
        shell.library = AppLibrary::scan_roots_with_cache(&[self.0.clone()], &self.0.join("cache"));
        shell.ensure_visible_selection();
        shell
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn search_filter_and_navigation_keep_exact_difficulty() {
    let fixture = Fixture::new();
    let mut shell = fixture.shell();
    assert_eq!(shell.library_indices().len(), 2);
    shell.search_active = true;
    assert!(shell.search_input(KeyCode::KeyQ, Some("someone hard"), true));
    assert_eq!(shell.library_indices().len(), 1);
    let entry = shell.library.get(shell.library_index).unwrap();
    assert_eq!(entry.chart_index, 1);
    shell.move_library_selection(5);
    assert_eq!(
        shell.library.get(shell.library_index).unwrap().chart_index,
        1
    );
    shell.search_query = "no matches".into();
    shell.ensure_visible_selection();
    assert!(shell.library.get(shell.library_index).is_none());
    shell.go_back();
    assert_eq!(shell.library_indices().len(), 2);
    shell.ready_only = false;
    assert_eq!(shell.library_indices().len(), 3);
}

#[test]
fn queued_launch_deactivates_search_and_cancel_cannot_start_a_stream() {
    let fixture = Fixture::new();
    let mut shell = fixture.shell();
    shell.search_active = true;
    shell.prepare_selected_library_entry().unwrap();
    assert!(!shell.search_active);
    assert!(shell.asset_load.is_some());
    shell.go_back();
    shell.poll_chart_load();
    assert!(shell.asset_load.is_none());
    assert!(shell.live_session.is_none());
    assert_eq!(shell.state.screen, AppScreen::SongSelect);
    shell.search_active = true;
    shell.state.screen = AppScreen::Gameplay;
    assert!(!shell.search_input(KeyCode::KeyD, Some("d"), true));
}

#[test]
fn every_settings_selection_renders_inside_the_viewport() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    for (width, height) in [(760, 520), (960, 640), (1280, 720)] {
        shell.options.width = width;
        shell.options.height = height;
        for panel in [
            SettingsPanel::Audio,
            SettingsPanel::Input,
            SettingsPanel::Video,
            SettingsPanel::Gameplay,
            SettingsPanel::Diagnostics,
        ] {
            shell.state.open_settings(panel);
            for selected in 0..=settings_row_count(panel) {
                shell.settings_row_index = selected;
                let rects = shell.build_ui_rects(width as f32, height as f32);
                assert!(rects.iter().all(|rect| rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= width as f32
                    && rect.y + rect.height <= height as f32));
            }
        }
    }
}
#[test]
fn help_tracks_lane_releases_but_consumes_resume_keys() {
    let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
    shell.state.screen = AppScreen::Gameplay;
    shell.help_visible = true;
    shell.active_lanes = vec![true, false, false, false];
    assert!(
        shell
            .process_help_input(KeyCode::KeyD, false, false)
            .unwrap()
    );
    assert!(!shell.active_lanes[0]);
    assert!(
        shell
            .process_help_input(KeyCode::Enter, true, false)
            .unwrap()
    );
    assert!(shell.help_visible);
    assert!(
        shell
            .process_help_input(KeyCode::Escape, true, false)
            .unwrap()
    );
    assert!(!shell.help_visible);
}
