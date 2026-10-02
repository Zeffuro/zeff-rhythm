use super::*;

impl WgpuAppShell {
    pub(super) fn handle_key(
        &mut self,
        event_loop: &ActiveEventLoop,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
    ) -> Result<(), Box<dyn Error>> {
        if self.binding_capture.is_some() {
            if pressed && !repeat {
                self.capture_binding(code);
            }
            self.request_redraw();
            return Ok(());
        }
        if self.handle_volume_key(code, pressed, repeat) {
            return Ok(());
        }
        if code == KeyCode::F10 && self.state.screen == AppScreen::SongSelect {
            if pressed && !repeat {
                self.toggle_song_previews();
            }
            return Ok(());
        }
        if pressed && !repeat && code == KeyCode::F1 {
            if let Some(session) = self.live_session.as_mut() {
                session.pause()?;
            }
            self.help_visible = !self.help_visible;
            self.request_redraw();
            return Ok(());
        }
        if self.process_help_input(code, pressed, repeat)? {
            self.request_redraw();
            return Ok(());
        }
        if self.asset_load.is_some() {
            if pressed && matches!(code, KeyCode::Escape | KeyCode::Backspace) {
                self.asset_load = None;
            }
            self.request_redraw();
            return Ok(());
        }
        if self.handle_scroll_key(code, pressed, repeat) {
            self.request_redraw();
            return Ok(());
        }
        if self.handle_library_key(code, pressed, repeat) {
            self.request_redraw();
            return Ok(());
        }
        if self.state.screen == AppScreen::Gameplay {
            if pressed && !repeat && code == KeyCode::Escape {
                if let Some(session) = self.live_session.as_mut() {
                    if session.is_paused() && session.pause_countdown_seconds().is_none() {
                        session.request_resume()?;
                    } else {
                        session.pause()?;
                    }
                }
                self.request_redraw();
                return Ok(());
            }
            if pressed && !repeat && matches!(code, KeyCode::Enter | KeyCode::Space) {
                if let Some(session) = self.live_session.as_mut() {
                    session.request_resume()?;
                }
                self.request_redraw();
                return Ok(());
            }
            if let Some(lane) = self.bound_lane(code) {
                if !repeat {
                    self.process_winit_lane_input(lane as u8, pressed)?;
                }
                self.request_redraw();
                return Ok(());
            }
        }

        if !pressed || repeat {
            return Ok(());
        }

        match code {
            KeyCode::F2 if self.state.screen != AppScreen::Gameplay => {
                self.navigate_to(AppScreen::Settings(SettingsPanel::Audio));
            }
            KeyCode::F3 if self.state.screen != AppScreen::Gameplay => {
                self.navigate_to(AppScreen::Settings(SettingsPanel::Input));
            }
            KeyCode::Slash if self.state.screen == AppScreen::SongSelect => {
                self.search_active = true
            }
            KeyCode::Tab if self.state.screen == AppScreen::SongSelect => {
                self.ready_only = !self.ready_only;
                self.ensure_visible_selection();
            }
            KeyCode::KeyR if self.state.screen == AppScreen::SongSelect => {
                self.library_scanner
                    .request(self.state.settings.library_roots.clone());
            }
            KeyCode::KeyR if self.state.screen == AppScreen::Gameplay => {
                self.restart_live_session()?
            }
            KeyCode::KeyQ => {
                self.finish_live_session()?;
                event_loop.exit();
            }
            KeyCode::Escape | KeyCode::Backspace => self.go_back(),
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.move_selection(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust_or_move_panel(-1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust_or_move_panel(1),
            KeyCode::Enter | KeyCode::Space => self.confirm(event_loop)?,
            _ => {}
        }

        self.request_redraw();
        Ok(())
    }

    pub(super) fn process_help_input(
        &mut self,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
    ) -> Result<bool, Box<dyn Error>> {
        if !self.help_visible {
            return Ok(false);
        }
        if pressed && matches!(code, KeyCode::Escape | KeyCode::Backspace) {
            self.help_visible = false;
        } else if self.state.screen == AppScreen::Gameplay && !repeat {
            if let Some(lane) = self.bound_lane(code) {
                self.process_winit_lane_input(lane as u8, pressed)?;
            }
        }
        Ok(true)
    }

    pub(super) fn move_selection(&mut self, direction: i32) {
        match self.state.screen {
            AppScreen::MainMenu => {
                self.menu_index = cycle_index(self.menu_index, MENU_ITEMS.len(), direction);
            }
            AppScreen::SongSelect => {
                self.library_launch_error = None;
                self.move_library_selection(direction);
            }
            AppScreen::Settings(_) => {
                self.settings_row_index = cycle_index(
                    self.settings_row_index,
                    settings_row_count(self.settings_panel) + 1,
                    direction,
                );
            }
            AppScreen::Results => {
                self.result_row_index = cycle_index(
                    self.result_row_index,
                    self.result_actions().len(),
                    direction,
                );
            }
            AppScreen::Calibration | AppScreen::Diagnostics | AppScreen::Gameplay => {}
        }
    }

    pub(super) fn adjust_or_move_panel(&mut self, direction: i32) {
        if let AppScreen::Settings(_) = self.state.screen {
            self.adjust_selected_setting(direction);
        }
    }

    pub(super) fn confirm(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        match self.state.screen {
            AppScreen::MainMenu => match MENU_ITEMS[self.menu_index] {
                WgpuMenuItem::SongSelect => self.state.open_song_select(),
                WgpuMenuItem::Settings => {
                    self.settings_row_index = 0;
                    self.state.open_settings(self.settings_panel);
                }
                WgpuMenuItem::Calibration => self.state.open_calibration(),
                WgpuMenuItem::Diagnostics => self.state.open_diagnostics(),
                WgpuMenuItem::Quit => event_loop.exit(),
            },
            AppScreen::SongSelect => {
                if let Err(error) = self.prepare_selected_library_entry() {
                    self.library_launch_error = Some(error.to_string());
                    self.state.open_song_select();
                }
            }
            AppScreen::Settings(SettingsPanel::Input)
                if (4..=7).contains(&self.settings_row_index) =>
            {
                self.binding_capture = Some(self.settings_row_index - 4);
            }
            AppScreen::Settings(_) => self.adjust_selected_setting(1),
            AppScreen::Calibration => self.start_generated_calibration()?,
            AppScreen::Diagnostics => {}
            AppScreen::Gameplay => {}
            AppScreen::Results => self.confirm_results()?,
        }

        Ok(())
    }

    pub(super) fn go_back(&mut self) {
        self.binding_capture = None;
        if self.asset_load.take().is_some() {
            self.state.open_song_select();
            return;
        }
        if self.state.screen == AppScreen::SongSelect && !self.search_query.is_empty() {
            self.search_query.clear();
            self.search_active = false;
            self.ensure_visible_selection();
            return;
        }
        match self.state.screen {
            AppScreen::MainMenu => {}
            AppScreen::Gameplay => {
                if let Err(error) = self.finish_live_session() {
                    println!("app_wgpu_live_finish_error={error}");
                }
                self.state.screen = AppScreen::Results;
            }
            AppScreen::Results
            | AppScreen::Settings(_)
            | AppScreen::Calibration
            | AppScreen::Diagnostics => self.state.open_song_select(),
            AppScreen::SongSelect => self.state.open_main_menu(),
        }
    }
}
