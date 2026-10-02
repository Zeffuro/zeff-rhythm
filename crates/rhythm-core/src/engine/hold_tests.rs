use super::*;
use crate::{Note, NoteId, ReplayEvent};

fn engine() -> RhythmEngine {
    let mut chart = Chart::new(4);
    chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
    RhythmEngine::new(chart, JudgementWindows::default())
}

fn input(engine: &mut RhythmEngine, lane: u8, pressed: bool, time: f64) -> Option<JudgementResult> {
    engine.submit_input(InputEvent {
        key: GameKey::Lane(LaneIndex::new(lane)),
        pressed,
        time_seconds: time,
    })
}

#[test]
fn hold_head_is_not_final_and_held_tail_scores_once() {
    let mut engine = engine();
    let head = input(&mut engine, 0, true, 1.025).unwrap();
    assert_eq!(head.phase, JudgementPhase::HoldHead);
    assert_eq!(head.rating, HitRating::Perfect);
    assert!(!head.is_final());
    assert_eq!(engine.judged_count(), 0);
    let mut results = Vec::new();
    engine.collect_judgements(1.9, &mut results);
    assert!(results.is_empty());
    assert!(!engine.is_complete());
    engine.collect_judgements(2.0, &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].phase, JudgementPhase::HoldTail);
    assert_eq!(results[0].rating, HitRating::Perfect);
    assert!(results[0].is_final());
    assert!(engine.is_complete());
    engine.collect_judgements(9.0, &mut results);
    assert_eq!(results.len(), 1);
    assert!(input(&mut engine, 0, false, 9.0).is_none());
    assert_eq!(
        engine
            .replay()
            .events()
            .iter()
            .filter(|e| matches!(e, ReplayEvent::Judgement(_)))
            .count(),
        2
    );
}

#[test]
fn early_release_breaks_hold_and_cannot_be_repaired() {
    let mut engine = engine();
    input(&mut engine, 0, true, 1.0).unwrap();
    let result = input(&mut engine, 0, false, 1.5).unwrap();
    assert_eq!(result.phase, JudgementPhase::HoldTail);
    assert_eq!(result.rating, HitRating::Miss);
    assert_eq!(result.input_time_seconds, Some(1.5));
    assert!(!engine.lane_is_pressed(LaneIndex::new(0)));
    assert!(input(&mut engine, 0, true, 1.6).is_none());
    let mut results = Vec::new();
    engine.collect_judgements(3.0, &mut results);
    assert!(results.is_empty());
    assert_eq!(engine.judged_count(), 1);
}

#[test]
fn release_grace_is_bounded_by_perfect_window() {
    for (release, expected) in [
        (1.954, HitRating::Miss),
        (1.956, HitRating::Marvelous),
        (2.1, HitRating::Marvelous),
    ] {
        let mut engine = engine();
        input(&mut engine, 0, true, 1.0).unwrap();
        assert_eq!(
            input(&mut engine, 0, false, release).unwrap().rating,
            expected
        );
        assert!(engine.is_complete());
    }
}

#[test]
fn missing_head_is_one_final_miss_and_late_press_cannot_revive_it() {
    let mut engine = engine();
    let mut results = Vec::new();
    engine.collect_judgements(1.181, &mut results);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].phase, JudgementPhase::HoldHead);
    assert!(results[0].is_final());
    assert!(input(&mut engine, 0, true, 1.2).is_none());
    engine.collect_judgements(3.0, &mut results);
    assert_eq!(results.len(), 1);
}

#[test]
fn repeat_press_does_not_start_another_note_under_active_hold() {
    let mut engine = engine();
    input(&mut engine, 0, true, 1.0).unwrap();
    assert!(input(&mut engine, 0, true, 1.02).is_none());
    assert!(input(&mut engine, 1, false, 1.5).is_none());
    assert!(engine.lane_is_pressed(LaneIndex::new(0)));
    assert!(input(&mut engine, 250, false, 1.5).is_none());
    assert!(input(&mut engine, 0, false, f64::NAN).is_none());
    assert_eq!(
        input(&mut engine, 0, false, 2.0).unwrap().rating,
        HitRating::Marvelous
    );
}

#[test]
fn short_hold_cannot_be_started_after_its_tail() {
    let mut chart = Chart::new(1);
    chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 1.05));
    let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
    assert!(input(&mut engine, 0, true, 1.06).is_none());
}

#[test]
fn simultaneous_holds_and_tap_have_independent_final_scores() {
    let mut chart = Chart::new(4);
    chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 3.0));
    chart.push_note(Note::hold(NoteId::new(2), LaneIndex::new(1), 1.0, 2.0));
    chart.push_note(Note::tap(NoteId::new(3), LaneIndex::new(2), 1.0));
    let mut engine = RhythmEngine::new(chart, JudgementWindows::default());
    input(&mut engine, 0, true, 1.0).unwrap();
    input(&mut engine, 1, true, 1.0).unwrap();
    assert!(input(&mut engine, 2, true, 1.0).unwrap().is_final());
    assert_eq!(engine.judged_count(), 1);
    let mut results = Vec::new();
    engine.collect_judgements(2.0, &mut results);
    assert_eq!(engine.judged_count(), 2);
    assert_eq!(results[0].note_id, NoteId::new(2));
    engine.collect_judgements(3.0, &mut results);
    assert!(engine.is_complete());
    assert_eq!(results.len(), 2);
}
