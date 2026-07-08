use super::settings::RenderLatencySettings;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub struct WgpuSurfaceState {
    pub window: Arc<Window>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub adapter_info: wgpu::AdapterInfo,
    pub supported_present_modes: Vec<wgpu::PresentMode>,
    surface: wgpu::Surface<'static>,
}

impl WgpuSurfaceState {
    pub async fn new(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        latency: RenderLatencySettings,
        power_preference: wgpu::PowerPreference,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::from_window_without_display(
                window.clone(),
            ))
            .map_err(|error| format!("failed to create wgpu surface: {error:?}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| format!("failed to request wgpu adapter: {error:?}"))?;
        let adapter_info = adapter.get_info();
        let caps = surface.get_capabilities(&adapter);
        let size = window.inner_size();
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or("wgpu surface returned no supported formats")?;
        let present_mode = latency.select_supported_wgpu_present_mode(&caps.present_modes);
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("zeff-rhythm wgpu device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("failed to request wgpu device: {error:?}"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: latency.desired_maximum_frame_latency,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            adapter_info,
            supported_present_modes: caps.present_modes,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn apply_latency(&mut self, latency: RenderLatencySettings) {
        self.config.present_mode =
            latency.select_supported_wgpu_present_mode(&self.supported_present_modes);
        self.config.desired_maximum_frame_latency = latency.desired_maximum_frame_latency;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn begin_frame(&mut self) -> Result<WgpuSurfaceFrame, WgpuFrameError> {
        if self.config.width == 0 || self.config.height == 0 {
            return Err(WgpuFrameError::Recoverable(
                "surface size is zero".to_owned(),
            ));
        }

        let acquire_start = Instant::now();
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => {
                return Err(WgpuFrameError::Recoverable(
                    "surface acquire timed out".to_owned(),
                ));
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                return Err(WgpuFrameError::Recoverable("surface occluded".to_owned()));
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Err(WgpuFrameError::Recoverable(
                    "surface reconfigured".to_owned(),
                ));
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(WgpuFrameError::Fatal(
                    "surface acquisition validation error".to_owned(),
                ));
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let acquire_surface_ms = acquire_start.elapsed().as_secs_f64() * 1_000.0;

        Ok(WgpuSurfaceFrame {
            texture,
            view,
            acquire_surface_ms,
        })
    }

    pub fn submit_and_present(
        &self,
        encoder: wgpu::CommandEncoder,
        frame: WgpuSurfaceFrame,
    ) -> f64 {
        let start = Instant::now();
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame.texture);
        start.elapsed().as_secs_f64() * 1_000.0
    }
}

pub struct WgpuSurfaceFrame {
    pub view: wgpu::TextureView,
    pub acquire_surface_ms: f64,
    texture: wgpu::SurfaceTexture,
}

#[derive(Debug)]
pub enum WgpuFrameError {
    Recoverable(String),
    Fatal(String),
}

pub fn format_present_modes(modes: &[wgpu::PresentMode]) -> String {
    modes
        .iter()
        .map(|mode| format!("{mode:?}"))
        .collect::<Vec<_>>()
        .join(",")
}
