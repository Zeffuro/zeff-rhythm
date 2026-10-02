use super::*;

impl WgpuAppShell {
    pub(super) fn prepare_selected_library_entry(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(entry) = self.library.get(self.library_index) else {
            return Ok(());
        };

        if !entry.is_available() {
            println!("app_wgpu_song_unavailable={}", entry.chart_path().display());
            return Ok(());
        }

        self.state.select_chart(entry.chart_selection());
        self.finish_live_session()?;
        self.state.request_play()?;
        self.start_live_session_from_pending_launch()
    }

    pub(super) fn start_live_session_from_pending_launch(&mut self) -> Result<(), Box<dyn Error>> {
        let launch = self
            .state
            .take_pending_launch()
            .ok_or("app state did not create a play launch request")?;
        let session_options = PlaySessionOptions::from_app_launch(&launch)?;
        self.queue_chart_load(session_options);
        Ok(())
    }

    pub(super) fn start_generated_calibration(&mut self) -> Result<(), Box<dyn Error>> {
        self.finish_live_session()?;
        let session_options = PlaySessionOptions::from_app_calibration(&self.state.settings)?;
        self.start_live_session(session_options, PendingSessionSource::Calibration)
    }

    pub(super) fn start_live_session(
        &mut self,
        session_options: PlaySessionOptions,
        source: PendingSessionSource,
    ) -> Result<(), Box<dyn Error>> {
        let assets = match source {
            PendingSessionSource::Chart => {
                LiveSessionAssets::from(load_play_session_assets(&session_options)?)
            }
            PendingSessionSource::Calibration => build_generated_calibration_assets(
                &session_options.audio,
                CalibrationPattern::default(),
            )?,
        };
        self.start_live_session_with_assets(session_options, source, assets)
    }

    pub(super) fn start_live_session_with_assets(
        &mut self,
        mut session_options: PlaySessionOptions,
        source: PendingSessionSource,
        assets: LiveSessionAssets,
    ) -> Result<(), Box<dyn Error>> {
        self.stop_song_preview();
        self.update_resolved_audio(Some(LiveAudioSummary::from_target(&assets.target)), false);
        session_options.input_offset_ms = self.state.settings.input.input_offset_ms;
        session_options.volume = self.state.settings.audio.gain();
        let live_session = LivePlaySession::start_with_assets(session_options.clone(), assets)?;

        self.latest_run = None;
        self.latest_run_source = None;
        self.result_row_index = 0;
        self.pending_session_options = Some(session_options);
        self.pending_session_source = Some(source);
        self.live_session = Some(live_session);
        self.process_winit_focus(self.window_focused)?;
        self.last_redraw = None;
        self.state.screen = AppScreen::Gameplay;
        Ok(())
    }

    pub(super) fn restart_live_session(&mut self) -> Result<(), Box<dyn Error>> {
        match self.pending_session_source {
            Some(PendingSessionSource::Chart) | None if self.state.selected_chart.is_some() => {
                self.finish_live_session()?;
                self.state.request_play()?;
                if let Err(error) = self.start_live_session_from_pending_launch() {
                    self.library_launch_error = Some(error.to_string());
                    self.state.open_song_select();
                }
                Ok(())
            }
            Some(PendingSessionSource::Calibration) => self.start_generated_calibration(),
            Some(source) => {
                let Some(options) = self.pending_session_options.clone() else {
                    return Ok(());
                };
                self.finish_live_session()?;
                self.start_live_session(options, source)
            }
            None => Ok(()),
        }
    }

    pub(super) fn finish_live_session(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(mut live_session) = self.live_session.take() else {
            return Ok(());
        };

        let source = self.pending_session_source;
        let summary = live_session.summary();
        if source == Some(PendingSessionSource::Calibration) {
            self.calibration_history.record_run(&summary);
        }

        self.latest_run = Some(summary);
        self.latest_run_source = source;
        self.result_row_index = 0;
        live_session.print_summary()
    }

    pub(super) fn confirm_results(&mut self) -> Result<(), Box<dyn Error>> {
        let action = self
            .result_actions()
            .get(self.result_row_index)
            .copied()
            .unwrap_or(ResultAction::MainMenu);

        match action {
            ResultAction::Retry => self.restart_live_session()?,
            ResultAction::ApplyOffset => {
                if let Some(total_offset_ms) = self.suggested_results_offset_ms() {
                    let total_offset_ms = total_offset_ms.clamp(-200.0, 200.0);
                    self.state.settings.input.input_offset_ms = total_offset_ms;
                    if let Some(options) = self.pending_session_options.as_mut() {
                        options.input_offset_ms = total_offset_ms;
                    }
                    self.persist_applied_offset(total_offset_ms);
                }
            }
            ResultAction::ClearCalibration => {
                self.calibration_history.clear();
                self.result_row_index = 0;
            }
            ResultAction::SongSelect => self.state.open_song_select(),
            ResultAction::MainMenu => self.state.open_main_menu(),
        }

        Ok(())
    }

    pub(super) fn persist_current_settings(&mut self) {
        if let Err(error) = self.persistence.save_settings(&self.state.settings) {
            println!("app_wgpu_settings_save_error={error}");
        }
    }

    pub(super) fn persist_applied_offset(&mut self, total_offset_ms: f64) {
        self.persistence.set_settings(&self.state.settings);

        if self.latest_run_source == Some(PendingSessionSource::Calibration)
            && let (Some(key), Some(aggregate)) = (
                self.calibration_history.device_key(),
                self.calibration_history.aggregate(),
            )
        {
            let saved_offset = SavedCalibrationOffset::new(
                key.clone(),
                total_offset_ms,
                aggregate.hit_count,
                aggregate.trial_count,
                aggregate.confidence.display_label(),
            );

            if let Err(error) = self.persistence.upsert_calibration_offset(saved_offset) {
                println!("app_wgpu_calibration_save_error={error}");
            }
            return;
        }

        self.persistence.set_manual_input_offset_ms(total_offset_ms);
        if let Err(error) = self.persistence.save() {
            println!("app_wgpu_settings_save_error={error}");
        }
    }

    pub(super) fn apply_saved_calibration_for_current_settings(&mut self) {
        let audio = (self.audio_resolver)(&self.state.settings);
        self.update_resolved_audio(audio, true);
    }

    pub(super) fn update_resolved_audio(&mut self, audio: Option<LiveAudioSummary>, force: bool) {
        let same_stream = match (&self.resolved_audio, &audio) {
            (Some(previous), Some(current)) => CalibrationDeviceKey::from_audio(previous)
                .same_stream_as(&CalibrationDeviceKey::from_audio(current)),
            _ => false,
        };
        self.resolved_audio = audio;
        if !same_stream {
            self.calibration_history.clear();
            self.latest_run = None;
            self.latest_run_source = None;
        }
        if force || !same_stream {
            self.state.settings.input.input_offset_ms = self
                .saved_calibration_for_current_settings()
                .map(|saved| saved.input_offset_ms)
                .unwrap_or_else(|| self.persistence.manual_input_offset_ms())
                .clamp(-200.0, 200.0);
        }
    }

    pub(super) fn saved_calibration_for_current_settings(&self) -> Option<&SavedCalibrationOffset> {
        self.resolved_audio
            .as_ref()
            .and_then(|audio| self.persistence.calibration_for_audio(audio))
    }

    pub(super) fn process_winit_lane_input(
        &mut self,
        lane: u8,
        pressed: bool,
    ) -> Result<(), Box<dyn Error>> {
        let now = Instant::now();
        let kind = if pressed {
            NativeInputEventKind::LanePress(lane)
        } else {
            NativeInputEventKind::LaneRelease(lane)
        };
        let input = NativeInputEvent {
            kind,
            source: NativeInputSource::Winit,
            timestamp_kind: NativeInputTimestampKind::ReceiptMonotonic,
            event_time: now,
            received_time: now,
            source_timestamp_ns: None,
            queue_age_ms: None,
        };

        if let Some(live_session) = self.live_session.as_mut() {
            live_session.process_input(input)?;
        } else {
            if lane as usize >= self.active_lanes.len() {
                self.active_lanes.resize(lane as usize + 1, false);
            }
            self.active_lanes[lane as usize] = pressed;
        }

        Ok(())
    }
}

impl WgpuAppShell {
    pub(super) fn result_actions(&self) -> Vec<ResultAction> {
        let mut actions = Vec::new();
        if self.pending_session_options.is_some() || self.state.selected_chart.is_some() {
            actions.push(ResultAction::Retry);
        }
        if self.suggested_results_offset_ms().is_some() {
            actions.push(ResultAction::ApplyOffset);
        }
        if self.latest_run_source == Some(PendingSessionSource::Calibration)
            && self.calibration_history.attempts() > 0
        {
            actions.push(ResultAction::ClearCalibration);
        }
        actions.push(ResultAction::SongSelect);
        actions.push(ResultAction::MainMenu);
        actions
    }

    pub(super) fn result_action_label(&self, action: ResultAction) -> String {
        match action {
            ResultAction::Retry => "RETRY RUN".to_owned(),
            ResultAction::ApplyOffset => self
                .suggested_results_offset_ms()
                .map(|offset_ms| format!("APPLY {:+.1} MS", offset_ms.clamp(-200.0, 200.0)))
                .unwrap_or_else(|| "APPLY OFFSET".to_owned()),
            ResultAction::ClearCalibration => "CLEAR TRIALS".to_owned(),
            ResultAction::SongSelect => "SONG SELECT".to_owned(),
            ResultAction::MainMenu => "MAIN MENU".to_owned(),
        }
    }

    pub(super) fn suggested_results_offset_ms(&self) -> Option<f64> {
        if self.latest_run_source == Some(PendingSessionSource::Calibration) {
            return self
                .calibration_history
                .aggregate()
                .map(|aggregate| aggregate.suggested_offset_ms);
        }

        self.latest_run
            .as_ref()
            .and_then(LiveRunSummary::suggested_total_input_offset_ms)
    }
}

pub(super) fn resolve_calibration_audio(
    settings: &super::super::settings::AppSettings,
) -> Option<LiveAudioSummary> {
    match output_stream_target(&audio_options_from_settings(settings)) {
        Ok(target) => Some(LiveAudioSummary::from_target(&target)),
        Err(error) => {
            println!("app_wgpu_calibration_device_error={error}");
            None
        }
    }
}
