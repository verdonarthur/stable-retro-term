//! Physical-pixel scene data supplied by the app.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub(crate) fn linear(self) -> [f32; 3] {
        [self.0, self.1, self.2].map(|channel| {
            let value = f32::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cluster {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub bounds: Rect,
    pub cluster: Cluster,
    pub color: Rgb,
}

#[derive(Clone, Copy, Debug)]
pub struct Fill {
    pub bounds: Rect,
    pub color: Rgb,
}

pub struct Scene {
    pub clear: Rgb,
    pub font_size: u16,
    pub line_height: u16,
    pub fills: Vec<Fill>,
    pub glyphs: Vec<Glyph>,
}

impl Scene {
    pub fn new(clear: Rgb, font_size: u16, line_height: u16) -> Self {
        Self {
            clear,
            font_size,
            line_height,
            fills: vec![],
            glyphs: vec![],
        }
    }

    pub fn fill(&mut self, bounds: Rect, color: Rgb) {
        if bounds.width > 0 && bounds.height > 0 {
            self.fills.push(Fill { bounds, color });
        }
    }

    /// Draw the common light box junctions to the edges of the cell.
    pub fn box_drawing(&mut self, character: char, bounds: Rect, color: Rgb) -> bool {
        let (left, right, up, down) = match character {
            '─' => (true, true, false, false),
            '│' => (false, false, true, true),
            '┌' => (false, true, false, true),
            '┐' => (true, false, false, true),
            '└' => (false, true, true, false),
            '┘' => (true, false, true, false),
            '├' => (false, true, true, true),
            '┤' => (true, false, true, true),
            '┬' => (true, true, false, true),
            '┴' => (true, true, true, false),
            '┼' => (true, true, true, true),
            _ => return false,
        };
        let Rect {
            x,
            y,
            width,
            height,
        } = bounds;
        let middle_x = width / 2;
        let middle_y = height / 2;
        let stroke = (width / 10).max(1);
        if left || right {
            let start = if left { 0 } else { middle_x };
            let end = if right {
                width
            } else {
                (middle_x + stroke).min(width)
            };
            self.fill(
                Rect {
                    x: x + start,
                    y: y + middle_y,
                    width: end - start,
                    height: stroke.min(height - middle_y),
                },
                color,
            );
        }
        if up || down {
            let start = if up { 0 } else { middle_y };
            let end = if down {
                height
            } else {
                (middle_y + stroke).min(height)
            };
            self.fill(
                Rect {
                    x: x + middle_x,
                    y: y + start,
                    width: stroke.min(width - middle_x),
                    height: end - start,
                },
                color,
            );
        }
        true
    }
}
