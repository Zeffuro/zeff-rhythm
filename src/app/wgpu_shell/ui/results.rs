use super::*;

impl WgpuAppShell {
    pub(in super::super) fn push_results_view(
        &self,
        rects: &mut Vec<WgpuRect>,
        width: f32,
        height: f32,
    ) {
        push_text(rects, 64.0, 108.0, "RESULTS", 3, TEXT);
        let max_chars = ((width - 440.0) / 12.0).max(18.0) as usize;
        let mut lines = Vec::new();
        if let Some(summary) = &self.latest_run {
            lines.push(summary.title.clone());
            lines.push(if summary.complete {
                "RUN COMPLETE".to_owned()
            } else {
                "RUN ENDED EARLY".to_owned()
            });
            lines.push(format!(
                "HITS {} / MISSES {}",
                summary.report.hits, summary.report.misses
            ));
            lines.push(format!(
                "MARVELOUS {} / PERFECT {}",
                summary.counts.marvelous, summary.counts.perfect
            ));
            lines.push(format!(
                "GREAT {} / GOOD {}",
                summary.counts.great, summary.counts.good
            ));
            if let Some(stats) = summary.report.hit_delta_ms {
                lines.push(format!("MEAN DELTA {:+.1} MS", stats.mean));
            }
            if let Some(aggregate) = self
                .calibration_history
                .aggregate()
                .filter(|_| self.latest_run_source == Some(PendingSessionSource::Calibration))
            {
                lines.push(format!(
                    "TRIALS {} / HITS {}",
                    aggregate.trial_count, aggregate.hit_count
                ));
                lines.push(format!(
                    "OFFSET {:+.1} MS",
                    aggregate.suggested_offset_ms.clamp(-200.0, 200.0)
                ));
                lines.push(format!("CONF {}", aggregate.confidence.display_label()));
            }
        } else {
            lines.push("NO RUN SUMMARY YET".to_owned());
        }
        let rows = ((height - 250.0) / 32.0).max(1.0) as usize;
        for (index, line) in lines.iter().take(rows).enumerate() {
            push_text(
                rects,
                64.0,
                164.0 + index as f32 * 32.0,
                &compact_text(line, max_chars),
                2,
                if index == 0 { TEXT } else { MUTED_TEXT },
            );
        }
        let action_labels = self
            .result_actions()
            .iter()
            .map(|action| self.result_action_label(*action))
            .collect::<Vec<_>>();
        self.push_action_rows(
            rects,
            width,
            height,
            &action_labels,
            self.result_row_index,
            screen_color(AppScreen::Results),
        );
    }

    pub(in super::super) fn push_diagnostics_view(
        &self,
        rects: &mut Vec<WgpuRect>,
        width: f32,
        height: f32,
    ) {
        rects.push(WgpuRect::new(
            72.0,
            112.0,
            width - 144.0,
            height - 216.0,
            rgba(0.13, 0.16, 0.20, 1.0),
        ));
        push_text(rects, 96.0, 138.0, "DIAGNOSTICS", 3, TEXT);

        let mut lines = Vec::new();
        lines.push(format!(
            "AUDIO HOST {}",
            self.state
                .settings
                .audio
                .host
                .as_deref()
                .unwrap_or("DEFAULT")
        ));
        lines.push(format!(
            "DEVICE {}",
            compact_text(
                self.state
                    .settings
                    .audio
                    .device_label
                    .as_deref()
                    .or(self.state.settings.audio.device_id.as_deref())
                    .unwrap_or("DEFAULT"),
                36,
            )
        ));
        lines.push(format!(
            "RATE {}  BUFFER {}",
            optional_u32_text(self.state.settings.audio.sample_rate),
            optional_u32_text(self.state.settings.audio.buffer_frames)
        ));
        lines.push(format!(
            "INPUT {:?}  OFFSET {:+.1}MS",
            self.state.settings.input.backend, self.state.settings.input.input_offset_ms
        ));
        if let Some(saved_offset) = self.saved_calibration_for_current_settings() {
            lines.push(format!(
                "SAVED CAL {:+.1}MS  HITS {}  {}",
                saved_offset.input_offset_ms,
                saved_offset.hit_count,
                compact_text(&saved_offset.confidence, 10)
            ));
        } else {
            lines.push("SAVED CAL NONE FOR CURRENT AUDIO".to_owned());
        }
        lines.push(format!(
            "PRESENT {}  FRAME LATENCY {}",
            self.state
                .settings
                .video
                .render_latency
                .present_mode
                .display_label(),
            self.state
                .settings
                .video
                .render_latency
                .desired_maximum_frame_latency
        ));
        if let Some(gpu) = self.gpu.as_ref() {
            lines.push(format!(
                "SURFACE {}X{} {:?}",
                gpu.config.width, gpu.config.height, gpu.config.format
            ));
        }
        lines.push(format!(
            "EVENT LOG {}  OVERLAY {}",
            on_off(self.state.settings.diagnostics.event_log_enabled),
            on_off(self.state.settings.diagnostics.overlay_enabled)
        ));
        lines.push(
            self.persistence
                .path()
                .map(|path| format!("CONFIG {}", compact_text(&path.display().to_string(), 38)))
                .unwrap_or_else(|| "CONFIG DISABLED".to_owned()),
        );
        lines.push("ESC MAIN MENU".to_owned());

        for (index, line) in lines.iter().take(12).enumerate() {
            push_text(
                rects,
                96.0,
                204.0 + index as f32 * 30.0,
                line,
                2,
                if index == 0 { TEXT } else { MUTED_TEXT },
            );
        }
    }

    pub(in super::super) fn push_calibration_status(
        &self,
        rects: &mut Vec<WgpuRect>,
        x: f32,
        y: f32,
        scale: u32,
    ) {
        let mut lines = vec![format!(
            "CURRENT OFFSET {:+.1}MS",
            self.state.settings.input.input_offset_ms
        )];

        if let Some(saved_offset) = self.saved_calibration_for_current_settings() {
            lines.push(format!(
                "SAVED OFFSET {:+.1}MS",
                saved_offset.input_offset_ms
            ));
            lines.push(format!(
                "SAVED HITS {}  TRIALS {}",
                saved_offset.hit_count, saved_offset.trial_count
            ));
            lines.push(format!(
                "SAVED CONF {}",
                compact_text(&saved_offset.confidence, 16)
            ));
        } else {
            lines.push("NO SAVED OFFSET FOR DEVICE".to_owned());
        }

        if let Some(aggregate) = self.calibration_history.aggregate() {
            lines.push(format!(
                "SESSION CAL {:+.1}MS  {}",
                aggregate.suggested_offset_ms.clamp(-200.0, 200.0),
                aggregate.confidence.display_label()
            ));
        }

        for (index, line) in lines.iter().take(5).enumerate() {
            push_text(
                rects,
                x,
                y + index as f32 * 24.0,
                line,
                scale,
                if index == 0 { TEXT } else { MUTED_TEXT },
            );
        }
    }
}
