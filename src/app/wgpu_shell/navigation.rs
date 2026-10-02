use super::*;
use unicode_segmentation::UnicodeSegmentation;

impl WgpuAppShell {
    pub(super) fn search_input(
        &mut self,
        code: KeyCode,
        text: Option<&str>,
        pressed: bool,
    ) -> bool {
        if !self.search_accepts_text() {
            return false;
        }
        if !self.search_preedit.is_empty() && code != KeyCode::F1 {
            if pressed && code == KeyCode::Escape {
                self.clear_preedit();
            }
            return true;
        }
        if matches!(
            code,
            KeyCode::F1
                | KeyCode::F2
                | KeyCode::F3
                | KeyCode::F5
                | KeyCode::F6
                | KeyCode::F7
                | KeyCode::F8
                | KeyCode::F9
                | KeyCode::F10
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
                | KeyCode::PageUp
                | KeyCode::PageDown
        ) {
            return false;
        }
        if !pressed {
            return true;
        }
        if self.modifiers.control_key() {
            if code == KeyCode::KeyA {
                self.search_selected_all = !self.search_query.is_empty();
            }
            self.request_redraw();
            return true;
        }
        match code {
            KeyCode::Escape | KeyCode::Enter => {
                self.search_active = false;
                self.search_selected_all = false;
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if self.search_selected_all {
                    self.search_query.clear();
                } else if code == KeyCode::Backspace {
                    let end = self
                        .search_query
                        .grapheme_indices(true)
                        .last()
                        .map(|(index, _)| index)
                        .unwrap_or(0);
                    self.search_query.truncate(end);
                }
                self.search_selected_all = false;
            }
            _ => {
                if let Some(text) = text {
                    self.insert_search_text(text);
                }
            }
        }
        self.ensure_visible_selection();
        self.request_redraw();
        true
    }

    pub(super) fn insert_search_text(&mut self, text: &str) {
        if !self.search_accepts_text() {
            return;
        }
        let input: String = text.chars().filter(|c| !c.is_control()).collect();
        if input.is_empty() {
            return;
        }
        if self.search_selected_all {
            self.search_query.clear();
            self.search_selected_all = false;
        }
        let remaining = 128usize.saturating_sub(self.search_query.graphemes(true).count());
        self.search_query
            .extend(input.graphemes(true).take(remaining));
        self.ensure_visible_selection();
        self.request_redraw();
    }

    pub(super) fn handle_library_key(
        &mut self,
        code: KeyCode,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        if self.state.screen != AppScreen::SongSelect || !pressed {
            return false;
        }
        match code {
            KeyCode::ArrowUp => self.move_library_selection(-1),
            KeyCode::ArrowDown => self.move_library_selection(1),
            KeyCode::PageUp => self.move_library_selection(-5),
            KeyCode::PageDown => self.move_library_selection(5),
            KeyCode::Home => {
                self.library_index = self
                    .library_indices()
                    .first()
                    .copied()
                    .unwrap_or(usize::MAX)
            }
            KeyCode::End => {
                self.library_index = self.library_indices().last().copied().unwrap_or(usize::MAX)
            }
            KeyCode::F2 => {
                if !repeat {
                    self.random_song();
                }
            }
            _ => return false,
        }
        self.last_song_click = None;
        self.library_launch_error = None;
        true
    }

    pub(super) fn random_song(&mut self) {
        let mut candidates: Vec<usize> = self
            .library_indices()
            .into_iter()
            .filter(|index| {
                self.library
                    .get(*index)
                    .is_some_and(|entry| entry.is_available())
            })
            .collect();
        if candidates.len() > 1 {
            candidates.retain(|index| *index != self.library_index);
        }
        if !candidates.is_empty() {
            self.library_index = candidates[fastrand::usize(..candidates.len())];
            self.library_launch_error = None;
            self.last_song_click = None;
        }
    }
}

#[cfg(test)]
mod tests;
