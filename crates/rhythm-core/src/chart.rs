#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartMetadata {
    pub title: String,
    pub artist: String,
    pub title_unicode: Option<String>,
    pub artist_unicode: Option<String>,
    pub source: Option<String>,
    pub difficulty: Option<String>,
    pub audio_filename: Option<String>,
    pub background_filename: Option<String>,
    pub banner_filename: Option<String>,
    pub preview_start_seconds: Option<f64>,
    pub preview_duration_seconds: Option<f64>,
}

impl ChartMetadata {
    pub fn display_title(&self) -> &str {
        self.title_unicode
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&self.title)
    }

    pub fn display_artist(&self) -> &str {
        self.artist_unicode
            .as_deref()
            .filter(|artist| !artist.trim().is_empty())
            .unwrap_or(&self.artist)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Beat(pub f64);

impl Beat {
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    pub const fn as_f64(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LaneIndex(u8);

impl LaneIndex {
    pub const fn new(index: u8) -> Self {
        Self(index)
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteId(u32);

impl NoteId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteKind {
    Tap,
    Hold { end_time_seconds: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub lane: LaneIndex,
    pub time_seconds: f64,
    pub kind: NoteKind,
}

impl Note {
    pub const fn tap(id: NoteId, lane: LaneIndex, time_seconds: f64) -> Self {
        Self {
            id,
            lane,
            time_seconds,
            kind: NoteKind::Tap,
        }
    }

    pub const fn hold(
        id: NoteId,
        lane: LaneIndex,
        time_seconds: f64,
        end_time_seconds: f64,
    ) -> Self {
        Self {
            id,
            lane,
            time_seconds,
            kind: NoteKind::Hold { end_time_seconds },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingPoint {
    pub beat: Beat,
    pub time_seconds: f64,
    pub beat_length_seconds: f64,
}

impl TimingPoint {
    pub const fn new(beat: Beat, time_seconds: f64, beat_length_seconds: f64) -> Self {
        Self {
            beat,
            time_seconds,
            beat_length_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingStop {
    pub beat: Beat,
    pub duration_seconds: f64,
}

impl TimingStop {
    pub const fn new(beat: Beat, duration_seconds: f64) -> Self {
        Self {
            beat,
            duration_seconds,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    metadata: ChartMetadata,
    lane_count: u8,
    notes: Vec<Note>,
    timing_points: Vec<TimingPoint>,
    timing_stops: Vec<TimingStop>,
}

impl Chart {
    pub fn new(lane_count: u8) -> Self {
        assert!(lane_count > 0, "charts need at least one lane");

        Self {
            metadata: ChartMetadata::default(),
            lane_count,
            notes: Vec::new(),
            timing_points: vec![TimingPoint::new(Beat::new(0.0), 0.0, 0.5)],
            timing_stops: Vec::new(),
        }
    }

    pub fn metadata(&self) -> &ChartMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut ChartMetadata {
        &mut self.metadata
    }

    pub const fn lane_count(&self) -> u8 {
        self.lane_count
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn timing_points(&self) -> &[TimingPoint] {
        &self.timing_points
    }

    pub fn timing_stops(&self) -> &[TimingStop] {
        &self.timing_stops
    }

    pub fn push_note(&mut self, note: Note) {
        assert!(
            note.lane.as_u8() < self.lane_count,
            "note lane is outside chart lane count"
        );

        self.notes.push(note);
        self.notes
            .sort_by(|left, right| left.time_seconds.total_cmp(&right.time_seconds));
    }

    pub fn push_timing_point(&mut self, timing_point: TimingPoint) {
        assert!(
            timing_point.beat_length_seconds > 0.0,
            "beat length must be positive"
        );

        self.timing_points.push(timing_point);
        self.timing_points
            .sort_by(|left, right| left.beat.0.total_cmp(&right.beat.0));
    }

    pub fn set_timing_points(&mut self, mut timing_points: Vec<TimingPoint>) {
        assert!(!timing_points.is_empty(), "charts need timing data");
        assert!(
            timing_points
                .iter()
                .all(|point| point.beat_length_seconds > 0.0),
            "beat lengths must be positive"
        );

        timing_points.sort_by(|left, right| left.beat.0.total_cmp(&right.beat.0));
        self.timing_points = timing_points;
    }

    pub fn set_timing_stops(&mut self, mut timing_stops: Vec<TimingStop>) {
        assert!(
            timing_stops.iter().all(|stop| stop.duration_seconds >= 0.0),
            "stop durations must be non-negative"
        );

        timing_stops.sort_by(|left, right| left.beat.0.total_cmp(&right.beat.0));
        self.timing_stops = timing_stops;
    }

    pub fn time_at_beat(&self, beat: Beat) -> f64 {
        let base_time = self.base_time_at_beat(beat);
        let stop_time: f64 = self
            .timing_stops
            .iter()
            .filter(|stop| stop.beat < beat)
            .map(|stop| stop.duration_seconds)
            .sum();

        base_time + stop_time
    }

    fn base_time_at_beat(&self, beat: Beat) -> f64 {
        let point = self
            .timing_points
            .iter()
            .rev()
            .find(|point| point.beat <= beat)
            .unwrap_or(&self.timing_points[0]);

        let beat_delta = beat.as_f64() - point.beat.as_f64();
        point.time_seconds + beat_delta * point.beat_length_seconds
    }

    pub fn beat_at_time(&self, time_seconds: f64) -> Beat {
        let mut adjusted_time = time_seconds;
        let mut accumulated_stop_time = 0.0;

        for stop in &self.timing_stops {
            let stop_start_time = self.base_time_at_beat(stop.beat) + accumulated_stop_time;
            let stop_end_time = stop_start_time + stop.duration_seconds;

            if time_seconds < stop_start_time {
                break;
            }

            if time_seconds <= stop_end_time {
                return stop.beat;
            }

            adjusted_time -= stop.duration_seconds;
            accumulated_stop_time += stop.duration_seconds;
        }

        self.base_beat_at_time(adjusted_time)
    }

    fn base_beat_at_time(&self, time_seconds: f64) -> Beat {
        let point = self
            .timing_points
            .iter()
            .rev()
            .find(|point| point.time_seconds <= time_seconds)
            .unwrap_or(&self.timing_points[0]);

        let time_delta = time_seconds - point.time_seconds;
        Beat::new(point.beat.as_f64() + time_delta / point.beat_length_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_prefers_native_text_with_empty_and_missing_fallbacks() {
        let mut metadata = ChartMetadata {
            title: "Tsuki".to_owned(),
            artist: "Hoshi".to_owned(),
            title_unicode: Some("月".to_owned()),
            artist_unicode: Some("星".to_owned()),
            ..ChartMetadata::default()
        };
        assert_eq!(metadata.display_title(), "月");
        assert_eq!(metadata.display_artist(), "星");
        metadata.title_unicode = Some("   ".to_owned());
        metadata.artist_unicode = None;
        assert_eq!(metadata.display_title(), "Tsuki");
        assert_eq!(metadata.display_artist(), "Hoshi");
    }

    #[test]
    fn converts_between_beat_and_time() {
        let mut chart = Chart::new(4);
        chart.push_timing_point(TimingPoint::new(Beat::new(4.0), 2.0, 0.25));

        assert_eq!(chart.time_at_beat(Beat::new(2.0)), 1.0);
        assert_eq!(chart.time_at_beat(Beat::new(6.0)), 2.5);
        assert_eq!(chart.beat_at_time(2.5), Beat::new(6.0));
    }

    #[test]
    fn accounts_for_timing_stops() {
        let mut chart = Chart::new(4);
        chart.set_timing_stops(vec![TimingStop::new(Beat::new(2.0), 1.5)]);

        assert_eq!(chart.time_at_beat(Beat::new(2.0)), 1.0);
        assert_eq!(chart.time_at_beat(Beat::new(3.0)), 3.0);
        assert_eq!(chart.beat_at_time(1.25), Beat::new(2.0));
        assert_eq!(chart.beat_at_time(3.0), Beat::new(3.0));
    }
}
