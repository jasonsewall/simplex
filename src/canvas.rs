/// Active drawing tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pen,
    Eraser,
}

/// CPU pixel buffer with drawing primitives and undo history.
///
/// Pixel format matches softbuffer: `0x00RRGGBB` (upper byte ignored).
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    history: Vec<Vec<u32>>,
    pub color: u32,
    pub brush_size: u32,
    pub tool: Tool,
}

const BACKGROUND: u32 = 0x00FFFFFF;
const MAX_HISTORY: usize = 50;

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![BACKGROUND; (width * height) as usize],
            history: Vec::new(),
            color: 0x00111111,
            brush_size: 4,
            tool: Tool::Pen,
        }
    }

    /// Resize the canvas, preserving existing content in the top-left region.
    pub fn resize(&mut self, new_w: u32, new_h: u32) {
        let mut buf = vec![BACKGROUND; (new_w * new_h) as usize];
        let copy_w = self.width.min(new_w) as usize;
        let copy_h = self.height.min(new_h) as usize;
        for row in 0..copy_h {
            let src = row * self.width as usize;
            let dst = row * new_w as usize;
            buf[dst..dst + copy_w].copy_from_slice(&self.pixels[src..src + copy_w]);
        }
        self.pixels = buf;
        self.width = new_w;
        self.height = new_h;
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    #[allow(dead_code)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[allow(dead_code)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Save current state for undo. Call once before each stroke begins.
    pub fn push_history(&mut self) {
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }
        self.history.push(self.pixels.clone());
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            if prev.len() == self.pixels.len() {
                self.pixels = prev;
            }
        }
    }

    pub fn clear(&mut self) {
        self.push_history();
        self.pixels.fill(BACKGROUND);
    }

    pub fn set_color(&mut self, rgb: u32) {
        self.color = rgb & 0x00FFFFFF;
        self.tool = Tool::Pen;
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub fn adjust_brush(&mut self, delta: f32) {
        self.brush_size = ((self.brush_size as f32 + delta).max(1.0).min(128.0)) as u32;
    }

    /// Draw a continuous stroke from (x0,y0) to (x1,y1) using the active tool.
    /// For the first point of a new stroke, call with x0==x1, y0==y1.
    pub fn stroke(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let color = match self.tool {
            Tool::Pen => self.color,
            Tool::Eraser => BACKGROUND,
        };
        let r = self.brush_size as i32;
        for (x, y) in bresenham(x0, y0, x1, y1) {
            self.stamp_circle(x, y, r, color);
        }
    }

    /// Stamp a filled circle of `color` centered at (cx, cy) with radius r.
    pub fn stamp_circle(&mut self, cx: i32, cy: i32, r: i32, color: u32) {
        let w = self.width as i32;
        let h = self.height as i32;
        let r2 = r * r;
        let y_lo = (cy - r).max(0);
        let y_hi = (cy + r).min(h - 1);
        let x_lo = (cx - r).max(0);
        let x_hi = (cx + r).min(w - 1);
        for py in y_lo..=y_hi {
            for px in x_lo..=x_hi {
                let dx = px - cx;
                let dy = py - cy;
                if dx * dx + dy * dy <= r2 {
                    self.pixels[(py * w + px) as usize] = color;
                }
            }
        }
    }
}

/// Bresenham integer line iterator.
fn bresenham(x0: i32, y0: i32, x1: i32, y1: i32) -> impl Iterator<Item = (i32, i32)> {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1i32 } else { -1 };
    let sy = if y0 < y1 { 1i32 } else { -1 };

    struct Line {
        x: i32, y: i32,
        x1: i32, y1: i32,
        dx: i32, dy: i32,
        sx: i32, sy: i32,
        err: i32,
        done: bool,
    }

    impl Iterator for Line {
        type Item = (i32, i32);

        fn next(&mut self) -> Option<(i32, i32)> {
            if self.done {
                return None;
            }
            let p = (self.x, self.y);
            if self.x == self.x1 && self.y == self.y1 {
                self.done = true;
                return Some(p);
            }
            let e2 = 2 * self.err;
            if e2 >= self.dy {
                self.err += self.dy;
                self.x += self.sx;
            }
            if e2 <= self.dx {
                self.err += self.dx;
                self.y += self.sy;
            }
            Some(p)
        }
    }

    Line { x: x0, y: y0, x1, y1, dx, dy, sx, sy, err: dx + dy, done: false }
}
