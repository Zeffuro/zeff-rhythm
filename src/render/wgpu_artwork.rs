use crate::app::artwork::{ArtworkFit, DecodedArtwork};

#[cfg(test)]
mod tests;

pub struct WgpuArtworkRenderer {
    pipeline: wgpu::RenderPipeline,
    parameters: wgpu::Buffer,
    sampler: wgpu::Sampler,
    bind_group: Option<wgpu::BindGroup>,
    image_size: Option<(u32, u32)>,
    fit: ArtworkFit,
}

impl WgpuArtworkRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zeff-rhythm artwork shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wgpu_artwork/shader.wgsl").into()),
        });
        let targets = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zeff-rhythm artwork pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zeff-rhythm artwork parameters"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zeff-rhythm artwork sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            parameters,
            sampler,
            bind_group: None,
            image_size: None,
            fit: ArtworkFit::Cover,
        }
    }

    pub fn set_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: Option<&DecodedArtwork>,
    ) {
        self.bind_group = None;
        self.image_size = None;
        let Some(image) = image else { return };
        let max_size = device.limits().max_texture_dimension_2d;
        if image.width == 0
            || image.height == 0
            || image.width > max_size
            || image.height > max_size
            || u64::from(image.width) * u64::from(image.height) * 4 != image.rgba.len() as u64
        {
            return;
        }
        let size = wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zeff-rhythm artwork texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zeff-rhythm artwork bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.parameters.as_entire_binding(),
                },
            ],
        }));
        self.image_size = Some((image.width, image.height));
        self.fit = image.fit;
    }

    pub fn prepare(&self, queue: &wgpu::Queue, width: u32, height: u32, brightness: f32) {
        let Some(image_size) = self.image_size else {
            return;
        };
        let (uv, scale) = artwork_geometry(image_size, (width, height), self.fit);
        let brightness = if brightness.is_finite() {
            brightness.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut bytes = [0; 32];
        for (index, value) in [
            uv[0], uv[1], uv[2], uv[3], brightness, scale[0], scale[1], 0.0,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
        queue.write_buffer(&self.parameters, 0, &bytes);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

fn artwork_geometry(
    image: (u32, u32),
    viewport: (u32, u32),
    fit: ArtworkFit,
) -> ([f32; 4], [f32; 2]) {
    let uv = cover_uv(image, viewport);
    match fit {
        ArtworkFit::Cover => (uv, [1.0, 1.0]),
        ArtworkFit::Contain => ([0.0, 0.0, 1.0, 1.0], [uv[3], uv[2]]),
    }
}

fn cover_uv(image: (u32, u32), viewport: (u32, u32)) -> [f32; 4] {
    let image_aspect = image.0.max(1) as f32 / image.1.max(1) as f32;
    let view_aspect = viewport.0.max(1) as f32 / viewport.1.max(1) as f32;
    let (width, height) = if image_aspect > view_aspect {
        (view_aspect / image_aspect, 1.0)
    } else {
        (1.0, image_aspect / view_aspect)
    };
    [(1.0 - width) / 2.0, (1.0 - height) / 2.0, width, height]
}
