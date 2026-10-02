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
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    for ink in &crate::render::text::line(text, scale).rects {
        canvas.set_draw_color(Color::RGBA(
            (u16::from(color.r) * u16::from(ink.color[0]) / 255) as u8,
            (u16::from(color.g) * u16::from(ink.color[1]) / 255) as u8,
            (u16::from(color.b) * u16::from(ink.color[2]) / 255) as u8,
            (u16::from(color.a) * u16::from(ink.color[3]) / 255) as u8,
        ));
        canvas.fill_rect(Rect::new(x + ink.x, y + ink.y, ink.width, ink.height))?;
    }

    Ok(())
}

pub fn text_height(scale: u32) -> u32 {
    9 * scale.max(1) + 2
}
