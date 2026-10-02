mod cache;
#[cfg(test)]
mod cache_tests;
mod scan;
#[cfg(test)]
mod tests;

use super::state::ChartSelection;
use crate::play::ChartFormat;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Debug, Default)]
pub struct AppLibrary {
    entries: Vec<LibraryEntry>,
    pub records: Vec<ScanRecord>,
    pub roots: Vec<PathBuf>,
    pub files_scanned: usize,
    pub skipped_modes: usize,
    pub is_cached_snapshot: bool,
    pub scan_metrics: ScanMetrics,
}

impl AppLibrary {
    pub fn scan_roots(roots: &[PathBuf]) -> Self {
        Self::scan_roots_with_cache(roots, &cache::default_directory())
    }

    pub fn scan_roots_with_cache(roots: &[PathBuf], cache_directory: &Path) -> Self {
        scan::scan_roots(roots, cache_directory)
    }

    pub fn cached_snapshot(roots: &[PathBuf]) -> Option<Self> {
        Self::cached_snapshot_with_cache(roots, &cache::default_directory())
    }

    pub fn cached_snapshot_with_cache(roots: &[PathBuf], cache_directory: &Path) -> Option<Self> {
        cache::snapshot(roots, cache_directory)
    }

    pub fn scan_background(roots: Vec<PathBuf>) -> Receiver<Self> {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(Self::scan_roots(&roots));
        });
        receiver
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }
    pub fn get(&self, index: usize) -> Option<&LibraryEntry> {
        self.entries.get(index)
    }

    pub fn first_available_index(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(LibraryEntry::is_available)
            .or_else(|| (!self.entries.is_empty()).then_some(0))
    }

    pub fn selection_index(&self, selection: Option<&ChartSelection>) -> usize {
        selection
            .and_then(|selection| {
                self.entries.iter().position(|entry| {
                    entry.chart_path == selection.chart_path
                        && entry.chart_index == selection.chart_index
                })
            })
            .or_else(|| self.first_available_index())
            .unwrap_or(0)
    }

    pub fn playable_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.problem.is_none())
            .count()
    }

    pub fn problem_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.status == "error")
            .count()
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LibraryEntry {
    pub title: String,
    pub subtitle: String,
    pub search_aliases: Vec<String>,
    pub difficulty: Option<String>,
    pub chart_path: PathBuf,
    pub chart_index: usize,
    pub audio_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub banner_path: Option<PathBuf>,
    #[serde(default)]
    pub preview_start_seconds: Option<f64>,
    #[serde(default)]
    pub preview_duration_seconds: Option<f64>,
    #[serde(with = "cache::format")]
    pub format: ChartFormat,
    pub lane_count: u8,
    pub note_count: usize,
    pub problem: Option<String>,
}

impl LibraryEntry {
    pub fn is_available(&self) -> bool {
        self.problem.is_none()
            && self.chart_path.is_file()
            && self.audio_path.as_ref().is_some_and(|path| path.is_file())
    }

    pub fn status(&self) -> &str {
        if let Some(problem) = &self.problem {
            return problem;
        }
        if !self.chart_path.is_file() {
            return "Chart file is missing";
        }
        if !self.audio_path.as_ref().is_some_and(|path| path.is_file()) {
            return "Audio file is missing";
        }
        "Ready"
    }

    pub fn chart_selection(&self) -> ChartSelection {
        ChartSelection {
            chart_path: self.chart_path.clone(),
            chart_index: self.chart_index,
            audio_path: self.audio_path.clone(),
        }
    }

    pub fn chart_path(&self) -> &Path {
        &self.chart_path
    }
}

pub fn resolve_artwork_path(chart_path: &Path, candidate: &Path) -> Option<PathBuf> {
    let directory = chart_path.parent()?.canonicalize().ok()?;
    let resolved = candidate.canonicalize().ok()?;
    (resolved.starts_with(directory) && resolved.is_file()).then_some(resolved)
}

fn artwork_candidate(chart_path: &Path, filename: Option<&str>) -> Option<PathBuf> {
    let filename = filename?.trim();
    if filename.is_empty() || filename.contains(':') || filename.chars().any(char::is_control) {
        return None;
    }
    let normalized = filename.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative.components().any(|part| {
        !matches!(
            part,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return None;
    }
    Some(chart_path.parent()?.join(relative))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScanRecord {
    pub path: PathBuf,
    pub chart_index: Option<usize>,
    pub status: String,
    pub detail: String,
}

pub fn visible_range(selected: usize, count: usize, rows: usize) -> Range<usize> {
    let rows = rows.max(1);
    let start = selected
        .min(count.saturating_sub(1))
        .saturating_sub(rows / 2)
        .min(count.saturating_sub(rows));
    start..(start + rows).min(count)
}

pub fn library_roots_from_args(args: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        if argument != "--library" {
            return Err(format!("unknown option: {argument}; use --library PATH"));
        }
        roots.push(PathBuf::from(
            args.next().ok_or("missing value for --library")?,
        ));
    }
    Ok(roots)
}

pub fn absolute_library_roots(roots: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let cwd = std::env::current_dir()?;
    Ok(roots
        .iter()
        .map(|root| {
            if root.is_absolute() {
                root.clone()
            } else {
                cwd.join(root)
            }
        })
        .collect())
}

#[derive(Default)]
pub struct LibraryScanner {
    receiver: Option<Receiver<AppLibrary>>,
    roots: Vec<PathBuf>,
    generation: u64,
    active_generation: u64,
}

impl LibraryScanner {
    pub fn request(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
        self.generation = self.generation.wrapping_add(1);
        if self.receiver.is_none() {
            self.start();
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn poll(&mut self) -> Option<AppLibrary> {
        let result = self.receiver.as_ref()?.try_recv();
        match result {
            Err(TryRecvError::Empty) => None,
            Ok(_) | Err(TryRecvError::Disconnected)
                if self.active_generation != self.generation =>
            {
                self.start();
                None
            }
            Ok(library) => {
                self.receiver = None;
                Some(library)
            }
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                Some(self.failed_scan())
            }
        }
    }

    pub fn wait(&mut self) -> AppLibrary {
        while let Some(receiver) = self.receiver.take() {
            match receiver.recv() {
                Ok(library) if self.active_generation == self.generation => return library,
                Ok(_) => self.start(),
                Err(_) if self.active_generation != self.generation => self.start(),
                Err(_) => return self.failed_scan(),
            }
        }
        AppLibrary::default()
    }

    fn start(&mut self) {
        self.active_generation = self.generation;
        self.receiver = Some(AppLibrary::scan_background(self.roots.clone()));
    }

    fn failed_scan(&self) -> AppLibrary {
        let mut library = AppLibrary {
            roots: self.roots.clone(),
            ..AppLibrary::default()
        };
        library.records.push(ScanRecord {
            path: PathBuf::new(),
            chart_index: None,
            status: "error".to_owned(),
            detail: "Library scan stopped unexpectedly. Press R to retry.".to_owned(),
        });
        library
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScanMetrics {
    pub discovered: usize,
    pub read: usize,
    pub reused: usize,
    pub discovery_micros: u64,
    pub parse_micros: u64,
    pub cache_load_micros: u64,
    pub cache_save_micros: u64,
    pub total_micros: u64,
}

impl PartialEq for AppLibrary {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
            && self.records == other.records
            && self.roots == other.roots
            && self.files_scanned == other.files_scanned
            && self.skipped_modes == other.skipped_modes
    }
}
