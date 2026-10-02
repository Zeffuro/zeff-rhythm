use super::*;

pub(super) fn handle_lane_press(
    lane_number: u8,
    input: NativeInputEvent,
    clock: &ChartClock,
    engine: &mut RhythmEngine,
    counts: &mut JudgementCounts,
    judged_note_ids: &mut HashSet<u32>,
    messages: &mut VecDeque<String>,
    report: &mut PlayReport,
    options: &PlaySessionOptions,
    display: PlayDisplay,
) -> Result<(), Box<dyn Error>> {
    if lane_number >= engine.chart().lane_count() {
        return Ok(());
    }

    let lane = LaneIndex::new(lane_number);
    let input_time_seconds = options.judgement_time_seconds(clock.chart_time_at(input.event_time));
    let result = engine.submit_input(CoreInputEvent {
        key: GameKey::Lane(lane),
        pressed: true,
        time_seconds: input_time_seconds,
    });

    match result {
        Some(result) => {
            report.record_hit(result, input)?;
            record_judgement("hit", result, counts, judged_note_ids, messages, display);
        }
        None => {
            report.record_unmatched_input(lane.as_u8(), input, input_time_seconds)?;

            let input_age = input
                .queue_age_ms
                .map(|milliseconds| format!(" input_age_ms={milliseconds:.3}"))
                .unwrap_or_default();
            let source_timestamp = input
                .source_timestamp_ns
                .map(|timestamp| format!(" input_timestamp_ns={timestamp}"))
                .unwrap_or_default();

            record_message(
                format!(
                    "input lane={} song_time={input_time_seconds:.6}s unmatched{input_age}{source_timestamp}",
                    lane.as_u8()
                ),
                messages,
                display,
            );
        }
    }

    Ok(())
}

pub(super) fn record_judgement(
    kind: &str,
    result: JudgementResult,
    counts: &mut JudgementCounts,
    judged_note_ids: &mut HashSet<u32>,
    messages: &mut VecDeque<String>,
    display: PlayDisplay,
) {
    counts.add(result);
    if result.is_final() {
        judged_note_ids.insert(result.note_id.as_u32());
    }

    match display {
        PlayDisplay::Log => print_judgement(kind, result),
        PlayDisplay::AppWgpu | PlayDisplay::Highway | PlayDisplay::Sdl => {
            push_message(messages, judgement_message(kind, result))
        }
    }
}

pub(super) fn record_message(
    message: String,
    messages: &mut VecDeque<String>,
    display: PlayDisplay,
) {
    match display {
        PlayDisplay::Log => println!("{message}"),
        PlayDisplay::AppWgpu | PlayDisplay::Highway | PlayDisplay::Sdl => {
            push_message(messages, message)
        }
    }
}

fn print_judgement(kind: &str, result: JudgementResult) {
    match result.delta_seconds {
        Some(delta) => println!(
            "{kind} note={} lane={} rating={:?} scheduled={:.6}s delta_ms={:.3}",
            result.note_id.as_u32(),
            result.lane.as_u8(),
            result.rating,
            result.scheduled_time_seconds,
            delta * 1_000.0
        ),
        None => println!(
            "{kind} note={} lane={} rating={:?} scheduled={:.6}s",
            result.note_id.as_u32(),
            result.lane.as_u8(),
            result.rating,
            result.scheduled_time_seconds
        ),
    }
}
