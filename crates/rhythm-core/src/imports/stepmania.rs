use super::ImportError;
use super::util::{parse_f64, strip_bom, strip_line_comment};
use crate::{Beat, Chart, LaneIndex, Note, NoteId, TimingPoint, TimingStop};

pub fn parse_stepmania_sm(input: &str) -> Result<Chart, ImportError> {
    let tags = parse_sm_tags(input)?;
    let mut title = String::new();
    let mut artist = String::new();
    let mut audio_filename = None;
    let mut offset_seconds = 0.0;
    let mut bpm_changes = Vec::new();
    let mut stops = Vec::new();
    let mut notes = None;

    for tag in tags {
        match tag.key.as_str() {
            "TITLE" => title = tag.value.trim().to_owned(),
            "ARTIST" => artist = tag.value.trim().to_owned(),
            "MUSIC" => audio_filename = Some(tag.value.trim().to_owned()),
            "OFFSET" => offset_seconds = parse_f64(tag.value.trim(), tag.line, "offset")?,
            "BPMS" => bpm_changes = parse_sm_bpms(&tag.value, tag.line)?,
            "STOPS" => stops = parse_sm_stops(&tag.value, tag.line)?,
            "NOTES" if notes.is_none() => notes = Some((tag.line, tag.value)),
            _ => {}
        }
    }

    let mut chart = Chart::new(4);
    chart.metadata_mut().title = title;
    chart.metadata_mut().artist = artist;
    chart.metadata_mut().audio_filename = audio_filename;

    if bpm_changes.is_empty() {
        return Err(ImportError::new("missing #BPMS data"));
    }

    chart.set_timing_points(sm_timing_points(offset_seconds, bpm_changes));
    chart.set_timing_stops(stops);

    let (line, notes_value) = notes.ok_or_else(|| ImportError::new("missing #NOTES chart"))?;
    parse_sm_notes(&notes_value, line, &mut chart)?;

    Ok(chart)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmTag {
    key: String,
    value: String,
    line: usize,
}

fn parse_sm_tags(input: &str) -> Result<Vec<SmTag>, ImportError> {
    let mut tags = Vec::new();
    let mut current: Option<(String, String, usize)> = None;

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_bom(raw_line);

        if let Some((key, mut value, start_line)) = current.take() {
            value.push('\n');
            let (line_value, finished) = take_until_semicolon(line);
            value.push_str(line_value);

            if finished {
                tags.push(SmTag {
                    key,
                    value,
                    line: start_line,
                });
            } else {
                current = Some((key, value, start_line));
            }

            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || !trimmed.starts_with('#') {
            continue;
        }

        let Some((key, rest)) = trimmed[1..].split_once(':') else {
            return Err(ImportError::at_line(line_number, "invalid StepMania tag"));
        };

        let (value, finished) = take_until_semicolon(rest);
        if finished {
            tags.push(SmTag {
                key: key.trim().to_ascii_uppercase(),
                value: value.to_owned(),
                line: line_number,
            });
        } else {
            current = Some((
                key.trim().to_ascii_uppercase(),
                value.to_owned(),
                line_number,
            ));
        }
    }

    if let Some((key, _, line)) = current {
        return Err(ImportError::at_line(
            line,
            format!("unterminated StepMania tag #{key}"),
        ));
    }

    Ok(tags)
}

fn take_until_semicolon(line: &str) -> (&str, bool) {
    match line.find(';') {
        Some(index) => (&line[..index], true),
        None => (line, false),
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
    let _description = fields.next();
    let _difficulty = fields.next();
    let _meter = fields.next();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoteKind;

    #[test]
    fn parses_minimal_stepmania_chart() {
        let chart = parse_stepmania_sm(
            r#"
#TITLE:Probe;
#ARTIST:Test;
#MUSIC:probe.mp3;
#OFFSET:0.250;
#BPMS:0.000=120.000,4.000=240.000;
#STOPS:6.000=1.500;
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
,
2000
0000
0000
3000
;
"#,
        )
        .unwrap();

        assert_eq!(chart.lane_count(), 4);
        assert_eq!(chart.metadata().title, "Probe");
        assert_eq!(
            chart.metadata().audio_filename.as_deref(),
            Some("probe.mp3")
        );
        assert_eq!(chart.timing_stops().len(), 1);
        assert_eq!(chart.notes().len(), 5);
        assert_eq!(chart.notes()[0].time_seconds, 0.25);
        assert_eq!(chart.notes()[3].time_seconds, 1.75);
        assert_eq!(
            chart.notes()[4].kind,
            NoteKind::Hold {
                end_time_seconds: 4.5
            }
        );
    }

    #[test]
    fn reserves_unique_ids_for_holds_before_they_end() {
        let chart = parse_stepmania_sm(
            r#"
#TITLE:Hold Ids;
#BPMS:0.000=120.000;
#NOTES:
     dance-single:
     basic:
     Easy:
     1:
     0,0,0,0,0:
2000
0100
3000
;
"#,
        )
        .unwrap();

        let ids: Vec<u32> = chart.notes().iter().map(|note| note.id.as_u32()).collect();
        assert_eq!(ids, vec![0, 1]);
    }
}
