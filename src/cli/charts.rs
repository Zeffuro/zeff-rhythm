use super::args::parse_required_path;
use crate::play::parse_chart_format_option;
pub use crate::play::{ChartFormat, load_chart};
use rhythm_core::{Chart, NoteKind};
use std::error::Error;
use std::path::Path;

pub fn parse_format_option(value: &str) -> Result<Option<ChartFormat>, Box<dyn Error>> {
    parse_chart_format_option(value)
}

pub fn inspect_osu(args: &[String]) -> Result<(), Box<dyn Error>> {
    let path = parse_required_path(args, "inspect-osu <path-to-osu-mania-file>")?;
    let chart = load_chart(&path, ChartFormat::OsuMania)?;
    print_chart_summary(&path, &chart);
    Ok(())
}

pub fn inspect_sm(args: &[String]) -> Result<(), Box<dyn Error>> {
    let path = parse_required_path(args, "inspect-sm <path-to-stepmania-sm-file>")?;
    let chart = load_chart(&path, ChartFormat::StepMania)?;
    print_chart_summary(&path, &chart);
    Ok(())
}

pub fn print_chart_summary(path: impl AsRef<Path>, chart: &Chart) {
    println!("chart={}", path.as_ref().display());
    println!("title={}", empty_dash(&chart.metadata().title));
    println!("artist={}", empty_dash(&chart.metadata().artist));
    println!(
        "audio={}",
        chart.metadata().audio_filename.as_deref().unwrap_or("-")
    );
    println!("lanes={}", chart.lane_count());
    println!("notes={}", chart.notes().len());
    println!("holds={}", hold_count(chart));
    println!("timing_points={}", chart.timing_points().len());
    println!("timing_stops={}", chart.timing_stops().len());

    if let Some(first) = chart.notes().first() {
        println!(
            "first_note id={} lane={} time={:.6}s",
            first.id.as_u32(),
            first.lane.as_u8(),
            first.time_seconds
        );
    }

    if let Some(last) = chart.notes().last() {
        println!(
            "last_note id={} lane={} time={:.6}s",
            last.id.as_u32(),
            last.lane.as_u8(),
            last.time_seconds
        );
    }
}

pub fn hold_count(chart: &Chart) -> usize {
    chart
        .notes()
        .iter()
        .filter(|note| matches!(note.kind, NoteKind::Hold { .. }))
        .count()
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}
