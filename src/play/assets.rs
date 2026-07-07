use super::ChartFormat;
use crate::platform::audio::{
    AudioClip, OutputStreamTarget, load_audio_clip, output_stream_target,
};
use rhythm_core::Chart;
use rhythm_core::imports::{parse_osu_mania, parse_stepmania_sm};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::PlaySessionOptions;

pub struct LoadedPlaySession {
    pub chart: Chart,
    pub chart_format: ChartFormat,
    pub audio_path: PathBuf,
    pub clip: Arc<AudioClip>,
    pub target: OutputStreamTarget,
}

pub fn load_play_session_assets(
    options: &PlaySessionOptions,
) -> Result<LoadedPlaySession, Box<dyn Error>> {
    let chart_format = match options.format {
        Some(format) => format,
        None => ChartFormat::detect(&options.chart_path)?,
    };
    let chart = load_chart(&options.chart_path, chart_format)?;
    let audio_path =
        resolve_audio_path(&options.chart_path, &chart, options.audio_path.as_deref())?;
    let clip = Arc::new(load_audio_clip(&audio_path)?);
    let target = output_stream_target(&options.audio)?;

    Ok(LoadedPlaySession {
        chart,
        chart_format,
        audio_path,
        clip,
        target,
    })
}

pub fn load_chart(path: impl AsRef<Path>, format: ChartFormat) -> Result<Chart, Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    let chart = match format {
        ChartFormat::OsuMania => parse_osu_mania(&source)?,
        ChartFormat::StepMania => parse_stepmania_sm(&source)?,
    };

    Ok(chart)
}

pub fn resolve_audio_path(
    chart_path: &Path,
    chart: &Chart,
    override_path: Option<&Path>,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = override_path {
        return Ok(path.to_owned());
    }

    let audio_filename = chart
        .metadata()
        .audio_filename
        .as_deref()
        .ok_or("chart does not declare an audio file; pass --audio PATH")?;
    let audio_path = Path::new(audio_filename);

    if audio_path.is_absolute() {
        return Ok(audio_path.to_owned());
    }

    let chart_dir = chart_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(chart_dir.join(audio_path))
}

#[cfg(test)]
mod tests {
    use super::resolve_audio_path;
    use rhythm_core::{Chart, ChartMetadata};
    use std::path::Path;

    #[test]
    fn resolves_relative_audio_next_to_chart() {
        let mut chart = Chart::new(4);
        *chart.metadata_mut() = ChartMetadata {
            title: String::new(),
            artist: String::new(),
            source: None,
            audio_filename: Some("song.mp3".to_owned()),
        };

        let path = resolve_audio_path(Path::new("packs/song/chart.sm"), &chart, None).unwrap();

        assert_eq!(path, Path::new("packs/song/song.mp3"));
    }
}
