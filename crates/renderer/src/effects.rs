use wgpu::util::DeviceExt;

// The capture probe in xtask/src/perf/wayland.py fits this geometry.
const SCALE: f32 = 1.025;
const CURVATURE: f32 = 0.018;

pub const CRT_CONTROLS: [(&str, &str, u8); 28] = [
    ("bloom", "Bloom", 0),
    ("burn_in", "Burn-in", 0),
    ("static_noise", "Static noise", 0),
    ("jitter", "Jitter", 0),
    ("glow_line", "Glow line", 0),
    ("curvature", "Screen curvature", 30),
    ("ambient_light", "Ambient light", 0),
    ("flicker", "Flicker", 0),
    ("horizontal_sync", "Horizontal sync", 0),
    ("rgb_shift", "RGB shift", 0),
    ("frame_shininess", "Frame shininess", 0),
    ("scanlines", "Scanlines", 12),
    ("red", "Red phosphor", 95),
    ("green", "Green phosphor", 100),
    ("blue", "Blue phosphor", 100),
    ("brightness", "Brightness", 50),
    ("contrast", "Contrast", 50),
    ("margin", "Margin", 25),
    ("radius", "Corner radius", 0),
    ("frame_size", "Frame size", 0),
    ("background_red", "Background red", 0),
    ("background_green", "Background green", 0),
    ("background_blue", "Background blue", 0),
    ("chroma", "Chroma", 100),
    ("frame_red", "Frame red", 100),
    ("frame_green", "Frame green", 100),
    ("frame_blue", "Frame blue", 100),
    ("saturation", "Saturation lift", 0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrtLevels(pub [u8; CRT_CONTROLS.len()]);

impl Default for CrtLevels {
    fn default() -> Self {
        Self(CRT_CONTROLS.map(|(_, _, default)| default))
    }
}

impl CrtLevels {
    fn geometry(self) -> (f32, f32) {
        (
            1.0 / (1.0
                - f32::from(self.0[17]) * 0.001
                - f32::from(self.0[19]) * 0.0012
                - f32::from(self.0[18]) * 0.001),
            f32::from(self.0[5]) * CURVATURE / 30.0,
        )
    }
}

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
    CustomCrt(CrtLevels, bool),
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
        let (scale, curvature) = match self {
            Self::CustomCrt(levels, _) => levels.geometry(),
            _ => (SCALE, CURVATURE),
        };
        let scale = f64::from(scale);
        let curve = f64::from(curvature) * radius_squared;
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
    persistence: Option<wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    animated: bool,
    custom: bool,
    target: Option<Target>,
}

struct Target {
    size: [u32; 2],
    view: wgpu::TextureView,
    history: [wgpu::TextureView; 2],
    binds: [wgpu::BindGroup; 2],
    previous: usize,
}

impl Crt {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        mode: RenderMode,
    ) -> Self {
        let (levels, animated, custom) = match mode {
            RenderMode::CustomCrt(levels, animation) => (levels, animation, true),
            _ => (CrtLevels::default(), mode == RenderMode::AnimatedCrt, false),
        };
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/crt.wgsl"));
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("CRT bindings"),
            entries: &[
                texture(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture(3),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CRT layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            ..Default::default()
        });
        let make_pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("CRT"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make_pipeline("fragment");
        let persistence = (levels.0[1] != 0).then(|| make_pipeline("persistence"));
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (scale, curve) = if custom {
            levels.geometry()
        } else {
            (SCALE, CURVATURE)
        };
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CRT geometry"),
            contents: &[scale, curve, 1.0, if custom { 0.0 } else { -1.0 }]
                .into_iter()
                .chain(levels.0.map(|v| f32::from(v) / 100.0))
                .flat_map(f32::to_ne_bytes)
                .collect::<Vec<_>>(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            persistence,
            sampler,
            params,
            animated,
            custom,
            target: None,
        }
    }

    /// Static mode never writes the parameters after creation.
    pub(crate) fn set_frame(&self, queue: &wgpu::Queue, frame: u32) {
        if self.custom || self.animated {
            let gain = if self.animated {
                flicker_gain(frame)
            } else {
                1.0
            };
            queue.write_buffer(
                &self.params,
                8,
                &[
                    gain,
                    if self.custom {
                        (frame % 16384) as f32
                    } else {
                        -1.0
                    },
                ]
                .map(f32::to_ne_bytes)
                .concat(),
            );
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
            let descriptor = wgpu::TextureDescriptor {
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
            };
            let view = device
                .create_texture(&descriptor)
                .create_view(&Default::default());
            let history = std::array::from_fn(|_| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("CRT history"),
                        size: if self.persistence.is_some() {
                            descriptor.size
                        } else {
                            wgpu::Extent3d {
                                width: 1,
                                height: 1,
                                depth_or_array_layers: 1,
                            }
                        },
                        ..descriptor
                    })
                    .create_view(&Default::default())
            });
            let binds = history.each_ref().map(|history_view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(history_view),
                        },
                    ],
                })
            });
            self.target = Some(Target {
                size,
                view,
                history,
                binds,
                previous: 0,
            });
        }
        &self.target.as_ref().unwrap().view
    }

    pub(crate) fn draw(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let target = self.target.as_mut().unwrap();
        if let Some(pipeline) = &self.persistence {
            let next = 1 - target.previous;
            Self::draw_pass(
                encoder,
                &target.history[next],
                pipeline,
                &target.binds[target.previous],
            );
            target.previous = next;
        }
        Self::draw_pass(
            encoder,
            view,
            &self.pipeline,
            &target.binds[target.previous],
        );
    }

    fn draw_pass(
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        bind: &wgpu::BindGroup,
    ) {
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
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
