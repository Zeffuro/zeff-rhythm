use rhythm_core::{Chart, NoteKind};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighwayRenderLayout {
    pub lane_count: usize,
    pub lookahead_seconds: f64,
    pub late_visibility_seconds: f64,
    pub top_y: f32,
    pub judgement_y: f32,
}

impl HighwayRenderLayout {
    pub fn new(
        lane_count: usize,
        lookahead_seconds: f64,
        late_visibility_seconds: f64,
        top_y: f32,
        judgement_y: f32,
    ) -> Self {
        Self {
            lane_count: lane_count.max(1),
            lookahead_seconds: lookahead_seconds.max(0.001),
            late_visibility_seconds: late_visibility_seconds.max(0.0),
            top_y,
            judgement_y,
        }
    }

    pub fn y_for_time(self, note_time_seconds: f64, song_time_seconds: f64) -> f32 {
        let delta_seconds = note_time_seconds - song_time_seconds;
        let travel = (self.judgement_y - self.top_y).max(1.0);
        self.judgement_y - (delta_seconds / self.lookahead_seconds) as f32 * travel
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighwayNoteSprite {
    pub note_id: u32,
    pub lane: usize,
    pub y: f32,
    pub delta_seconds: f64,
    pub kind: HighwayNoteSpriteKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HighwayNoteSpriteKind {
    Tap,
    Hold { end_y: f32 },
}

pub fn build_highway_note_sprites(
    layout: HighwayRenderLayout,
    chart: &Chart,
    judged_note_ids: &HashSet<u32>,
    song_time_seconds: f64,
) -> Vec<HighwayNoteSprite> {
    let visible_start = song_time_seconds - layout.late_visibility_seconds;
    let visible_end = song_time_seconds + layout.lookahead_seconds;
    let mut sprites = Vec::new();

    for note in chart.notes() {
        if judged_note_ids.contains(&note.id.as_u32()) {
            continue;
        }

        let lane = note.lane.as_usize();
        if lane >= layout.lane_count {
            continue;
        }

        match note.kind {
            NoteKind::Tap => {
                if note.time_seconds < visible_start || note.time_seconds > visible_end {
                    continue;
                }

                sprites.push(HighwayNoteSprite {
                    note_id: note.id.as_u32(),
                    lane,
                    y: layout.y_for_time(note.time_seconds, song_time_seconds),
                    delta_seconds: note.time_seconds - song_time_seconds,
                    kind: HighwayNoteSpriteKind::Tap,
                });
            }
            NoteKind::Hold { end_time_seconds } => {
                if end_time_seconds < visible_start || note.time_seconds > visible_end {
                    continue;
                }

                sprites.push(HighwayNoteSprite {
                    note_id: note.id.as_u32(),
                    lane,
                    y: layout.y_for_time(note.time_seconds, song_time_seconds),
                    delta_seconds: note.time_seconds - song_time_seconds,
                    kind: HighwayNoteSpriteKind::Hold {
                        end_y: layout.y_for_time(end_time_seconds, song_time_seconds),
                    },
                });
            }
        }
    }

    sprites
}

#[cfg(test)]
mod tests {
    use super::{HighwayNoteSpriteKind, HighwayRenderLayout, build_highway_note_sprites};
    use rhythm_core::{Chart, LaneIndex, Note, NoteId};
    use std::collections::HashSet;

    #[test]
    fn produces_visible_tap_sprites_by_song_time() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(2), 1.0));
        chart.push_note(Note::tap(NoteId::new(2), LaneIndex::new(1), 9.0));
        let layout = HighwayRenderLayout::new(4, 4.0, 0.180, 100.0, 500.0);

        let sprites = build_highway_note_sprites(layout, &chart, &HashSet::new(), 0.0);

        assert_eq!(sprites.len(), 1);
        assert_eq!(sprites[0].note_id, 1);
        assert_eq!(sprites[0].lane, 2);
        assert_eq!(sprites[0].kind, HighwayNoteSpriteKind::Tap);
        assert_eq!(sprites[0].y, 400.0);
    }

    #[test]
    fn skips_judged_notes() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::tap(NoteId::new(1), LaneIndex::new(0), 1.0));
        let layout = HighwayRenderLayout::new(4, 4.0, 0.180, 100.0, 500.0);
        let judged = HashSet::from([1]);

        let sprites = build_highway_note_sprites(layout, &chart, &judged, 0.0);

        assert!(sprites.is_empty());
    }

    #[test]
    fn includes_hold_end_position() {
        let mut chart = Chart::new(4);
        chart.push_note(Note::hold(NoteId::new(1), LaneIndex::new(0), 1.0, 2.0));
        let layout = HighwayRenderLayout::new(4, 4.0, 0.180, 100.0, 500.0);

        let sprites = build_highway_note_sprites(layout, &chart, &HashSet::new(), 0.0);

        assert_eq!(sprites.len(), 1);
        assert_eq!(
            sprites[0].kind,
            HighwayNoteSpriteKind::Hold { end_y: 300.0 }
        );
    }
}
