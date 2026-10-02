use super::ImportError;
use super::util::{parse_f64, strip_bom, strip_line_comment};
use crate::{Beat, Chart, LaneIndex, Note, NoteId, TimingPoint, TimingStop};

pub fn parse_stepmania_sm(input: &str) -> Result<Chart, ImportError> {
    parse_stepmania_sm_chart(input, 0)
}

pub fn parse_stepmania_sm_chart(input: &str, index: usize) -> Result<Chart, ImportError> {
    parse_stepmania_sm_catalog(input)?
        .into_iter()
        .nth(index)
        .ok_or_else(|| ImportError::new(format!("#NOTES chart index {index} is out of range")))?
}

// Catalog positions retain the original #NOTES ordinal, including invalid charts.
pub fn parse_stepmania_sm_catalog(
    input: &str,
) -> Result<Vec<Result<Chart, ImportError>>, ImportError> {
    let tags = parse_sm_tags(input)?;
    let mut title = String::new();
    let mut artist = String::new();
    let mut title_translit = None;
    let mut artist_translit = None;
    let mut audio_filename = None;
    let mut background_filename = None;
    let mut banner_filename = None;
    let mut preview_start_seconds = None;
    let mut preview_duration_seconds = None;
    let mut offset_seconds = 0.0;
    let mut bpm_changes = Vec::new();
    let mut stops = Vec::new();
    let mut notes = Vec::new();

    for tag in tags {
        match tag.key.as_str() {
            "TITLE" => title = tag.value.trim().to_owned(),
            "ARTIST" => artist = tag.value.trim().to_owned(),
            "TITLETRANSLIT" => title_translit = nonempty(&tag.value),
            "ARTISTTRANSLIT" => artist_translit = nonempty(&tag.value),
            "MUSIC" => audio_filename = Some(tag.value.trim().to_owned()),
            "BACKGROUND" => background_filename = nonempty(&tag.value),
            "BANNER" => banner_filename = nonempty(&tag.value),
            "SAMPLESTART" => {
                preview_start_seconds = tag
                    .value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0);
            }
            "SAMPLELENGTH" => {
                preview_duration_seconds = tag
                    .value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value > 0.0);
            }
            "OFFSET" => offset_seconds = parse_f64(tag.value.trim(), tag.line, "offset")?,
            "BPMS" => bpm_changes = parse_sm_bpms(&tag.value, tag.line)?,
            "STOPS" => stops = parse_sm_stops(&tag.value, tag.line)?,
            "NOTES" => notes.push(tag),
            _ => {}
        }
    }

    let mut chart = Chart::new(4);
    let metadata = chart.metadata_mut();
    metadata.title_unicode = title_translit.as_ref().and_then(|_| nonempty(&title));
    metadata.artist_unicode = artist_translit.as_ref().and_then(|_| nonempty(&artist));
    metadata.title = title_translit.unwrap_or(title);
    metadata.artist = artist_translit.unwrap_or(artist);
    metadata.audio_filename = audio_filename;
    metadata.background_filename = background_filename;
    metadata.banner_filename = banner_filename;
    metadata.preview_start_seconds = preview_start_seconds;
    metadata.preview_duration_seconds = preview_duration_seconds;

    if bpm_changes.is_empty() {
        return Err(ImportError::new("missing #BPMS data"));
    }

    chart.set_timing_points(sm_timing_points(offset_seconds, bpm_changes));
    chart.set_timing_stops(stops);

    if notes.is_empty() {
        return Err(ImportError::new("missing #NOTES chart"));
    }

    Ok(notes
        .into_iter()
        .map(|tag| {
            if !tag.terminated {
                return Err(ImportError::at_line(
                    tag.line,
                    "unterminated StepMania tag #NOTES",
                ));
            }
            let mut difficulty_chart = chart.clone();
            parse_sm_notes(&tag.value, tag.line, &mut difficulty_chart)?;
            Ok(difficulty_chart)
        })
        .collect())
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmTag {
    key: String,
    value: String,
    line: usize,
    terminated: bool,
}

fn parse_sm_tags(input: &str) -> Result<Vec<SmTag>, ImportError> {
    let mut tags = Vec::new();
    let mut current: Option<(String, String, usize)> = None;

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let mut line = strip_bom(raw_line);

        if let Some((key, mut value, start_line)) = current.take() {
            if key == "NOTES" && line.trim_start().starts_with('#') {
                tags.push(SmTag {
                    key,
                    value,
                    line: start_line,
                    terminated: false,
                });
            } else {
                value.push('\n');
                let (line_value, remainder) = take_until_semicolon(line);
                value.push_str(line_value);

                if let Some(remainder) = remainder {
                    tags.push(SmTag {
                        key,
                        value,
                        line: start_line,
                        terminated: true,
                    });
                    line = remainder;
                } else {
                    current = Some((key, value, start_line));
                    continue;
                }
            }
        }

        loop {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || !trimmed.starts_with('#') {
                break;
            }

            let Some((key, rest)) = trimmed[1..].split_once(':') else {
                return Err(ImportError::at_line(line_number, "invalid StepMania tag"));
            };

            let (value, remainder) = take_until_semicolon(rest);
            if let Some(remainder) = remainder {
                tags.push(SmTag {
                    key: key.trim().to_ascii_uppercase(),
                    value: value.to_owned(),
                    line: line_number,
                    terminated: true,
                });
                line = remainder;
            } else {
                current = Some((
                    key.trim().to_ascii_uppercase(),
                    value.to_owned(),
                    line_number,
                ));
                break;
            }
        }
    }

    if let Some((key, value, line)) = current {
        if key == "NOTES" {
            tags.push(SmTag {
                key,
                value,
                line,
                terminated: false,
            });
        } else {
            return Err(ImportError::at_line(
                line,
                format!("unterminated StepMania tag #{key}"),
            ));
        }
    }

    Ok(tags)
}

fn take_until_semicolon(line: &str) -> (&str, Option<&str>) {
    match line.find(';') {
        Some(index) => (&line[..index], Some(&line[index + 1..])),
        None => (line, None),
    }
}

fn parse_sm_bpms(value: &str, line: usize) -> Result<Vec<(f64, f64)>, ImportError> {
    let mut bpms = Vec::new();

    for pair in value.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let Some((beat, bpm)) = pair.split_once('=') else {
            return Err(ImportError::at_line(line, "invalid BPM pair"));
        };

        let beat = parse_f64(beat.trim(), line, "BPM beat")?;
        let bpm = parse_f64(bpm.trim(), line, "BPM value")?;
        if bpm <= 0.0 {
            return Err(ImportError::at_line(line, "BPM must be positive"));
        }
        bpms.push((beat, bpm));
    }

    if bpms.is_empty() {
        return Err(ImportError::at_line(line, "missing BPM data"));
    }

    bpms.sort_by(|left, right| left.0.total_cmp(&right.0));
    Ok(bpms)
}

fn parse_sm_stops(value: &str, line: usize) -> Result<Vec<TimingStop>, ImportError> {
    let mut stops = Vec::new();

    for pair in value.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let Some((beat, duration)) = pair.split_once('=') else {
            return Err(ImportError::at_line(line, "invalid stop pair"));
        };

        let beat = parse_f64(beat.trim(), line, "stop beat")?;
        let duration = parse_f64(duration.trim(), line, "stop duration")?;
        if duration < 0.0 {
            return Err(ImportError::at_line(
                line,
                "stop duration must be non-negative",
            ));
        }

        stops.push(TimingStop::new(Beat::new(beat), duration));
    }

    stops.sort_by(|left, right| left.beat.0.total_cmp(&right.beat.0));
    Ok(stops)
}

fn sm_timing_points(offset_seconds: f64, bpm_changes: Vec<(f64, f64)>) -> Vec<TimingPoint> {
    let mut points = Vec::with_capacity(bpm_changes.len());
    let mut previous_beat = bpm_changes[0].0;
    let mut previous_time = offset_seconds;
    let mut previous_beat_length = 60.0 / bpm_changes[0].1;

    for (index, (beat, bpm)) in bpm_changes.into_iter().enumerate() {
        let beat_length = 60.0 / bpm;
        let time_seconds = if index == 0 {
            offset_seconds
        } else {
            previous_time + (beat - previous_beat) * previous_beat_length
        };

        points.push(TimingPoint::new(Beat::new(beat), time_seconds, beat_length));
        previous_beat = beat;
        previous_time = time_seconds;
        previous_beat_length = beat_length;
    }

    points
}

fn parse_sm_notes(value: &str, line: usize, chart: &mut Chart) -> Result<(), ImportError> {
    let mut fields = value.splitn(6, ':');
    let chart_type = fields.next().unwrap_or_default().trim();
    let description = fields.next().unwrap_or_default().trim();
    let difficulty = fields.next().unwrap_or_default().trim();
    let meter = fields.next().unwrap_or_default().trim();
    let _radar = fields.next();
    let note_data = fields
        .next()
        .ok_or_else(|| ImportError::at_line(line, "invalid #NOTES data"))?;

    if chart_type != "dance-single" {
        return Err(ImportError::at_line(
            line,
            "only dance-single StepMania charts are supported for now",
        ));
    }

    chart.metadata_mut().difficulty = sm_difficulty_label(description, difficulty, meter);

    let mut next_id = 0;
    let mut hold_starts = [None; 4];

    for (measure_index, measure) in note_data.split(',').enumerate() {
        let rows: Vec<&str> = measure
            .lines()
            .map(strip_line_comment)
            .filter(|row| !row.is_empty() && !row.starts_with("//"))
            .collect();

        if rows.is_empty() {
            continue;
        }

        let rows_per_measure = rows.len() as f64;
        for (row_index, row) in rows.into_iter().enumerate() {
            if row.chars().count() < 4 {
                return Err(ImportError::at_line(
                    line,
                    "dance-single note rows need at least 4 lanes",
                ));
            }

            let beat = measure_index as f64 * 4.0 + row_index as f64 * 4.0 / rows_per_measure;
            let time_seconds = chart.time_at_beat(Beat::new(beat));

            for (lane, value) in row.chars().take(4).enumerate() {
                let lane = LaneIndex::new(lane as u8);
                match value {
                    '1' => {
                        chart.push_note(Note::tap(NoteId::new(next_id), lane, time_seconds));
                        next_id += 1;
                    }
                    '2' | '4' => {
                        hold_starts[lane.as_usize()] = Some((NoteId::new(next_id), time_seconds));
                        next_id += 1;
                    }
                    '3' => {
                        if let Some((id, start_time)) = hold_starts[lane.as_usize()].take() {
                            chart.push_note(Note::hold(id, lane, start_time, time_seconds));
                        }
                    }
                    '0' | 'M' | 'K' | 'L' | 'F' => {}
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn sm_difficulty_label(description: &str, difficulty: &str, meter: &str) -> Option<String> {
    let mut label = [description, difficulty]
        .into_iter()
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    if !meter.is_empty() {
        if !label.is_empty() {
            label.push(' ');
        }
        label.push_str(&format!("({meter})"));
    }
    (!label.is_empty()).then_some(label)
}

#[cfg(test)]
mod tests;
