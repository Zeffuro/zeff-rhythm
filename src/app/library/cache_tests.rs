use super::{
    AppLibrary, cache,
    tests::{Fixture, osu, sm},
};
use std::fs;
use std::path::PathBuf;

fn scan(fixture: &Fixture) -> AppLibrary {
    AppLibrary::scan_roots_with_cache(&[fixture.0.clone()], &fixture.0.join("cache"))
}
fn snapshot(fixture: &Fixture) -> Option<AppLibrary> {
    AppLibrary::cached_snapshot_with_cache(&[fixture.0.clone()], &fixture.0.join("cache"))
}

#[test]
fn warm_scan_reuses_complete_records_and_original_sm_indices() {
    let fixture = Fixture::new();
    fixture.write(
        "chart.sm",
        &sm().replace(
            "#NOTES:dance-single:Author:Easy",
            "#NOTES:pump-single:Author:Easy",
        ),
    );
    fixture.write("mania.osu", &osu("Mania", 3, 4));
    fixture.write("standard.osu", &osu("Standard", 0, 4));
    fixture.write("bad.osu", "[General]\nMode:broken\n");
    fixture.write("song.wav", "fixture");
    let cold = scan(&fixture);
    assert_eq!(cold.scan_metrics.read, 4);
    assert_eq!(cold.scan_metrics.reused, 0);
    let warm = scan(&fixture);
    assert_eq!(warm, cold);
    assert_eq!(warm.scan_metrics.read, 0);
    assert_eq!(warm.scan_metrics.reused, 4);
    assert!(
        warm.entries()
            .iter()
            .any(|entry| entry.chart_index == 1 && entry.problem.is_none())
    );
    let cached = snapshot(&fixture).unwrap();
    assert!(cached.is_cached_snapshot);
    assert!(!warm.is_cached_snapshot);
    assert_eq!(cached, warm);
}

#[test]
fn changed_added_and_deleted_charts_update_incrementally() {
    let fixture = Fixture::new();
    fixture.write("a.osu", &osu("Before", 3, 4));
    fixture.write("b.osu", &osu("Deleted", 3, 4));
    fixture.write("song.wav", "fixture");
    scan(&fixture);
    fixture.write("a.osu", &osu("After with different size", 3, 4));
    fixture.write("c.osu", &osu("Added", 3, 4));
    fs::remove_file(fixture.0.join("b.osu")).unwrap();
    let changed = scan(&fixture);
    assert_eq!(changed.scan_metrics.read, 2);
    assert_eq!(changed.files_scanned, 2);
    assert_eq!(
        changed
            .entries()
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Added", "After with different size"]
    );
    assert_eq!(scan(&fixture).scan_metrics.reused, 2);
    assert_eq!(snapshot(&fixture).unwrap(), changed);
}

#[test]
fn cached_audio_availability_repairs_in_both_directions() {
    let fixture = Fixture::new();
    fixture.write("chart.osu", &osu("Mania", 3, 4));
    fixture.write("wide.osu", &osu("Wide", 3, 7));
    assert_eq!(scan(&fixture).problem_count(), 2);
    fixture.write("song.wav", "fixture");
    let repaired = scan(&fixture);
    assert_eq!(repaired.scan_metrics.reused, 2);
    assert_eq!(repaired.playable_count(), 1);
    assert_eq!(repaired.problem_count(), 1);
    assert!(
        repaired
            .entries()
            .iter()
            .any(|entry| entry.status().contains("7 lanes"))
    );
    fs::remove_file(fixture.0.join("song.wav")).unwrap();
    let missing = scan(&fixture);
    assert_eq!(missing.scan_metrics.read, 0);
    assert_eq!(missing.problem_count(), 2);
    fixture.write("song.wav", "fixture");
    assert_eq!(scan(&fixture).playable_count(), 1);
}

#[test]
fn corrupt_and_incompatible_cache_fall_back_to_chart_scan() {
    let fixture = Fixture::new();
    fixture.write("chart.osu", &osu("Mania", 3, 4));
    let expected = scan(&fixture);
    let roots = cache::root_identity(&[fixture.0.clone()]);
    let path = cache::path(&fixture.0.join("cache"), &roots);
    fs::write(&path, "corrupt").unwrap();
    assert!(snapshot(&fixture).is_none());
    let repaired = scan(&fixture);
    assert_eq!(repaired, expected);
    assert_eq!(repaired.scan_metrics.read, 1);
    let source = fs::read_to_string(&path).unwrap();
    fs::write(&path, source.replacen("schema = 3", "schema = 999", 1)).unwrap();
    assert!(snapshot(&fixture).is_none());
    assert_eq!(scan(&fixture).scan_metrics.read, 1);
}

#[test]
fn old_metadata_schema_refreshes_once_and_preserves_new_fields() {
    let fixture = Fixture::new();
    let source = osu("Tsuki", 3, 4).replace("Title:Tsuki", "Title:Tsuki\nTitleUnicode:月");
    fixture.write("chart.osu", &format!("{source}[Events]\n0,0,\"月.jpg\"\n"));
    scan(&fixture);
    let roots = cache::root_identity(&[fixture.0.clone()]);
    let path = cache::path(&fixture.0.join("cache"), &roots);
    let source = fs::read_to_string(&path).unwrap();
    let mut old: toml::Value = toml::from_str(&source).unwrap();
    old["schema"] = toml::Value::Integer(1);
    for file in old["files"].as_array_mut().unwrap() {
        for entry in file["entries"].as_array_mut().unwrap() {
            let entry = entry.as_table_mut().unwrap();
            entry.remove("search_aliases");
            entry.remove("background_path");
            entry.remove("banner_path");
        }
    }
    fs::write(&path, toml::to_string(&old).unwrap()).unwrap();
    assert!(snapshot(&fixture).is_none());
    let refreshed = scan(&fixture);
    assert_eq!(refreshed.scan_metrics.read, 1);
    assert_eq!(refreshed.entries()[0].title, "月");
    assert!(
        refreshed.entries()[0]
            .search_aliases
            .iter()
            .any(|alias| alias == "Tsuki")
    );
    assert!(
        refreshed.entries()[0]
            .background_path
            .as_ref()
            .unwrap()
            .ends_with("月.jpg")
    );
    assert_eq!(scan(&fixture).scan_metrics.read, 0);
    assert_eq!(snapshot(&fixture).unwrap(), refreshed);
}

#[test]
fn preview_metadata_survives_cache_and_schema_refresh() {
    let fixture = Fixture::new();
    fixture.write(
        "chart.osu",
        &osu("Mania", 3, 4).replace("Mode:3", "Mode:3\nPreviewTime:12345"),
    );
    fixture.write(
        "chart.sm",
        &format!("#SAMPLESTART:45.5;#SAMPLELENGTH:12.25;{}", sm()),
    );
    let cold = scan(&fixture);
    assert_eq!(cold.entries().len(), 3);
    for entry in cold.entries() {
        let expected = if entry.chart_path.extension().unwrap() == "osu" {
            (Some(12.345), None)
        } else {
            (Some(45.5), Some(12.25))
        };
        assert_eq!(
            (entry.preview_start_seconds, entry.preview_duration_seconds),
            expected
        );
    }
    assert_eq!(snapshot(&fixture).unwrap(), cold);
    let warm = scan(&fixture);
    assert_eq!(warm.scan_metrics.read, 0);
    assert_eq!(warm, cold);
    let roots = cache::root_identity(&[fixture.0.clone()]);
    let path = cache::path(&fixture.0.join("cache"), &roots);
    let mut old: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    old["schema"] = toml::Value::Integer(2);
    for file in old["files"].as_array_mut().unwrap() {
        for entry in file["entries"].as_array_mut().unwrap() {
            let entry = entry.as_table_mut().unwrap();
            entry.remove("preview_start_seconds");
            entry.remove("preview_duration_seconds");
        }
    }
    let source = toml::to_string(&old).unwrap();
    let defaults: cache::Cache = toml::from_str(&source).unwrap();
    assert!(
        defaults
            .files
            .iter()
            .flat_map(|file| &file.entries)
            .all(|entry| entry.preview_start_seconds.is_none()
                && entry.preview_duration_seconds.is_none())
    );
    fs::write(&path, source).unwrap();
    assert!(snapshot(&fixture).is_none());
    let refreshed = scan(&fixture);
    assert_eq!(refreshed.scan_metrics.read, 2);
    assert_eq!(refreshed, cold);
    assert_eq!(scan(&fixture).scan_metrics.read, 0);
}

#[test]
fn cached_optional_art_can_appear_and_disappear_without_chart_reparse() {
    let fixture = Fixture::new();
    fixture.write(
        "chart.osu",
        &format!("{}[Events]\n0,0,\"art.jpg\"\n", osu("Mania", 3, 4)),
    );
    fixture.write("song.wav", "fixture");
    let first = scan(&fixture);
    let entry = &first.entries()[0];
    let candidate = entry.background_path.as_ref().unwrap();
    assert!(super::resolve_artwork_path(&entry.chart_path, candidate).is_none());
    fixture.write("art.jpg", "decorative bytes");
    let added = scan(&fixture);
    assert_eq!(added.scan_metrics.read, 0);
    assert_eq!(added.playable_count(), 1);
    assert!(super::resolve_artwork_path(&entry.chart_path, candidate).is_some());
    assert_eq!(added, first);
    fs::remove_file(fixture.0.join("art.jpg")).unwrap();
    let removed = snapshot(&fixture).unwrap();
    assert_eq!(removed.playable_count(), 1);
    assert!(super::resolve_artwork_path(&entry.chart_path, candidate).is_none());
}

#[test]
fn root_sets_order_and_overlap_are_isolated() {
    let first = Fixture::new();
    let second = Fixture::new();
    first.write("nested/chart.osu", &osu("First", 3, 4));
    second.write("chart.osu", &osu("Second", 3, 4));
    let cache_dir = first.0.join("cache");
    let both = vec![first.0.clone(), second.0.clone(), first.0.join("nested")];
    let cold = AppLibrary::scan_roots_with_cache(&both, &cache_dir);
    assert_eq!(cold.files_scanned, 2);
    let reversed = both.iter().rev().cloned().collect::<Vec<_>>();
    assert!(AppLibrary::cached_snapshot_with_cache(&reversed, &cache_dir).is_none());
    assert!(AppLibrary::cached_snapshot_with_cache(&[second.0.clone()], &cache_dir).is_none());
    AppLibrary::scan_roots_with_cache(&[second.0.clone()], &cache_dir);
    assert_eq!(
        AppLibrary::cached_snapshot_with_cache(&both, &cache_dir).unwrap(),
        cold
    );
    let relative = vec![PathBuf::from(".local_assets")];
    assert!(cache::root_identity(&relative)[0].is_absolute());
}

#[test]
fn non_mania_header_does_not_read_invalid_large_body() {
    let fixture = Fixture::new();
    let mut source = b"[General]\nMode:0\n[HitObjects]\n".to_vec();
    source.extend(vec![0xff; 1024 * 1024]);
    fs::write(fixture.0.join("standard.osu"), source).unwrap();
    let library = scan(&fixture);
    assert_eq!(library.skipped_modes, 1);
    assert_eq!(library.problem_count(), 0);
    assert_eq!(scan(&fixture), library);
}
#[test]
fn same_size_edit_invalidates_on_modified_time() {
    let fixture = Fixture::new();
    fixture.write("chart.osu", &osu("Before", 3, 4));
    let path = fixture.0.join("chart.osu");
    let old_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    scan(&fixture);
    fixture.write("chart.osu", &osu("Edited", 3, 4));
    let edited = scan(&fixture);
    assert_eq!(edited.scan_metrics.read, 1);
    assert_eq!(edited.entries()[0].title, "Edited");
    assert_eq!(scan(&fixture).scan_metrics.reused, 1);
}

#[test]
fn cache_write_failure_keeps_complete_scan_result() {
    let fixture = Fixture::new();
    fixture.write("chart.osu", &osu("Mania", 3, 4));
    fixture.write("song.wav", "fixture");
    fixture.write("blocked-cache", "file occupying cache directory");
    let library =
        AppLibrary::scan_roots_with_cache(&[fixture.0.clone()], &fixture.0.join("blocked-cache"));
    assert_eq!(library.playable_count(), 1);
    assert_eq!(library.scan_metrics.read, 1);
    assert!(!library.is_cached_snapshot);
}
#[test]
fn unavailable_root_preserves_previous_complete_disk_snapshot() {
    let fixture = Fixture::new();
    fixture.write("songs/chart.osu", &osu("Retained", 3, 4));
    fixture.write("songs/song.wav", "fixture");
    let roots = vec![fixture.0.join("songs")];
    let cache_dir = fixture.0.join("cache");
    let complete = AppLibrary::scan_roots_with_cache(&roots, &cache_dir);
    let identity = cache::root_identity(&roots);
    let cache_path = cache::path(&cache_dir, &identity);
    let before = fs::read(&cache_path).unwrap();
    fs::rename(&roots[0], fixture.0.join("temporarily-unavailable")).unwrap();
    assert_eq!(cache::root_identity(&roots), identity);
    let incomplete = AppLibrary::scan_roots_with_cache(&roots, &cache_dir);
    assert_eq!(incomplete.problem_count(), 1);
    assert_eq!(incomplete.files_scanned, 0);
    assert_eq!(fs::read(&cache_path).unwrap(), before);
    assert_eq!(
        AppLibrary::cached_snapshot_with_cache(&roots, &cache_dir)
            .unwrap()
            .entries()
            .len(),
        complete.entries().len()
    );
    fs::rename(fixture.0.join("temporarily-unavailable"), &roots[0]).unwrap();
    let restored = AppLibrary::scan_roots_with_cache(&roots, &cache_dir);
    assert_eq!(restored, complete);
    assert_eq!(restored.scan_metrics.reused, 1);
}

#[test]
fn directory_read_failure_marks_traversal_incomplete() {
    let fixture = Fixture::new();
    let mut library = AppLibrary::default();
    let mut files = Vec::new();
    let mut visited = std::collections::HashSet::new();
    assert!(!super::scan::collect_files(
        &fixture.0.join("removed-subtree"),
        &mut visited,
        &mut files,
        &mut library
    ));
    assert_eq!(library.problem_count(), 1);
    assert!(files.is_empty());
    library.records.clear();
    assert!(super::scan::collect_files(
        &fixture.0,
        &mut visited,
        &mut files,
        &mut library
    ));
    assert_eq!(library.problem_count(), 0);
}

#[cfg(windows)]
#[test]
fn missing_verbatim_unc_and_plain_unc_share_cache_identity() {
    let plain = PathBuf::from(r"\\zeff-invalid-test-host\missing-share\songs");
    let verbatim = PathBuf::from(r"\\?\UNC\zeff-invalid-test-host\missing-share\songs");
    assert_eq!(
        cache::normalize_identity(plain),
        cache::normalize_identity(verbatim)
    );
}
