use super::{PlaySessionOptions, PlaySessionPreview, load_play_session_preview};
use crate::play::metrics::print_metric;
use crate::render::highway::build_highway_note_sprites;
use crate::render::settings::RenderLatencySettings;
use crate::render::wgpu_highway::{
    WgpuHighwayFrame, WgpuHighwayRenderer, wgpu_highway_render_layout,
};
use crate::render::wgpu_surface::{WgpuFrameError, WgpuSurfaceState, format_present_modes};
use pollster::block_on;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub const DEFAULT_WGPU_PREVIEW_WIDTH: u32 = 960;
pub const DEFAULT_WGPU_PREVIEW_HEIGHT: u32 = 720;
pub const DEFAULT_WGPU_PREVIEW_MAX_SECONDS: f64 = 30.0;

#[derive(Clone, Debug)]
pub struct WgpuPreviewRunOptions {
    pub session: PlaySessionOptions,
    pub width: u32,
    pub height: u32,
    pub max_seconds: f64,
    pub latency: RenderLatencySettings,
    pub power_preference: wgpu::PowerPreference,
}

impl WgpuPreviewRunOptions {
    pub fn new(session: PlaySessionOptions) -> Self {
        let max_seconds = session
            .max_seconds
            .unwrap_or(DEFAULT_WGPU_PREVIEW_MAX_SECONDS)
            .max(0.1);

        Self {
            session,
            width: DEFAULT_WGPU_PREVIEW_WIDTH,
            height: DEFAULT_WGPU_PREVIEW_HEIGHT,
            max_seconds,
            latency: RenderLatencySettings::default(),
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }

    pub fn with_latency(mut self, latency: RenderLatencySettings) -> Self {
        self.latency = latency;
        self
    }

    pub fn with_window_size(mut self, width: u32, height: u32) -> Self {
        self.width = width.max(1);
        self.height = height.max(1);
        self
    }
}

pub fn run_wgpu_preview(options: WgpuPreviewRunOptions) -> Result<(), Box<dyn Error>> {
    let preview = load_play_session_preview(&options.session)?;
    let end_seconds = preview
        .chart
        .notes()
        .last()
        .map(|note| note.time_seconds + 1.0)
        .unwrap_or(options.max_seconds);
    let event_loop = EventLoop::new()?;
    let mut app = WgpuPreviewApp::new(options, preview, end_seconds);

    event_loop.run_app(&mut app)?;

    if let Some(error) = app.error {
        return Err(error.into());
    }

    app.print_summary();
    Ok(())
}

struct WgpuPreviewApp {
    options: WgpuPreviewRunOptions,
    preview: PlaySessionPreview,
    end_seconds: f64,
    gpu: Option<WgpuSurfaceState>,
    renderer: Option<WgpuHighwayRenderer>,
    start: Option<Instant>,
    last_redraw: Option<Instant>,
    judged_note_ids: HashSet<u32>,
    active_lanes: Vec<bool>,
    frame_interval_ms: Vec<f64>,
    acquire_surface_ms: Vec<f64>,
    encode_submit_present_ms: Vec<f64>,
    vertices_per_frame: Vec<f64>,
    rendered_frames: u32,
    error: Option<String>,
}

impl WgpuPreviewApp {
    fn new(options: WgpuPreviewRunOptions, preview: PlaySessionPreview, end_seconds: f64) -> Self {
        let active_lanes = vec![false; preview.lane_count.max(1) as usize];

        Self {
            options,
            preview,
            end_seconds,
            gpu: None,
            renderer: None,
            start: None,
            last_redraw: None,
            judged_note_ids: HashSet::new(),
            active_lanes,
            frame_interval_ms: Vec::new(),
            acquire_surface_ms: Vec::new(),
            encode_submit_present_ms: Vec::new(),
            vertices_per_frame: Vec::new(),
            rendered_frames: 0,
            error: None,
        }
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gpu), Some(renderer), Some(start)) = (
            self.gpu.as_mut(),
            self.renderer.as_mut(),
            self.start.as_ref(),
        ) else {
            return;
        };

        let now = Instant::now();
        if let Some(previous) = self.last_redraw.replace(now) {
            self.frame_interval_ms
                .push(previous.elapsed().as_secs_f64() * 1_000.0);
        }

        let elapsed_seconds = start.elapsed().as_secs_f64();
        let song_time_seconds = self.preview.chart_start_seconds + elapsed_seconds;
        let layout = wgpu_highway_render_layout(
            gpu.config.width,
            gpu.config.height,
            self.preview.lane_count as usize,
            self.options.session.lookahead_seconds,
        );
        let sprites = build_highway_note_sprites(
            layout,
            &self.preview.chart,
            &self.judged_note_ids,
            song_time_seconds,
        );
        let frame = WgpuHighwayFrame {
            sprites: &sprites,
            lane_count: self.preview.lane_count as usize,
            active_lanes: &self.active_lanes,
            song_time_seconds,
            end_seconds: self.end_seconds,
        };

        match renderer.render(gpu, frame) {
            Ok(sample) => {
                self.acquire_surface_ms.push(sample.acquire_surface_ms);
                self.encode_submit_present_ms
                    .push(sample.encode_submit_present_ms);
                self.vertices_per_frame.push(sample.vertex_count as f64);
                self.rendered_frames += 1;

                if elapsed_seconds >= self.options.max_seconds
                    || song_time_seconds >= self.end_seconds
                {
                    event_loop.exit();
                } else {
                    gpu.window.set_title(&format!(
                        "zeff-rhythm wgpu preview | {} | time {:.3}s | notes {}",
                        self.preview.title,
                        song_time_seconds,
                        sprites.len()
                    ));
                    gpu.window.request_redraw();
                }
            }
            Err(WgpuFrameError::Recoverable(reason)) => {
                println!("wgpu_preview_frame_skipped={reason}");
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
            println!(
                "wgpu_preview_adapter name=\"{}\" backend={:?} device_type={:?}",
                gpu.adapter_info.name, gpu.adapter_info.backend, gpu.adapter_info.device_type
            );
            println!(
                "wgpu_preview_surface size={}x{} format={:?}",
                gpu.config.width, gpu.config.height, gpu.config.format
            );
            println!(
                "wgpu_preview_present requested={} selected={:?} supported={}",
                self.options.latency.present_mode.as_str(),
                gpu.config.present_mode,
                format_present_modes(&gpu.supported_present_modes)
            );
        }

        println!(
            "wgpu_preview chart={} title={} frames={} end_seconds={:.3}",
            self.options.session.chart_path.display(),
            self.preview.title,
            self.rendered_frames,
            self.end_seconds
        );
        print_metric("wgpu_preview_frame_interval_ms", &self.frame_interval_ms);
        print_metric("wgpu_preview_acquire_surface_ms", &self.acquire_surface_ms);
        print_metric(
            "wgpu_preview_encode_submit_present_ms",
            &self.encode_submit_present_ms,
        );
        print_metric("wgpu_preview_vertices", &self.vertices_per_frame);
    }
}

impl ApplicationHandler for WgpuPreviewApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("zeff-rhythm wgpu preview")
            .with_inner_size(PhysicalSize::new(self.options.width, self.options.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(format!("failed to create winit window: {error}"));
                event_loop.exit();
                return;
            }
        };
        let gpu = match block_on(WgpuSurfaceState::new(
            event_loop,
            window,
            self.options.latency,
            self.options.power_preference,
        )) {
            Ok(gpu) => gpu,
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        };
        let renderer = WgpuHighwayRenderer::new(&gpu.device, gpu.config.format);

        println!(
            "wgpu_preview=started chart={} title={} lanes={} notes={} holds={} start={:.3}s",
            self.options.session.chart_path.display(),
            self.preview.title,
            self.preview.lane_count,
            self.preview.note_count,
            self.preview.hold_count,
            self.preview.chart_start_seconds
        );
        gpu.window.request_redraw();
        self.start = Some(Instant::now());
        self.renderer = Some(renderer);
        self.gpu = Some(gpu);
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
