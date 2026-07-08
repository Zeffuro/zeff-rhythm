use super::highway::{HighwayNoteSprite, HighwayNoteSpriteKind, HighwayRenderLayout};
use super::wgpu_surface::{WgpuFrameError, WgpuSurfaceState};

const TOP_PAD: f32 = 44.0;
const BOTTOM_PAD: f32 = 92.0;
const LANE_GAP: f32 = 8.0;
const LINE_HEIGHT: f32 = 8.0;
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
        let surface_frame = surface.begin_frame()?;
        let acquire_surface_ms = surface_frame.acquire_surface_ms;
        let width = surface.config.width.max(1) as f32;
        let height = surface.config.height.max(1) as f32;
        let layout = WgpuHighwayLayout::new(width, height, frame.lane_count);
        let vertex_count = self.build_vertices(&layout, width, height, frame);
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct WgpuHighwayLayout {
    lane_count: usize,
    lane_width: f32,
    lane_start_x: f32,
    judgement_y: f32,
    height: f32,
}

impl WgpuHighwayLayout {
    fn new(width: f32, height: f32, lane_count: usize) -> Self {
        let lane_count = lane_count.max(1);
        let usable_width = (width - 128.0).max(320.0);
        let total_gap = LANE_GAP * lane_count.saturating_sub(1) as f32;
        let lane_width = ((usable_width - total_gap) / lane_count as f32).clamp(52.0, 110.0);
        let total_width = lane_width * lane_count as f32 + total_gap;
        let lane_start_x = (width - total_width) * 0.5;
        let judgement_y = height - BOTTOM_PAD;

        Self {
            lane_count,
            lane_width,
            lane_start_x,
            judgement_y,
            height,
        }
    }

    fn lane_x(self, lane: usize) -> f32 {
        self.lane_start_x + lane as f32 * (self.lane_width + LANE_GAP)
    }
}

fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zeff-rhythm highway vertices"),
        size: (vertex_capacity.max(1) * VERTEX_BYTES) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn draw_lanes(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    width: f32,
    height: f32,
    active_lanes: &[bool],
) {
    for lane in 0..layout.lane_count {
        let x = layout.lane_x(lane);
        let active = active_lanes.get(lane).copied().unwrap_or(false);
        let lane_color = if active {
            rgba(0.19, 0.24, 0.25, 1.0)
        } else {
            rgba(0.12, 0.14, 0.17, 1.0)
        };
        push_rect(
            bytes,
            (width, height),
            x,
            TOP_PAD,
            layout.lane_width,
            layout.judgement_y - TOP_PAD + 52.0,
            lane_color,
        );
        push_rect(
            bytes,
            (width, height),
            x,
            layout.judgement_y,
            layout.lane_width,
            LINE_HEIGHT,
            rgba(0.88, 0.86, 0.78, 1.0),
        );
        push_rect(
            bytes,
            (width, height),
            x,
            TOP_PAD,
            1.0,
            layout.height,
            rgba(0.22, 0.25, 0.29, 1.0),
        );
        push_rect(
            bytes,
            (width, height),
            x + layout.lane_width - 1.0,
            TOP_PAD,
            1.0,
            layout.height,
            rgba(0.22, 0.25, 0.29, 1.0),
        );
    }
}

fn draw_notes(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    width: f32,
    height: f32,
    sprites: &[HighwayNoteSprite],
) {
    for sprite in sprites {
        match sprite.kind {
            HighwayNoteSpriteKind::Tap => {
                draw_tap(
                    bytes,
                    layout,
                    width,
                    height,
                    sprite.lane,
                    sprite.y,
                    sprite.delta_seconds,
                );
            }
            HighwayNoteSpriteKind::Hold { end_y } => {
                draw_hold(
                    bytes,
                    layout,
                    (width, height),
                    sprite.lane,
                    sprite.y,
                    end_y,
                    sprite.delta_seconds,
                );
            }
        }
    }
}

fn draw_tap(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    width: f32,
    height: f32,
    lane: usize,
    y: f32,
    delta_seconds: f64,
) {
    let x = layout.lane_x(lane) + 8.0;
    let rect_width = layout.lane_width - 16.0;
    push_rect(
        bytes,
        (width, height),
        x,
        y - 7.0,
        rect_width,
        14.0,
        note_color(delta_seconds),
    );
}

fn draw_hold(
    bytes: &mut Vec<u8>,
    layout: &WgpuHighwayLayout,
    surface_size: (f32, f32),
    lane: usize,
    start_y: f32,
    end_y: f32,
    delta_seconds: f64,
) {
    let first_y = end_y
        .min(start_y)
        .clamp(TOP_PAD, layout.height - BOTTOM_PAD);
    let last_y = end_y
        .max(start_y)
        .clamp(TOP_PAD, layout.height - BOTTOM_PAD);
    let x = layout.lane_x(lane) + layout.lane_width * 0.5 - 8.0;
    push_rect(
        bytes,
        surface_size,
        x,
        first_y,
        16.0,
        (last_y - first_y).max(8.0),
        rgba(0.24, 0.45, 0.75, 1.0),
    );
    draw_tap(
        bytes,
        layout,
        surface_size.0,
        surface_size.1,
        lane,
        start_y,
        delta_seconds,
    );
}

fn draw_progress(
    bytes: &mut Vec<u8>,
    width: f32,
    height: f32,
    song_time_seconds: f64,
    end_seconds: f64,
) {
    let progress = if end_seconds > 0.0 {
        (song_time_seconds / end_seconds).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let bar_width = width - 48.0;
    let y = height - 30.0;
    push_rect(
        bytes,
        (width, height),
        24.0,
        y,
        bar_width,
        8.0,
        rgba(0.16, 0.19, 0.22, 1.0),
    );
    push_rect(
        bytes,
        (width, height),
        24.0,
        y,
        (bar_width * progress).max(1.0),
        8.0,
        rgba(0.38, 0.75, 0.69, 1.0),
    );
}

fn push_rect(
    bytes: &mut Vec<u8>,
    surface_size: (f32, f32),
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let (surface_width, surface_height) = surface_size;
    let x0 = pixel_x_to_ndc(x, surface_width);
    let x1 = pixel_x_to_ndc(x + width, surface_width);
    let y0 = pixel_y_to_ndc(y, surface_height);
    let y1 = pixel_y_to_ndc(y + height, surface_height);
    push_vertex(bytes, x0, y0, color);
    push_vertex(bytes, x1, y0, color);
    push_vertex(bytes, x1, y1, color);
    push_vertex(bytes, x0, y0, color);
    push_vertex(bytes, x1, y1, color);
    push_vertex(bytes, x0, y1, color);
}

fn push_vertex(bytes: &mut Vec<u8>, x: f32, y: f32, color: [f32; 4]) {
    for value in [x, y, color[0], color[1], color[2], color[3]] {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
}

fn pixel_x_to_ndc(x: f32, width: f32) -> f32 {
    x / width * 2.0 - 1.0
}

fn pixel_y_to_ndc(y: f32, height: f32) -> f32 {
    1.0 - y / height * 2.0
}

fn note_color(delta_seconds: f64) -> [f32; 4] {
    if delta_seconds < -0.050 {
        rgba(0.77, 0.29, 0.29, 1.0)
    } else if delta_seconds.abs() <= 0.050 {
        rgba(0.45, 0.86, 0.56, 1.0)
    } else {
        rgba(0.37, 0.80, 0.85, 1.0)
    }
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> [f32; 4] {
    [r, g, b, a]
}

#[cfg(test)]
mod tests {
    use super::{WgpuHighwayLayout, pixel_x_to_ndc, pixel_y_to_ndc};

    #[test]
    fn converts_pixels_to_ndc() {
        assert_eq!(pixel_x_to_ndc(0.0, 100.0), -1.0);
        assert_eq!(pixel_x_to_ndc(100.0, 100.0), 1.0);
        assert_eq!(pixel_y_to_ndc(0.0, 100.0), 1.0);
        assert_eq!(pixel_y_to_ndc(100.0, 100.0), -1.0);
    }

    #[test]
    fn lays_out_lanes_with_stable_widths() {
        let layout = WgpuHighwayLayout::new(960.0, 640.0, 4);

        assert_eq!(layout.lane_count, 4);
        assert!(layout.lane_width >= 52.0);
        assert!(layout.judgement_y > 500.0);
    }
}
