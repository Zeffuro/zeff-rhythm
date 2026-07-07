use super::ImportError;
use super::util::{parse_f64, parse_i32, parse_u8, split_csv, split_key_value, strip_bom};
use crate::{Beat, Chart, LaneIndex, Note, NoteId, TimingPoint};

pub fn parse_osu_mania(input: &str) -> Result<Chart, ImportError> {
    let mut section = "";
    let mut mode = None;
    let mut lane_count = None;
    let mut audio_filename = None;
    let mut title = String::new();
    let mut artist = String::new();
    let mut source = None;
    let mut raw_timing_points = Vec::new();
    let mut raw_hit_objects = Vec::new();

    for (line_index, raw_line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_bom(raw_line).trim();

        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }

        match section {
            "General" => {
                let Some((key, value)) = split_key_value(line) else {
                    continue;
                };

                if key == "Mode" {
                    mode = Some(parse_u8(value, line_number, "mode")?);
                } else if key == "AudioFilename" {
                    audio_filename = Some(value.to_owned());
                }
            }
            "Metadata" => {
                let Some((key, value)) = split_key_value(line) else {
                    continue;
                };

                match key {
                    "Title" => title = value.to_owned(),
                    "Artist" => artist = value.to_owned(),
                    "Source" if !value.is_empty() => source = Some(value.to_owned()),
                    _ => {}
                }
            }
            "Difficulty" => {
                let Some((key, value)) = split_key_value(line) else {
                    continue;
                };

                if key == "CircleSize" {
                    lane_count = Some(parse_u8(value, line_number, "circle size")?);
                }
            }
            "TimingPoints" => {
                let fields = split_csv(line);
                if fields.len() < 2 {
                    return Err(ImportError::at_line(
                        line_number,
                        "osu timing point needs at least offset and beat length",
                    ));
                }

                let offset_seconds = parse_f64(fields[0], line_number, "timing offset")? / 1000.0;
                let beat_length_seconds =
                    parse_f64(fields[1], line_number, "beat length")? / 1000.0;

                if beat_length_seconds > 0.0 {
                    raw_timing_points.push((offset_seconds, beat_length_seconds));
                }
            }
            "HitObjects" => raw_hit_objects.push((line_number, line.to_owned())),
            _ => {}
        }
    }

    if mode != Some(3) {
        return Err(ImportError::new(
            "only osu!mania mode 3 charts are supported",
        ));
    }

    let lane_count = lane_count.ok_or_else(|| ImportError::new("missing CircleSize"))?;
    if lane_count == 0 {
        return Err(ImportError::new("CircleSize must be at least 1"));
    }

    let mut chart = Chart::new(lane_count);
    chart.metadata_mut().title = title;
    chart.metadata_mut().artist = artist;
    chart.metadata_mut().source = source;
    chart.metadata_mut().audio_filename = audio_filename;
    chart.set_timing_points(osu_timing_points(raw_timing_points));

    for (next_id, (line_number, line)) in raw_hit_objects.into_iter().enumerate() {
        let note =
            parse_osu_hit_object(&line, line_number, lane_count, NoteId::new(next_id as u32))?;
        chart.push_note(note);
    }

    Ok(chart)
}

fn parse_osu_hit_object(
    line: &str,
    line_number: usize,
    lane_count: u8,
    note_id: NoteId,
) -> Result<Note, ImportError> {
    let fields = split_csv(line);
    if fields.len() < 5 {
        return Err(ImportError::at_line(
            line_number,
            "osu hit object needs at least 5 fields",
        ));
    }

    let x = parse_i32(fields[0], line_number, "hit object x")?;
    let time_seconds = parse_f64(fields[2], line_number, "hit object time")? / 1000.0;
    let object_type = parse_i32(fields[3], line_number, "hit object type")?;
    let lane = osu_lane_from_x(x, lane_count);

    if object_type & 128 != 0 {
        let Some(hold_field) = fields.get(5) else {
            return Err(ImportError::at_line(
                line_number,
                "osu mania hold object is missing end time",
            ));
        };
        let end_time = hold_field.split(':').next().unwrap_or_default();
        let end_time_seconds = parse_f64(end_time, line_number, "hold end time")? / 1000.0;
        return Ok(Note::hold(note_id, lane, time_seconds, end_time_seconds));
    }

    Ok(Note::tap(note_id, lane, time_seconds))
}

fn osu_lane_from_x(x: i32, lane_count: u8) -> LaneIndex {
    let clamped_x = x.clamp(0, 511) as u32;
    let lane = clamped_x * lane_count as u32 / 512;
    LaneIndex::new(lane.min(lane_count as u32 - 1) as u8)
}

fn osu_timing_points(raw_points: Vec<(f64, f64)>) -> Vec<TimingPoint> {
    if raw_points.is_empty() {
        return vec![TimingPoint::new(Beat::new(0.0), 0.0, 0.5)];
    }

    let mut points = Vec::with_capacity(raw_points.len());
    let mut previous_beat = 0.0;
    let mut previous_time = raw_points[0].0;
    let mut previous_beat_length = raw_points[0].1;

    for (index, (time_seconds, beat_length_seconds)) in raw_points.into_iter().enumerate() {
        let beat = if index == 0 {
            0.0
        } else {
            previous_beat + (time_seconds - previous_time) / previous_beat_length
        };

        points.push(TimingPoint::new(
            Beat::new(beat),
            time_seconds,
            beat_length_seconds,
        ));

        previous_beat = beat;
        previous_time = time_seconds;
        previous_beat_length = beat_length_seconds;
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoteKind;

    #[test]
    fn parses_minimal_osu_mania_chart() {
        let chart = parse_osu_mania(
            r#"
osu file format v14

[General]
Mode: 3
AudioFilename: probe.mp3

[Metadata]
Title:Probe
Artist:Test

[Difficulty]
CircleSize:4

[TimingPoints]
1000,500,4,1,0,0,1,0

[HitObjects]
64,192,1000,1,0,0:0:0:0:
448,192,1250,128,0,1500:0:0:0:
"#,
        )
        .unwrap();

        assert_eq!(chart.lane_count(), 4);
        assert_eq!(
            chart.metadata().audio_filename.as_deref(),
            Some("probe.mp3")
        );
        assert_eq!(chart.notes().len(), 2);
        assert_eq!(chart.notes()[0].lane.as_u8(), 0);
        assert_eq!(chart.notes()[1].lane.as_u8(), 3);
        assert_eq!(
            chart.notes()[1].kind,
            NoteKind::Hold {
                end_time_seconds: 1.5
            }
        );
    }
}
