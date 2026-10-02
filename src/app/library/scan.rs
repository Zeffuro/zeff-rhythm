use super::{AppLibrary, LibraryEntry, ScanRecord, artwork_candidate, cache};
use crate::play::ChartFormat;
use rhythm_core::{Chart, parse_osu_mania, parse_stepmania_sm_catalog};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(super) fn scan_roots(roots: &[PathBuf], cache_directory: &Path) -> AppLibrary {
    let start = Instant::now();
    let identities = cache::root_identity(roots);
    let load_start = Instant::now();
    let previous = cache::load(cache_directory, &identities);
    let loaded = previous.is_some();
    let old_walk_records = previous
        .as_ref()
        .map(|cache| cache.walk_records.clone())
        .unwrap_or_default();
    let mut old_files: HashMap<_, _> = previous
        .into_iter()
        .flat_map(|cache| cache.files)
        .map(|file| (file.path.clone(), file))
        .collect();
    let cache_load_micros = load_start.elapsed().as_micros() as u64;
    let discovery_start = Instant::now();
    let mut library = AppLibrary {
        roots: roots.to_vec(),
        ..AppLibrary::default()
    };
    let mut visited = HashSet::new();
    let mut files = Vec::new();
    let mut complete = true;
    for root in roots {
        match fs::canonicalize(root) {
            Ok(root) => complete &= collect_files(&root, &mut visited, &mut files, &mut library),
            Err(error) => {
                complete = false;
                record(&mut library, root, None, "error", &error.to_string());
            }
        }
    }
    files.sort();
    library.scan_metrics.cache_load_micros = cache_load_micros;
    library.scan_metrics.discovery_micros = discovery_start.elapsed().as_micros() as u64;
    library.scan_metrics.discovered = files.len();
    let walk_records = library.records.clone();
    let mut changed = !loaded || walk_records != old_walk_records;
    let parse_start = Instant::now();
    let mut cached_files = Vec::with_capacity(files.len());
    for path in files {
        let signature = cache::signature(&path);
        let previous = old_files.remove(&path);
        let file = match previous {
            Some(previous) if signature.as_ref() == Some(&previous.signature) => {
                library.scan_metrics.reused += 1;
                Some(previous)
            }
            _ => {
                changed = true;
                library.scan_metrics.read += 1;
                let mut parsed = AppLibrary::default();
                let cacheable = scan_file(&path, &mut parsed);
                if let Some(signature) = signature.filter(|signature| {
                    cacheable && Some(signature) == cache::signature(&path).as_ref()
                }) {
                    for entry in &mut parsed.entries {
                        if entry.problem.as_deref() == Some("Audio file is missing") {
                            entry.problem = None;
                        }
                    }
                    Some(cache::CachedFile {
                        path: path.clone(),
                        signature,
                        entries: parsed.entries,
                        records: parsed.records,
                        skipped_modes: parsed.skipped_modes,
                    })
                } else {
                    complete = false;
                    library.files_scanned += parsed.files_scanned;
                    library.skipped_modes += parsed.skipped_modes;
                    library.entries.extend(parsed.entries);
                    library.records.extend(parsed.records);
                    None
                }
            }
        };
        if let Some(file) = file {
            cache::append(&file, &mut library);
            cached_files.push(file);
        }
    }
    changed |= !old_files.is_empty();
    library.scan_metrics.parse_micros = parse_start.elapsed().as_micros() as u64;
    sort(&mut library);
    if changed && complete {
        let save_start = Instant::now();
        let cache = cache::Cache {
            schema: 0,
            roots: identities,
            files: cached_files,
            walk_records,
        };
        let _ = cache::save(cache_directory, cache);
        library.scan_metrics.cache_save_micros = save_start.elapsed().as_micros() as u64;
    }
    library.scan_metrics.total_micros = start.elapsed().as_micros() as u64;
    library
}

pub(super) fn sort(library: &mut AppLibrary) {
    library.entries.sort_by_cached_key(|entry| {
        (
            entry.problem.is_some(),
            entry.title.to_lowercase(),
            entry.subtitle.to_lowercase(),
            entry.chart_path.clone(),
            entry.chart_index,
        )
    });
    library.records.sort_by(|left, right| {
        (&left.path, left.chart_index).cmp(&(&right.path, right.chart_index))
    });
}

pub(super) fn collect_files(
    root: &Path,
    visited: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
    library: &mut AppLibrary,
) -> bool {
    let mut complete = true;
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if path.is_file() {
            if ChartFormat::detect(&path).is_ok() {
                files.push(path);
            }
            continue;
        }
        let children = match fs::read_dir(&path) {
            Ok(children) => children,
            Err(error) => {
                complete = false;
                record(library, &path, None, "error", &error.to_string());
                continue;
            }
        };
        for child in children {
            match child {
                Ok(child) => match child.file_type() {
                    Ok(kind) if kind.is_symlink() => record(
                        library,
                        &child.path(),
                        None,
                        "skipped_link",
                        "Nested symbolic links are not followed; add the target as a library root",
                    ),
                    Ok(kind) if kind.is_dir() => pending.push(child.path()),
                    Ok(kind) if kind.is_file() && ChartFormat::detect(child.path()).is_ok() => {
                        if visited.insert(child.path()) {
                            files.push(child.path());
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        complete = false;
                        record(library, &child.path(), None, "error", &error.to_string());
                    }
                },
                Err(error) => {
                    complete = false;
                    record(library, &path, None, "error", &error.to_string());
                }
            }
        }
    }
    complete
}

fn scan_file(path: &Path, library: &mut AppLibrary) -> bool {
    library.files_scanned += 1;
    let format = ChartFormat::detect(path).expect("collected supported extension");
    if format == ChartFormat::OsuMania {
        match osu_mode(path) {
            Ok(3) => {}
            Ok(mode) => {
                library.skipped_modes += 1;
                record(
                    library,
                    path,
                    None,
                    "skipped",
                    &format!("osu! mode {mode}; only mania is supported"),
                );
                return true;
            }
            Err(error) => {
                let cacheable = error.kind() == std::io::ErrorKind::InvalidData;
                invalid_entry(library, path, format, 0, error.to_string());
                return cacheable;
            }
        }
    }
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            let cacheable = error.kind() == std::io::ErrorKind::InvalidData;
            invalid_entry(library, path, format, 0, error.to_string());
            return cacheable;
        }
    };
    let charts = match format {
        ChartFormat::OsuMania => Ok(vec![parse_osu_mania(&source)]),
        ChartFormat::StepMania => parse_stepmania_sm_catalog(&source),
    };
    match charts {
        Ok(charts) => {
            for (index, chart) in charts.into_iter().enumerate() {
                match chart {
                    Ok(chart) => chart_entry(library, path, format, index, chart),
                    Err(error) => invalid_entry(library, path, format, index, error.to_string()),
                }
            }
        }
        Err(error) => invalid_entry(library, path, format, 0, error.to_string()),
    }
    true
}

fn chart_entry(
    library: &mut AppLibrary,
    path: &Path,
    format: ChartFormat,
    index: usize,
    chart: Chart,
) {
    let metadata = chart.metadata();
    let audio_path = metadata
        .audio_filename
        .as_ref()
        .filter(|name| !name.trim().is_empty())
        .map(|name| path.parent().unwrap_or_else(|| Path::new(".")).join(name));
    let problem = if chart.lane_count() > 4 {
        Some(format!(
            "{} lanes; this app currently supports up to 4",
            chart.lane_count()
        ))
    } else if chart.notes().is_empty() {
        Some("Chart has no notes".to_owned())
    } else if audio_path.is_none() {
        Some("No audio file declared".to_owned())
    } else if !audio_path.as_ref().is_some_and(|path| path.is_file()) {
        Some("Audio file is missing".to_owned())
    } else {
        None
    };
    let title = if metadata.display_title().trim().is_empty() {
        fallback_title(path)
    } else {
        metadata.display_title().to_owned()
    };
    let subtitle = format!(
        "{} / {} / {}K / {} notes",
        metadata.display_artist(),
        metadata
            .difficulty
            .as_deref()
            .unwrap_or("Unnamed difficulty"),
        chart.lane_count(),
        chart.notes().len()
    );
    record(
        library,
        path,
        Some(index),
        if problem.is_some() { "error" } else { "ready" },
        problem.as_deref().unwrap_or(&subtitle),
    );
    library.entries.push(LibraryEntry {
        title,
        subtitle,
        search_aliases: [&metadata.title, &metadata.artist]
            .into_iter()
            .filter(|alias| !alias.trim().is_empty())
            .cloned()
            .collect(),
        difficulty: metadata.difficulty.clone(),
        chart_path: path.to_owned(),
        chart_index: index,
        audio_path,
        background_path: artwork_candidate(path, metadata.background_filename.as_deref()),
        banner_path: artwork_candidate(path, metadata.banner_filename.as_deref()),
        preview_start_seconds: metadata.preview_start_seconds,
        preview_duration_seconds: metadata.preview_duration_seconds,
        format,
        lane_count: chart.lane_count(),
        note_count: chart.notes().len(),
        problem,
    });
}

fn invalid_entry(
    library: &mut AppLibrary,
    path: &Path,
    format: ChartFormat,
    index: usize,
    error: String,
) {
    record(library, path, Some(index), "error", &error);
    library.entries.push(LibraryEntry {
        title: fallback_title(path),
        subtitle: "Could not import chart".to_owned(),
        search_aliases: Vec::new(),
        difficulty: None,
        chart_path: path.to_owned(),
        chart_index: index,
        audio_path: None,
        background_path: None,
        banner_path: None,
        preview_start_seconds: None,
        preview_duration_seconds: None,
        format,
        lane_count: 0,
        note_count: 0,
        problem: Some(error),
    });
}

fn record(
    library: &mut AppLibrary,
    path: &Path,
    chart_index: Option<usize>,
    status: &str,
    detail: &str,
) {
    library.records.push(ScanRecord {
        path: path.to_owned(),
        chart_index,
        status: status.to_owned(),
        detail: detail.to_owned(),
    });
}

fn fallback_title(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn osu_mode(path: &Path) -> std::io::Result<u8> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut general = false;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.starts_with('[') {
            if general {
                return Ok(0);
            }
            general = line == "[General]";
        }
        if general
            && let Some((key, value)) = line.split_once(':')
            && key.trim() == "Mode"
        {
            return value.trim().parse().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid osu! Mode value")
            });
        }
    }
    Ok(0)
}
