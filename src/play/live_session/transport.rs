use super::*;

impl LivePlaySession {
    pub fn is_paused(&self) -> bool {
        self.pause.is_paused()
    }

    pub fn pause(&mut self) -> Result<(), Box<dyn Error>> {
        if self.finished {
            return Ok(());
        }
        let required = std::array::from_fn(|lane| {
            self.engine.lane_has_active_hold(LaneIndex::new(lane as u8))
        });
        if self.pause.pause(required) {
            self.chart_clock.pause(Instant::now());
            self.report
                .record_transport("pause", self.chart_clock.chart_time_seconds())?;
            self.report.flush()?;
        }
        Ok(())
    }

    pub fn request_resume(&mut self) -> Result<(), Box<dyn Error>> {
        if self.pause.request_resume(Instant::now()) {
            self.report
                .record_transport("resume_countdown", self.chart_clock.chart_time_seconds())?;
        }
        Ok(())
    }

    pub fn pause_countdown_seconds(&self) -> Option<f64> {
        self.pause.countdown_seconds(Instant::now())
    }

    pub fn missing_hold_keys(&self) -> [bool; 4] {
        self.pause.missing_hold_keys()
    }

    pub fn has_focus(&self) -> bool {
        self.pause.is_focused()
    }

    pub(super) fn update_transport(&mut self) -> Result<(), Box<dyn Error>> {
        let now = Instant::now();
        if self.pause.resume_was_interrupted() {
            self.pause()?;
        }
        if self.pause.ready_to_resume(now) {
            self.engine
                .synchronize_pressed_lanes(&self.pause.physical_lanes);
            self.chart_clock.resume(now);
            self.pause.awaiting_audio();
        }
        if self.chart_clock.is_running() && self.pause.audio_is_running() {
            self.report
                .record_transport("resume", self.chart_clock.chart_time_seconds())?;
            self.report.flush()?;
        }
        Ok(())
    }
}
