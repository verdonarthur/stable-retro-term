use wgpu::util::DeviceExt;

// The capture probe in xtask/src/perf/wayland.py fits this geometry.
const SCALE: f32 = 1.025;
const CURVATURE: f32 = 0.018;

/// Brightness gains of the animated CRT flicker. The capture probe in
/// xtask/src/perf/wayland.py copies these values.
pub const FLICKER_GAINS: [f32; 3] = [1.0, 0.975, 0.955];

/// The flicker gain of one presented frame. It is a pure function of the frame number.
pub fn flicker_gain(frame: u32) -> f32 {
    let mut hash = frame;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7feb_352d);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846c_a68b);
    hash ^= hash >> 16;
    FLICKER_GAINS[hash as usize % FLICKER_GAINS.len()]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderMode {
    Plain,
    #[default]
    StaticCrt,
    /// Static CRT with a flicker gain that changes only on frames the app already draws.
    AnimatedCrt,
}

impl RenderMode {
    pub fn source_point(self, point: [f64; 2], size: [u32; 2]) -> [f64; 2] {
        self.map(point, size, false)
    }

    pub fn display_point(self, point: [f64; 2], size: [u32; 2]) -> [f64; 2] {
        self.map(point, size, true)
    }

    fn map(self, point: [f64; 2], size: [u32; 2], inverse: bool) -> [f64; 2] {
        if self == Self::Plain || size.contains(&0) {
            return point;
        }
        let [w, h] = size.map(f64::from);
        let [x, y] = [point[0] * 2.0 / w - 1.0, point[1] * 2.0 / h - 1.0];
        let radius_squared = x * x + y * y;
        let scale = f64::from(SCALE);
        let curve = f64::from(CURVATURE) * radius_squared;
        let mut factor = scale + curve;
        if inverse {
            // The warp preserves direction. Solve for the inverse radial scale.
            factor = scale.recip();
            for _ in 0..5 {
                factor -= (scale * factor + curve * factor.powi(3) - 1.0)
                    / (scale + 3.0 * curve * factor * factor);
            }
        }
        [(x * factor + 1.0) * w / 2.0, (y * factor + 1.0) * h / 2.0]
    }
}

pub(crate) struct Crt {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    animated: bool,
    target: Option<Target>,
}

struct Target {
    size: [u32; 2],
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

impl Crt {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, animated: bool) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/crt.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CRT"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CRT geometry"),
            contents: &[SCALE, CURVATURE, 1.0, 0.0].map(f32::to_ne_bytes).concat(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            sampler,
            params,
            animated,
            target: None,
        }
    }

    /// Static mode never writes the parameters after creation.
    pub(crate) fn set_frame(&self, queue: &wgpu::Queue, frame: u32) {
        if self.animated {
            queue.write_buffer(&self.params, 8, &flicker_gain(frame).to_ne_bytes());
        }
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> &wgpu::TextureView {
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.size != size)
        {
            self.target = None;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("CRT scene"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("CRT scene"),
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
                        resource: self.params.as_entire_binding(),
                    },
                ],
            });
            self.target = Some(Target { size, view, bind });
        }
        &self.target.as_ref().unwrap().view
    }

    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.target.as_ref().unwrap().bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curvature_retains_edges_and_round_trips() {
        for size in [[800, 500], [500, 900]] {
            let [w, h] = size.map(f64::from);
            for point in [[0.0, 0.0], [w, h], [w / 2.0, h / 2.0], [w, h / 2.0]] {
                let screen = RenderMode::StaticCrt.display_point(point, size);
                assert!((0.0..w).contains(&screen[0]) && (0.0..h).contains(&screen[1]));
                let source = RenderMode::StaticCrt.source_point(screen, size);
                assert!((source[0] - point[0]).abs() < 1e-8);
                assert!((source[1] - point[1]).abs() < 1e-8);
                assert_eq!(RenderMode::Plain.source_point(point, size), point);
                assert_eq!(RenderMode::AnimatedCrt.display_point(point, size), screen);
                assert_eq!(RenderMode::AnimatedCrt.source_point(screen, size), source);
            }
        }
    }

    #[test]
    fn flicker_gain_is_deterministic_and_bounded() {
        let mut seen = [false; FLICKER_GAINS.len()];
        for frame in 0..10_000 {
            let gain = flicker_gain(frame);
            assert_eq!(gain, flicker_gain(frame));
            let index = FLICKER_GAINS
                .iter()
                .position(|&value| value == gain)
                .unwrap();
            seen[index] = true;
        }
        assert!(seen.iter().all(|&found| found));
        assert_eq!(FLICKER_GAINS[0], 1.0);
        assert!(FLICKER_GAINS.iter().all(|&gain| gain >= 0.95));
    }

    #[test]
    fn flicker_gain_varies_between_frames() {
        let gains: Vec<f32> = (0..64).map(flicker_gain).collect();
        assert!(gains.iter().any(|&gain| gain != gains[0]));
        let mut run = 1;
        for pair in gains.windows(2) {
            run = if pair[0] == pair[1] { run + 1 } else { 1 };
            assert!(run <= 8, "flicker gain is constant for more than 8 frames");
        }
    }
}
