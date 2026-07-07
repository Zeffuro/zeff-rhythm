use super::args::parse_f64_arg;
use super::charts::{
    ChartFormat, hold_count, load_chart, parse_format_option, print_chart_summary,
};
use crate::play::JudgementCounts;
use rhythm_core::{Chart, GameKey, InputEvent, JudgementWindows, RhythmEngine};
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
    println!("hold_starts_tested={}", hold_count(&chart));

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
    let notes = chart.notes().to_vec();
    let end_time = notes
        .last()
        .map(|note| note.time_seconds + windows.miss_seconds + offset_seconds.abs() + 1.0)
        .unwrap_or_default();
    let mut engine = RhythmEngine::new(chart.clone(), windows);
    let mut counts = JudgementCounts::default();
    let mut unmatched_inputs = 0;
    let mut misses = Vec::new();

    for note in notes {
        let input_time = note.time_seconds + offset_seconds;
        let result = engine.submit_input(InputEvent {
            key: GameKey::Lane(note.lane),
            pressed: true,
            time_seconds: input_time,
        });

        match result {
            Some(result) => counts.add(result),
            None => unmatched_inputs += 1,
        }

        misses.clear();
        engine.collect_misses(input_time, &mut misses);
        for miss in misses.drain(..) {
            counts.add(miss);
        }
    }

    misses.clear();
    engine.collect_misses(end_time, &mut misses);
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
