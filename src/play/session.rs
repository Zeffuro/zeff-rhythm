use super::chart_clock::{ChartClock, hard_end_seconds};
mod input;
use super::assets::LoadedPlaySession;
use super::{
    CalibrationPattern, HighwaySnapshot, JudgementCounts, PlayDisplayMode as PlayDisplay,
    PlayReport, PlaySessionOptions, SdlPlayWindow, TerminalHighway,
    build_generated_calibration_assets, judgement_message, load_play_session_assets, push_message,
};
use crate::platform::audio::{
    AudioClip, OutputStreamTarget, PlaybackVolume, build_clip_stream_with_volume,
    print_target_summary,
};
use crate::platform::input::{
    NativeInputBackend, NativeInputBackendKind, NativeInputEvent, NativeInputEventKind,
};
use input::{handle_lane_press, record_judgement, record_message};
use rhythm_core::{
    Chart, GameKey, InputEvent as CoreInputEvent, JudgementPhase, JudgementResult,
    JudgementWindows, LaneIndex, NoteKind, RhythmEngine,
};
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPlaySessionSummary {
    pub title: String,
    pub artist: String,
    pub chart_path: PathBuf,
    pub audio_path: PathBuf,
    pub lane_count: u8,
    pub note_count: usize,
    pub hold_count: usize,
    pub audio_duration_seconds: f64,
    pub output_device: String,
}

pub fn run_play_session(options: PlaySessionOptions) -> Result<(), Box<dyn Error>> {
    if options.display == PlayDisplay::AppWgpu {
        return Err("app_wgpu sessions are event-loop owned; use `app` or `app-wgpu`".into());
    }

    let assets = SessionRunAssets::from(load_play_session_assets(&options)?);
    run_play_session_with_assets(options, assets)
}

pub fn run_generated_calibration_session(
    options: PlaySessionOptions,
    pattern: CalibrationPattern,
) -> Result<(), Box<dyn Error>> {
    if options.display == PlayDisplay::AppWgpu {
        return Err("app_wgpu sessions are event-loop owned; use `app` or `app-wgpu`".into());
    }

    let assets = build_generated_calibration_assets(&options.audio, pattern)?;
    run_play_session_with_assets(
        options,
        SessionRunAssets {
            chart: assets.chart,
            chart_format_label: "GeneratedCalibration".to_owned(),
            audio_path_label: assets.audio_label,
            clip: assets.clip,
            target: assets.target,
        },
    )
}

fn run_play_session_with_assets(
    options: PlaySessionOptions,
    assets: SessionRunAssets,
) -> Result<(), Box<dyn Error>> {
    println!("play_map={}", options.display.as_str());
    println!("chart_format={}", assets.chart_format_label);
    print_chart_summary(&options.chart_path, &assets.chart);
    println!("audio_path={}", assets.audio_path_label);
    println!(
        "audio_clip=sample_rate={} channels={} frames={} duration={:.6}s",
        assets.clip.sample_rate,
        assets.clip.channels,
        assets.clip.frame_count(),
        assets.clip.duration_seconds()
    );
    print_target_summary(&assets.target);
    let effective_input = options.effective_input();
    println!("display={}", options.display.as_str());
    println!("input={}", effective_input.as_str());
    println!(
        "event_log={}",
        options
            .event_log_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned())
    );
    println!("input_offset_ms={:.3}", options.input_offset_ms);
    println!("lookahead_seconds={:.3}", options.lookahead_seconds);
    println!("lead_in_seconds={:.3}", options.effective_lead_in_seconds());
    println!(
        "chart_start_seconds={:.3}",
        options.chart_start_seconds(&assets.chart)
    );
    println!(
        "start_delay_seconds={:.3}",
        options.effective_start_delay_seconds()
    );
    println!("dry_run={}", options.dry_run);
    println!("keys=d/f/j/k lanes=0..3, quit=esc/q");
    if options.display == PlayDisplay::Sdl {
        println!("display_window=focus the SDL play window for visuals and timestamped input");
    } else if effective_input == NativeInputBackendKind::Sdl {
        println!("input_window=focus the SDL window for timestamped keyboard input");
    }

    if options.dry_run {
        return Ok(());
    }

    let (stream, clock) = build_clip_stream_with_volume(
        &assets.target,
        Arc::clone(&assets.clip),
        PlaybackVolume::new(options.volume),
    )?;

    let windows = JudgementWindows::default();
    let chart_start_seconds = options.chart_start_seconds(&assets.chart);
    let hard_end_seconds = match options.max_seconds {
        Some(seconds) => seconds,
        None => hard_end_seconds(
            &assets.chart,
            assets.clip.duration_seconds(),
            windows,
            options.input_offset_ms,
        ),
    };
    let mut engine = RhythmEngine::new(assets.chart, windows);
    let mut counts = JudgementCounts::default();
    let mut judged_note_ids = HashSet::new();
    let mut messages = VecDeque::new();
    let mut misses = Vec::new();
    let mut pending_inputs = Vec::new();
    let mut report = PlayReport::new(options.event_log_path.as_deref())?;
    let mut chart_clock = ChartClock::new(
        clock,
        chart_start_seconds,
        options.effective_start_delay_seconds(),
    );
    let mut highway = match options.display {
        PlayDisplay::Highway => Some(TerminalHighway::enter(options.lookahead_seconds)?),
        PlayDisplay::AppWgpu | PlayDisplay::Log | PlayDisplay::Sdl => None,
    };
    let mut sdl_window = match options.display {
        PlayDisplay::Sdl => Some(SdlPlayWindow::enter(options.lookahead_seconds)?),
        PlayDisplay::AppWgpu | PlayDisplay::Highway | PlayDisplay::Log => None,
    };

    if matches!(
        options.display,
        PlayDisplay::Highway | PlayDisplay::Sdl | PlayDisplay::AppWgpu
    ) {
        push_message(
            &mut messages,
            format!(
                "lookahead {:.1}s; hit notes at the === line",
                options.lookahead_seconds
            ),
        );
    }

    let mut input_backend = if options.display == PlayDisplay::Sdl {
        None
    } else {
        Some(NativeInputBackend::new(effective_input)?)
    };
    if let Some(sdl_window) = sdl_window.as_ref() {
        report.record_initial_focus(
            sdl_window.has_input_focus(),
            chart_clock.chart_time_seconds(),
        )?;
    }

    let mut last_status = Instant::now();
    let mut last_render = Instant::now();

    'playback: loop {
        chart_clock.start_audio_if_due(&stream)?;
        let countdown_active = chart_clock.countdown_seconds().is_some();

        pending_inputs.clear();
        if let Some(sdl_window) = sdl_window.as_mut() {
            sdl_window.poll_input(&mut pending_inputs)?;
        } else if let Some(input_backend) = input_backend.as_mut() {
            input_backend.poll(&mut pending_inputs)?;
        }

        for input in pending_inputs.drain(..) {
            match input.kind {
                NativeInputEventKind::Quit => break 'playback,
                NativeInputEventKind::LanePress(lane) => {
                    let input_chart_time = chart_clock.chart_time_at(input.event_time);
                    report.record_input_press(lane, input, input_chart_time)?;

                    if countdown_active {
                        continue;
                    }

                    handle_lane_press(
                        lane,
                        input,
                        &chart_clock,
                        &mut engine,
                        &mut counts,
                        &mut judged_note_ids,
                        &mut messages,
                        &mut report,
                        &options,
                        options.display,
                    )?;
                }
                NativeInputEventKind::LaneRelease(lane) => {
                    let input_chart_time = chart_clock.chart_time_at(input.event_time);
                    report.record_input_release(lane, input, input_chart_time)?;
                    if let Some(result) = engine.submit_input(CoreInputEvent {
                        key: GameKey::Lane(LaneIndex::new(lane)),
                        pressed: false,
                        time_seconds: options.judgement_time_seconds(input_chart_time),
                    }) {
                        report.record_hold_tail(result, Some(input), input_chart_time)?;
                        record_judgement(
                            "hold end",
                            result,
                            &mut counts,
                            &mut judged_note_ids,
                            &mut messages,
                            options.display,
                        );
                    }
                }
                NativeInputEventKind::FocusGained => {
                    let input_chart_time = chart_clock.chart_time_at(input.event_time);
                    report.record_focus_gained(input, input_chart_time)?;
                    record_message(
                        "sdl focus gained".to_owned(),
                        &mut messages,
                        options.display,
                    );
                }
                NativeInputEventKind::FocusLost => {
                    let input_chart_time = chart_clock.chart_time_at(input.event_time);
                    report.record_focus_lost(input, input_chart_time)?;
                    for lane in 0..engine.chart().lane_count() {
                        if let Some(result) = engine.submit_input(CoreInputEvent {
                            key: GameKey::Lane(LaneIndex::new(lane)),
                            pressed: false,
                            time_seconds: options.judgement_time_seconds(input_chart_time),
                        }) {
                            report.record_hold_tail(result, Some(input), input_chart_time)?;
                            record_judgement(
                                "hold end",
                                result,
                                &mut counts,
                                &mut judged_note_ids,
                                &mut messages,
                                options.display,
                            );
                        }
                    }
                    record_message(
                        "sdl focus lost; click window for input".to_owned(),
                        &mut messages,
                        options.display,
                    );
                }
            }
        }

        let chart_time_seconds = chart_clock.chart_time_seconds();
        if !countdown_active {
            misses.clear();
            engine.collect_judgements(
                options.judgement_time_seconds(chart_time_seconds),
                &mut misses,
            );
            for miss in misses.drain(..) {
                if miss.phase == JudgementPhase::HoldTail {
                    report.record_hold_tail(miss, None, chart_time_seconds)?;
                } else {
                    report.record_miss(miss, chart_time_seconds)?;
                }
                record_judgement(
                    if miss.phase == JudgementPhase::HoldTail {
                        "hold complete"
                    } else {
                        "miss"
                    },
                    miss,
                    &mut counts,
                    &mut judged_note_ids,
                    &mut messages,
                    options.display,
                );
            }
        }

        match options.display {
            PlayDisplay::Log if last_status.elapsed() >= Duration::from_millis(500) => {
                print_status(&chart_clock, &engine, hard_end_seconds);
                report.record_audio_output_latency_seconds(chart_clock.output_latency_seconds());
                last_status = Instant::now();
            }
            PlayDisplay::Highway if last_render.elapsed() >= Duration::from_millis(16) => {
                let render_start = Instant::now();
                let frame_time_ms =
                    render_start.duration_since(last_render).as_secs_f64() * 1_000.0;
                render_highway(
                    &mut highway,
                    &chart_clock,
                    &engine,
                    &counts,
                    &judged_note_ids,
                    &messages,
                    hard_end_seconds,
                )?;
                report.record_render_sample(
                    frame_time_ms,
                    render_start.elapsed().as_secs_f64() * 1_000.0,
                    chart_clock.output_latency_seconds(),
                );
                last_render = render_start;
            }
            PlayDisplay::Sdl if last_render.elapsed() >= Duration::from_millis(16) => {
                let render_start = Instant::now();
                let frame_time_ms =
                    render_start.duration_since(last_render).as_secs_f64() * 1_000.0;
                render_sdl(
                    &mut sdl_window,
                    &chart_clock,
                    &engine,
                    &counts,
                    &judged_note_ids,
                    &messages,
                    hard_end_seconds,
                )?;
                report.record_render_sample(
                    frame_time_ms,
                    render_start.elapsed().as_secs_f64() * 1_000.0,
                    chart_clock.output_latency_seconds(),
                );
                last_render = render_start;
            }
            _ => {}
        }

        if (engine.is_complete() && chart_clock.is_finished())
            || chart_time_seconds >= hard_end_seconds
        {
            break;
        }
    }

    drop(highway);
    drop(sdl_window);
    drop(input_backend);
    drop(stream);

    println!("judged={}", engine.judged_count());
    println!("complete={}", engine.is_complete());
    println!("marvelous={}", counts.marvelous);
    println!("perfect={}", counts.perfect);
    println!("great={}", counts.great);
    println!("good={}", counts.good);
    println!("miss={}", counts.miss);
    report.print_summary();
    report.flush()?;
    if let Some(path) = options.event_log_path.as_ref() {
        println!("event_log_written={}", path.display());
    }

    Ok(())
}

struct SessionRunAssets {
    chart: Chart,
    chart_format_label: String,
    audio_path_label: String,
    clip: Arc<AudioClip>,
    target: OutputStreamTarget,
}

impl From<LoadedPlaySession> for SessionRunAssets {
    fn from(assets: LoadedPlaySession) -> Self {
        Self {
            chart: assets.chart,
            chart_format_label: format!("{:?}", assets.chart_format),
            audio_path_label: assets.audio_path.display().to_string(),
            clip: assets.clip,
            target: assets.target,
        }
    }
}

pub fn prepare_play_session(
    options: &PlaySessionOptions,
) -> Result<PreparedPlaySessionSummary, Box<dyn Error>> {
    let assets = load_play_session_assets(options)?;
    let metadata = assets.chart.metadata();
    let hold_count = assets
        .chart
        .notes()
        .iter()
        .filter(|note| matches!(note.kind, NoteKind::Hold { .. }))
        .count();

    Ok(PreparedPlaySessionSummary {
        title: metadata.title.clone(),
        artist: metadata.artist.clone(),
        chart_path: options.chart_path.clone(),
        audio_path: assets.audio_path,
        lane_count: assets.chart.lane_count(),
        note_count: assets.chart.notes().len(),
        hold_count,
        audio_duration_seconds: assets.clip.duration_seconds(),
        output_device: assets.target.device.to_string(),
    })
}

fn render_highway(
    highway: &mut Option<TerminalHighway>,
    clock: &ChartClock,
    engine: &RhythmEngine,
    counts: &JudgementCounts,
    judged_note_ids: &HashSet<u32>,
    messages: &VecDeque<String>,
    hard_end_seconds: f64,
) -> Result<(), Box<dyn Error>> {
    let Some(highway) = highway.as_mut() else {
        return Ok(());
    };

    highway.render(HighwaySnapshot {
        chart: engine.chart(),
        judged_note_ids,
        counts,
        messages,
        active_lanes: std::array::from_fn(|lane| {
            engine.lane_is_pressed(LaneIndex::new(lane as u8))
        }),
        song_time_seconds: clock.chart_time_seconds(),
        scheduled_time_seconds: clock.scheduled_time_seconds(),
        audio_duration_seconds: clock.duration_seconds(),
        end_seconds: hard_end_seconds,
        judged_count: engine.judged_count(),
        output_latency_seconds: clock.output_latency_seconds(),
        countdown_seconds: clock.countdown_seconds(),
    })
}

fn render_sdl(
    sdl_window: &mut Option<SdlPlayWindow>,
    clock: &ChartClock,
    engine: &RhythmEngine,
    counts: &JudgementCounts,
    judged_note_ids: &HashSet<u32>,
    messages: &VecDeque<String>,
    hard_end_seconds: f64,
) -> Result<(), Box<dyn Error>> {
    let Some(sdl_window) = sdl_window.as_mut() else {
        return Ok(());
    };

    sdl_window.render(HighwaySnapshot {
        chart: engine.chart(),
        judged_note_ids,
        counts,
        messages,
        active_lanes: std::array::from_fn(|lane| {
            engine.lane_is_pressed(LaneIndex::new(lane as u8))
        }),
        song_time_seconds: clock.chart_time_seconds(),
        scheduled_time_seconds: clock.scheduled_time_seconds(),
        audio_duration_seconds: clock.duration_seconds(),
        end_seconds: hard_end_seconds,
        judged_count: engine.judged_count(),
        output_latency_seconds: clock.output_latency_seconds(),
        countdown_seconds: clock.countdown_seconds(),
    })
}

fn print_status(clock: &ChartClock, engine: &RhythmEngine, end: f64) {
    let latency_ms = clock
        .output_latency_seconds()
        .map(|seconds| format!("{:.3}", seconds * 1_000.0))
        .unwrap_or_else(|| "-".to_owned());

    println!(
        "status song_time={:.3}s scheduled={:.3}s audio_duration={:.3}s end={end:.3}s judged={}/{} started={} finished={} output_latency_ms={latency_ms}",
        clock.chart_time_seconds(),
        clock.scheduled_time_seconds(),
        clock.duration_seconds(),
        engine.judged_count(),
        engine.chart().notes().len(),
        clock.is_started(),
        clock.is_finished()
    );
}

fn print_chart_summary(path: impl AsRef<Path>, chart: &rhythm_core::Chart) {
    println!("chart={}", path.as_ref().display());
    println!("title={}", empty_dash(&chart.metadata().title));
    println!("artist={}", empty_dash(&chart.metadata().artist));
    println!(
        "audio={}",
        chart.metadata().audio_filename.as_deref().unwrap_or("-")
    );
    println!("lanes={}", chart.lane_count());
    println!("notes={}", chart.notes().len());
    println!("holds={}", hold_count(chart));
    println!("timing_points={}", chart.timing_points().len());
    println!("timing_stops={}", chart.timing_stops().len());

    if let Some(first) = chart.notes().first() {
        println!(
            "first_note id={} lane={} time={:.6}s",
            first.id.as_u32(),
            first.lane.as_u8(),
            first.time_seconds
        );
    }

    if let Some(last) = chart.notes().last() {
        println!(
            "last_note id={} lane={} time={:.6}s",
            last.id.as_u32(),
            last.lane.as_u8(),
            last.time_seconds
        );
    }
}

fn hold_count(chart: &rhythm_core::Chart) -> usize {
    chart
        .notes()
        .iter()
        .filter(|note| matches!(note.kind, NoteKind::Hold { .. }))
        .count()
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}
