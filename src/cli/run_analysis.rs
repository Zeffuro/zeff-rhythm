use crate::play::metrics::MetricStats;
use std::error::Error;
use std::fs;

const EXPECTED_HEADER: &str = "event,chart_time_seconds,lane,note_id,rating,delta_ms,input_queue_age_ms,input_timestamp_ns,scheduled_time_seconds";

#[derive(Default)]
struct RunAnalysis {
    files: usize,
    rows: usize,
    input_presses: usize,
    input_releases: usize,
    initial_focus_gained: usize,
    initial_focus_lost: usize,
    focus_gained: usize,
    focus_lost: usize,
    unmatched_inputs: usize,
    hits: usize,
    misses: usize,
    marvelous: usize,
    perfect: usize,
    great: usize,
    good: usize,
    hit_delta_ms: Vec<f64>,
    abs_hit_delta_ms: Vec<f64>,
    input_queue_age_ms: Vec<f64>,
}

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.is_empty() {
        return Err("usage: zeff-rhythm analyze-run <event-log.csv> [more-event-log.csv]".into());
    }

    let mut analysis = RunAnalysis::default();
    for path in args {
        let file_analysis = analyze_file(path)?;
        println!("file={} rows={}", path, file_analysis.rows);
        file_analysis.print_file(path);
        analysis.merge_from(&file_analysis);
    }

    analysis.print_aggregate();
    Ok(())
}

fn analyze_file(path: &str) -> Result<RunAnalysis, Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    let mut lines = source.lines();
    let header = lines
        .next()
        .ok_or_else(|| format!("empty event log: {path}"))?;
    if header.trim_start_matches('\u{feff}') != EXPECTED_HEADER {
        return Err(format!("unexpected event log header in {path}: {header}").into());
    }

    let mut analysis = RunAnalysis {
        files: 1,
        ..RunAnalysis::default()
    };

    for (line_index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let row = EventLogRow::parse(line)
            .map_err(|message| format!("{path}:{}: {message}", line_index + 2))?;
        analysis.record(row);
        analysis.rows += 1;
    }

    Ok(analysis)
}

impl RunAnalysis {
    fn merge_from(&mut self, other: &Self) {
        self.files += other.files;
        self.rows += other.rows;
        self.input_presses += other.input_presses;
        self.input_releases += other.input_releases;
        self.initial_focus_gained += other.initial_focus_gained;
        self.initial_focus_lost += other.initial_focus_lost;
        self.focus_gained += other.focus_gained;
        self.focus_lost += other.focus_lost;
        self.unmatched_inputs += other.unmatched_inputs;
        self.hits += other.hits;
        self.misses += other.misses;
        self.marvelous += other.marvelous;
        self.perfect += other.perfect;
        self.great += other.great;
        self.good += other.good;
        self.hit_delta_ms.extend_from_slice(&other.hit_delta_ms);
        self.abs_hit_delta_ms
            .extend_from_slice(&other.abs_hit_delta_ms);
        self.input_queue_age_ms
            .extend_from_slice(&other.input_queue_age_ms);
    }

    fn record(&mut self, row: EventLogRow<'_>) {
        match row.event {
            "input_press" => {
                self.input_presses += 1;
                self.record_input_queue_age(row);
            }
            "input_release" => {
                self.input_releases += 1;
                self.record_input_queue_age(row);
            }
            "focus_initial_gained" => self.initial_focus_gained += 1,
            "focus_initial_lost" => self.initial_focus_lost += 1,
            "focus_gained" => self.focus_gained += 1,
            "focus_lost" => self.focus_lost += 1,
            "unmatched_input" => self.unmatched_inputs += 1,
            "hit" => {
                self.hits += 1;
                self.record_rating(row.rating);
                if let Some(delta_ms) = row.delta_ms {
                    self.hit_delta_ms.push(delta_ms);
                    self.abs_hit_delta_ms.push(delta_ms.abs());
                }
            }
            "miss" => {
                self.misses += 1;
            }
            _ => {}
        }
    }

    fn record_rating(&mut self, rating: Option<&str>) {
        match rating {
            Some("Marvelous") => self.marvelous += 1,
            Some("Perfect") => self.perfect += 1,
            Some("Great") => self.great += 1,
            Some("Good") => self.good += 1,
            _ => {}
        }
    }

    fn record_input_queue_age(&mut self, row: EventLogRow<'_>) {
        if let Some(queue_age_ms) = row.input_queue_age_ms {
            self.input_queue_age_ms.push(queue_age_ms);
        }
    }

    fn print_file(&self, path: &str) {
        self.print_counts("file_summary", Some(path));
        self.print_ratings("file_ratings", Some(path));
        print_scoped_metric(
            "file_metric",
            Some(path),
            "hit_delta_ms",
            &self.hit_delta_ms,
        );
        print_scoped_metric(
            "file_metric",
            Some(path),
            "abs_hit_delta_ms",
            &self.abs_hit_delta_ms,
        );
        print_scoped_metric(
            "file_metric",
            Some(path),
            "input_queue_age_ms",
            &self.input_queue_age_ms,
        );
        self.print_input_diagnostic("file_input_diagnostic", Some(path));
        print_offset_estimate("file", Some(path), &self.hit_delta_ms);
    }

    fn print_aggregate(&self) {
        self.print_counts("analyze_run", None);
        self.print_ratings("ratings", None);
        print_scoped_metric("metric", None, "hit_delta_ms", &self.hit_delta_ms);
        print_scoped_metric("metric", None, "abs_hit_delta_ms", &self.abs_hit_delta_ms);
        print_scoped_metric(
            "metric",
            None,
            "input_queue_age_ms",
            &self.input_queue_age_ms,
        );
        self.print_input_diagnostic("input_diagnostic", None);
        if self.files > 1 {
            print_aggregate_offset_note(&self.hit_delta_ms);
        } else {
            print_offset_estimate("aggregate", None, &self.hit_delta_ms);
        }
    }

    fn print_counts(&self, label: &str, path: Option<&str>) {
        let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
        println!(
            "{label}{path} files={} rows={} input_presses={} input_releases={} focus_initial_gained={} focus_initial_lost={} focus_gained={} focus_lost={} unmatched_inputs={} hits={} misses={}",
            self.files,
            self.rows,
            self.input_presses,
            self.input_releases,
            self.initial_focus_gained,
            self.initial_focus_lost,
            self.focus_gained,
            self.focus_lost,
            self.unmatched_inputs,
            self.hits,
            self.misses
        );
    }

    fn print_ratings(&self, label: &str, path: Option<&str>) {
        let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
        println!(
            "{label}{path} marvelous={} perfect={} great={} good={}",
            self.marvelous, self.perfect, self.great, self.good
        );
    }

    fn print_input_diagnostic(&self, label: &str, path: Option<&str>) {
        let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
        if self.input_presses > 0 {
            println!("{label}{path}=input events captured");
        } else if self.focus_gained + self.initial_focus_gained == 0 {
            println!(
                "{label}{path}=no input presses and no focus_gained events; click/focus the SDL play window before pressing D/F/J/K"
            );
        } else {
            println!(
                "{label}{path}=window focus was observed but no lane presses were logged; verify D/F/J/K scancodes with sdl-input-probe"
            );
        }
    }
}

fn print_scoped_metric(label: &str, path: Option<&str>, name: &str, samples: &[f64]) {
    let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
    let Some(stats) = MetricStats::from_samples(samples) else {
        println!("{label}{path} {name} count=0");
        return;
    };

    println!(
        "{label}{path} {name} count={} mean={:.3} stddev={:.3} min={:.3} p50={:.3} p95={:.3} p99={:.3} max={:.3}",
        stats.count,
        stats.mean,
        stats.stddev,
        stats.min,
        stats.p50,
        stats.p95,
        stats.p99,
        stats.max
    );
}

fn print_offset_estimate(scope: &str, path: Option<&str>, hit_delta_ms: &[f64]) {
    let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
    let Some(stats) = MetricStats::from_samples(hit_delta_ms) else {
        println!("{scope}_suggested_input_offset_adjustment_ms{path}=none");
        println!("{scope}_suggested_input_offset_reason{path}=no hit delta samples");
        return;
    };

    println!(
        "{scope}_suggested_input_offset_adjustment_ms{path}={:.3}",
        -stats.mean
    );
    println!(
        "{scope}_suggested_input_offset_reason{path}=mean hit_delta_ms was {:.3}; add this adjustment to that run's --input-offset-ms",
        stats.mean
    );
}

fn print_aggregate_offset_note(hit_delta_ms: &[f64]) {
    let Some(stats) = MetricStats::from_samples(hit_delta_ms) else {
        println!("aggregate_suggested_input_offset_adjustment_ms=none");
        println!("aggregate_suggested_input_offset_reason=no hit delta samples");
        return;
    };

    println!("aggregate_mean_hit_delta_ms={:.3}", stats.mean);
    println!(
        "aggregate_suggested_input_offset_reason=multiple files may use different offsets; use file_suggested_input_offset_adjustment_ms for calibration"
    );
}

#[derive(Clone, Copy)]
struct EventLogRow<'a> {
    event: &'a str,
    rating: Option<&'a str>,
    delta_ms: Option<f64>,
    input_queue_age_ms: Option<f64>,
}

impl<'a> EventLogRow<'a> {
    fn parse(line: &'a str) -> Result<Self, String> {
        let columns: Vec<&str> = line.split(',').collect();
        if columns.len() != 9 {
            return Err(format!("expected 9 columns, got {}", columns.len()));
        }

        Ok(Self {
            event: columns[0],
            rating: nonempty(columns[4]),
            delta_ms: parse_optional_f64(columns[5], "delta_ms")?,
            input_queue_age_ms: parse_optional_f64(columns[6], "input_queue_age_ms")?,
        })
    }
}

fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() { None } else { Some(value) }
}

fn parse_optional_f64(value: &str, name: &str) -> Result<Option<f64>, String> {
    if value.is_empty() {
        return Ok(None);
    }

    value
        .parse()
        .map(Some)
        .map_err(|_| format!("invalid {name}: {value}"))
}

#[cfg(test)]
mod tests {
    use super::{EventLogRow, RunAnalysis};

    #[test]
    fn parses_hit_rows() {
        let row =
            EventLogRow::parse("hit,1.000000,2,42,Perfect,-12.500,0.200,123,1.012500").unwrap();

        assert_eq!(row.event, "hit");
        assert_eq!(row.rating, Some("Perfect"));
        assert_eq!(row.delta_ms, Some(-12.5));
        assert_eq!(row.input_queue_age_ms, Some(0.2));
    }

    #[test]
    fn aggregates_hit_metrics() {
        let mut analysis = RunAnalysis::default();
        analysis.record(
            EventLogRow::parse("hit,1.000000,2,42,Perfect,-12.500,0.200,123,1.012500").unwrap(),
        );
        analysis.record(EventLogRow::parse("miss,1.200000,0,43,Miss,,,,1.000000").unwrap());

        assert_eq!(analysis.hits, 1);
        assert_eq!(analysis.misses, 1);
        assert_eq!(analysis.perfect, 1);
        assert_eq!(analysis.hit_delta_ms, vec![-12.5]);
    }

    #[test]
    fn queue_age_uses_raw_input_rows_only() {
        let mut analysis = RunAnalysis::default();
        analysis.record(EventLogRow::parse("input_press,1.000000,2,,,0.000,0.200,123,").unwrap());
        analysis.record(
            EventLogRow::parse("hit,1.000000,2,42,Perfect,-12.500,0.200,123,1.012500").unwrap(),
        );
        analysis
            .record(EventLogRow::parse("unmatched_input,1.200000,2,,,0.000,0.300,124,").unwrap());

        assert_eq!(analysis.input_presses, 1);
        assert_eq!(analysis.hits, 1);
        assert_eq!(analysis.unmatched_inputs, 1);
        assert_eq!(analysis.input_queue_age_ms, vec![0.2]);
    }
}
