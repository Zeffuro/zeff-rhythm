use super::wgpu_surface::{WgpuFrameError, WgpuSurfaceState};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WgpuRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
}

impl WgpuRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }
}

pub struct WgpuRectFrame<'a> {
    pub rects: &'a [WgpuRect],
    pub clear_color: wgpu::Color,
}

pub struct WgpuRectRenderSample {
    pub acquire_surface_ms: f64,
    pub encode_submit_present_ms: f64,
    pub vertex_count: usize,
}

pub struct WgpuRectRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_bytes: Vec<u8>,
}

impl WgpuRectRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zeff-rhythm rect shader"),
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
            label: Some("zeff-rhythm rect pipeline"),
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
        frame: WgpuRectFrame<'_>,
    ) -> Result<WgpuRectRenderSample, WgpuFrameError> {
        let surface_frame = surface.begin_frame()?;
        let acquire_surface_ms = surface_frame.acquire_surface_ms;
        let width = surface.config.width.max(1) as f32;
        let height = surface.config.height.max(1) as f32;
        let vertex_count = self.build_vertices(width, height, frame.rects);
        self.ensure_vertex_capacity(&surface.device, vertex_count);

        if vertex_count > 0 {
            surface
                .queue
                .write_buffer(&self.vertex_buffer, 0, &self.vertex_bytes);
        }

        let mut encoder = surface
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zeff-rhythm rect encoder"),
            });
        {
            let color_attachment = wgpu::RenderPassColorAttachment {
                view: &surface_frame.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(frame.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            };
            let attachments = [Some(color_attachment)];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zeff-rhythm rect pass"),
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

        Ok(WgpuRectRenderSample {
            acquire_surface_ms,
            encode_submit_present_ms,
            vertex_count,
        })
    }

    fn build_vertices(&mut self, width: f32, height: f32, rects: &[WgpuRect]) -> usize {
        self.vertex_bytes.clear();
        for rect in rects {
            push_rect(&mut self.vertex_bytes, width, height, *rect);
        }
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

fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("zeff-rhythm rect vertices"),
        size: (vertex_capacity.max(1) * VERTEX_BYTES) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn push_rect(bytes: &mut Vec<u8>, surface_width: f32, surface_height: f32, rect: WgpuRect) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let x0 = pixel_x_to_ndc(rect.x, surface_width);
    let x1 = pixel_x_to_ndc(rect.x + rect.width, surface_width);
    let y0 = pixel_y_to_ndc(rect.y, surface_height);
    let y1 = pixel_y_to_ndc(rect.y + rect.height, surface_height);
    push_vertex(bytes, x0, y0, rect.color);
    push_vertex(bytes, x1, y0, rect.color);
    push_vertex(bytes, x1, y1, rect.color);
    push_vertex(bytes, x0, y0, rect.color);
    push_vertex(bytes, x1, y1, rect.color);
    push_vertex(bytes, x0, y1, rect.color);
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

#[cfg(test)]
mod tests {
    use super::{WgpuRect, pixel_x_to_ndc, pixel_y_to_ndc, push_rect};

    #[test]
    fn converts_pixels_to_ndc() {
        assert_eq!(pixel_x_to_ndc(0.0, 200.0), -1.0);
        assert_eq!(pixel_x_to_ndc(200.0, 200.0), 1.0);
        assert_eq!(pixel_y_to_ndc(0.0, 100.0), 1.0);
        assert_eq!(pixel_y_to_ndc(100.0, 100.0), -1.0);
    }

    #[test]
    fn rectangle_emits_two_triangles() {
        let mut bytes = Vec::new();

        push_rect(
            &mut bytes,
            100.0,
            100.0,
            WgpuRect::new(10.0, 10.0, 20.0, 30.0, [1.0, 0.0, 0.0, 1.0]),
        );

        assert_eq!(bytes.len(), 6 * 6 * std::mem::size_of::<f32>());
    }
}
