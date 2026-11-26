//! GPU rendering of terminal content into a texture or window surface.

pub mod scene;
mod text;

use std::error::Error;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use scene::Scene;
pub use text::Fonts;
pub use wgpu::{Instance, InstanceDescriptor, SurfaceTarget};

type RenderResult<T> = Result<T, Box<dyn Error>>;

const FILL_STRIDE: u64 = 7 * 4;

pub struct GridRenderer {
    text: text::Text,
    pipeline: wgpu::RenderPipeline,
    fills: wgpu::Buffer,
    fill_bytes: Vec<u8>,
}

impl GridRenderer {
    /// The target format must be an sRGB color attachment.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: Fonts,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("rect.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terminal fills"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: FILL_STRIDE,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x3],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            text: text::Text::new(device, queue, format, fonts),
            pipeline,
            fills: fill_buffer(device, FILL_STRIDE),
            fill_bytes: Vec::new(),
        }
    }

    pub fn cell_width(&mut self, font_size: u16, line_height: u16) -> u16 {
        self.text.fonts.cell_width(font_size, line_height)
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        size: [u32; 2],
        scene: &Scene,
    ) -> RenderResult<wgpu::SubmissionIndex> {
        self.text.prepare(device, queue, size, scene)?;
        self.fill_bytes.clear();
        let x_unit = 2.0 / size[0] as f32;
        let y_unit = 2.0 / size[1] as f32;
        for fill in &scene.fills {
            let bounds = fill.bounds;
            let positions = [
                bounds.x as f32 * x_unit - 1.0,
                1.0 - bounds.y as f32 * y_unit,
                bounds.width as f32 * x_unit,
                -(bounds.height as f32 * y_unit),
            ];
            for value in positions.into_iter().chain(fill.color.linear()) {
                self.fill_bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        let fill_bytes = self.fill_bytes.len() as u64;
        if fill_bytes > self.fills.size() {
            self.fills = fill_buffer(device, fill_bytes.next_power_of_two());
        }
        if fill_bytes > 0 {
            queue.write_buffer(&self.fills, 0, &self.fill_bytes);
        }

        let mut commands = device.create_command_encoder(&Default::default());
        let [r, g, b] = scene.clear.linear().map(f64::from);
        {
            let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if fill_bytes > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.fills.slice(..fill_bytes));
                pass.draw(0..4, 0..scene.fills.len() as u32);
            }
            self.text.draw(&mut pass)?;
        }
        let submission = queue.submit([commands.finish()]);
        self.text.trim();
        Ok(submission)
    }
}

fn fill_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terminal fill instances"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Presented,
    Retry,
    Occluded,
    RecreateSurface,
    RecreateDevice,
}

pub struct SurfaceRenderer {
    instance: Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    grid: GridRenderer,
    lost: Arc<AtomicBool>,
    size: [u32; 2],
}

impl SurfaceRenderer {
    pub async fn new(
        instance: Instance,
        target: impl Into<SurfaceTarget<'static>>,
    ) -> RenderResult<Self> {
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;
        let config = surface_config(&surface, &adapter)?;
        let grid = GridRenderer::new(&device, &queue, config.format, Fonts::system());
        let lost = Arc::new(AtomicBool::new(false));
        let device_lost = lost.clone();
        device.set_device_lost_callback(move |_, _| device_lost.store(true, Ordering::Release));
        Ok(Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
            grid,
            lost,
            size: [0, 0],
        })
    }

    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }

    pub fn cell_width(&mut self, font_size: u16, line_height: u16) -> u16 {
        self.grid.cell_width(font_size, line_height)
    }

    pub fn resize(&mut self, size: [u32; 2]) -> RenderResult<()> {
        if size
            .into_iter()
            .any(|side| side > self.device.limits().max_texture_dimension_2d)
        {
            return Err(io::Error::other("window exceeds GPU texture size limit").into());
        }
        if self.size != size {
            self.size = size;
            if !size.contains(&0) && !self.lost.load(Ordering::Acquire) {
                self.configure();
            }
        }
        Ok(())
    }

    pub fn recreate_surface(
        &mut self,
        target: impl Into<SurfaceTarget<'static>>,
    ) -> RenderResult<()> {
        let surface = self.instance.create_surface(target)?;
        let config = surface_config(&surface, &self.adapter)?;
        if config.format != self.config.format {
            self.grid =
                GridRenderer::new(&self.device, &self.queue, config.format, Fonts::system());
        }
        self.surface = surface;
        self.config = config;
        if !self.size.contains(&0) {
            self.configure();
        }
        Ok(())
    }

    pub fn draw(&mut self, scene: &Scene, before_present: impl FnOnce()) -> RenderResult<Frame> {
        use wgpu::CurrentSurfaceTexture as Texture;
        if self.lost.load(Ordering::Acquire) {
            return Ok(Frame::RecreateDevice);
        }
        if self.size.contains(&0) {
            return Ok(Frame::Occluded);
        }
        let frame = match self.surface.get_current_texture() {
            Texture::Success(frame) | Texture::Suboptimal(frame) => frame,
            Texture::Timeout => return Ok(Frame::Retry),
            Texture::Occluded => return Ok(Frame::Occluded),
            Texture::Lost => return Ok(Frame::RecreateSurface),
            Texture::Outdated => {
                self.configure();
                return Ok(Frame::Retry);
            }
            Texture::Validation => {
                if self.lost.load(Ordering::Acquire) {
                    return Ok(Frame::RecreateDevice);
                }
                return Err(io::Error::other("GPU rejected the surface configuration").into());
            }
        };
        let view = frame.texture.create_view(&Default::default());
        self.grid
            .draw(&self.device, &self.queue, &view, self.size, scene)?;
        before_present();
        frame.present();
        Ok(Frame::Presented)
    }

    fn configure(&mut self) {
        [self.config.width, self.config.height] = self.size;
        self.surface.configure(&self.device, &self.config);
    }
}

fn surface_config(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
) -> RenderResult<wgpu::SurfaceConfiguration> {
    let mut config = surface
        .get_default_config(adapter, 1, 1)
        .ok_or_else(|| io::Error::other("GPU cannot present to this Wayland surface"))?;
    config.format = surface
        .get_capabilities(adapter)
        .formats
        .into_iter()
        .find(wgpu::TextureFormat::is_srgb)
        .ok_or_else(|| io::Error::other("Wayland surface has no sRGB format"))?;
    Ok(config)
}
