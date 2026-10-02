use super::*;

impl NativeAppShell {
    pub(super) fn refresh_library(&mut self) {
        if let Some(library) = self.library_scanner.poll() {
            let selected = self
                .library
                .get(self.library_index)
                .map(|entry| entry.chart_selection());
            self.library_index = library.selection_index(selected.as_ref());
            self.library = library;
            self.library_launch_error = None;
        }
    }

    pub(super) fn draw_song_select(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 2)?;
        let status = if self.library_scanner.is_scanning() {
            "SCANNING LIBRARY...".to_owned()
        } else {
            format!(
                "{} READY / {} OTHER MODES / {} ISSUES",
                self.library.playable_count(),
                self.library.skipped_modes,
                self.library.problem_count()
            )
        };
        draw_text(canvas, 64, 92, &status, 2, MUTED_TEXT)?;
        let rows = (height.saturating_sub(330) / 76).max(1) as usize;
        let range = visible_range(self.library_index, self.library.entries().len(), rows);
        let max_chars = (width.saturating_sub(160) / 12).max(1) as usize;
        for (row, index) in range.enumerate() {
            let entry = &self.library.entries()[index];
            let y = 132 + row as i32 * 76;
            canvas.set_draw_color(if index == self.library_index {
                Color::RGB(43, 71, 92)
            } else {
                Color::RGB(26, 33, 43)
            });
            canvas.fill_rect(Rect::new(64, y, width.saturating_sub(128).max(1), 66))?;
            draw_text(
                canvas,
                80,
                y + 8,
                &compact_text(&entry.title, max_chars),
                2,
                if entry.is_available() {
                    TEXT
                } else {
                    MUTED_TEXT
                },
            )?;
            draw_text(
                canvas,
                80,
                y + 40,
                &compact_text(&entry.subtitle, max_chars),
                2,
                MUTED_TEXT,
            )?;
        }
        if let Some(entry) = self.library.get(self.library_index) {
            let status = self
                .library_launch_error
                .as_deref()
                .unwrap_or_else(|| entry.status());
            draw_text(
                canvas,
                64,
                height as i32 - 164,
                &compact_text(status, max_chars),
                2,
                TEXT,
            )?;
            draw_text(
                canvas,
                64,
                height as i32 - 132,
                &compact_text(&entry.chart_path.display().to_string(), max_chars),
                2,
                MUTED_TEXT,
            )?;
        } else if !self.library_scanner.is_scanning() {
            let message = self
                .library
                .records
                .iter()
                .find(|record| record.status == "error")
                .map(|record| record.detail.as_str())
                .unwrap_or("NO MANIA OR SM CHARTS FOUND");
            draw_text(canvas, 64, 156, &compact_text(message, max_chars), 2, TEXT)?;
        }
        draw_text(
            canvas,
            64,
            height as i32 - 96,
            "R RESCAN / DROP SONGS FOLDER / PGUP PGDN",
            2,
            MUTED_TEXT,
        )?;
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }
}

fn compact_text(value: &str, max_chars: usize) -> String {
    crate::render::text::fit_text(value, max_chars as f32 * 12.0, 2)
}
