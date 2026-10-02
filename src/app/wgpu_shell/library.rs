use super::*;

impl WgpuAppShell {
    pub(super) fn refresh_library(&mut self) {
        if let Some(library) = self.library_scanner.poll() {
            self.replace_library(library);
        }
    }

    pub(super) fn replace_library(&mut self, library: AppLibrary) {
        self.stop_song_preview();
        let selected = self
            .library
            .get(self.library_index)
            .map(|entry| entry.chart_selection());
        self.library_index = library.selection_index(selected.as_ref());
        self.library = library;
        self.ensure_visible_selection();
        self.artwork_request = self.selected_artwork();
        self.artwork_loader.request(self.artwork_request.clone());
        self.library_launch_error = None;
        self.request_redraw();
    }

    pub(super) fn add_library_root(&mut self, path: std::path::PathBuf) {
        if !self.state.settings.library_roots.contains(&path) {
            self.state.settings.library_roots.push(path);
            self.persist_current_settings();
        }
        self.library_scanner
            .request(self.state.settings.library_roots.clone());
        if self.state.screen != AppScreen::Gameplay && self.asset_load.is_none() {
            self.state.open_song_select();
        }
        self.request_redraw();
    }

    pub(super) fn library_indices(&self) -> Vec<usize> {
        let terms = self.search_query.to_lowercase();
        self.library
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (!self.ready_only || entry.problem.is_none()) && {
                    let searchable = format!(
                        "{} {} {}",
                        entry.title,
                        entry.subtitle,
                        entry.search_aliases.join(" ")
                    )
                    .to_lowercase();
                    terms
                        .split_whitespace()
                        .all(|term| searchable.contains(term))
                }
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn ensure_visible_selection(&mut self) {
        let indices = self.library_indices();
        if !indices.contains(&self.library_index) {
            self.library_index = indices.first().copied().unwrap_or(usize::MAX);
        }
        self.library_launch_error = None;
    }

    pub(super) fn move_library_selection(&mut self, direction: i32) {
        let indices = self.library_indices();
        if indices.is_empty() {
            self.library_index = usize::MAX;
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.library_index)
            .unwrap_or(0);
        let next = (current as i64 + direction as i64).clamp(0, indices.len() as i64 - 1) as usize;
        self.library_index = indices[next];
        self.library_launch_error = None;
    }

    pub(super) fn push_song_select(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        let indices = self.library_indices();
        let max_chars = ((width - 152.0) / 12.0).max(1.0) as usize;
        let status = format!(
            "{} PLAYABLE / {} ISSUES{}",
            self.library.playable_count(),
            self.library.problem_count(),
            if self.library_scanner.is_scanning() {
                " / CHECKING CHANGES..."
            } else {
                ""
            }
        );
        push_text(
            rects,
            48.0,
            94.0,
            &compact_text(&status, ((width - 290.0) / 12.0).max(1.0) as usize),
            2,
            MUTED_TEXT,
        );
        self.push_preview_control(rects, width);
        rects.push(WgpuRect::new(
            48.0,
            128.0,
            width - 284.0,
            38.0,
            rgba(0.08, 0.11, 0.15, 1.0),
        ));
        let search = if self.search_query.is_empty() && self.search_preedit.is_empty() {
            "/ SEARCH TITLE, ARTIST, DIFFICULTY".to_owned()
        } else {
            self.search_display_text()
        };
        if self.search_active && self.search_selected_all && self.search_preedit.is_empty() {
            let selection_width = crate::render::text::line(&self.search_query, 2)
                .width
                .min(width - 312.0);
            rects.push(WgpuRect::new(
                60.0,
                136.0,
                selection_width + 4.0,
                26.0,
                rgba(0.09, 0.32, 0.52, 1.0),
            ));
        }
        push_text(
            rects,
            60.0,
            140.0,
            &compact_text(&search, ((width - 316.0) / 12.0) as usize),
            2,
            if self.search_active { TEXT } else { MUTED_TEXT },
        );
        if !self.search_preedit.is_empty() {
            let base = if self.search_selected_all {
                0.0
            } else {
                crate::render::text::line(&self.search_query, 2).width
            }
            .min(width - 316.0);
            let length = crate::render::text::line(&self.search_preedit, 2)
                .width
                .min(width - 316.0 - base);
            rects.push(WgpuRect::new(60.0 + base, 163.0, length, 1.0, TEXT));
        }
        push_button(
            rects,
            width - 220.0,
            128.0,
            172.0,
            if self.ready_only {
                "PLAYABLE"
            } else {
                "ALL CHARTS"
            },
            self.ready_only,
        );
        let selected = indices
            .iter()
            .position(|index| *index == self.library_index)
            .unwrap_or(0);
        let range = visible_range(
            selected,
            indices.len(),
            super::interaction::song_rows(height),
        );
        for (row, position) in range.enumerate() {
            let index = indices[position];
            let entry = &self.library.entries()[index];
            let y = 184.0 + row as f32 * 70.0;
            let selected = index == self.library_index;
            rects.push(WgpuRect::new(
                48.0,
                y,
                width - 96.0,
                60.0,
                if selected {
                    rgba(0.13, 0.24, 0.31, 1.0)
                } else {
                    rgba(0.08, 0.11, 0.15, 1.0)
                },
            ));
            if selected {
                rects.push(WgpuRect::new(
                    48.0,
                    y,
                    4.0,
                    60.0,
                    rgba(0.43, 0.80, 0.78, 1.0),
                ));
            }
            push_text(
                rects,
                64.0,
                y + 8.0,
                &compact_text(&entry.title, max_chars),
                2,
                TEXT,
            );
            push_text(
                rects,
                64.0,
                y + 34.0,
                &compact_text(&entry.subtitle, max_chars),
                2,
                MUTED_TEXT,
            );
        }
        if indices.is_empty() {
            let message = if self.library.entries().is_empty() {
                if self.library_scanner.is_scanning() {
                    "FIRST SCAN IN PROGRESS"
                } else {
                    "DROP YOUR SONGS FOLDER HERE TO ADD SONGS"
                }
            } else {
                "NO MATCHES / TAB SHOWS UNAVAILABLE CHARTS"
            };
            push_text(
                rects,
                64.0,
                212.0,
                &compact_text(message, max_chars),
                2,
                TEXT,
            );
        }
        if let Some(entry) = self
            .library
            .get(self.library_index)
            .filter(|_| indices.contains(&self.library_index))
        {
            push_text(
                rects,
                48.0,
                height - 156.0,
                &compact_text(
                    self.library_launch_error
                        .as_deref()
                        .unwrap_or_else(|| entry.status()),
                    max_chars,
                ),
                2,
                TEXT,
            );
            let path = entry.chart_path.display().to_string().replace('\\', "/");
            let path = path.strip_prefix("//?/").unwrap_or(&path);
            push_text(
                rects,
                48.0,
                height - 128.0,
                &compact_text(path, max_chars),
                1,
                MUTED_TEXT,
            );
        } else if let Some(error) = self.library_launch_error.as_ref().or_else(|| {
            self.library
                .records
                .iter()
                .find(|r| r.status == "error")
                .map(|r| &r.detail)
        }) {
            push_text(
                rects,
                48.0,
                height - 156.0,
                &compact_text(error, max_chars),
                2,
                TEXT,
            );
        }
        push_button(rects, 48.0, height - 104.0, 156.0, "ENTER PLAY", true);
        push_button(rects, 220.0, height - 104.0, 140.0, "R RESCAN", false);
        push_button(rects, 376.0, height - 104.0, 156.0, "F2 RANDOM", false);
        push_text(
            rects,
            554.0,
            height - 90.0,
            &compact_text(
                &format!(
                    "{} / {}",
                    selected + usize::from(!indices.is_empty()),
                    indices.len()
                ),
                ((width - 590.0) / 12.0) as usize,
            ),
            2,
            MUTED_TEXT,
        );
    }
}
