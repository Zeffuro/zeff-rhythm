use crate::platform::SmokeAudioClock;
use rhythm_core::{
    AudioClock, Chart, GameKey, InputEvent, JudgementWindows, LaneIndex, Note, NoteId, RhythmEngine,
};
use std::error::Error;

pub fn run(_: &[String]) -> Result<(), Box<dyn Error>> {
    let clock = SmokeAudioClock::started_now();
    let chart = smoke_chart();
    let mut engine = RhythmEngine::new(chart, JudgementWindows::default());

    let event = InputEvent {
        key: GameKey::Lane(LaneIndex::new(1)),
        pressed: true,
        time_seconds: 1.002,
    };

    let result = engine.submit_input(event);
    let now = clock.now();

    println!("native timing smoke");
    println!(
        "clock_now={now:.6}s start={:.6}s output_latency={:?}",
        clock.start_time(),
        clock.output_latency()
    );

    match result {
        Some(result) => println!(
            "judged note={} rating={:?} delta={:.3}ms",
            result.note_id.as_u32(),
            result.rating,
            result.delta_seconds.unwrap_or_default() * 1_000.0
        ),
        None => println!("input did not match a note"),
    }

    Ok(())
}

fn smoke_chart() -> Chart {
    let mut chart = Chart::new(4);
    chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(1), 1.000));
    chart
}
