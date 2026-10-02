use super::*;
use crate::render::highway::build_highway_note_sprites;
use crate::render::wgpu_highway::wgpu_highway_render_layout;
use rhythm_core::{Chart, LaneIndex, Note, NoteId};
use std::collections::HashSet;

struct PaintedRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
}

fn painted_rects(bytes: &[u8], width: f32, height: f32) -> Vec<PaintedRect> {
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|value| f32::from_ne_bytes(value.try_into().unwrap()))
        .collect();
    values
        .chunks_exact(36)
        .map(|rect| {
            let x = (rect[0] + 1.0) * width * 0.5;
            let y = (1.0 - rect[1]) * height * 0.5;
            PaintedRect {
                x,
                y,
                width: (rect[6] + 1.0) * width * 0.5 - x,
                height: (1.0 - rect[13]) * height * 0.5 - y,
                color: rect[2..6].try_into().unwrap(),
            }
        })
        .collect()
}

fn sprite(lane: usize, y: f32, kind: HighwayNoteSpriteKind) -> HighwayNoteSprite {
    HighwayNoteSprite {
        note_id: 1,
        lane,
        y,
        delta_seconds: 0.0,
        kind,
    }
}

#[test]
fn converts_pixels_to_ndc() {
    assert_eq!(pixel_x_to_ndc(0.0, 100.0), -1.0);
    assert_eq!(pixel_x_to_ndc(100.0, 100.0), 1.0);
    assert_eq!(pixel_y_to_ndc(0.0, 100.0), 1.0);
    assert_eq!(pixel_y_to_ndc(100.0, 100.0), -1.0);
}

#[test]
fn dense_stream_heads_separate_with_one_second_travel() {
    let mut chart = Chart::new(4);
    for index in 0..10 {
        chart.push_note(Note::tap(
            NoteId::new(index),
            LaneIndex::new(0),
            10.0 + f64::from(index) * 0.1,
        ));
    }
    for (width, height) in [(760, 520), (960, 640)] {
        let normal = wgpu_highway_render_layout(width, height, 4, 1.0);
        let crowded = wgpu_highway_render_layout(width, height, 4, 4.0);
        let normal_sprites = build_highway_note_sprites(normal, &chart, &HashSet::new(), 10.0);
        let crowded_sprites = build_highway_note_sprites(crowded, &chart, &HashSet::new(), 10.0);
        assert_eq!(normal_sprites.len(), 10);
        assert_eq!(crowded_sprites.len(), 10);
        for pair in normal_sprites.windows(2) {
            assert!(pair[0].y - pair[1].y > NOTE_HEIGHT);
        }
        for pair in crowded_sprites.windows(2) {
            assert!(pair[0].y - pair[1].y < NOTE_HEIGHT);
        }
        assert_eq!(normal.y_for_time(10.0, 10.0), crowded.judgement_y);
        assert!(normal.top_y >= 66.0);
    }
}

#[test]
fn clips_note_heads_and_hold_bodies_to_the_playfield() {
    for (width, height) in [(760.0, 520.0), (960.0, 640.0)] {
        let layout = WgpuHighwayLayout::new(width, height, 4);
        let sprites = [
            sprite(0, TOP_PAD, HighwayNoteSpriteKind::Tap),
            sprite(1, layout.bottom_y, HighwayNoteSpriteKind::Tap),
            sprite(
                2,
                layout.judgement_y,
                HighwayNoteSpriteKind::Hold { end_y: -500.0 },
            ),
            sprite(3, -500.0, HighwayNoteSpriteKind::Tap),
            sprite(3, height + 500.0, HighwayNoteSpriteKind::Tap),
        ];
        let mut bytes = Vec::new();
        draw_notes(&mut bytes, &layout, width, height, &sprites);
        let rects = painted_rects(&bytes, width, height);
        assert!(!rects.is_empty());
        assert!(rects.iter().any(|rect| (rect.y - TOP_PAD).abs() < 0.001));
        assert!(
            rects
                .iter()
                .any(|rect| { (rect.y + rect.height - layout.bottom_y).abs() < 0.001 })
        );
        for rect in rects {
            assert!(rect.y >= TOP_PAD - 0.001);
            assert!(rect.y + rect.height <= layout.bottom_y + 0.001);
            assert!(rect.x >= layout.lane_start_x - 0.001);
            assert!(rect.x + rect.width <= width - layout.lane_start_x + 0.001);
        }
    }
}

#[test]
fn later_hold_body_cannot_obscure_another_note_head() {
    let (width, height) = (960.0, 640.0);
    let layout = WgpuHighwayLayout::new(width, height, 4);
    let sprites = [
        sprite(0, 200.0, HighwayNoteSpriteKind::Tap),
        sprite(0, 300.0, HighwayNoteSpriteKind::Hold { end_y: 100.0 }),
    ];
    let mut bytes = Vec::new();
    draw_notes(&mut bytes, &layout, width, height, &sprites);
    let sample_x = layout.lane_x(0) + layout.lane_width * 0.5;
    let sample_y = 201.0;
    let last_paint = painted_rects(&bytes, width, height)
        .into_iter()
        .filter(|rect| {
            (rect.x..=rect.x + rect.width).contains(&sample_x)
                && (rect.y..=rect.y + rect.height).contains(&sample_y)
        })
        .last()
        .unwrap();
    assert_eq!(last_paint.color, lane_color(0));
}

#[test]
fn offscreen_hold_tail_does_not_draw_a_false_cap_at_the_top() {
    let (width, height) = (960.0, 640.0);
    let layout = WgpuHighwayLayout::new(width, height, 4);
    let sprites = [sprite(
        0,
        layout.judgement_y,
        HighwayNoteSpriteKind::Hold {
            end_y: TOP_PAD - 200.0,
        },
    )];
    let mut bytes = Vec::new();
    draw_notes(&mut bytes, &layout, width, height, &sprites);
    let top_rects: Vec<_> = painted_rects(&bytes, width, height)
        .into_iter()
        .filter(|rect| (rect.y - TOP_PAD).abs() < 0.001)
        .collect();
    assert!(!top_rects.is_empty());
    assert!(
        top_rects
            .iter()
            .all(|rect| rect.width <= HOLD_WIDTH + 0.001)
    );
}

#[test]
fn pressing_a_lane_changes_only_the_receptor() {
    let (width, height) = (960.0, 640.0);
    let layout = WgpuHighwayLayout::new(width, height, 4);
    let mut released = Vec::new();
    let mut pressed = Vec::new();
    draw_lanes(&mut released, &layout, width, height, &[false; 4]);
    draw_lanes(&mut pressed, &layout, width, height, &[true; 4]);
    let released = painted_rects(&released, width, height);
    let pressed = painted_rects(&pressed, width, height);
    assert_eq!(released.len(), pressed.len());
    let changed: Vec<_> = released
        .iter()
        .zip(&pressed)
        .filter(|(left, right)| left.color != right.color)
        .collect();
    assert!(!changed.is_empty());
    assert!(changed.iter().all(|(_, rect)| {
        rect.y > layout.judgement_y && rect.y + rect.height <= layout.bottom_y
    }));
}

#[test]
fn progress_bar_remains_above_the_gameplay_footer() {
    for (width, height) in [(760.0, 520.0), (960.0, 640.0)] {
        let mut bytes = Vec::new();
        draw_progress(&mut bytes, width, height, 1.0, 2.0);
        let rects = painted_rects(&bytes, width, height);
        assert_eq!(rects.len(), 2);
        assert!(
            rects
                .iter()
                .all(|rect| rect.y + rect.height <= height - 40.0)
        );
        assert!((rects[1].width * 2.0 - rects[0].width).abs() < 0.001);
    }
}
