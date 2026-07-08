use crate::play::metrics::print_metric;
use crate::render::settings::{
    RenderLatencySettings, RenderPresentModePreference, clamp_desired_frame_latency,
};
use crate::render::wgpu_surface::{WgpuFrameError, WgpuSurfaceState, format_present_modes};
use pollster::block_on;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const DEFAULT_WIDTH: u32 = 960;
const DEFAULT_HEIGHT: u32 = 640;
const DEFAULT_FRAMES: u32 = 120;

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let options = WgpuSmokeOptions::parse(args)?;
    let event_loop = EventLoop::new()?;
    let mut app = WgpuSmokeApp::new(options);

    event_loop.run_app(&mut app)?;

    if let Some(error) = app.error {
        return Err(error.into());
    }

    app.print_summary();
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct WgpuSmokeOptions {
    frames: u32,
    width: u32,
    height: u32,
    latency: RenderLatencySettings,
    power_preference: wgpu::PowerPreference,
}

impl Default for WgpuSmokeOptions {
    fn default() -> Self {
        Self {
            frames: DEFAULT_FRAMES,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            latency: RenderLatencySettings::default(),
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }
}

impl WgpuSmokeOptions {
    fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        let mut options = Self::default();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--frames" => {
                    options.frames = parse_u32_value(args, &mut index, "--frames")?.max(1);
                }
                "--width" => {
                    options.width = parse_u32_value(args, &mut index, "--width")?.max(1);
                }
                "--height" => {
                    options.height = parse_u32_value(args, &mut index, "--height")?.max(1);
                }
                "--present" => {
                    let value = parse_string_value(args, &mut index, "--present")?;
                    options.latency.present_mode = parse_present_mode(&value)?;
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
                unknown => {
                    return Err(format!(
                        "unknown option: {unknown}. usage: zeff-rhythm wgpu-smoke [--frames N] [--width PX] [--height PX] [--present fifo|mailbox|immediate] [--frame-latency 1..3] [--power high|low|none]"
                    )
                    .into());
                }
            }

            index += 1;
        }

        Ok(options)
    }
}

struct WgpuSmokeApp {
    options: WgpuSmokeOptions,
    gpu: Option<WgpuSurfaceState>,
    rendered_frames: u32,
    frame_interval_ms: Vec<f64>,
    acquire_surface_ms: Vec<f64>,
    encode_submit_present_ms: Vec<f64>,
    last_redraw: Option<Instant>,
    error: Option<String>,
}

impl WgpuSmokeApp {
    fn new(options: WgpuSmokeOptions) -> Self {
        Self {
            options,
            gpu: None,
            rendered_frames: 0,
            frame_interval_ms: Vec::new(),
            acquire_surface_ms: Vec::new(),
            encode_submit_present_ms: Vec::new(),
            last_redraw: None,
            error: None,
        }
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        let now = Instant::now();
        if let Some(previous) = self.last_redraw.replace(now) {
            self.frame_interval_ms
                .push(previous.elapsed().as_secs_f64() * 1_000.0);
        }

        match render_clear_frame(gpu) {
            Ok(sample) => {
                self.acquire_surface_ms.push(sample.acquire_surface_ms);
                self.encode_submit_present_ms
                    .push(sample.encode_submit_present_ms);
                self.rendered_frames += 1;

                if self.rendered_frames >= self.options.frames {
                    event_loop.exit();
                } else {
                    gpu.window.request_redraw();
                }
            }
            Err(WgpuFrameError::Recoverable(reason)) => {
                println!("wgpu_smoke_frame_skipped={reason}");
                gpu.window.request_redraw();
            }
            Err(WgpuFrameError::Fatal(reason)) => {
                self.error = Some(reason);
                event_loop.exit();
            }
        }
    }

    fn print_summary(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            let info = &gpu.adapter_info;
            println!(
                "wgpu_adapter name=\"{}\" backend={:?} device_type={:?} driver=\"{}\" driver_info=\"{}\"",
                info.name, info.backend, info.device_type, info.driver, info.driver_info
            );
            println!(
                "wgpu_surface size={}x{} format={:?} color_space={:?} alpha={:?}",
                gpu.config.width,
                gpu.config.height,
                gpu.config.format,
                gpu.config.color_space,
                gpu.config.alpha_mode
            );
            println!(
                "wgpu_present requested={} selected={:?} supported={}",
                self.options.latency.present_mode.as_str(),
                gpu.config.present_mode,
                format_present_modes(&gpu.supported_present_modes)
            );
            println!(
                "wgpu_frame_latency requested={} configured={}",
                self.options.latency.desired_maximum_frame_latency,
                gpu.config.desired_maximum_frame_latency
            );
        }

        println!("wgpu_smoke frames={}", self.rendered_frames);
        print_metric("wgpu_frame_interval_ms", &self.frame_interval_ms);
        print_metric("wgpu_acquire_surface_ms", &self.acquire_surface_ms);
        print_metric(
            "wgpu_encode_submit_present_ms",
            &self.encode_submit_present_ms,
        );
    }
}

impl ApplicationHandler for WgpuSmokeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("zeff-rhythm wgpu smoke")
            .with_inner_size(PhysicalSize::new(self.options.width, self.options.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(format!("failed to create winit window: {error}"));
                event_loop.exit();
                return;
            }
        };

        match block_on(WgpuSurfaceState::new(
            event_loop,
            window,
            self.options.latency,
            self.options.power_preference,
        )) {
            Ok(gpu) => {
                println!("wgpu_smoke=started");
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.render_frame(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

fn render_clear_frame(gpu: &mut WgpuSurfaceState) -> Result<RenderFrameSample, WgpuFrameError> {
    let frame = gpu.begin_frame()?;
    let acquire_surface_ms = frame.acquire_surface_ms;
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zeff-rhythm wgpu smoke clear"),
        });
    {
        let color_attachment = wgpu::RenderPassColorAttachment {
            view: &frame.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.020,
                    g: 0.024,
                    b: 0.030,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        };
        let attachments = [Some(color_attachment)];
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("zeff-rhythm wgpu smoke pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    let encode_submit_present_ms = gpu.submit_and_present(encoder, frame);

    Ok(RenderFrameSample {
        acquire_surface_ms,
        encode_submit_present_ms,
    })
}

struct RenderFrameSample {
    acquire_surface_ms: f64,
    encode_submit_present_ms: f64,
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

fn parse_present_mode(value: &str) -> Result<RenderPresentModePreference, Box<dyn Error>> {
    match value {
        "fifo" => Ok(RenderPresentModePreference::Fifo),
        "mailbox" => Ok(RenderPresentModePreference::Mailbox),
        "immediate" => Ok(RenderPresentModePreference::Immediate),
        _ => {
            Err(format!("invalid --present `{value}`; expected fifo, mailbox, or immediate").into())
        }
    }
}

fn parse_power_preference(value: &str) -> Result<wgpu::PowerPreference, Box<dyn Error>> {
    match value {
        "high" => Ok(wgpu::PowerPreference::HighPerformance),
        "low" => Ok(wgpu::PowerPreference::LowPower),
        "none" => Ok(wgpu::PowerPreference::None),
        _ => Err(format!("invalid --power `{value}`; expected high, low, or none").into()),
    }
}
