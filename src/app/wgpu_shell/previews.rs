use super::*;
use crate::app::song_preview::PreviewRequest;
use crate::platform::audio::build_preview_stream;
use cpal::traits::StreamTrait;

impl WgpuAppShell {
    pub(super) fn selected_preview(&self) -> Option<PreviewRequest> {
        if self.state.screen != AppScreen::SongSelect
            || !self.window_focused
            || !self.state.settings.audio.song_previews
            || self.asset_load.is_some()
            || self.help_visible
            || self.binding_capture.is_some()
        {
            return None;
        }
        let entry = self.library.get(self.library_index)?;
        if entry.problem.is_some() {
            return None;
        }
        Some(PreviewRequest {
            path: entry.audio_path.clone()?,
            start_seconds: entry.preview_start_seconds,
            duration_seconds: entry.preview_duration_seconds,
        })
    }

    pub(super) fn stop_song_preview(&mut self) {
        self.preview_stream = None;
        self.preview_loader.request(None, Instant::now());
        self.preview_error = false;
    }

    pub(super) fn poll_song_preview(&mut self) {
        let request = self.selected_preview();
        if request.is_none() {
            self.preview_stream = None;
        }
        if self.preview_loader.request(request, Instant::now()) {
            self.preview_error = false;
            self.request_redraw();
        }
        let Some(result) = self.preview_loader.poll() else {
            return;
        };
        self.preview_stream = None;
        let result = result.and_then(|(request, clip)| {
            let target = output_stream_target(&audio_options_from_settings(&self.state.settings))
                .map_err(|error| error.to_string())?;
            self.preview_volume
                .set_gain(self.state.settings.audio.gain());
            let stream = build_preview_stream(
                &target,
                clip,
                request.start_seconds,
                request.duration_seconds,
                self.preview_volume.clone(),
            )
            .map_err(|error| error.to_string())?;
            stream.play().map_err(|error| error.to_string())?;
            Ok(stream)
        });
        match result {
            Ok(stream) => self.preview_stream = Some(stream),
            Err(error) => {
                self.preview_error = true;
                eprintln!("Song preview unavailable: {error}");
            }
        }
        self.request_redraw();
    }

    pub(super) fn toggle_song_previews(&mut self) {
        self.state.settings.audio.song_previews = !self.state.settings.audio.song_previews;
        if !self.state.settings.audio.song_previews {
            self.stop_song_preview();
        }
        self.persist_current_settings();
        self.request_redraw();
    }

    pub(super) fn push_preview_control(&self, rects: &mut Vec<WgpuRect>, width: f32) {
        let label = if !self.state.settings.audio.song_previews {
            "F10 PREVIEW OFF"
        } else if self.preview_error {
            "PREVIEW UNAVAILABLE"
        } else if self.preview_loader.is_loading() {
            "F10 PREVIEW LOADING"
        } else {
            "F10 PREVIEW ON"
        };
        rects.push(WgpuRect::new(
            width - 220.0,
            88.0,
            172.0,
            28.0,
            rgba(0.07, 0.11, 0.15, 1.0),
        ));
        push_text(rects, width - 212.0, 96.0, label, 1, TEXT);
    }
}

#[cfg(test)]
mod tests;
