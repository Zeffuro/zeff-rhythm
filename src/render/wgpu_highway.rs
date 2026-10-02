use super::highway::{HighwayNoteSprite, HighwayNoteSpriteKind, HighwayRenderLayout};
use super::wgpu_surface::{WgpuFrameError, WgpuSurfaceState};

mod geometry;
use geometry::{TOP_PAD, WgpuHighwayLayout, draw_lanes, draw_notes, draw_progress, push_rect};
const VERTEX_FLOATS: usize = 6;
const VERTEX_BYTES: usize = VERTEX_FLOATS * std::mem::size_of::<f32>();
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
const SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

pub struct WgpuHighwayRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_bytes: Vec<u8>,
}

impl WgpuHighwayRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zeff-rhythm highway shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: VERTEX_BYTES as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        };
        let vertex_buffers = [Some(vertex_layout)];
        let color_targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zeff-rhythm highway pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &color_targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_capacity = 256;
        let vertex_buffer = create_vertex_buffer(device, vertex_capacity);

        Self {
            pipeline,
            vertex_buffer,
            vertex_capacity,
            vertex_bytes: Vec::with_capacity(vertex_capacity * VERTEX_BYTES),
        }
    }

    pub fn render(
        &mut self,
        surface: &mut WgpuSurfaceState,
        frame: WgpuHighwayFrame<'_>,
    ) -> Result<WgpuHighwayRenderSample, WgpuFrameError> {
        self.render_with_overlay(surface, frame, &[])
    }

    pub fn render_with_overlay(
        &mut self,
        surface: &mut WgpuSurfaceState,
        frame: WgpuHighwayFrame<'_>,
        overlay: &[super::wgpu_rects::WgpuRect],
    ) -> Result<WgpuHighwayRenderSample, WgpuFrameError> {
        self.render_with_overlay_and_artwork(surface, frame, overlay, None)
    }

    pub fn render_with_overlay_and_artwork(
        &mut self,
        surface: &mut WgpuSurfaceState,
        frame: WgpuHighwayFrame<'_>,
        overlay: &[super::wgpu_rects::WgpuRect],
        artwork: Option<&super::wgpu_artwork::WgpuArtworkRenderer>,
    ) -> Result<WgpuHighwayRenderSample, WgpuFrameError> {
        let surface_frame = surface.begin_frame()?;
        let acquire_surface_ms = surface_frame.acquire_surface_ms;
        let width = surface.config.width.max(1) as f32;
        let height = surface.config.height.max(1) as f32;
        let layout = WgpuHighwayLayout::new(width, height, frame.lane_count);
        self.build_vertices(&layout, width, height, frame);
        for rect in overlay {
            push_rect(
                &mut self.vertex_bytes,
                (width, height),
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                rect.color,
            );
        }
        let vertex_count = self.vertex_bytes.len() / VERTEX_BYTES;
        self.ensure_vertex_capacity(&surface.device, vertex_count);

        if vertex_count > 0 {
            surface
                .queue
                .write_buffer(&self.vertex_buffer, 0, &self.vertex_bytes);
        }

        let mut encoder = surface
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zeff-rhythm highway encoder"),
            });
        {
            let color_attachment = wgpu::RenderPassColorAttachment {
                view: &surface_frame.view,
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
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zeff-rhythm highway pass"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(artwork) = artwork {
                artwork.draw(&mut pass);
            }
            if vertex_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.draw(0..vertex_count as u32, 0..1);
            }
        }
        let encode_submit_present_ms = surface.submit_and_present(encoder, surface_frame);

        Ok(WgpuHighwayRenderSample {
            acquire_surface_ms,
            encode_submit_present_ms,
            vertex_count,
        })
    }

    fn build_vertices(
        &mut self,
        layout: &WgpuHighwayLayout,
        width: f32,
        height: f32,
        frame: WgpuHighwayFrame<'_>,
    ) -> usize {
        self.vertex_bytes.clear();
        draw_lanes(
            &mut self.vertex_bytes,
            layout,
            width,
            height,
            frame.active_lanes,
        );
        draw_notes(&mut self.vertex_bytes, layout, width, height, frame.sprites);
        draw_progress(
            &mut self.vertex_bytes,
            width,
            height,
            frame.song_time_seconds,
            frame.end_seconds,
        );
        self.vertex_bytes.len() / VERTEX_BYTES
    }

    fn ensure_vertex_capacity(&mut self, device: &wgpu::Device, vertex_count: usize) {
        if vertex_count <= self.vertex_capacity {
            return;
        }

        self.vertex_capacity = vertex_count.next_power_of_two();
        self.vertex_buffer = create_vertex_buffer(device, self.vertex_capacity);
    }
}

pub struct WgpuHighwayFrame<'a> {
    pub sprites: &'a [HighwayNoteSprite],
    pub lane_count: usize,
    pub active_lanes: &'a [bool],
    pub song_time_seconds: f64,
    pub end_seconds: f64,
}

pub struct WgpuHighwayRenderSample {
    pub acquire_surface_ms: f64,
    pub encode_submit_present_ms: f64,
    pub vertex_count: usize,
}

pub fn wgpu_highway_render_layout(
    width: u32,
    height: u32,
    lane_count: usize,
    lookahead_seconds: f64,
) -> HighwayRenderLayout {
    let layout = WgpuHighwayLayout::new(width.max(1) as f32, height.max(1) as f32, lane_count);
    HighwayRenderLayout::new(
        layout.lane_count,
        lookahead_seconds,
        0.180,
        TOP_PAD,
        layout.judgement_y,
    )
}

fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zeff-rhythm highway vertices"),
        size: (vertex_capacity.max(1) * VERTEX_BYTES) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
