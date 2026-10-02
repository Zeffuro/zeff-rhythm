use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{
    BufferSize, Device, DeviceId, Host, HostId, SampleFormat, StreamConfig, SupportedBufferSize,
    SupportedStreamConfig, SupportedStreamConfigRange,
};
use std::error::Error;
use std::str::FromStr;

#[derive(Clone, Debug, Default)]
pub struct AudioDeviceSelection {
    pub host: Option<String>,
    pub device: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct AudioStreamOptions {
    pub selection: AudioDeviceSelection,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
}

pub struct OutputStreamTarget {
    pub host_name: String,
    pub device: Device,
    pub device_id: Option<String>,
    pub sample_format: SampleFormat,
    pub config: StreamConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    pub host_name: String,
    pub name: String,
    pub id: Option<String>,
    pub is_default: bool,
}

pub fn print_latency_probe(selection: &AudioDeviceSelection) -> Result<(), Box<dyn Error>> {
    println!("latency probe");

    for host_id in selected_host_ids(selection)? {
        println!("host={}", host_id.name());

        let host = match cpal::host_from_id(host_id) {
            Ok(host) => host,
            Err(error) => {
                println!("  unavailable: {error}");
                continue;
            }
        };

        match host.default_output_device() {
            Some(device) if device_matches(&device, selection.device.as_deref()) => {
                println!("  default_output={device}");
                print_device_id(&device);
                print_default_output_config(&device);
            }
            Some(_) => println!("  default_output=<filtered>"),
            None => println!("  default_output=<none>"),
        }

        match host.output_devices() {
            Ok(devices) => {
                for (index, device) in devices
                    .filter(|device| device_matches(device, selection.device.as_deref()))
                    .enumerate()
                {
                    println!("  output[{index}]={device}");
                    print_device_id(&device);
                    print_supported_output_configs(&device);
                }
            }
            Err(error) => println!("  output_devices_error={error}"),
        }
    }

    Ok(())
}

pub fn output_stream_target(
    options: &AudioStreamOptions,
) -> Result<OutputStreamTarget, Box<dyn Error>> {
    let (host_name, device) = output_device(&options.selection)?;
    let device_id = device.id().ok().map(|id| id.to_string());
    let supported_config = device.default_output_config()?;
    let sample_format = supported_config.sample_format();
    let mut config: StreamConfig = supported_config.into();

    if let Some(sample_rate) = options.sample_rate {
        config.sample_rate = sample_rate;
    }

    if let Some(buffer_frames) = options.buffer_frames {
        config.buffer_size = BufferSize::Fixed(buffer_frames);
    }

    Ok(OutputStreamTarget {
        host_name,
        device,
        device_id,
        sample_format,
        config,
    })
}

pub fn list_output_devices(
    selection: &AudioDeviceSelection,
) -> Result<Vec<OutputDeviceInfo>, Box<dyn Error>> {
    let (host_name, host) = selected_host(selection)?;
    let default_device = host.default_output_device();
    let default_id = default_device
        .as_ref()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let default_name = default_device.as_ref().map(ToString::to_string);
    let mut devices = Vec::new();

    for device in host.output_devices()? {
        if !device_matches(&device, selection.device.as_deref()) {
            continue;
        }

        let name = device.to_string();
        let id = device.id().ok().map(|id| id.to_string());
        let is_default = match (&id, &default_id) {
            (Some(id), Some(default_id)) => id == default_id,
            _ => default_name.as_deref() == Some(name.as_str()),
        };

        devices.push(OutputDeviceInfo {
            host_name: host_name.clone(),
            name,
            id,
            is_default,
        });
    }

    Ok(devices)
}

pub fn print_target_summary(target: &OutputStreamTarget) {
    println!("host={}", target.host_name);
    println!("device={}", target.device);

    if let Some(device_id) = &target.device_id {
        println!("device_id={device_id}");
    }

    println!(
        "config=format={:?} channels={} sample_rate={} buffer={:?}",
        target.sample_format,
        target.config.channels,
        target.config.sample_rate,
        target.config.buffer_size
    );
}

fn output_device(selection: &AudioDeviceSelection) -> Result<(String, Device), Box<dyn Error>> {
    if let Some(device_query) = selection.device.as_deref() {
        if let Ok(device_id) = DeviceId::from_str(device_query) {
            return output_device_by_id(selection, device_id);
        }
    }

    let (host_name, host) = selected_host(selection)?;
    let device = match selection.device.as_deref() {
        Some(device_query) => host
            .output_devices()?
            .find(|device| device_matches(device, Some(device_query)))
            .ok_or_else(|| {
                format!(
                    "no output device matching \"{device_query}\" on host {host_name}; run latency-probe to list devices"
                )
            })?,
        None => host.default_output_device().ok_or("no default output device")?,
    };

    Ok((host_name, device))
}

fn output_device_by_id(
    selection: &AudioDeviceSelection,
    device_id: DeviceId,
) -> Result<(String, Device), Box<dyn Error>> {
    if let Some(host) = selection.host.as_deref() {
        let requested_host = HostId::from_str(host)?;
        if requested_host != device_id.host() {
            return Err(format!(
                "device id host {} does not match requested host {}",
                device_id.host(),
                requested_host
            )
            .into());
        }
    }

    let host_id = device_id.host();
    let host = cpal::host_from_id(host_id)?;
    let device = host
        .device_by_id(&device_id)
        .ok_or_else(|| format!("output device id not found: {device_id}"))?;

    Ok((host_id.name().to_owned(), device))
}

fn selected_host(selection: &AudioDeviceSelection) -> Result<(String, Host), Box<dyn Error>> {
    match selection.host.as_deref() {
        Some(host) => {
            let host_id = HostId::from_str(host)?;
            Ok((host_id.name().to_owned(), cpal::host_from_id(host_id)?))
        }
        None => {
            let host = cpal::default_host();
            Ok((host.id().name().to_owned(), host))
        }
    }
}

fn selected_host_ids(selection: &AudioDeviceSelection) -> Result<Vec<HostId>, Box<dyn Error>> {
    match selection.host.as_deref() {
        Some(host) => Ok(vec![HostId::from_str(host)?]),
        None => Ok(cpal::available_hosts()),
    }
}

fn device_matches(device: &Device, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };

    let query = query.to_ascii_lowercase();
    let name_matches = device
        .to_string()
        .to_ascii_lowercase()
        .contains(query.as_str());
    let id_matches = device
        .id()
        .ok()
        .map(|id| id.to_string().to_ascii_lowercase().contains(query.as_str()))
        .unwrap_or(false);

    name_matches || id_matches
}

fn print_device_id(device: &Device) {
    match device.id() {
        Ok(id) => println!("    id={id}"),
        Err(error) => println!("    id_error={error}"),
    }
}

fn print_default_output_config(device: &Device) {
    match device.default_output_config() {
        Ok(config) => println!("    default_config={}", describe_supported_config(&config)),
        Err(error) => println!("    default_config_error={error}"),
    }
}

fn print_supported_output_configs(device: &Device) {
    match device.supported_output_configs() {
        Ok(configs) => {
            for config in configs.take(12) {
                println!("    supported={}", describe_config_range(&config));
            }
        }
        Err(error) => println!("    supported_configs_error={error}"),
    }
}

fn describe_supported_config(config: &SupportedStreamConfig) -> String {
    format!(
        "format={:?} channels={} sample_rate={} buffer={}",
        config.sample_format(),
        config.channels(),
        config.sample_rate(),
        describe_supported_buffer(config.buffer_size(), config.sample_rate())
    )
}

fn describe_config_range(config: &SupportedStreamConfigRange) -> String {
    format!(
        "format={:?} channels={} sample_rate={}..{} buffer={}",
        config.sample_format(),
        config.channels(),
        config.min_sample_rate(),
        config.max_sample_rate(),
        describe_supported_buffer(config.buffer_size(), config.max_sample_rate())
    )
}

fn describe_supported_buffer(buffer: &SupportedBufferSize, sample_rate: u32) -> String {
    match *buffer {
        SupportedBufferSize::Range { min, max } => {
            let min_ms = frames_to_ms(min, sample_rate);
            let max_ms = frames_to_ms(max, sample_rate);
            format!("{min}..{max} frames ({min_ms:.3}..{max_ms:.3} ms)")
        }
        SupportedBufferSize::Unknown => "unknown".to_owned(),
    }
}

fn frames_to_ms(frames: u32, sample_rate: u32) -> f64 {
    frames as f64 * 1_000.0 / sample_rate as f64
}
