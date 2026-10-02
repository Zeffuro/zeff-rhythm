use crate::app::library::AppLibrary;
use crate::app::persistence::AppPersistence;
use crate::app::state::PlayLaunchRequest;
use crate::play::{PlaySessionOptions, load_play_session_preview};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let mut roots = Vec::new();
    let mut report = None;
    let mut verify = false;
    let mut verify_audio = false;
    let mut save_roots = false;
    let mut cache_directory = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cache-dir" => {
                cache_directory = Some(PathBuf::from(
                    args.next().ok_or("missing --cache-dir path")?,
                ))
            }
            "--report" => report = Some(PathBuf::from(args.next().ok_or("missing --report path")?)),
            "--verify" => verify = true,
            "--verify-audio" => verify_audio = true,
            "--save-roots" => save_roots = true,
            _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}").into()),
            _ => roots.push(PathBuf::from(arg)),
        }
    }
    let mut persistence = AppPersistence::load_default()?;
    if roots.is_empty() {
        roots = persistence.settings().library_roots.clone();
    }
    let start = std::time::Instant::now();
    let library = match cache_directory {
        Some(directory) => AppLibrary::scan_roots_with_cache(&roots, &directory),
        None => AppLibrary::scan_roots(&roots),
    };
    println!(
        "library files={} charts={} ready={} skipped_modes={} problems={} scan_seconds={:.3}",
        library.files_scanned,
        library.entries().len(),
        library.playable_count(),
        library.skipped_modes,
        library.problem_count(),
        start.elapsed().as_secs_f64()
    );
    let metrics = &library.scan_metrics;
    println!(
        "library_scan discovered={} read={} reused={} discovery_seconds={:.3} parse_seconds={:.3} cache_load_seconds={:.3} cache_save_seconds={:.3} total_seconds={:.3}",
        metrics.discovered,
        metrics.read,
        metrics.reused,
        metrics.discovery_micros as f64 / 1_000_000.0,
        metrics.parse_micros as f64 / 1_000_000.0,
        metrics.cache_load_micros as f64 / 1_000_000.0,
        metrics.cache_save_micros as f64 / 1_000_000.0,
        metrics.total_micros as f64 / 1_000_000.0
    );
    if let Some(path) = report {
        write_report(&library, &path)?;
        println!("library_report={}", path.display());
    }
    if verify {
        verify_entries(&library)?;
    }
    if verify_audio {
        verify_audio_files(&library)?;
    }
    if save_roots {
        let mut settings = persistence.settings().clone();
        settings.library_roots = crate::app::library::absolute_library_roots(&roots)?;
        persistence.save_settings(&settings)?;
        println!("library_roots_saved=true");
    }
    Ok(())
}

fn verify_audio_files(library: &AppLibrary) -> Result<(), Box<dyn Error>> {
    let mut paths = HashSet::new();
    let mut failures = 0;
    for entry in library
        .entries()
        .iter()
        .filter(|entry| entry.problem.is_none())
    {
        let path = entry
            .audio_path
            .as_ref()
            .ok_or("ready entry has no audio")?;
        if !paths.insert(path.clone()) {
            continue;
        }
        match crate::platform::audio::load_audio_clip(path) {
            Ok(clip) if clip.frame_count() > 0 => {}
            Ok(_) => {
                failures += 1;
                eprintln!(
                    "audio_decode_error={} detail=No decoded frames",
                    path.display()
                );
            }
            Err(error) => {
                failures += 1;
                eprintln!("audio_decode_error={} detail={error}", path.display());
            }
        }
    }
    println!(
        "library_audio_verified={} failures={failures}",
        paths.len() - failures
    );
    if failures > 0 {
        return Err(format!("{failures} library audio files failed decoding").into());
    }
    Ok(())
}

fn verify_entries(library: &AppLibrary) -> Result<(), Box<dyn Error>> {
    let mut verified = 0;
    for entry in library
        .entries()
        .iter()
        .filter(|entry| entry.problem.is_none())
    {
        let mut settings = crate::app::settings::AppSettings::default();
        settings.diagnostics.event_log_enabled = false;
        let request = PlayLaunchRequest {
            chart: entry.chart_selection(),
            settings,
        };
        for options in [
            PlaySessionOptions::from_app_launch(&request)?,
            PlaySessionOptions::from_app_launch_for_sdl_harness(&request)?,
        ] {
            let preview = load_play_session_preview(&options)?;
            if preview.note_count != entry.note_count
                || preview.lane_count != entry.lane_count
                || preview.chart.metadata().difficulty != entry.difficulty
            {
                return Err(format!(
                    "discovery/launch mismatch at {} chart {}",
                    entry.chart_path.display(),
                    entry.chart_index
                )
                .into());
            }
        }
        verified += 1;
    }
    println!("library_verified={} launch_paths=wgpu,sdl", verified);
    Ok(())
}

fn write_report(library: &AppLibrary, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        "path,chart_index,status,detail,title,difficulty,lanes,notes,audio"
    )?;
    let entries = library
        .entries()
        .iter()
        .map(|entry| ((&entry.chart_path, entry.chart_index), entry))
        .collect::<HashMap<_, _>>();
    for record in &library.records {
        let entry = record
            .chart_index
            .and_then(|index| entries.get(&(&record.path, index)).copied());
        let fields = [
            record.path.display().to_string(),
            record
                .chart_index
                .map(|index| index.to_string())
                .unwrap_or_default(),
            record.status.clone(),
            record.detail.clone(),
            entry.map(|entry| entry.title.clone()).unwrap_or_default(),
            entry
                .and_then(|entry| entry.difficulty.clone())
                .unwrap_or_default(),
            entry
                .map(|entry| entry.lane_count.to_string())
                .unwrap_or_default(),
            entry
                .map(|entry| entry.note_count.to_string())
                .unwrap_or_default(),
            entry
                .and_then(|entry| entry.audio_path.as_ref())
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ];
        writeln!(
            writer,
            "{}",
            fields
                .iter()
                .map(|field| format!("\"{}\"", field.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(",")
        )?;
    }
    writer.flush()?;
    Ok(())
}
