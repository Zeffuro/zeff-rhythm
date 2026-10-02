use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;

const CACHE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_LINES: usize = 256;
const FIT_CACHE_BYTES: usize = 256 * 1024;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_GRAPHEMES: usize = 512;

#[derive(Clone, Copy, Debug)]
pub struct InkRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}

pub struct TextLine {
    pub rects: Vec<InkRect>,
    pub width: f32,
    pub missing_glyphs: usize,
}

struct CachedLine {
    text: String,
    scale: u32,
    line: Arc<TextLine>,
    bytes: usize,
}

struct CachedFit {
    text: String,
    scale: u32,
    width: u32,
    output: String,
    bytes: usize,
}

struct TextSystem {
    fonts: FontSystem,
    swash: SwashCache,
    cache: VecDeque<CachedLine>,
    bytes: usize,
    fits: VecDeque<CachedFit>,
    fit_bytes: usize,
}

impl TextSystem {
    fn new() -> Self {
        let mut fonts = FontSystem::new();
        #[cfg(windows)]
        fonts.db_mut().set_sans_serif_family("Segoe UI");
        Self {
            fonts,
            swash: SwashCache::new(),
            cache: VecDeque::new(),
            bytes: 0,
            fits: VecDeque::new(),
            fit_bytes: 0,
        }
    }

    fn shape(&mut self, text: &str, scale: u32) -> Buffer {
        let size = 9.0 * scale as f32;
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size + 2.0));
        buffer.set_wrap(Wrap::None);
        buffer.set_text(
            text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
    }

    fn measure(&mut self, text: &str, scale: u32) -> f32 {
        buffer_width(&self.shape(text, scale))
    }

    fn fit(&mut self, text: String, max_width: f32, scale: u32) -> String {
        let width = max_width.to_bits();
        if let Some(index) = self
            .fits
            .iter()
            .position(|entry| entry.text == text && entry.scale == scale && entry.width == width)
        {
            let entry = self.fits.remove(index).unwrap();
            let output = entry.output.clone();
            self.fits.push_back(entry);
            return output;
        }
        let output = if self.measure(&text, scale) <= max_width {
            text.clone()
        } else if self.measure("…", scale) > max_width {
            String::new()
        } else {
            let boundaries: Vec<usize> = text
                .grapheme_indices(true)
                .map(|(index, _)| index)
                .chain(std::iter::once(text.len()))
                .collect();
            let (mut low, mut high) = (0, boundaries.len() - 1);
            while low < high {
                let middle = (low + high + 1) / 2;
                let candidate = format!("{}…", &text[..boundaries[middle]]);
                if self.measure(&candidate, scale) <= max_width {
                    low = middle;
                } else {
                    high = middle - 1;
                }
            }
            format!("{}…", &text[..boundaries[low]])
        };
        let bytes = text.capacity() + output.capacity();
        while self.fit_bytes + bytes > FIT_CACHE_BYTES || self.fits.len() >= CACHE_LINES {
            if let Some(old) = self.fits.pop_front() {
                self.fit_bytes -= old.bytes;
            }
        }
        self.fit_bytes += bytes;
        self.fits.push_back(CachedFit {
            text,
            scale,
            width,
            output: output.clone(),
            bytes,
        });
        output
    }

    fn line(&mut self, text: &str, scale: u32) -> Arc<TextLine> {
        if let Some(index) = self
            .cache
            .iter()
            .position(|entry| entry.text == text && entry.scale == scale)
        {
            let entry = self.cache.remove(index).unwrap();
            let line = Arc::clone(&entry.line);
            self.cache.push_back(entry);
            return line;
        }
        let mut buffer = self.shape(text, scale);
        let width = buffer_width(&buffer);
        let missing_glyphs = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs)
            .filter(|glyph| glyph.glyph_id == 0)
            .count();
        let mut rects: Vec<InkRect> = Vec::new();
        buffer.draw(
            &mut self.fonts,
            &mut self.swash,
            Color::rgb(255, 255, 255),
            |x, y, width, height, color| {
                let color = color.as_rgba();
                if color[3] == 0 {
                    return;
                }
                if let Some(last) = rects.last_mut() {
                    if last.y == y
                        && last.height == height
                        && last.x + last.width as i32 == x
                        && last.color == color
                    {
                        last.width += width;
                        return;
                    }
                }
                rects.push(InkRect {
                    x,
                    y,
                    width,
                    height,
                    color,
                });
            },
        );
        let bytes = rects.capacity() * std::mem::size_of::<InkRect>() + text.len();
        let line = Arc::new(TextLine {
            rects,
            width,
            missing_glyphs,
        });
        if bytes <= CACHE_BYTES {
            while self.bytes + bytes > CACHE_BYTES || self.cache.len() >= CACHE_LINES {
                if let Some(old) = self.cache.pop_front() {
                    self.bytes -= old.bytes;
                }
            }
            self.bytes += bytes;
            self.cache.push_back(CachedLine {
                text: text.to_owned(),
                scale,
                line: Arc::clone(&line),
                bytes,
            });
        }
        if self.swash.image_cache.len() > 4096 {
            self.swash = SwashCache::new();
        }
        line
    }
}

fn buffer_width(buffer: &Buffer) -> f32 {
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max)
}

fn normalize(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = end < text.len();
    let mut parts: Vec<&str> = text[..end]
        .graphemes(true)
        .take(MAX_GRAPHEMES + 1)
        .collect();
    if truncated {
        parts.pop();
    }
    if parts.len() > MAX_GRAPHEMES {
        truncated = true;
        parts.truncate(MAX_GRAPHEMES);
    }
    let mut output: String = parts
        .into_iter()
        .map(|part| {
            if part.chars().any(char::is_control) {
                " "
            } else {
                part
            }
        })
        .collect();
    if truncated {
        output.push('…');
    }
    output
}

fn system() -> &'static Mutex<TextSystem> {
    static SYSTEM: OnceLock<Mutex<TextSystem>> = OnceLock::new();
    SYSTEM.get_or_init(|| Mutex::new(TextSystem::new()))
}

pub fn line(text: &str, scale: u32) -> Arc<TextLine> {
    system()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .line(&normalize(text), scale.clamp(1, 8))
}

pub fn fit_text(text: &str, max_width: f32, scale: u32) -> String {
    if max_width <= 0.0 || max_width.is_nan() {
        return String::new();
    }
    system()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .fit(normalize(text), max_width, scale.clamp(1, 8))
}

pub fn ink_color(ink: [u8; 4], color: [f32; 4]) -> [f32; 4] {
    [
        color[0] * f32::from(ink[0]) / 255.0,
        color[1] * f32::from(ink[1]) / 255.0,
        color[2] * f32::from(ink[2]) / 255.0,
        color[3] * f32::from(ink[3]) / 255.0,
    ]
}

#[cfg(test)]
mod tests;
