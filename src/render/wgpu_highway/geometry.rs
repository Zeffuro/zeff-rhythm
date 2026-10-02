use super::{HighwayNoteSprite, HighwayNoteSpriteKind};

pub(super) const TOP_PAD: f32 = 78.0;
const BOTTOM_PAD: f32 = 92.0;
const LANE_GAP: f32 = 8.0;
const NOTE_HEIGHT: f32 = 14.0;
const HOLD_WIDTH: f32 = 26.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WgpuHighwayLayout {
    pub(super) lane_count: usize,
    pub(super) judgement_y: f32,
    lane_width: f32,
    lane_start_x: f32,
    bottom_y: f32,
}

impl WgpuHighwayLayout {
    pub(super) fn new(width: f32, height: f32, lane_count: usize) -> Self {
        let lane_count = lane_count.max(1);
        let usable_width = (width - 128.0).max(320.0);
        let total_gap = LANE_GAP * lane_count.saturating_sub(1) as f32;
        let lane_width = ((usable_width - total_gap) / lane_count as f32).clamp(52.0, 110.0);
        let total_width = lane_width * lane_count as f32 + total_gap;
        let judgement_y = (height - BOTTOM_PAD).max(TOP_PAD + 1.0);

        Self {
            lane_count,
            judgement_y,
            lane_width,
            lane_start_x: (width - total_width) * 0.5,
            bottom_y: judgement_y + 42.0,
        }
    }

    fn lane_x(self, lane: usize) -> f32 {
        self.lane_start_x + lane as f32 * (self.lane_width + LANE_GAP)
    }
}

pub(super) fn draw_lanes(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    width: f32,
    height: f32,
    active_lanes: &[bool],
) {
    let size = (width, height);
    for lane in 0..layout.lane_count {
        let x = layout.lane_x(lane);
        let accent = lane_color(lane);
        let active = active_lanes.get(lane).copied().unwrap_or(false);
        push_rect(
            bytes,
            size,
            x,
            TOP_PAD,
            layout.lane_width,
            layout.bottom_y - TOP_PAD,
            [0.035, 0.045, 0.060, 1.0],
        );
        for edge_x in [x, x + layout.lane_width - 1.0] {
            push_rect(
                bytes,
                size,
                edge_x,
                TOP_PAD,
                1.0,
                layout.bottom_y - TOP_PAD,
                [0.10, 0.13, 0.17, 1.0],
            );
        }
        push_rect(
            bytes,
            size,
            x + 8.0,
            layout.judgement_y + 9.0,
            layout.lane_width - 16.0,
            25.0,
            if active {
                tint(accent, 0.42)
            } else {
                [0.07, 0.09, 0.12, 1.0]
            },
        );
        push_rect(
            bytes,
            size,
            x + 8.0,
            layout.judgement_y + 9.0,
            layout.lane_width - 16.0,
            3.0,
            tint(accent, if active { 1.0 } else { 0.40 }),
        );
        push_rect(
            bytes,
            size,
            x,
            layout.judgement_y - 1.0,
            layout.lane_width,
            3.0,
            [0.87, 0.92, 0.94, 1.0],
        );
    }
}

pub(super) fn draw_notes(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    width: f32,
    height: f32,
    sprites: &[HighwayNoteSprite],
) {
    let size = (width, height);
    for sprite in sprites {
        if sprite.lane < layout.lane_count
            && let HighwayNoteSpriteKind::Hold { end_y } = sprite.kind
        {
            let x = layout.lane_x(sprite.lane) + (layout.lane_width - HOLD_WIDTH) * 0.5;
            let y = end_y.min(sprite.y);
            let length = (sprite.y - end_y).abs();
            let color = lane_color(sprite.lane);
            push_note_rect(
                bytes,
                layout,
                size,
                x,
                y,
                HOLD_WIDTH,
                length,
                tint(color, 0.20),
            );
            push_note_rect(
                bytes,
                layout,
                size,
                x + 5.0,
                y,
                HOLD_WIDTH - 10.0,
                length,
                tint(color, 0.55),
            );
        }
    }

    // Hold bodies must never cover another note's head.
    for sprite in sprites {
        if sprite.lane >= layout.lane_count {
            continue;
        }
        if let HighwayNoteSpriteKind::Hold { end_y } = sprite.kind {
            let cap_width = layout.lane_width - 32.0;
            push_note_rect(
                bytes,
                layout,
                size,
                layout.lane_x(sprite.lane) + 16.0,
                end_y - 3.0,
                cap_width,
                6.0,
                lane_color(sprite.lane),
            );
        }
        draw_note_head(bytes, layout, size, sprite.lane, sprite.y);
    }
}

fn draw_note_head(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    size: (f32, f32),
    lane: usize,
    y: f32,
) {
    let x = layout.lane_x(lane) + 8.0;
    let note_width = layout.lane_width - 16.0;
    let top = y - NOTE_HEIGHT * 0.5;
    push_note_rect(
        bytes,
        layout,
        size,
        x,
        top,
        note_width,
        NOTE_HEIGHT,
        [0.015, 0.020, 0.030, 1.0],
    );
    push_note_rect(
        bytes,
        layout,
        size,
        x + 1.0,
        top + 1.0,
        note_width - 2.0,
        NOTE_HEIGHT - 3.0,
        lane_color(lane),
    );
    push_note_rect(
        bytes,
        layout,
        size,
        x + 2.0,
        top + 1.0,
        note_width - 4.0,
        2.0,
        [0.90, 0.95, 1.0, 1.0],
    );
}

#[allow(clippy::too_many_arguments)]
fn push_note_rect(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    size: (f32, f32),
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    let top = y.max(TOP_PAD);
    let bottom = (y + height).min(layout.bottom_y);
    push_rect(bytes, size, x, top, width, bottom - top, color);
}

pub(super) fn draw_progress(
    bytes: &mut Vec<u8>,
    width: f32,
    height: f32,
    song_time_seconds: f64,
    end_seconds: f64,
) {
    let progress = if end_seconds > 0.0 {
        (song_time_seconds / end_seconds).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let bar_width = (width - 48.0).max(1.0);
    for (length, color) in [
        (bar_width, [0.10, 0.13, 0.16, 1.0]),
        (bar_width * progress, [0.32, 0.70, 0.66, 1.0]),
    ] {
        push_rect(
            bytes,
            (width, height),
            24.0,
            height - 46.0,
            length,
            4.0,
            color,
        );
    }
}

pub(super) fn push_rect(
    bytes: &mut Vec<u8>,
    surface_size: (f32, f32),
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let (surface_width, surface_height) = surface_size;
    let x0 = pixel_x_to_ndc(x, surface_width);
    let x1 = pixel_x_to_ndc(x + width, surface_width);
    let y0 = pixel_y_to_ndc(y, surface_height);
    let y1 = pixel_y_to_ndc(y + height, surface_height);
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)] {
        for value in [x, y, color[0], color[1], color[2], color[3]] {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
}

fn pixel_x_to_ndc(x: f32, width: f32) -> f32 {
    x / width * 2.0 - 1.0
}

fn pixel_y_to_ndc(y: f32, height: f32) -> f32 {
    1.0 - y / height * 2.0
}

fn lane_color(lane: usize) -> [f32; 4] {
    [
        [0.28, 0.78, 1.00, 1.0],
        [0.72, 0.53, 1.00, 1.0],
        [1.00, 0.70, 0.28, 1.0],
        [0.30, 0.88, 0.67, 1.0],
    ][lane % 4]
}

fn tint(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

#[cfg(test)]
mod tests;
