use super::*;

impl WgpuAppShell {
    pub(super) fn navigate_to(&mut self, screen: AppScreen) {
        self.stop_song_preview();
        if self.state.screen == AppScreen::Gameplay {
            if let Err(error) = self.finish_live_session() {
                self.library_launch_error = Some(error.to_string());
            }
        }
        self.asset_load = None;
        self.binding_capture = None;
        self.help_visible = false;
        self.search_active = false;
        self.search_selected_all = false;
        self.clear_preedit();
        if let AppScreen::Settings(panel) = screen {
            self.settings_panel = panel;
            self.settings_row_index = 0;
        }
        self.state.screen = screen;
        self.request_redraw();
    }

    pub(super) fn viewport(&self) -> (f32, f32) {
        self.gpu
            .as_ref()
            .map(|gpu| (gpu.config.width as f32, gpu.config.height as f32))
            .unwrap_or((self.options.width as f32, self.options.height as f32))
    }

    pub(super) fn click_at(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let (x, y) = self.cursor_position;
        let (width, height) = self.viewport();
        if self.binding_capture.is_some() || self.asset_load.is_some() {
            return Ok(());
        }
        if self.volume_hovered(x, y, width, height) {
            self.toggle_mute();
            return Ok(());
        }
        if self.help_visible {
            self.help_visible = false;
            self.request_redraw();
            return Ok(());
        }
        if self.state.screen == AppScreen::SongSelect
            && (width - 220.0..width - 48.0).contains(&x)
            && (88.0..116.0).contains(&y)
        {
            self.toggle_song_previews();
            return Ok(());
        }
        if self
            .live_session
            .as_ref()
            .is_some_and(LivePlaySession::is_paused)
        {
            self.click_pause_button(x, y, width, height)?;
            self.request_redraw();
            return Ok(());
        }
        if self.state.screen != AppScreen::Gameplay
            && (18.0..52.0).contains(&y)
            && (width - 432.0..width - 24.0).contains(&x)
        {
            if (x - (width - 432.0)) % 104.0 >= 96.0 {
                return Ok(());
            }
            match ((x - (width - 432.0)) / 104.0) as usize {
                0 => self.navigate_to(AppScreen::SongSelect),
                1 => self.navigate_to(AppScreen::Settings(SettingsPanel::Audio)),
                2 => self.navigate_to(AppScreen::Calibration),
                _ => self.help_visible = true,
            }
            self.request_redraw();
            return Ok(());
        }
        match self.state.screen {
            AppScreen::SongSelect => {
                if (128.0..166.0).contains(&y) {
                    if (width - 220.0..width - 48.0).contains(&x) {
                        self.ready_only = !self.ready_only;
                        self.ensure_visible_selection();
                    } else if (48.0..width - 236.0).contains(&x) {
                        self.search_active = true;
                    }
                } else if y >= 184.0
                    && y < height - 176.0
                    && (48.0..width - 48.0).contains(&x)
                    && (y - 184.0) % 70.0 < 60.0
                {
                    let indices = self.library_indices();
                    let selected = indices
                        .iter()
                        .position(|index| *index == self.library_index)
                        .unwrap_or(0);
                    let range = visible_range(selected, indices.len(), song_rows(height));
                    let row = ((y - 184.0) / 70.0) as usize;
                    if let Some(index) = indices
                        .get(range.start + row)
                        .copied()
                        .filter(|_| row < range.len())
                    {
                        let double = self.last_song_click.is_some_and(|(previous, time)| {
                            previous == index && time.elapsed().as_millis() < 450
                        });
                        self.library_index = index;
                        self.library_launch_error = None;
                        self.last_song_click = Some((index, Instant::now()));
                        if double {
                            self.confirm(event_loop)?;
                        }
                    }
                } else if y >= height - 104.0 && y < height - 68.0 {
                    if (48.0..204.0).contains(&x) {
                        self.confirm(event_loop)?;
                    } else if (220.0..360.0).contains(&x) {
                        self.library_scanner
                            .request(self.state.settings.library_roots.clone());
                    } else if (376.0..532.0).contains(&x) {
                        self.random_song();
                    }
                }
            }
            AppScreen::Settings(panel) => {
                if (100.0..146.0).contains(&y)
                    && (48.0..width - 48.0).contains(&x)
                    && (x - 48.0) % ((width - 96.0) / 5.0) < (width - 96.0) / 5.0 - 12.0
                {
                    let panels = [
                        SettingsPanel::Audio,
                        SettingsPanel::Input,
                        SettingsPanel::Video,
                        SettingsPanel::Gameplay,
                        SettingsPanel::Diagnostics,
                    ];
                    let index = ((x - 48.0).max(0.0) / ((width - 96.0) / 5.0)) as usize;
                    if let Some(panel) = panels.get(index) {
                        self.navigate_to(AppScreen::Settings(*panel));
                    }
                } else if y >= 208.0
                    && y < height - 84.0
                    && (y - 208.0) % 48.0 < 40.0
                    && menu_x_range(width).contains(&x)
                {
                    let labels = settings_rows(&self.state, panel);
                    let range = visible_range(
                        self.settings_row_index,
                        labels.len(),
                        menu_rows(height, 208.0),
                    );
                    let row = ((y - 208.0) / 48.0) as usize;
                    if row < range.len() {
                        self.settings_row_index = range.start + row;
                        self.confirm(event_loop)?;
                    }
                }
            }
            AppScreen::MainMenu => {
                if y >= 122.0
                    && y < height - 84.0
                    && (y - 122.0) % 48.0 < 40.0
                    && menu_x_range(width).contains(&x)
                {
                    let row = ((y - 122.0) / 48.0) as usize;
                    if row < MENU_ITEMS.len() {
                        self.menu_index = row;
                        self.confirm(event_loop)?;
                    }
                }
            }
            AppScreen::Results => {
                let count = self.result_actions().len();
                let start = (height - 116.0 - count as f32 * 56.0).max(168.0);
                if ((width - 352.0).max(96.0)..width - 72.0).contains(&x)
                    && y >= start
                    && (y - start) % 56.0 < 42.0
                {
                    let row = ((y - start) / 56.0) as usize;
                    if row < count {
                        self.result_row_index = row;
                        self.confirm_results()?;
                    }
                }
            }
            AppScreen::Calibration => self.start_generated_calibration()?,
            _ => {}
        }
        self.request_redraw();
        Ok(())
    }
}

pub(super) fn menu_rows(height: f32, top: f32) -> usize {
    ((height - top - 84.0) / 48.0).max(1.0) as usize
}
pub(super) fn song_rows(height: f32) -> usize {
    ((height - 360.0) / 70.0).max(1.0) as usize
}

fn menu_x_range(width: f32) -> std::ops::Range<f32> {
    let row_width = (width - 160.0).min(640.0);
    (width - row_width) * 0.5..(width + row_width) * 0.5
}
