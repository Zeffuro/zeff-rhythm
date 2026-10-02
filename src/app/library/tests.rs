use super::{AppLibrary, LibraryScanner, artwork_candidate, resolve_artwork_path, visible_range};
use crate::app::settings::AppSettings;
use crate::app::state::{AppState, ChartSelection};
use crate::play::{PlaySessionOptions, load_play_session_preview};
use std::fs;
use std::path::PathBuf;

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("zeff-library-{unique}"));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    pub(super) fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn scan(&self) -> AppLibrary {
        AppLibrary::scan_roots_with_cache(&[self.0.clone()], &self.0.join("cache"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn osu(title: &str, mode: u8, lanes: u8) -> String {
    format!(
        "[General]\nMode:{mode}\nAudioFilename:song.wav\n[Metadata]\nTitle:{title}\nArtist:Artist\nVersion:Hard\n[Difficulty]\nCircleSize:{lanes}\n[TimingPoints]\n0,500\n[HitObjects]\n64,192,1000,1,0\n"
    )
}
pub(super) fn sm() -> &'static str {
    "#TITLE:Step song;\n#ARTIST:Artist;\n#MUSIC:song.wav;\n#BPMS:0=120;\n#NOTES:dance-single:Author:Easy:1:0,0,0,0,0:\n1000\n;\n#NOTES:dance-single:Author:Hard:9:0,0,0,0,0:\n1000\n0100\n;\n"
}

#[test]
fn mixed_library_selects_exact_difficulty_without_audio_or_window() {
    let fixture = Fixture::new();
    fixture.write("nested/chart.SM", sm());
    fixture.write("nested/song.wav", "fixture");
    fixture.write("mania/chart.osu", &osu("Mania song", 3, 4));
    fixture.write("mania/song.wav", "fixture");
    fixture.write("standard/chart.osu", &osu("Standard song", 0, 4));
    let library = fixture.scan();
    assert_eq!(library.files_scanned, 3);
    assert_eq!(library.skipped_modes, 1);
    assert_eq!(library.playable_count(), 3);
    let hard = library
        .entries()
        .iter()
        .find(|entry| entry.chart_index == 1)
        .unwrap();
    assert!(hard.subtitle.contains("Hard"));
    let mut state = AppState::new();
    state.settings.diagnostics.event_log_enabled = false;
    state.select_chart(hard.chart_selection());
    state.request_play().unwrap();
    let options =
        PlaySessionOptions::from_app_launch(&state.take_pending_launch().unwrap()).unwrap();
    let preview = load_play_session_preview(&options).unwrap();
    assert_eq!(preview.note_count, 2);
    assert!(
        preview
            .chart
            .metadata()
            .difficulty
            .as_ref()
            .unwrap()
            .contains("Hard")
    );
    assert_eq!(options.chart_index, 1);
    assert_eq!(fixture.scan(), library);
}

#[test]
fn malformed_missing_audio_and_unsupported_lanes_are_visible() {
    let fixture = Fixture::new();
    fixture.write("bad.osu", "[General]\nMode:3\n");
    fixture.write("missing.osu", &osu("No audio", 3, 4));
    fixture.write("wide.osu", &osu("Seven lanes", 3, 7));
    let library = fixture.scan();
    assert_eq!(library.entries().len(), 3);
    assert_eq!(library.problem_count(), 3);
    assert_eq!(library.playable_count(), 0);
    assert!(library.entries().iter().all(|entry| !entry.is_available()));
    assert!(
        library
            .entries()
            .iter()
            .any(|entry| entry.status().contains("7 lanes"))
    );
    assert!(
        library
            .entries()
            .iter()
            .any(|entry| entry.status().contains("missing"))
    );
}

#[test]
fn overlapping_roots_and_rescan_preserve_chart_identity() {
    let fixture = Fixture::new();
    fixture.write("pack/chart.sm", sm());
    fixture.write("pack/song.wav", "fixture");
    let library = AppLibrary::scan_roots(&[fixture.0.clone(), fixture.0.join("pack")]);
    assert_eq!(library.files_scanned, 1);
    let selected = library
        .entries()
        .iter()
        .find(|entry| entry.chart_index == 1)
        .unwrap()
        .chart_selection();
    fixture.write("aaa.osu", &osu("AAA", 3, 4));
    fixture.write("song.wav", "fixture");
    let rescanned = fixture.scan();
    assert_eq!(
        rescanned
            .get(rescanned.selection_index(Some(&selected)))
            .unwrap()
            .chart_selection(),
        selected
    );
    fs::remove_file(fixture.0.join("pack/song.wav")).unwrap();
    assert!(!library.get(0).unwrap().is_available());
}

#[test]
fn empty_and_missing_roots_have_distinct_results() {
    let fixture = Fixture::new();
    assert!(fixture.scan().entries().is_empty());
    assert_eq!(fixture.scan().problem_count(), 0);
    let missing = AppLibrary::scan_roots(&[fixture.0.join("missing")]);
    assert!(missing.entries().is_empty());
    assert_eq!(missing.problem_count(), 1);
    let missing_selection = ChartSelection::new("missing.sm");
    assert_eq!(missing.selection_index(Some(&missing_selection)), 0);
}

#[test]
fn every_selection_remains_inside_its_visible_window() {
    for count in 0..30 {
        for rows in 1..8 {
            for selected in 0..count {
                let range = visible_range(selected, count, rows);
                assert!(range.contains(&selected));
                assert!(range.end <= count);
                assert!(range.len() <= rows);
            }
        }
    }
    assert!(visible_range(0, 0, 4).is_empty());
}

#[test]
fn old_settings_use_local_library_root() {
    let mut value = toml::Value::try_from(AppSettings::default()).unwrap();
    value.as_table_mut().unwrap().remove("library_roots");
    let settings: AppSettings = value.try_into().unwrap();
    assert_eq!(settings.library_roots, vec![PathBuf::from(".local_assets")]);
}

#[test]
fn background_scan_returns_latest_requested_roots() {
    let first = Fixture::new();
    first.write("chart.osu", &osu("First", 3, 4));
    first.write("song.wav", "fixture");
    let second = Fixture::new();
    second.write("chart.osu", &osu("Second", 3, 4));
    second.write("song.wav", "fixture");
    let mut scanner = LibraryScanner::default();
    scanner.request(vec![first.0.clone()]);
    scanner.request(vec![second.0.clone()]);
    assert!(scanner.is_scanning());
    let library = scanner.wait();
    assert_eq!(library.entries()[0].title, "Second");
    assert!(!scanner.is_scanning());
}

#[test]
fn same_root_rescan_discards_in_flight_snapshot() {
    let fixture = Fixture::new();
    fixture.write("chart.osu", &osu("Before edit", 3, 4));
    fixture.write("song.wav", "fixture");
    let stale = fixture.scan();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut scanner = LibraryScanner {
        receiver: Some(receiver),
        roots: vec![fixture.0.clone()],
        generation: 1,
        active_generation: 1,
    };
    fixture.write("chart.osu", &osu("After edit", 3, 4));
    scanner.request(vec![fixture.0.clone()]);
    sender.send(stale).unwrap();
    assert!(scanner.poll().is_none());
    assert_eq!(scanner.wait().entries()[0].title, "After edit");
    assert!(!scanner.is_scanning());
}

#[test]
fn native_metadata_keeps_romanized_aliases_and_local_artwork() {
    let fixture = Fixture::new();
    let source = osu("Tsuki", 3, 4).replace(
        "Artist:Artist",
        "TitleUnicode:月\nArtist:Hoshi\nArtistUnicode:星",
    );
    fixture.write(
        "song/chart.osu",
        &format!("{source}[Events]\n0,0,\"art\\月,光.jpg\",0,0\n"),
    );
    fixture.write("song/song.wav", "fixture");
    fixture.write("song/art/月,光.jpg", "corrupt image is optional");
    let library = fixture.scan();
    let entry = &library.entries()[0];
    assert_eq!(entry.title, "月");
    assert!(entry.subtitle.starts_with("星 / Hard"));
    assert_eq!(entry.search_aliases, ["Tsuki", "Hoshi"]);
    assert!(entry.is_available());
    assert_eq!(library.problem_count(), 0);
    assert_eq!(
        resolve_artwork_path(&entry.chart_path, entry.background_path.as_ref().unwrap()),
        Some(fixture.0.join("song/art/月,光.jpg").canonicalize().unwrap())
    );
    assert!(entry.banner_path.is_none());
}

#[test]
fn sm_artwork_and_titles_are_retained_for_each_difficulty() {
    let fixture = Fixture::new();
    fixture.write(
        "song/chart.sm",
        &format!("#BACKGROUND:bg.jpg;#BANNER:帯.png;\n{}", sm()),
    );
    fixture.write("song/song.wav", "fixture");
    fixture.write("song/帯.png", "fixture");
    let library = fixture.scan();
    assert_eq!(library.entries().len(), 2);
    assert_eq!(library.playable_count(), 2);
    for entry in library.entries() {
        assert_eq!(entry.title, "Step song");
        assert!(entry.background_path.as_ref().unwrap().ends_with("bg.jpg"));
        assert!(
            resolve_artwork_path(&entry.chart_path, entry.background_path.as_ref().unwrap())
                .is_none()
        );
        assert!(
            resolve_artwork_path(&entry.chart_path, entry.banner_path.as_ref().unwrap()).is_some()
        );
        assert!(entry.is_available());
    }
    assert_ne!(
        library.entries()[0].difficulty,
        library.entries()[1].difficulty
    );
}

#[test]
fn decorative_paths_reject_absolute_traversal_and_remote_references() {
    let fixture = Fixture::new();
    fixture.write("song/chart.sm", sm());
    let chart_path = fixture.0.join("song/chart.sm");
    for filename in [
        "",
        " ",
        "../outside.jpg",
        r"..\outside.jpg",
        "art/../outside.jpg",
        "/outside.jpg",
        r"\outside.jpg",
        r"C:\outside.jpg",
        "C:outside.jpg",
        r"\\server\share\outside.jpg",
        "https://example.com/art.jpg",
        "file:///art.jpg",
        "image.jpg:stream",
        "image\0.jpg",
    ] {
        assert!(
            artwork_candidate(&chart_path, Some(filename)).is_none(),
            "{filename}"
        );
    }
    assert!(artwork_candidate(&chart_path, None).is_none());
    fixture.write("outside.jpg", "outside");
    assert!(resolve_artwork_path(&chart_path, &fixture.0.join("outside.jpg")).is_none());
    let source =
        sm().to_owned() + "#BACKGROUND:../outside.jpg;#BANNER:https://example.com/art.jpg;";
    fixture.write("song/chart.sm", &source);
    fixture.write("song/song.wav", "fixture");
    let library = fixture.scan();
    assert_eq!(library.playable_count(), 2);
    assert!(
        library
            .entries()
            .iter()
            .all(|entry| entry.background_path.is_none() && entry.banner_path.is_none())
    );
}

#[cfg(unix)]
#[test]
fn decorative_symlinks_cannot_leave_the_chart_directory() {
    let fixture = Fixture::new();
    fixture.write("song/chart.sm", sm());
    fixture.write("outside.jpg", "outside");
    std::os::unix::fs::symlink(
        fixture.0.join("outside.jpg"),
        fixture.0.join("song/link.jpg"),
    )
    .unwrap();
    let chart_path = fixture.0.join("song/chart.sm");
    let candidate = artwork_candidate(&chart_path, Some("link.jpg")).unwrap();
    assert!(resolve_artwork_path(&chart_path, &candidate).is_none());
}
