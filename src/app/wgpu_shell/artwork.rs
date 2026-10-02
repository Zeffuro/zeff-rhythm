use super::*;
use crate::app::artwork::ArtworkRequest;

impl WgpuAppShell {
    pub(super) fn poll_artwork(&mut self) {
        let request = self.selected_artwork();
        if request != self.artwork_request {
            self.artwork_request = request.clone();
            self.artwork_loader.request(request);
        }
        if let Some(update) = self.artwork_loader.poll() {
            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.artwork_renderer) {
                renderer.set_image(&gpu.device, &gpu.queue, update.image.as_deref());
            }
            self.request_redraw();
        }
    }

    pub(super) fn selected_artwork(&self) -> Option<ArtworkRequest> {
        let entry = match self.state.screen {
            AppScreen::SongSelect => self.library.get(self.library_index),
            AppScreen::Gameplay | AppScreen::Results
                if self.pending_session_source == Some(PendingSessionSource::Chart) =>
            {
                self.state.selected_chart.as_ref().and_then(|selection| {
                    self.library.entries().iter().find(|entry| {
                        entry.chart_path == selection.chart_path
                            && entry.chart_index == selection.chart_index
                    })
                })
            }
            _ => None,
        }?;
        Some(ArtworkRequest {
            chart_path: entry.chart_path.clone(),
            background_path: entry.background_path.clone(),
            banner_path: entry.banner_path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_reconciliation_clears_previous_art_when_selection_disappears() {
        let mut shell = WgpuAppShell::new(WgpuShellOptions::default());
        let request = ArtworkRequest {
            chart_path: "removed/chart.osu".into(),
            background_path: Some("removed/background.jpg".into()),
            banner_path: None,
        };
        shell.artwork_request = Some(request.clone());
        shell.artwork_loader.request(Some(request));
        assert!(shell.artwork_loader.poll().is_none());
        shell.replace_library(AppLibrary::default());
        assert!(shell.selected_artwork().is_none());
        assert!(shell.artwork_loader.poll().unwrap().image.is_none());
        assert!(!shell.artwork_loader.is_loading());
    }
}
