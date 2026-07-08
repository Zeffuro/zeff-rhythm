use crate::render::bitmap_font::{
    GLYPH_SPACING, GLYPH_WIDTH, glyph_rows, text_height as font_text_height,
};
use sdl3::pixels::Color;
use sdl3::rect::Rect;
use sdl3::render::Canvas;
use sdl3::video::Window;
use std::error::Error;

pub fn draw_text(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    text: &str,
    scale: u32,
    color: Color,
) -> Result<(), Box<dyn Error>> {
    canvas.set_draw_color(color);
    let scale_i32 = scale as i32;

    for (index, character) in text.chars().enumerate() {
        let glyph_x = x + index as i32 * (GLYPH_WIDTH + GLYPH_SPACING) * scale_i32;
        draw_glyph(canvas, glyph_x, y, character, scale)?;
    }

    Ok(())
}

pub fn text_height(scale: u32) -> u32 {
    font_text_height(scale)
}

fn draw_glyph(
    canvas: &mut Canvas<Window>,
    x: i32,
    y: i32,
    character: char,
    scale: u32,
) -> Result<(), Box<dyn Error>> {
    let rows = glyph_rows(character);
    let scale_i32 = scale as i32;

    for (row_index, row) in rows.iter().copied().enumerate() {
        for column in 0..GLYPH_WIDTH {
            let mask = 1 << (GLYPH_WIDTH - 1 - column);
            if row & mask == 0 {
                continue;
            }

            canvas.fill_rect(Rect::new(
                x + column * scale_i32,
                y + row_index as i32 * scale_i32,
                scale,
                scale,
            ))?;
        }
    }

    Ok(())
}
