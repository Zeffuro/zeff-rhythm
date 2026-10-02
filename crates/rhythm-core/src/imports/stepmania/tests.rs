use super::*;
use crate::NoteKind;

#[test]
fn preview_values_are_optional_finite_and_shared_across_difficulties() {
    for (value, start, duration) in [
        ("", None, None),
        ("-1", None, None),
        ("bad", None, None),
        ("NaN", None, None),
        ("inf", None, None),
        ("1e309", None, None),
        ("0", Some(0.0), None),
        ("12.25", Some(12.25), Some(12.25)),
    ] {
        let tags = if value.is_empty() {
            String::new()
        } else {
            format!("#SAMPLESTART:{value};#SAMPLELENGTH:{value};")
        };
        let catalog = parse_stepmania_sm_catalog(&format!(
            "{tags}#BPMS:0=120;{}{}",
            notes_tag("", "Easy", "2", "1000"),
            notes_tag("", "Hard", "8", "0010"),
        ))
        .unwrap();
        for chart in catalog {
            let chart = chart.unwrap();
            assert_eq!(chart.metadata().preview_start_seconds, start, "{value}");
            assert_eq!(
                chart.metadata().preview_duration_seconds,
                duration,
                "{value}"
            );
        }
    }
}

fn notes_tag(description: &str, difficulty: &str, meter: &str, rows: &str) -> String {
    format!("#NOTES:dance-single:{description}:{difficulty}:{meter}:0,0,0,0,0:\n{rows}\n;\n")
}

#[test]
fn native_names_transliterations_and_art_are_shared_across_difficulties() {
    let input = format!(
        "#TITLE:月;#TITLETRANSLIT:Tsuki;#ARTIST:星;#ARTISTTRANSLIT:Hoshi;#BACKGROUND:art\\月,光.jpg;#BANNER:帯.png;#BPMS:0=120;\n{}{}",
        notes_tag("", "Easy", "2", "1000"),
        notes_tag("", "Hard", "8", "0010"),
    );
    let catalog = parse_stepmania_sm_catalog(&input).unwrap();
    assert_eq!(catalog.len(), 2);
    for (index, chart) in catalog.into_iter().enumerate() {
        let chart = chart.unwrap();
        let metadata = chart.metadata();
        assert_eq!(metadata.title, "Tsuki");
        assert_eq!(metadata.artist, "Hoshi");
        assert_eq!(metadata.display_title(), "月");
        assert_eq!(metadata.display_artist(), "星");
        assert_eq!(
            metadata.background_filename.as_deref(),
            Some(r"art\月,光.jpg")
        );
        assert_eq!(metadata.banner_filename.as_deref(), Some("帯.png"));
        assert_eq!(
            metadata.difficulty.as_deref(),
            Some(["Easy (2)", "Hard (8)"][index])
        );
    }
}

#[test]
fn empty_transliterations_and_art_are_optional() {
    let input = format!(
        "#TITLE:月;#TITLETRANSLIT: ;#ARTIST:星;#ARTISTTRANSLIT:;#BACKGROUND: ;#BANNER:;#BPMS:0=120;\n{}",
        notes_tag("", "Easy", "2", "1000"),
    );
    let chart = parse_stepmania_sm(&input).unwrap();
    let metadata = chart.metadata();
    assert_eq!(metadata.display_title(), "月");
    assert_eq!(metadata.display_artist(), "星");
    assert!(metadata.background_filename.is_none());
    assert!(metadata.banner_filename.is_none());
    assert!(metadata.title_unicode.is_none());
    assert!(metadata.artist_unicode.is_none());
}

#[test]
fn catalog_selects_exact_difficulty_with_shared_song_timing() {
    let input = format!(
        "#TITLE:Shared;\n#ARTIST:Artist;\n#MUSIC:song.ogg;\n#OFFSET:0.25;\n#BPMS:0=120,4=240;\n#STOPS:2=1;\n{}{}",
        notes_tag("Starter", "Easy", "2", "1000\n0000\n0000\n0000"),
        notes_tag("Expert", "Hard", "8", "0000\n0100\n0010\n0001"),
    );
    let catalog = parse_stepmania_sm_catalog(&input).unwrap();
    assert_eq!(catalog.len(), 2);
    let first = catalog[0].as_ref().unwrap();
    let second = catalog[1].as_ref().unwrap();
    assert_eq!(
        first.metadata().difficulty.as_deref(),
        Some("Starter / Easy (2)")
    );
    assert_eq!(
        second.metadata().difficulty.as_deref(),
        Some("Expert / Hard (8)")
    );
    assert_eq!(second.metadata().title, "Shared");
    assert_eq!(second.metadata().artist, "Artist");
    assert_eq!(
        second.metadata().audio_filename.as_deref(),
        Some("song.ogg")
    );
    assert_eq!(first.timing_points(), second.timing_points());
    assert_eq!(first.timing_stops(), second.timing_stops());
    assert_eq!(first.notes().len(), 1);
    assert_eq!(second.notes().len(), 3);
    assert_eq!(second.notes()[2].time_seconds, 2.75);
    assert_eq!(parse_stepmania_sm(&input).unwrap(), *first);
    assert_eq!(parse_stepmania_sm_chart(&input, 1).unwrap(), *second);
}

#[test]
fn catalog_keeps_invalid_difficulties_without_hiding_supported_siblings() {
    let invalid_charts = [
        "#NOTES:dance-double:Double:Hard:9:0,0,0,0,0:\n10000000\n;\n",
        "#NOTES:dance-single:Broken:Easy:1:0,0,0,0,0:\n10\n;\n",
        "#NOTES:missing fields;\n",
        "#NOTES:dance-single:Unterminated:Easy:1:0,0,0,0,0:\n1000\n",
    ];
    for invalid in invalid_charts {
        let input = format!(
            "#BPMS:0=120;\n{invalid}{}",
            notes_tag("", "Hard", "7", "0010")
        );
        let catalog = parse_stepmania_sm_catalog(&input).unwrap();
        assert_eq!(catalog.len(), 2);
        assert!(catalog[0].is_err());
        let supported = catalog[1].as_ref().unwrap();
        assert_eq!(supported.metadata().difficulty.as_deref(), Some("Hard (7)"));
        assert_eq!(supported.notes()[0].lane.as_u8(), 2);
        assert!(parse_stepmania_sm(&input).is_err());
        assert_eq!(parse_stepmania_sm_chart(&input, 1).unwrap(), *supported);
    }
}

#[test]
fn rejects_missing_charts_invalid_shared_timing_and_out_of_range_ordinals() {
    assert!(parse_stepmania_sm_catalog("#BPMS:0=120;").is_err());
    let chart = notes_tag("", "Easy", "1", "1000");
    assert!(parse_stepmania_sm_catalog(&format!("#BPMS:0=0;\n{chart}")).is_err());
    let input = format!("#BPMS:0=120;\n{chart}");
    assert!(
        parse_stepmania_sm_chart(&input, 1)
            .unwrap_err()
            .message()
            .contains("out of range")
    );
    assert!(parse_stepmania_sm_chart(&input, usize::MAX).is_err());
}

#[test]
fn catalog_discovers_multiple_tags_on_one_line_and_unterminated_last_chart() {
    let input = "#TITLE:Inline;#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;#NOTES:dance-single::Hard:9:0,0,0,0,0:0010;#NOTES:unfinished";
    let catalog = parse_stepmania_sm_catalog(input).unwrap();
    assert_eq!(catalog.len(), 3);
    assert_eq!(catalog[0].as_ref().unwrap().metadata().title, "Inline");
    assert_eq!(catalog[1].as_ref().unwrap().notes()[0].lane.as_u8(), 2);
    assert!(catalog[2].is_err());
    assert_eq!(
        parse_stepmania_sm(input).unwrap().notes()[0].lane.as_u8(),
        0
    );
}

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
