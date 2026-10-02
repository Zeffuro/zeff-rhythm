use super::*;
use winit::event::Ime;

impl WgpuAppShell {
    pub(super) fn search_accepts_text(&self) -> bool {
        self.state.screen == AppScreen::SongSelect
            && self.search_active
            && self.window_focused
            && !self.help_visible
            && self.asset_load.is_none()
            && self.binding_capture.is_none()
    }

    pub(super) fn clear_preedit(&mut self) {
        self.search_preedit.clear();
        self.search_preedit_cursor = None;
    }

    pub(super) fn process_search_ime(&mut self, ime: Ime) {
        if !self.search_accepts_text() {
            self.clear_preedit();
            return;
        }
        match ime {
            Ime::Preedit(text, cursor) => {
                self.search_preedit_cursor = cursor.filter(|(start, end)| {
                    start <= end && text.is_char_boundary(*start) && text.is_char_boundary(*end)
                });
                self.search_preedit = text;
            }
            Ime::Commit(text) => {
                self.clear_preedit();
                self.insert_search_text(&text);
            }
            Ime::Disabled => self.clear_preedit(),
            Ime::Enabled => {}
        }
        self.request_redraw();
    }

    pub(super) fn update_search_ime(&mut self) {
        let enabled = self.search_accepts_text();
        if !enabled {
            self.clear_preedit();
        }
        if let Some(gpu) = &self.gpu {
            if self.ime_enabled != enabled {
                gpu.window.set_ime_allowed(enabled);
                self.ime_enabled = enabled;
            }
            if enabled {
                let base = if self.search_selected_all {
                    ""
                } else {
                    &self.search_query
                };
                let end = self
                    .search_preedit_cursor
                    .map(|(start, _)| start)
                    .unwrap_or(0);
                let text = format!("{base}{}", &self.search_preedit[..end]);
                let x = (60.0 + crate::render::text::line(&text, 2).width)
                    .min(gpu.config.width as f32 - 250.0);
                gpu.window.set_ime_cursor_area(
                    winit::dpi::PhysicalPosition::new(x as i32, 140),
                    winit::dpi::PhysicalSize::new(1u32, 24),
                );
            }
        }
    }

    pub(super) fn search_display_text(&self) -> String {
        let base = if self.search_selected_all && !self.search_preedit.is_empty() {
            ""
        } else {
            &self.search_query
        };
        format!(
            "{base}{}{}",
            self.search_preedit,
            if self.search_active && !self.search_selected_all && self.search_preedit.is_empty() {
                "_"
            } else {
                ""
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_is_visible_but_only_commit_changes_the_filter() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        shell.window_focused = true;
        shell.search_active = true;
        shell.search_query = "previous".into();
        shell.search_selected_all = true;
        shell.process_search_ime(Ime::Preedit("にほん".into(), Some((9, 9))));
        assert_eq!(shell.search_display_text(), "にほん");
        assert_eq!(shell.search_query, "previous");
        assert!(shell.search_input(KeyCode::ArrowDown, None, true));
        shell.process_search_ime(Ime::Commit("日本".into()));
        assert_eq!(shell.search_query, "日本");
        assert!(shell.search_preedit.is_empty());
        shell.process_search_ime(Ime::Preedit("語".into(), Some((1, 99))));
        assert_eq!(shell.search_preedit_cursor, None);
        shell.process_search_ime(Ime::Preedit(String::new(), None));
        assert_eq!(shell.search_query, "日本");
    }

    #[test]
    fn help_capture_focus_and_gameplay_cancel_composition_and_reject_commits() {
        for blocked in 0..4 {
            let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
            shell.window_focused = true;
            shell.search_active = true;
            shell.process_search_ime(Ime::Preedit("にほん".into(), None));
            match blocked {
                0 => shell.help_visible = true,
                1 => shell.binding_capture = Some(0),
                2 => shell.window_focused = false,
                _ => shell.state.screen = AppScreen::Gameplay,
            }
            shell.update_search_ime();
            shell.process_search_ime(Ime::Commit("日本".into()));
            assert!(shell.search_query.is_empty() && shell.search_preedit.is_empty());
        }
    }
}
