use crate::platform::input::NativeInputBackendKind;
use crate::play::{
    ChartFormat, DEFAULT_WGPU_PREVIEW_HEIGHT, DEFAULT_WGPU_PREVIEW_MAX_SECONDS,
    DEFAULT_WGPU_PREVIEW_WIDTH, PlayDisplayMode, PlaySessionOptions, WgpuPreviewRunOptions,
    parse_chart_format_option, run_wgpu_preview,
};
use crate::render::settings::{
    RenderLatencySettings, RenderPresentModePreference, clamp_desired_frame_latency,
};
use std::error::Error;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let options = WgpuPreviewCliOptions::parse(args)?;
    let session_options = PlaySessionOptions {
        chart_path: options.chart_path,
        format: options.format,
        audio_path: None,
        input_offset_ms: 0.0,
        max_seconds: Some(options.max_seconds),
        lookahead_seconds: options.lookahead_seconds,
        lead_in_seconds: None,
        chart_start_seconds: None,
        start_delay_seconds: None,
        display: PlayDisplayMode::Sdl,
        input: NativeInputBackendKind::Sdl,
        event_log_path: None,
        dry_run: true,
        audio: Default::default(),
    };
    let run_options = WgpuPreviewRunOptions::new(session_options)
        .with_latency(options.latency)
        .with_window_size(options.width, options.height);

    run_wgpu_preview(WgpuPreviewRunOptions {
        power_preference: options.power_preference,
        ..run_options
    })
}

#[derive(Clone, Debug)]
struct WgpuPreviewCliOptions {
    chart_path: PathBuf,
    format: Option<ChartFormat>,
    width: u32,
    height: u32,
    max_seconds: f64,
    lookahead_seconds: f64,
    latency: RenderLatencySettings,
    power_preference: wgpu::PowerPreference,
}

impl WgpuPreviewCliOptions {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            chart_path: PathBuf::from(".local_assets/stepmania/speedcore/Speedcore.sm"),
            format: None,
            width: DEFAULT_WGPU_PREVIEW_WIDTH,
            height: DEFAULT_WGPU_PREVIEW_HEIGHT,
            max_seconds: DEFAULT_WGPU_PREVIEW_MAX_SECONDS,
            lookahead_seconds: 4.0,
            latency: RenderLatencySettings::default(),
            power_preference: wgpu::PowerPreference::HighPerformance,
        };
        let mut chart_path_seen = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--format" => {
                    let value = parse_string_value(args, &mut index, "--format")?;
                    options.format = parse_chart_format_option(&value)?;
                }
                "--width" => {
                    options.width = parse_u32_value(args, &mut index, "--width")?.max(1);
                }
                "--height" => {
                    options.height = parse_u32_value(args, &mut index, "--height")?.max(1);
                }
                "--max-seconds" => {
                    options.max_seconds =
                        parse_f64_value(args, &mut index, "--max-seconds")?.max(0.1);
                }
                "--lookahead-seconds" => {
                    options.lookahead_seconds =
                        parse_f64_value(args, &mut index, "--lookahead-seconds")?.max(0.1);
                }
                "--present" => {
                    let value = parse_string_value(args, &mut index, "--present")?;
                    options.latency.present_mode =
                        RenderPresentModePreference::parse(&value).map_err(|error| {
                            format!(
                                "{error}; usage: zeff-rhythm wgpu-preview [chart-path] [--format auto|osu|sm] [--max-seconds S] [--lookahead-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]"
                            )
                        })?;
                }
                "--frame-latency" => {
                    options.latency.desired_maximum_frame_latency = clamp_desired_frame_latency(
                        parse_u32_value(args, &mut index, "--frame-latency")?,
                    );
                }
                "--power" => {
                    let value = parse_string_value(args, &mut index, "--power")?;
                    options.power_preference = parse_power_preference(&value)?;
                }
                value if value.starts_with("--") => {
                    return Err(format!("unknown option: {value}").into());
                }
                value => {
                    if chart_path_seen {
                        return Err("usage: zeff-rhythm wgpu-preview [chart-path] [options]".into());
                    }
                    options.chart_path = PathBuf::from(value);
                    chart_path_seen = true;
                }
            }

            index += 1;
        }

        Ok(options)
    }
}

fn parse_u32_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<u32, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    value
        .parse::<u32>()
        .map_err(|error| format!("invalid {name} value `{value}`: {error}").into())
}

fn parse_f64_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<f64, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    value
        .parse::<f64>()
        .map_err(|error| format!("invalid {name} value `{value}`: {error}").into())
}

fn parse_string_value(
    args: &[String],
    index: &mut usize,
    name: &'static str,
) -> Result<String, Box<dyn Error>> {
    *index += 1;
    let Some(value) = args.get(*index) else {
        return Err(format!("{name} requires a value").into());
    };

    Ok(value.clone())
}

fn parse_power_preference(value: &str) -> Result<wgpu::PowerPreference, Box<dyn Error>> {
    match value {
        "high" => Ok(wgpu::PowerPreference::HighPerformance),
        "low" => Ok(wgpu::PowerPreference::LowPower),
        "none" => Ok(wgpu::PowerPreference::None),
        _ => Err(format!("invalid --power `{value}`; expected high, low, or none").into()),
    }
}
