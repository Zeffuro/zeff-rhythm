use crate::platform::audio::AudioStreamOptions;
use std::error::Error;

pub struct AudioArgs {
    pub positionals: Vec<String>,
    pub options: AudioStreamOptions,
}

pub fn parse_audio_args(args: &[String]) -> Result<AudioArgs, Box<dyn Error>> {
    let mut positionals = Vec::new();
    let mut options = AudioStreamOptions::default();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--host" => {
                options.selection.host = Some(next_value(args, &mut index, "--host")?);
            }
            "--device" => {
                options.selection.device = Some(next_value(args, &mut index, "--device")?);
            }
            "--sample-rate" => {
                let value = next_value(args, &mut index, "--sample-rate")?;
                options.sample_rate = Some(parse_u32_arg(&value, "sample rate")?);
            }
            "--buffer" | "--buffer-frames" => {
                let value = next_value(args, &mut index, arg)?;
                options.buffer_frames = Some(parse_u32_arg(&value, "buffer frames")?);
            }
            _ if arg.starts_with("--host=") => {
                options.selection.host = Some(arg["--host=".len()..].to_owned());
            }
            _ if arg.starts_with("--device=") => {
                options.selection.device = Some(arg["--device=".len()..].to_owned());
            }
            _ if arg.starts_with("--sample-rate=") => {
                let value = &arg["--sample-rate=".len()..];
                options.sample_rate = Some(parse_u32_arg(value, "sample rate")?);
            }
            _ if arg.starts_with("--buffer=") => {
                let value = &arg["--buffer=".len()..];
                options.buffer_frames = Some(parse_u32_arg(value, "buffer frames")?);
            }
            _ if arg.starts_with("--") => return Err(format!("unknown option: {arg}").into()),
            _ => positionals.push(arg.clone()),
        }

        index += 1;
    }

    Ok(AudioArgs {
        positionals,
        options,
    })
}

pub fn parse_optional_f64(
    value: Option<&String>,
    default: f64,
    name: &str,
) -> Result<f64, Box<dyn Error>> {
    match value {
        Some(value) => value
            .parse()
            .map_err(|_| format!("invalid {name}: {value}").into()),
        None => Ok(default),
    }
}

pub fn parse_required_path(args: &[String], usage: &str) -> Result<String, Box<dyn Error>> {
    match args {
        [path] => Ok(path.clone()),
        _ => Err(format!("usage: zeff-rhythm {usage}").into()),
    }
}

pub fn parse_u32_arg(value: &str, name: &str) -> Result<u32, Box<dyn Error>> {
    value
        .parse()
        .map_err(|_| format!("invalid {name}: {value}").into())
}

pub fn parse_f64_arg(value: &str, name: &str) -> Result<f64, Box<dyn Error>> {
    value
        .parse()
        .map_err(|_| format!("invalid {name}: {value}").into())
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String, Box<dyn Error>> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("missing value for {option}").into())
}
