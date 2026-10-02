use crate::play::metrics::{MetricStats, trimmed_mean};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;

const LEGACY_HEADER: &str = "event,chart_time_seconds,lane,note_id,rating,delta_ms,input_queue_age_ms,input_timestamp_ns,scheduled_time_seconds";
const CURRENT_HEADER: &str = "event,chart_time_seconds,lane,note_id,rating,delta_ms,input_queue_age_ms,input_timestamp_ns,input_source,input_timestamp_kind,scheduled_time_seconds";

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
    input_timestamp_kinds: BTreeMap<String, usize>,
    timestamp_kind_groups: BTreeMap<String, TimestampKindAnalysis>,
}

#[derive(Default)]
struct TimestampKindAnalysis {
    input_events: usize,
    hits: usize,
    unmatched_inputs: usize,
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
    let header = header.trim_start_matches('\u{feff}');
    let format = match header {
        CURRENT_HEADER => EventLogFormat::Current,
        LEGACY_HEADER => EventLogFormat::Legacy,
        _ => {
            return Err(format!("unexpected event log header in {path}: {header}").into());
        }
    };

    let mut analysis = RunAnalysis {
        files: 1,
        ..RunAnalysis::default()
    };

    for (line_index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let row = EventLogRow::parse(line, format)
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
        for (kind, count) in &other.input_timestamp_kinds {
            *self.input_timestamp_kinds.entry(kind.clone()).or_default() += count;
        }
        for (kind, group) in &other.timestamp_kind_groups {
            self.timestamp_kind_groups
                .entry(kind.clone())
                .or_default()
                .merge_from(group);
        }
    }

    fn record(&mut self, row: EventLogRow<'_>) {
        match row.event {
            "input_press" => {
                self.input_presses += 1;
                self.record_input_queue_age(row);
                self.record_input_timestamp_kind(row);
                self.record_timestamp_kind_input(row);
            }
            "input_release" => {
                self.input_releases += 1;
                self.record_input_queue_age(row);
                self.record_input_timestamp_kind(row);
                self.record_timestamp_kind_input(row);
            }
            "focus_initial_gained" => self.initial_focus_gained += 1,
            "focus_initial_lost" => self.initial_focus_lost += 1,
            "focus_gained" => self.focus_gained += 1,
            "focus_lost" => self.focus_lost += 1,
            "unmatched_input" => {
                self.unmatched_inputs += 1;
                self.record_timestamp_kind_unmatched(row);
            }
            "hit" => {
                self.hits += 1;
                self.record_rating(row.rating);
                if let Some(delta_ms) = row.delta_ms {
                    self.hit_delta_ms.push(delta_ms);
                    self.abs_hit_delta_ms.push(delta_ms.abs());
                }
                self.record_timestamp_kind_hit(row);
            }
            "hold_head" => {
                if let Some(delta_ms) = row.delta_ms {
                    self.hit_delta_ms.push(delta_ms);
                    self.abs_hit_delta_ms.push(delta_ms.abs());
                }
                self.record_timestamp_kind_hit(row);
            }
            "hold_complete" => {
                self.hits += 1;
                self.record_rating(row.rating);
            }
            "miss" | "hold_break" => {
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

    fn record_input_timestamp_kind(&mut self, row: EventLogRow<'_>) {
        if let Some(kind) = row.normalized_input_timestamp_kind() {
            *self
                .input_timestamp_kinds
                .entry(kind.to_owned())
                .or_default() += 1;
        }
    }

    fn record_timestamp_kind_input(&mut self, row: EventLogRow<'_>) {
        let Some(kind) = row.normalized_input_timestamp_kind() else {
            return;
        };
        let group = self
            .timestamp_kind_groups
            .entry(kind.to_owned())
            .or_default();
        group.input_events += 1;
        if let Some(queue_age_ms) = row.input_queue_age_ms {
            group.input_queue_age_ms.push(queue_age_ms);
        }
    }

    fn record_timestamp_kind_hit(&mut self, row: EventLogRow<'_>) {
        let Some(kind) = row.normalized_input_timestamp_kind() else {
            return;
        };
        let group = self
            .timestamp_kind_groups
            .entry(kind.to_owned())
            .or_default();
        group.hits += 1;
        if let Some(delta_ms) = row.delta_ms {
            group.hit_delta_ms.push(delta_ms);
            group.abs_hit_delta_ms.push(delta_ms.abs());
        }
    }

    fn record_timestamp_kind_unmatched(&mut self, row: EventLogRow<'_>) {
        let Some(kind) = row.normalized_input_timestamp_kind() else {
            return;
        };
        self.timestamp_kind_groups
            .entry(kind.to_owned())
            .or_default()
            .unmatched_inputs += 1;
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
        self.print_input_timestamp_kinds("file_input_timestamp_kinds", Some(path));
        self.print_timestamp_kind_groups("file_timestamp_kind", Some(path));
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
        self.print_input_timestamp_kinds("input_timestamp_kinds", None);
        self.print_timestamp_kind_groups("timestamp_kind", None);
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
                "{label}{path}=no input presses and no focus_gained events; focus the play window before pressing D/F/J/K"
            );
        } else {
            println!(
                "{label}{path}=window focus was observed but no lane presses were logged; verify D/F/J/K scancodes with sdl-input-probe"
            );
        }
    }

    fn print_input_timestamp_kinds(&self, label: &str, path: Option<&str>) {
        let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
        if self.input_timestamp_kinds.is_empty() {
            println!("{label}{path}=none");
            return;
        }

        let counts = self
            .input_timestamp_kinds
            .iter()
            .map(|(kind, count)| format!("{kind}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        println!("{label}{path} {counts}");
    }

    fn print_timestamp_kind_groups(&self, label: &str, path: Option<&str>) {
        let path_label = path.map(|path| format!(" file={path}")).unwrap_or_default();
        if self.timestamp_kind_groups.is_empty() {
            println!("{label}_summary{path_label} kind=none");
            return;
        }

        for (kind, group) in &self.timestamp_kind_groups {
            println!(
                "{label}_summary{path_label} kind={kind} input_events={} hits={} unmatched_inputs={}",
                group.input_events, group.hits, group.unmatched_inputs
            );
            print_timestamp_kind_metric(label, path, kind, "hit_delta_ms", &group.hit_delta_ms);
            print_timestamp_kind_metric(
                label,
                path,
                kind,
                "abs_hit_delta_ms",
                &group.abs_hit_delta_ms,
            );
            print_timestamp_kind_metric(
                label,
                path,
                kind,
                "input_queue_age_ms",
                &group.input_queue_age_ms,
            );
            print_timestamp_kind_offset_estimate(label, path, kind, &group.hit_delta_ms);
        }
    }
}

impl TimestampKindAnalysis {
    fn merge_from(&mut self, other: &Self) {
        self.input_events += other.input_events;
        self.hits += other.hits;
        self.unmatched_inputs += other.unmatched_inputs;
        self.hit_delta_ms.extend_from_slice(&other.hit_delta_ms);
        self.abs_hit_delta_ms
            .extend_from_slice(&other.abs_hit_delta_ms);
        self.input_queue_age_ms
            .extend_from_slice(&other.input_queue_age_ms);
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

fn print_timestamp_kind_metric(
    label: &str,
    path: Option<&str>,
    kind: &str,
    name: &str,
    samples: &[f64],
) {
    let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
    let Some(stats) = MetricStats::from_samples(samples) else {
        println!("{label}_metric{path} kind={kind} {name} count=0");
        return;
    };

    println!(
        "{label}_metric{path} kind={kind} {name} count={} mean={:.3} stddev={:.3} min={:.3} p50={:.3} p95={:.3} p99={:.3} max={:.3}",
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

fn print_timestamp_kind_offset_estimate(
    label: &str,
    path: Option<&str>,
    kind: &str,
    hit_delta_ms: &[f64],
) {
    let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
    let Some(stats) = MetricStats::from_samples(hit_delta_ms) else {
        println!("{label}_suggested_input_offset_adjustment_ms{path} kind={kind}=none");
        println!("{label}_robust_suggested_input_offset_adjustment_ms{path} kind={kind}=none");
        return;
    };
    let robust_mean = robust_hit_delta_mean_ms(hit_delta_ms).unwrap_or(stats.mean);

    println!(
        "{label}_suggested_input_offset_adjustment_ms{path} kind={kind}={:.3}",
        -stats.mean
    );
    println!(
        "{label}_robust_suggested_input_offset_adjustment_ms{path} kind={kind}={:.3}",
        -robust_mean
    );
}

fn print_offset_estimate(scope: &str, path: Option<&str>, hit_delta_ms: &[f64]) {
    let path = path.map(|path| format!(" file={path}")).unwrap_or_default();
    let Some(stats) = MetricStats::from_samples(hit_delta_ms) else {
        println!("{scope}_suggested_input_offset_adjustment_ms{path}=none");
        println!("{scope}_robust_suggested_input_offset_adjustment_ms{path}=none");
        println!("{scope}_suggested_input_offset_reason{path}=no hit delta samples");
        return;
    };
    let robust_mean = robust_hit_delta_mean_ms(hit_delta_ms).unwrap_or(stats.mean);

    println!(
        "{scope}_suggested_input_offset_adjustment_ms{path}={:.3}",
        -stats.mean
    );
    println!(
        "{scope}_robust_suggested_input_offset_adjustment_ms{path}={:.3}",
        -robust_mean
    );
    println!(
        "{scope}_suggested_input_offset_reason{path}=mean hit_delta_ms was {:.3}, robust 10pct trimmed mean was {:.3}; add the chosen adjustment to that run's --input-offset-ms",
        stats.mean, robust_mean
    );
}

fn print_aggregate_offset_note(hit_delta_ms: &[f64]) {
    let Some(stats) = MetricStats::from_samples(hit_delta_ms) else {
        println!("aggregate_suggested_input_offset_adjustment_ms=none");
        println!("aggregate_robust_suggested_input_offset_adjustment_ms=none");
        println!("aggregate_suggested_input_offset_reason=no hit delta samples");
        return;
    };
    let robust_mean = robust_hit_delta_mean_ms(hit_delta_ms).unwrap_or(stats.mean);

    println!("aggregate_mean_hit_delta_ms={:.3}", stats.mean);
    println!("aggregate_robust_mean_hit_delta_ms={:.3}", robust_mean);
    println!(
        "aggregate_robust_suggested_input_offset_adjustment_ms={:.3}",
        -robust_mean
    );
    println!(
        "aggregate_suggested_input_offset_reason=multiple files or timestamp kinds may use different offsets; use file_* or timestamp_kind_* robust suggestions for calibration"
    );
}

fn robust_hit_delta_mean_ms(hit_delta_ms: &[f64]) -> Option<f64> {
    trimmed_mean(hit_delta_ms, 0.10)
}

#[derive(Clone, Copy)]
struct EventLogRow<'a> {
    event: &'a str,
    rating: Option<&'a str>,
    delta_ms: Option<f64>,
    input_queue_age_ms: Option<f64>,
    input_timestamp_kind: Option<&'a str>,
}

#[derive(Clone, Copy)]
enum EventLogFormat {
    Legacy,
    Current,
}

impl<'a> EventLogRow<'a> {
    fn parse(line: &'a str, format: EventLogFormat) -> Result<Self, String> {
        let columns: Vec<&str> = line.split(',').collect();
        let expected_columns = match format {
            EventLogFormat::Legacy => 9,
            EventLogFormat::Current => 11,
        };
        if columns.len() != expected_columns {
            return Err(format!(
                "expected {expected_columns} columns, got {}",
                columns.len()
            ));
        }

        Ok(Self {
            event: columns[0],
            rating: nonempty(columns[4]),
            delta_ms: parse_optional_f64(columns[5], "delta_ms")?,
            input_queue_age_ms: parse_optional_f64(columns[6], "input_queue_age_ms")?,
            input_timestamp_kind: match format {
                EventLogFormat::Legacy => None,
                EventLogFormat::Current => nonempty(columns[9]),
            },
        })
    }

    fn normalized_input_timestamp_kind(self) -> Option<&'a str> {
        match self.input_timestamp_kind {
            Some("receipt_time") => Some("receipt_monotonic"),
            Some(kind) => Some(kind),
            None => None,
        }
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
mod tests;
