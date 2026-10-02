use super::*;

#[test]
fn aspect_cover_crops_only_the_long_axis_and_stays_centered() {
    assert_eq!(cover_uv((1600, 900), (1600, 900)), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(cover_uv((200, 100), (100, 100)), [0.25, 0.0, 0.5, 1.0]);
    assert_eq!(cover_uv((100, 200), (100, 100)), [0.0, 0.25, 1.0, 0.5]);
    for image in [(256, 80), (1920, 1080), (640, 480), (900, 1600)] {
        for viewport in [(760, 520), (960, 640), (1920, 1080)] {
            let [left, top, width, height] = cover_uv(image, viewport);
            assert!(left >= 0.0 && top >= 0.0 && width > 0.0 && height > 0.0);
            assert!(width <= 1.0 && height <= 1.0);
            assert!((left * 2.0 + width - 1.0).abs() < 1e-6);
            assert!((top * 2.0 + height - 1.0).abs() < 1e-6);
            let source_aspect = width * image.0 as f32 / (height * image.1 as f32);
            let target_aspect = viewport.0 as f32 / viewport.1 as f32;
            assert!((source_aspect - target_aspect).abs() < 1e-6);
        }
    }
}

#[test]
fn zero_viewport_does_not_produce_invalid_coordinates() {
    assert!(cover_uv((0, 0), (0, 0)).into_iter().all(f32::is_finite));
}

#[test]
fn wide_banners_remain_fully_visible_inside_the_viewport() {
    let (uv, scale) = artwork_geometry((256, 80), (960, 640), ArtworkFit::Contain);
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(scale, [1.0, 0.46875]);
    assert_eq!(scale[0] * 960.0, 960.0);
    assert_eq!(scale[1] * 640.0, 300.0);
    let (_, scale) = artwork_geometry((80, 256), (960, 640), ArtworkFit::Contain);
    assert_eq!(scale[1], 1.0);
    assert!((scale[0] * 960.0 - 200.0).abs() < 1e-4);
}

#[test]
#[ignore = "requires a native GPU adapter"]
fn native_gpu_upload_brightness_orientation_and_clear() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = WgpuArtworkRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut image = DecodedArtwork {
        width: 2,
        height: 2,
        rgba: vec![
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
        ],
        fit: ArtworkFit::Cover,
    };
    renderer.set_image(&device, &queue, Some(&image));
    renderer.prepare(&queue, 64, 64, 0.25);
    let pixels = render_pixels(&device, &queue, &renderer);
    assert_eq!(&pixels[..4], &[64, 0, 0, 255]);
    assert_eq!(&pixels[63 * 256..63 * 256 + 4], &[0, 64, 0, 255]);
    image.width = 4;
    image.height = 1;
    image.rgba = vec![255; 16];
    image.fit = ArtworkFit::Contain;
    renderer.set_image(&device, &queue, Some(&image));
    renderer.prepare(&queue, 64, 64, 1.0);
    let pixels = render_pixels(&device, &queue, &renderer);
    assert_eq!(&pixels[..4], &[0, 0, 255, 255]);
    assert_eq!(&pixels[32 * 256..32 * 256 + 4], &[255; 4]);
    assert_eq!(&pixels[32 * 256 + 63 * 4..32 * 256 + 64 * 4], &[255; 4]);
    renderer.set_image(&device, &queue, None);
    let pixels = render_pixels(&device, &queue, &renderer);
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255])
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn render_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &WgpuArtworkRenderer,
) -> Vec<u8> {
    let size = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &attachments,
            ..Default::default()
        });
        renderer.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        size,
    );
    let submission_index = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission_index),
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    buffer.slice(..).get_mapped_range().unwrap().to_vec()
}
