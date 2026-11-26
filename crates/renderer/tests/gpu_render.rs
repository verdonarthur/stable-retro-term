use std::sync::mpsc;
use std::time::Duration;

use retro_renderer::scene::{Cluster, Glyph, Rect, Rgb, Scene};
use retro_renderer::{Fonts, GridRenderer};
use wgpu::{
    BufferDescriptor, BufferUsages, Extent3d, Instance, MapMode, PollType, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

const SIZE: [u32; 2] = [128, 64];
const FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;
const CLEAR: Rgb = Rgb(8, 10, 12);
const INK: Rgb = Rgb(240, 240, 240);

#[test]
#[ignore = "needs display"]
fn fixed_grid_readback() {
    let software = std::env::var_os("RETRO_TERM_TEST_SOFTWARE_GPU").is_some();
    let native_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
        || std::env::var_os("WAYLAND_SOCKET").is_some_and(|value| !value.is_empty());
    assert!(
        software || native_wayland,
        "unsupported: native Wayland session required"
    );

    let instance = Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: software,
        ..Default::default()
    }))
    .expect("unsupported: no Vulkan adapter available");
    let info = adapter.get_info();
    eprintln!(
        "Render adapter: {} ({:?}, {:?}) {} {}",
        info.name, info.device_type, info.backend, info.driver, info.driver_info
    );
    assert!(
        software || info.device_type != wgpu::DeviceType::Cpu,
        "unsupported: only a software adapter is visible"
    );
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("unsupported: the adapter could not create a device");

    let fonts = Fonts::from_data(
        "Source Code Pro",
        &[
            include_bytes!("fixtures/SourceCodePro-Regular.otf"),
            include_bytes!("fixtures/DroidSansJapanese-subset.ttf"),
        ],
    );
    let mut renderer = GridRenderer::new(&device, &queue, FORMAT, fonts);
    let mut scene = Scene::new(CLEAR, 24, 32);
    let background = Rgb(49, 90, 130);
    let cursor = Rgb(90, 190, 32);
    scene.fill(rect(100, 0, 16, 32), background);
    scene.fill(rect(100, 0, 2, 32), cursor);
    assert!(scene.box_drawing('┌', rect(16, 32, 16, 32), INK));
    assert!(scene.box_drawing('─', rect(32, 32, 16, 32), INK));
    for (text, bounds) in [
        ("a", rect(0, 0, 16, 32)),
        ("a\u{0301}", rect(16, 0, 16, 32)),
        ("界", rect(32, 0, 32, 32)),
    ] {
        scene.glyphs.push(Glyph {
            bounds,
            cluster: Cluster {
                text: text.into(),
                bold: false,
                italic: false,
            },
            color: INK,
        });
    }

    let extent = Extent3d {
        width: SIZE[0],
        height: SIZE[1],
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("grid correctness target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    renderer
        .draw(
            &device,
            &queue,
            &texture.create_view(&Default::default()),
            SIZE,
            &scene,
        )
        .unwrap();
    let stride = SIZE[0] * 4;
    assert_eq!(stride % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("grid pixel readback"),
        size: u64::from(stride * SIZE[1]),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut commands = device.create_command_encoder(&Default::default());
    commands.copy_texture_to_buffer(
        texture.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(SIZE[1]),
            },
        },
        extent,
    );
    let submission = queue.submit([commands.finish()]);
    let (send, recv) = mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| send.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(15)),
        })
        .unwrap();
    recv.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
    let rendered = readback.slice(..).get_mapped_range();
    let pixel = |x: u32, y: u32| {
        let offset = (y * stride + x * 4) as usize;
        Rgb(rendered[offset], rendered[offset + 1], rendered[offset + 2])
    };
    let ink_top = |x: u32, width: u32| {
        (0..32)
            .find(|&y| (x..x + width).any(|column| pixel(column, y).0 > 96))
            .unwrap_or_else(|| panic!("no glyph ink in cells {x}..{}", x + width))
    };
    assert!(
        ink_top(16, 16) < ink_top(0, 16),
        "combining acute must add ink above plain a"
    );
    ink_top(32, 32);
    assert!(
        (0..32).any(|y| (48..64).any(|x| pixel(x, y).0 > 96)),
        "wide fallback glyph must extend into its second cell"
    );
    assert!(
        (0..32).all(|y| (64..100).all(|x| pixel(x, y) == CLEAR)),
        "glyphs must remain in their cells"
    );
    for x in 24..48 {
        assert_eq!(pixel(x, 48), INK, "box drawing has a gap at x={x}");
    }
    assert_eq!(pixel(24, 63), INK);
    assert_eq!(pixel(23, 48), CLEAR);
    assert_eq!(pixel(100, 15), cursor);
    assert_eq!(pixel(103, 15), background);
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
