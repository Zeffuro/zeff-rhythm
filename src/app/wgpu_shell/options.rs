use super::*;

#[derive(Clone, Debug)]
pub(super) struct WgpuShellOptions {
    pub(super) library_roots: Vec<std::path::PathBuf>,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) max_seconds: Option<f64>,
    pub(super) start_preview: bool,
    pub(super) start_calibration: bool,
    pub(super) latency: RenderLatencySettings,
    pub(super) latency_overridden: bool,
    pub(super) power_preference: wgpu::PowerPreference,
}

impl Default for WgpuShellOptions {
    fn default() -> Self {
        Self {
            library_roots: Vec::new(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            max_seconds: None,
            start_preview: false,
            start_calibration: false,
            latency: RenderLatencySettings::default(),
            latency_overridden: false,
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }
}

impl WgpuShellOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self::default();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--library" => {
                    options
                        .library_roots
                        .push(parse_string_value(args, &mut index, "--library")?.into());
                }
                "--width" => {
                    options.width = parse_u32_value(args, &mut index, "--width")?.max(1);
                }
                "--height" => {
                    options.height = parse_u32_value(args, &mut index, "--height")?.max(1);
                }
                "--max-seconds" => {
                    options.max_seconds =
                        Some(parse_f64_value(args, &mut index, "--max-seconds")?.max(0.1));
                }
                "--preview" => {
                    options.start_preview = true;
                }
                "--calibration" => {
                    options.start_calibration = true;
                }
                "--present" => {
                    let value = parse_string_value(args, &mut index, "--present")?;
                    options.latency.present_mode = parse_present_mode(&value)?;
                    options.latency_overridden = true;
                }
                "--frame-latency" => {
                    options.latency.desired_maximum_frame_latency = clamp_desired_frame_latency(
                        parse_u32_value(args, &mut index, "--frame-latency")?,
                    );
                    options.latency_overridden = true;
                }
                "--power" => {
                    let value = parse_string_value(args, &mut index, "--power")?;
                    options.power_preference = parse_power_preference(&value)?;
                }
                unknown => {
                    return Err(format!(
                        "unknown option: {unknown}. usage: zeff-rhythm app-wgpu [--preview|--calibration] [--max-seconds S] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]"
                    )
                    .into());
                }
            }

            index += 1;
        }

        Ok(options)
    }
}

pub(super) fn parse_u32_value(
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

pub(super) fn parse_f64_value(
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

pub(super) fn parse_string_value(
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

pub(super) fn parse_present_mode(
    value: &str,
) -> Result<RenderPresentModePreference, Box<dyn Error>> {
    match value {
        "fifo" => Ok(RenderPresentModePreference::Fifo),
        "mailbox" => Ok(RenderPresentModePreference::Mailbox),
        "immediate" => Ok(RenderPresentModePreference::Immediate),
        _ => {
            Err(format!("invalid --present `{value}`; expected fifo, mailbox, or immediate").into())
        }
    }
}

pub(super) fn parse_power_preference(value: &str) -> Result<wgpu::PowerPreference, Box<dyn Error>> {
    match value {
        "high" => Ok(wgpu::PowerPreference::HighPerformance),
        "low" => Ok(wgpu::PowerPreference::LowPower),
        "none" => Ok(wgpu::PowerPreference::None),
        _ => Err(format!("invalid --power `{value}`; expected high, low, or none").into()),
    }
}
