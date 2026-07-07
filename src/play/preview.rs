use super::{PlaySessionOptions, load_chart};
use rhythm_core::{Chart, NoteKind};
use std::error::Error;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct PlaySessionPreview {
    pub title: String,
    pub artist: String,
    pub chart_path: PathBuf,
    pub lane_count: u8,
    pub note_count: usize,
    pub hold_count: usize,
    pub chart_start_seconds: f64,
    pub chart: Chart,
}

pub fn load_play_session_preview(
    options: &PlaySessionOptions,
) -> Result<PlaySessionPreview, Box<dyn Error>> {
    let chart_format = match options.format {
        Some(format) => format,
        None => super::ChartFormat::detect(&options.chart_path)?,
    };
    let chart = load_chart(&options.chart_path, chart_format)?;
    let metadata = chart.metadata();
    let hold_count = chart
        .notes()
        .iter()
        .filter(|note| matches!(note.kind, NoteKind::Hold { .. }))
        .count();

    Ok(PlaySessionPreview {
        title: metadata.title.clone(),
        artist: metadata.artist.clone(),
        chart_path: options.chart_path.clone(),
        lane_count: chart.lane_count(),
        note_count: chart.notes().len(),
        hold_count,
        chart_start_seconds: options.chart_start_seconds(&chart),
        chart,
    })
}

#[cfg(test)]
mod tests {
    use super::load_play_session_preview;
    use crate::platform::input::NativeInputBackendKind;
    use crate::play::{PlayDisplayMode, PlaySessionOptions};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_preview_without_audio_device_setup() {
        let path = std::env::temp_dir().join(format!(
            "zeff-rhythm-preview-{}.sm",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            r#"
#TITLE:Preview Probe;
#ARTIST:Test;
#MUSIC:probe.mp3;
#BPMS:0.000=120.000;
#NOTES:
     dance-single:
     basic:
     Easy:
     1:
     0,0,0,0,0:
1000
0100
0010
0001
;
"#,
        )
        .unwrap();

        let preview = load_play_session_preview(&PlaySessionOptions {
            chart_path: path.clone(),
            format: None,
            audio_path: None,
            input_offset_ms: 0.0,
            max_seconds: None,
            lookahead_seconds: 4.0,
            lead_in_seconds: None,
            chart_start_seconds: None,
            start_delay_seconds: None,
            display: PlayDisplayMode::Sdl,
            input: NativeInputBackendKind::Sdl,
            event_log_path: None,
            dry_run: false,
            audio: Default::default(),
        })
        .unwrap();

        let _ = fs::remove_file(path);

        assert_eq!(preview.title, "Preview Probe");
        assert_eq!(preview.lane_count, 4);
        assert_eq!(preview.note_count, 4);
        assert!(preview.chart_start_seconds <= 0.0);
    }
}
