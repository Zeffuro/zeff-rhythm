use super::*;

impl NativeAppShell {
    pub(super) fn render(&mut self, canvas: &mut Canvas<Window>) -> Result<(), Box<dyn Error>> {
        let (width, height) = canvas.output_size().unwrap_or((WIDTH, HEIGHT));
        let width = width.max(640);
        let height = height.max(420);

        self.update_title(canvas)?;
        canvas.set_draw_color(Color::RGB(15, 18, 24));
        canvas.clear();

        match self.state.screen {
            AppScreen::MainMenu => self.draw_main_menu(canvas, width, height)?,
            AppScreen::Settings(panel) => self.draw_settings(canvas, width, height, panel)?,
            AppScreen::SongSelect => self.draw_song_select(canvas, width, height)?,
            AppScreen::Calibration => self.draw_calibration(canvas, width, height)?,
            AppScreen::Diagnostics => {
                self.draw_settings(canvas, width, height, SettingsPanel::Diagnostics)?
            }
            AppScreen::Gameplay => self.draw_gameplay_placeholder(canvas, width, height)?,
            AppScreen::Results => self.draw_results(canvas, width, height)?,
        }

        canvas.present();
        Ok(())
    }

    fn update_title(&mut self, canvas: &mut Canvas<Window>) -> Result<(), Box<dyn Error>> {
        if self.last_title_update.elapsed() < Duration::from_millis(250) {
            return Ok(());
        }

        let title = format!(
            "zeff-rhythm | {} | selected {}",
            screen_label(self.state.screen),
            selected_label(self),
        );
        canvas.window_mut().set_title(&title)?;
        self.last_title_update = Instant::now();
        Ok(())
    }

    fn draw_main_menu(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 0)?;
        let item_height = ((height - 180) / MENU_ITEMS.len() as u32).clamp(44, 78);
        let start_y = 120_i32;
        let item_width = (width - 160).min(640);
        let x = ((width - item_width) / 2) as i32;

        for (index, item) in MENU_ITEMS.iter().enumerate() {
            let y = start_y + index as i32 * (item_height as i32 + 14);
            let selected = index == self.menu_index;
            draw_menu_row(
                canvas,
                x,
                y,
                item_width,
                item_height,
                selected,
                item.color(),
                item.display_label(),
            )?;
        }

        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_settings(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
        panel: SettingsPanel,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 1)?;
        let panels = [
            SettingsPanel::Audio,
            SettingsPanel::Input,
            SettingsPanel::Video,
            SettingsPanel::Gameplay,
            SettingsPanel::Diagnostics,
        ];
        let panel_width = ((width - 96) / panels.len() as u32).max(80);

        for (index, candidate) in panels.iter().copied().enumerate() {
            let selected = candidate == panel;
            let x = 48 + index as i32 * panel_width as i32;
            draw_menu_row(
                canvas,
                x,
                120,
                panel_width.saturating_sub(12),
                74,
                selected,
                panel_color(candidate),
                panel_display_label(candidate),
            )?;
        }

        let body_height = height.saturating_sub(270).max(80);
        canvas.set_draw_color(Color::RGB(34, 39, 48));
        canvas.fill_rect(Rect::new(64, 240, width - 128, body_height))?;
        draw_settings_body(canvas, &self.state, panel, width, self.settings_row_index)?;
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_calibration(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 3)?;
        canvas.set_draw_color(Color::RGB(40, 44, 53));
        canvas.fill_rect(Rect::new(96, 130, width - 192, height - 250))?;
        draw_text(canvas, 124, 156, "CALIBRATION", 3, TEXT)?;
        draw_text(
            canvas,
            124,
            210,
            "GENERATED TIMING TEST PENDING",
            2,
            MUTED_TEXT,
        )?;
        canvas.set_draw_color(Color::RGB(238, 181, 81));
        for index in 0..4 {
            let x = 130 + index * 92;
            canvas.fill_rect(Rect::new(x, height as i32 / 2, 56, 56))?;
        }
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_gameplay_placeholder(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 4)?;
        if self.pending_session_options.is_some() {
            canvas.set_draw_color(Color::RGB(126, 206, 170));
            canvas.fill_rect(Rect::new(96, 92, width - 192, 12))?;
            draw_text(canvas, 96, 84, "SESSION READY", 2, TEXT)?;
            if let Some(summary) = self.pending_session_summary.as_ref() {
                draw_text(canvas, 96, 118, &summary.title, 3, TEXT)?;
                draw_text(
                    canvas,
                    96,
                    154,
                    &format!(
                        "{} LANES  {} NOTES  {} HOLDS",
                        summary.lane_count, summary.note_count, summary.hold_count
                    ),
                    2,
                    MUTED_TEXT,
                )?;
                draw_text(
                    canvas,
                    96,
                    184,
                    &format!("AUDIO {:.3} S", summary.audio_duration_seconds),
                    2,
                    MUTED_TEXT,
                )?;
            }
            if let Some(preview) = self.pending_session_preview.as_ref() {
                let lookahead_seconds = self
                    .pending_session_options
                    .as_ref()
                    .map(|options| options.lookahead_seconds)
                    .unwrap_or(4.0);
                draw_gameplay_preview(canvas, width, height, preview, lookahead_seconds)?;
            } else {
                draw_empty_lanes(canvas, width, height, 4)?;
            }
            draw_text(canvas, 96, 222, "WGPU CHART PREVIEW READY", 2, MUTED_TEXT)?;
            draw_text(canvas, 96, 252, "P LIVE SDL HARNESS", 2, MUTED_TEXT)?;
        } else {
            draw_empty_lanes(canvas, width, height, 4)?;
            draw_text(canvas, 96, 84, "NO SESSION", 2, MUTED_TEXT)?;
        }
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }

    fn draw_results(
        &self,
        canvas: &mut Canvas<Window>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        draw_header(canvas, width, 5)?;
        canvas.set_draw_color(Color::RGB(45, 55, 67));
        canvas.fill_rect(Rect::new(96, 140, width - 192, 64))?;
        canvas.fill_rect(Rect::new(96, 230, width - 300, 42))?;
        canvas.fill_rect(Rect::new(96, 296, width - 380, 42))?;
        draw_text(canvas, 124, 160, "RESULTS", 3, TEXT)?;
        draw_text(canvas, 124, 244, "LATEST RUN PENDING", 2, MUTED_TEXT)?;
        draw_footer(
            canvas,
            width,
            height,
            self.state.settings.input.input_offset_ms,
        )?;
        Ok(())
    }
}

fn draw_gameplay_preview(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    preview: &PlaySessionPreview,
    lookahead_seconds: f64,
) -> Result<(), Box<dyn Error>> {
    let layout = AppHighwayLayout::new(width, height, preview.lane_count as usize);
    draw_lanes(canvas, &layout)?;

    let render_layout = HighwayRenderLayout::new(
        layout.lane_count,
        lookahead_seconds,
        0.180,
        layout.top_y as f32,
        layout.judgement_y as f32,
    );
    let judged = HashSet::new();
    let sprites = build_highway_note_sprites(
        render_layout,
        &preview.chart,
        &judged,
        preview.chart_start_seconds,
    );

    for sprite in sprites {
        match sprite.kind {
            HighwayNoteSpriteKind::Tap => {
                canvas.set_draw_color(preview_note_color(sprite.delta_seconds));
                canvas.fill_rect(preview_note_rect(
                    &layout,
                    sprite.lane,
                    sprite.y.round() as i32,
                    14,
                ))?;
            }
            HighwayNoteSpriteKind::Hold { end_y } => {
                let start_y = sprite.y.round() as i32;
                let end_y = end_y.round() as i32;
                let first_y = end_y
                    .min(start_y)
                    .clamp(layout.top_y, layout.height as i32 - 120);
                let last_y = end_y
                    .max(start_y)
                    .clamp(layout.top_y, layout.height as i32 - 120);
                let height = (last_y - first_y).max(8) as u32;
                let x = layout.lane_x(sprite.lane) + layout.lane_width as i32 / 2 - 8;

                canvas.set_draw_color(Color::RGB(60, 116, 190));
                canvas.fill_rect(Rect::new(x, first_y, 16, height))?;
                canvas.set_draw_color(preview_note_color(sprite.delta_seconds));
                canvas.fill_rect(preview_note_rect(&layout, sprite.lane, start_y, 14))?;
            }
        }
    }

    Ok(())
}

fn draw_empty_lanes(
    canvas: &mut Canvas<Window>,
    width: u32,
    height: u32,
    lane_count: usize,
) -> Result<(), Box<dyn Error>> {
    let layout = AppHighwayLayout::new(width, height, lane_count);
    draw_lanes(canvas, &layout)
}

fn draw_lanes(
    canvas: &mut Canvas<Window>,
    layout: &AppHighwayLayout,
) -> Result<(), Box<dyn Error>> {
    for lane in 0..layout.lane_count {
        let x = layout.lane_x(lane);
        canvas.set_draw_color(Color::RGB(26, 31, 38));
        canvas.fill_rect(Rect::new(
            x,
            layout.top_y,
            layout.lane_width,
            layout.judgement_y.saturating_sub(layout.top_y) as u32 + 44,
        ))?;
        canvas.set_draw_color(Color::RGB(56, 64, 74));
        canvas.draw_rect(Rect::new(
            x,
            layout.top_y,
            layout.lane_width,
            layout.judgement_y.saturating_sub(layout.top_y) as u32 + 44,
        ))?;
        canvas.set_draw_color(Color::RGB(230, 226, 210));
        canvas.fill_rect(Rect::new(
            x + 8,
            layout.judgement_y,
            layout.lane_width.saturating_sub(16),
            8,
        ))?;
    }

    Ok(())
}

struct AppHighwayLayout {
    height: u32,
    lane_count: usize,
    lane_width: u32,
    lane_start_x: i32,
    top_y: i32,
    judgement_y: i32,
}

impl AppHighwayLayout {
    fn new(width: u32, height: u32, lane_count: usize) -> Self {
        let lane_count = lane_count.clamp(1, 8);
        let lane_gap = 8;
        let usable_width = width.saturating_sub(240).max(320);
        let total_gap = lane_gap as u32 * lane_count.saturating_sub(1) as u32;
        let lane_width = ((usable_width - total_gap) / lane_count as u32).clamp(52, 110);
        let total_width = lane_width * lane_count as u32 + total_gap;
        let lane_start_x = ((width - total_width) / 2) as i32;

        Self {
            height,
            lane_count,
            lane_width,
            lane_start_x,
            top_y: 250,
            judgement_y: height as i32 - 130,
        }
    }

    fn lane_x(&self, lane: usize) -> i32 {
        self.lane_start_x + lane as i32 * (self.lane_width as i32 + 8)
    }
}

fn preview_note_rect(layout: &AppHighwayLayout, lane: usize, y: i32, height: u32) -> Rect {
    let x = layout.lane_x(lane) + 8;
    let width = layout.lane_width.saturating_sub(16);
    Rect::new(x, y - height as i32 / 2, width, height)
}

fn preview_note_color(delta_seconds: f64) -> Color {
    if delta_seconds < -0.050 {
        Color::RGB(196, 74, 74)
    } else if delta_seconds.abs() <= 0.050 {
        Color::RGB(116, 220, 143)
    } else {
        Color::RGB(94, 204, 216)
    }
}
