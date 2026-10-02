use super::args::parse_f64_arg;
use super::charts::{
    ChartFormat, hold_count, load_chart, parse_format_option, print_chart_summary,
};
use crate::play::JudgementCounts;
use rhythm_core::{Chart, GameKey, InputEvent, JudgementWindows, NoteKind, RhythmEngine};
use std::error::Error;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let options = MapTestOptions::parse(args)?;
    let format = match options.format {
        Some(format) => format,
        None => ChartFormat::detect(&options.path)?,
    };
    let chart = load_chart(&options.path, format)?;
    let result = simulate_autoplay(&chart, options.offset_ms / 1_000.0);

    print_chart_summary(&options.path, &chart);
    println!("map_test=autoplay");
    println!("offset_ms={:.3}", options.offset_ms);
    println!("judged={}", result.judged);
    println!("complete={}", result.complete);
    println!("marvelous={}", result.counts.marvelous);
    println!("perfect={}", result.counts.perfect);
    println!("great={}", result.counts.great);
    println!("good={}", result.counts.good);
    println!("miss={}", result.counts.miss);
    println!("unmatched_inputs={}", result.unmatched_inputs);
    println!("holds_tested={}", hold_count(&chart));

    Ok(())
}

struct MapTestOptions {
    path: String,
    format: Option<ChartFormat>,
    offset_ms: f64,
}

impl MapTestOptions {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut path = None;
        let mut format = None;
        let mut offset_ms = 0.0;
        let mut index = 0;

        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--format" => {
                    index += 1;
                    let value = args.get(index).ok_or("missing value for --format")?;
                    format = parse_format_option(value)?;
                }
                "--offset-ms" => {
                    index += 1;
                    let value = args.get(index).ok_or("missing value for --offset-ms")?;
                    offset_ms = parse_f64_arg(value, "offset milliseconds")?;
                }
                _ if arg.starts_with("--format=") => {
                    format = parse_format_option(&arg["--format=".len()..])?;
                }
                _ if arg.starts_with("--offset-ms=") => {
                    offset_ms = parse_f64_arg(&arg["--offset-ms=".len()..], "offset milliseconds")?;
                }
                _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}").into()),
                _ => {
                    if path.replace(arg.clone()).is_some() {
                        return Err("usage: zeff-rhythm map-test <chart-path> [--format auto|osu|sm] [--offset-ms MS]".into());
                    }
                }
            }

            index += 1;
        }

        let Some(path) = path else {
            return Err(
                "usage: zeff-rhythm map-test <chart-path> [--format auto|osu|sm] [--offset-ms MS]"
                    .into(),
            );
        };

        Ok(Self {
            path,
            format,
            offset_ms,
        })
    }
}

struct MapTestResult {
    counts: JudgementCounts,
    judged: usize,
    complete: bool,
    unmatched_inputs: usize,
}

fn simulate_autoplay(chart: &Chart, offset_seconds: f64) -> MapTestResult {
    let windows = JudgementWindows::default();
    let mut inputs = Vec::new();
    for note in chart.notes() {
        inputs.push(InputEvent {
            key: GameKey::Lane(note.lane),
            pressed: true,
            time_seconds: note.time_seconds + offset_seconds,
        });
        inputs.push(InputEvent {
            key: GameKey::Lane(note.lane),
            pressed: false,
            time_seconds: match note.kind {
                NoteKind::Tap => note.time_seconds + 0.001,
                NoteKind::Hold { end_time_seconds } => end_time_seconds,
            } + offset_seconds,
        });
    }
    inputs.sort_by(|a, b| {
        a.time_seconds
            .total_cmp(&b.time_seconds)
            .then(a.pressed.cmp(&b.pressed))
    });
    let chart_end = chart
        .notes()
        .iter()
        .map(|note| match note.kind {
            NoteKind::Tap => note.time_seconds,
            NoteKind::Hold { end_time_seconds } => end_time_seconds,
        })
        .fold(0.0, f64::max);
    let end_time = inputs
        .last()
        .map(|event| event.time_seconds.max(chart_end) + windows.miss_seconds + 1.0)
        .unwrap_or_default();
    let mut engine = RhythmEngine::new(chart.clone(), windows);
    let mut counts = JudgementCounts::default();
    let mut unmatched_inputs = 0;
    let mut misses = Vec::new();

    for event in inputs {
        misses.clear();
        engine.collect_judgements(event.time_seconds, &mut misses);
        for result in misses.drain(..) {
            counts.add(result);
        }
        let result = engine.submit_input(event);

        match result {
            Some(result) => counts.add(result),
            None if event.pressed => unmatched_inputs += 1,
            None => {}
        }

        misses.clear();
        engine.collect_judgements(event.time_seconds, &mut misses);
        for miss in misses.drain(..) {
            counts.add(miss);
        }
    }

    misses.clear();
    engine.collect_judgements(end_time, &mut misses);
    for miss in misses {
        counts.add(miss);
    }

    MapTestResult {
        counts,
        judged: engine.judged_count(),
        complete: engine.is_complete(),
        unmatched_inputs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhythm_core::{LaneIndex, Note, NoteId};

    #[test]
    fn autoplay_holds_and_chords_complete_with_one_score_per_note() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 3.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 1.0));
        chart.push_note(Note::tap(NoteId::new(3), LaneIndex::new(0), 3.0));
        let result = simulate_autoplay(&chart, 0.0);
        assert!(result.complete);
        assert_eq!(result.judged, 3);
        assert_eq!(result.counts.marvelous, 3);
        assert_eq!(result.unmatched_inputs, 0);
    }

    #[test]
    fn large_negative_offsets_still_flush_every_missed_head() {
        let mut chart = Chart::new(2);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 1.0));
        let result = simulate_autoplay(&chart, -2.0);
        assert!(result.complete);
        assert_eq!(result.judged, 2);
        assert_eq!(result.counts.miss, 2);
    }
}
