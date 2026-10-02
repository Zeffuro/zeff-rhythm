use super::*;
use crate::play::AssetLoadJob;

pub(super) struct PendingAssetLoad {
    job: AssetLoadJob,
    options: PlaySessionOptions,
    started: Instant,
}

impl WgpuAppShell {
    pub(super) fn queue_chart_load(&mut self, options: PlaySessionOptions) {
        self.stop_song_preview();
        self.search_active = false;
        self.asset_load = Some(PendingAssetLoad {
            job: AssetLoadJob::start(options.clone()),
            options,
            started: Instant::now(),
        });
        self.library_launch_error = None;
        self.state.open_song_select();
        self.request_redraw();
    }

    pub(super) fn poll_chart_load(&mut self) {
        let Some(result) = self.asset_load.as_mut().and_then(|load| load.job.poll()) else {
            return;
        };
        let load = self.asset_load.take().unwrap();
        let result = result
            .map_err(|error| -> Box<dyn Error> { error.into() })
            .and_then(|assets| {
                assets
                    .resolve_output(&load.options.audio)
                    .map_err(|error| -> Box<dyn Error> {
                        format!("Could not resolve audio output: {error}").into()
                    })
            })
            .and_then(|assets| {
                self.start_live_session_with_assets(
                    load.options,
                    PendingSessionSource::Chart,
                    assets.into(),
                )
            });
        if let Err(error) = result {
            self.library_launch_error = Some(error.to_string());
            self.state.open_song_select();
        }
        self.request_redraw();
    }

    pub(super) fn push_loading(&self, rects: &mut Vec<WgpuRect>, width: f32, height: f32) {
        let Some(load) = &self.asset_load else {
            return;
        };
        rects.push(WgpuRect::new(
            0.0,
            72.0,
            width,
            height - 136.0,
            rgba(0.03, 0.04, 0.06, 0.96),
        ));
        push_text(rects, 64.0, 156.0, "LOADING SONG", 3, TEXT);
        let title = self
            .state
            .selected_chart
            .as_ref()
            .and_then(|selection| {
                self.library.entries().iter().find(|entry| {
                    entry.chart_path == selection.chart_path
                        && entry.chart_index == selection.chart_index
                })
            })
            .map(|entry| entry.title.as_str())
            .unwrap_or("CHART AND AUDIO");
        push_text(
            rects,
            64.0,
            208.0,
            &compact_text(title, ((width - 128.0) / 12.0) as usize),
            2,
            TEXT,
        );
        push_text(
            rects,
            64.0,
            250.0,
            &format!(
                "DECODING AUDIO {:.1}S",
                load.started.elapsed().as_secs_f64()
            ),
            2,
            MUTED_TEXT,
        );
        push_text(
            rects,
            64.0,
            292.0,
            "ESC CANCEL / WINDOW REMAINS RESPONSIVE",
            2,
            MUTED_TEXT,
        );
    }
}
