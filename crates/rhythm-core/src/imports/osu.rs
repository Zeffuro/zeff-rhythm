use super::ImportError;
use super::util::{parse_f64, parse_i32, parse_u8, split_csv, split_key_value, strip_bom};
use crate::{Beat, Chart, LaneIndex, Note, NoteId, TimingPoint};

pub fn parse_osu_mania(input: &str) -> Result<Chart, ImportError> {
    let mut section = "";
    let mut mode = None;
    let mut lane_count = None;
    let mut audio_filename = None;
    let mut background_filename = None;
    let mut preview_start_seconds = None;
    let mut title = String::new();
    let mut artist = String::new();
    let mut title_unicode = None;
    let mut artist_unicode = None;
    let mut source = None;
    let mut difficulty = None;
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
                } else if key == "PreviewTime" {
                    preview_start_seconds = value
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .map(|value| value / 1000.0);
                }
            }
            "Metadata" => {
                let Some((key, value)) = split_key_value(line) else {
                    continue;
                };

                match key {
                    "Title" => title = value.to_owned(),
                    "Artist" => artist = value.to_owned(),
                    "TitleUnicode" => title_unicode = nonempty(value),
                    "ArtistUnicode" => artist_unicode = nonempty(value),
                    "Source" if !value.is_empty() => source = Some(value.to_owned()),
                    "Version" if !value.is_empty() => difficulty = Some(value.to_owned()),
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
            "Events" => {
                if let Some(filename) = background_event_filename(line) {
                    background_filename = Some(filename.to_owned());
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
    chart.metadata_mut().title_unicode = title_unicode;
    chart.metadata_mut().artist_unicode = artist_unicode;
    chart.metadata_mut().source = source;
    chart.metadata_mut().difficulty = difficulty;
    chart.metadata_mut().audio_filename = audio_filename;
    chart.metadata_mut().background_filename = background_filename;
    chart.metadata_mut().preview_start_seconds = preview_start_seconds;
    chart.set_timing_points(osu_timing_points(raw_timing_points));

    for (next_id, (line_number, line)) in raw_hit_objects.into_iter().enumerate() {
        let note =
            parse_osu_hit_object(&line, line_number, lane_count, NoteId::new(next_id as u32))?;
        chart.push_note(note);
    }

    Ok(chart)
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn background_event_filename(line: &str) -> Option<&str> {
    let (kind, rest) = line.split_once(',')?;
    if !matches!(kind.trim(), "0" | "Background") {
        return None;
    }
    let (start, rest) = rest.split_once(',')?;
    start.trim().parse::<i32>().ok()?;
    let rest = rest.trim();
    let filename = if let Some(quoted) = rest.strip_prefix('"') {
        let (filename, tail) = quoted.split_once('"')?;
        let tail = tail.trim_start();
        if !tail.is_empty() && !tail.starts_with(',') {
            return None;
        }
        filename
    } else {
        let filename = rest.split(',').next()?.trim();
        if filename.contains('"') {
            return None;
        }
        filename
    };
    (!filename.trim().is_empty()).then_some(filename)
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
    fn preview_time_is_optional_nonnegative_milliseconds() {
        for (value, expected) in [
            ("", None),
            ("-1", None),
            ("bad", None),
            ("NaN", None),
            ("inf", None),
            ("1e309", None),
            ("0", Some(0.0)),
            ("12345", Some(12.345)),
        ] {
            let general = if value.is_empty() {
                String::new()
            } else {
                format!("PreviewTime:{value}\n")
            };
            let chart = parse_osu_mania(&format!(
                "[General]\nMode:3\n{general}[Difficulty]\nCircleSize:4\n"
            ))
            .unwrap();
            assert_eq!(chart.metadata().preview_start_seconds, expected, "{value}");
            assert_eq!(chart.metadata().preview_duration_seconds, None);
        }
    }

    fn chart_with_events(events: &str) -> Chart {
        parse_osu_mania(&format!(
            "[General]\nMode:3\n[Difficulty]\nCircleSize:4\n[Events]\n{events}\n[HitObjects]\n64,192,1000,1,0\n"
        ))
        .unwrap()
    }

    #[test]
    fn keeps_native_names_and_quoted_background_with_commas() {
        let chart = parse_osu_mania(
            "[General]\nMode:3\n[Metadata]\nTitle:Tsuki\nTitleUnicode:月\nArtist:Hoshi\nArtistUnicode:星\n[Difficulty]\nCircleSize:4\n[Events]\n0,0,\"art\\月,光.jpg\",0,0\n",
        )
        .unwrap();
        let metadata = chart.metadata();
        assert_eq!(metadata.title, "Tsuki");
        assert_eq!(metadata.artist, "Hoshi");
        assert_eq!(metadata.display_title(), "月");
        assert_eq!(metadata.display_artist(), "星");
        assert_eq!(
            metadata.background_filename.as_deref(),
            Some(r"art\月,光.jpg")
        );
        assert!(metadata.banner_filename.is_none());
    }

    #[test]
    fn accepts_named_backgrounds_and_ignores_video_and_storyboard_events() {
        for event in ["0,0,夜景.jpg", "Background,0,\"夜景.jpg\",0,0"] {
            let chart = chart_with_events(&format!(
                "Video,0,\"movie.mp4\"\n{event}\n1,0,\"another.mp4\"\nSprite,Background,Centre,\"sprite.png\",0,0"
            ));
            assert_eq!(
                chart.metadata().background_filename.as_deref(),
                Some("夜景.jpg")
            );
            assert_eq!(chart.notes().len(), 1);
        }
    }

    #[test]
    fn malformed_or_empty_art_events_do_not_reject_the_chart() {
        for event in [
            "0",
            "0,0",
            "0,0,",
            "0,0,\"\"",
            "0,0,\"unterminated",
            "0,invalid,image.jpg",
            "0,0,\"image.jpg\"extra",
            "Video,0,\"movie.mp4\"",
            "Sprite,Background,Centre,\"sprite.png\",0,0",
        ] {
            let chart = chart_with_events(event);
            assert!(chart.metadata().background_filename.is_none(), "{event}");
            assert_eq!(chart.notes().len(), 1);
        }
        let chart = parse_osu_mania(
            "[General]\nMode:3\n[Metadata]\nTitle:Fallback\nTitleUnicode:   \nArtist:Artist\nArtistUnicode:\n[Difficulty]\nCircleSize:4\n",
        )
        .unwrap();
        assert!(chart.metadata().title_unicode.is_none());
        assert!(chart.metadata().artist_unicode.is_none());
        assert_eq!(chart.metadata().display_title(), "Fallback");
        assert_eq!(chart.metadata().display_artist(), "Artist");
    }

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
Version:Expert

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
        assert_eq!(chart.metadata().difficulty.as_deref(), Some("Expert"));
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
