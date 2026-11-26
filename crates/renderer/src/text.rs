use std::collections::HashMap;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, Style,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap, fontdb,
};
use wgpu::{Device, MultisampleState, Queue, RenderPass, TextureFormat};

use crate::scene::{Cluster, Scene};

const MAX_LAYOUTS: usize = 2048;
const MAX_RASTER_ENTRIES: usize = 4096;

pub struct Fonts(FontSystem);

impl Fonts {
    pub fn system() -> Self {
        let mut fonts = FontSystem::new();
        let family = fonts
            .db()
            .faces()
            .find(|face| face.monospaced)
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .filter(|_| {
                fonts
                    .db()
                    .query(&fontdb::Query {
                        families: &[fontdb::Family::Monospace],
                        ..fontdb::Query::default()
                    })
                    .is_none()
            });
        if let Some(family) = family {
            fonts.db_mut().set_monospace_family(family);
        }
        Self(fonts)
    }

    pub fn from_data(monospace: &str, data: &[&[u8]]) -> Self {
        let mut db = fontdb::Database::new();
        for bytes in data {
            db.load_font_data(bytes.to_vec());
        }
        db.set_monospace_family(monospace);
        Self(FontSystem::new_with_locale_and_db("en-US".into(), db))
    }

    pub fn cell_width(&mut self, font_size: u16, line_height: u16) -> u16 {
        let cluster = Cluster {
            text: "M".into(),
            bold: false,
            italic: false,
        };
        let buffer = shape(&mut self.0, (font_size, line_height), &cluster);
        buffer
            .layout_runs()
            .next()
            .map_or(font_size.div_ceil(2), |run| run.line_w.round() as u16)
            .max(1)
    }
}

fn shape(
    fonts: &mut FontSystem,
    (font_size, line_height): (u16, u16),
    cluster: &Cluster,
) -> Buffer {
    let mut buffer = Buffer::new(
        fonts,
        Metrics::new(f32::from(font_size), f32::from(line_height)),
    );
    buffer.set_wrap(fonts, Wrap::None);
    let attrs = Attrs::new()
        .family(Family::Monospace)
        .weight(if cluster.bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        })
        .style(if cluster.italic {
            Style::Italic
        } else {
            Style::Normal
        });
    buffer.set_text(fonts, &cluster.text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

pub(crate) struct Text {
    pub(crate) fonts: Fonts,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    renderer: TextRenderer,
    layouts: HashMap<Cluster, Buffer>,
    metrics: (u16, u16),
}

impl Text {
    pub(crate) fn new(device: &Device, queue: &Queue, format: TextureFormat, fonts: Fonts) -> Self {
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, MultisampleState::default(), None);
        Self {
            fonts,
            swash: SwashCache::new(),
            viewport: Viewport::new(device, &cache),
            atlas,
            renderer,
            layouts: HashMap::new(),
            metrics: (0, 0),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        size: [u32; 2],
        scene: &Scene,
    ) -> Result<(), glyphon::PrepareError> {
        let metrics = (scene.font_size, scene.line_height);
        if self.metrics != metrics {
            self.layouts.clear();
            self.metrics = metrics;
        }
        let [width, height] = size;
        self.viewport.update(queue, Resolution { width, height });
        for glyph in &scene.glyphs {
            if !self.layouts.contains_key(&glyph.cluster) {
                self.layouts.insert(
                    glyph.cluster.clone(),
                    shape(&mut self.fonts.0, metrics, &glyph.cluster),
                );
            }
        }
        self.renderer.prepare(
            device,
            queue,
            &mut self.fonts.0,
            &mut self.atlas,
            &self.viewport,
            scene.glyphs.iter().map(|glyph| {
                let bounds = glyph.bounds;
                let crate::scene::Rgb(r, g, b) = glyph.color;
                TextArea {
                    buffer: &self.layouts[&glyph.cluster],
                    left: bounds.x as f32,
                    top: bounds.y as f32,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: bounds.x as i32,
                        top: bounds.y as i32,
                        right: bounds.x.saturating_add(bounds.width).min(width) as i32,
                        bottom: bounds.y.saturating_add(bounds.height).min(height) as i32,
                    },
                    default_color: Color::rgb(r, g, b),
                    custom_glyphs: &[],
                }
            }),
            &mut self.swash,
        )
    }

    pub(crate) fn draw(&self, pass: &mut RenderPass<'_>) -> Result<(), glyphon::RenderError> {
        self.renderer.render(&self.atlas, &self.viewport, pass)
    }

    pub(crate) fn trim(&mut self) {
        self.atlas.trim();
        if self.layouts.len() > MAX_LAYOUTS {
            self.layouts.clear();
        }
        if self.swash.image_cache.len() > MAX_RASTER_ENTRIES {
            self.swash.image_cache.clear();
        }
    }
}
