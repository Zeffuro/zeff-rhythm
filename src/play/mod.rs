mod asset_loader;
mod assets;
mod calibration;
mod chart_clock;
mod chart_format;
mod judgement_counts;
mod live_session;
pub(crate) mod metrics;
mod preview;
mod report;
mod sdl_highway;
mod session;
mod session_options;
mod terminal_highway;
mod wgpu_preview;

pub use asset_loader::AssetLoadJob;
pub(crate) use assets::load_cached_audio;
pub use assets::{
    PreparedChartAssets, load_chart, load_chart_at_index, load_chart_audio_assets,
    load_play_session_assets,
};
pub use calibration::CalibrationPattern;
pub(crate) use calibration::build_generated_calibration_assets;
pub use chart_format::{ChartFormat, parse_chart_format_option};
pub use judgement_counts::JudgementCounts;
pub(crate) use live_session::LiveSessionAssets;
pub use live_session::{LiveAudioSummary, LivePlaySession, LiveRunSummary};
pub use preview::{PlaySessionPreview, load_play_session_preview};
pub use report::{PlayReport, PlayReportSummary};
pub use sdl_highway::SdlPlayWindow;
pub use session::{
    PreparedPlaySessionSummary, prepare_play_session, run_generated_calibration_session,
    run_play_session,
};
pub(crate) use session_options::audio_options_from_settings;
pub use session_options::{PlayDisplayMode, PlaySessionOptions};
pub use terminal_highway::{
    HighwaySnapshot, TerminalHighway, active_lanes, judgement_message, push_message,
};
pub use wgpu_preview::{
    DEFAULT_WGPU_PREVIEW_HEIGHT, DEFAULT_WGPU_PREVIEW_MAX_SECONDS, DEFAULT_WGPU_PREVIEW_WIDTH,
    WgpuPreviewRunOptions, run_wgpu_preview,
};
