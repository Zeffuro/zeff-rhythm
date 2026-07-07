use super::state::ChartSelection;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppLibrary {
    entries: Vec<LibraryEntry>,
}

impl AppLibrary {
    pub fn local_defaults() -> Self {
        Self {
            entries: vec![LibraryEntry::new(
                "Speedcore",
                "StepMania fixture",
                ".local_assets/stepmania/speedcore/Speedcore.sm",
            )],
        }
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
            .position(|entry| entry.chart_path.exists())
            .or_else(|| (!self.entries.is_empty()).then_some(0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    pub title: String,
    pub subtitle: String,
    pub chart_path: PathBuf,
    pub audio_path: Option<PathBuf>,
}

impl LibraryEntry {
    pub fn new(title: &str, subtitle: &str, chart_path: impl Into<PathBuf>) -> Self {
        Self {
            title: title.to_owned(),
            subtitle: subtitle.to_owned(),
            chart_path: chart_path.into(),
            audio_path: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.chart_path.exists()
    }

    pub fn chart_selection(&self) -> ChartSelection {
        ChartSelection {
            chart_path: self.chart_path.clone(),
            audio_path: self.audio_path.clone(),
        }
    }

    pub fn chart_path(&self) -> &Path {
        &self.chart_path
    }
}

#[cfg(test)]
mod tests {
    use super::AppLibrary;

    #[test]
    fn default_library_has_speedcore_fixture_entry() {
        let library = AppLibrary::local_defaults();
        let entry = library.get(0).unwrap();

        assert_eq!(entry.title, "Speedcore");
        assert!(entry.chart_path().ends_with("Speedcore.sm"));
    }
}
