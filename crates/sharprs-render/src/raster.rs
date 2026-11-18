//! tiny-skia implementation of [`Canvas`].
//!
//! Conventions mirror Qt's raster engine closely enough for pixel matching:
//! antialiased geometry is placed exactly at the given float coordinates (so
//! a 1 px line on an integer coordinate straddles two pixel rows, as in Qt),
//! while aliased strokes are shifted by half a pixel so that a 1 px line at
//! integer `y` fills pixel row `y`, which is what Qt does when antialiasing is
//! off.

use std::collections::HashMap;

use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Stroke, StrokeDash, Transform,
};

use crate::canvas::{Align, Canvas, Cap, Color, Font, FontMetrics, HAlign, Path, PathCmd, Pen, Rect, VAlign};
use crate::text::{FontFamily, TextBackend, TextEngine, TextHinting, wrap_words};

#[derive(Clone, Copy)]
struct State {
    dx: f32,
    dy: f32,
    /// Uniform scale from logical to device pixels.
    s: f32,
    /// Device-space clip (pixel aligned) or None for the whole surface.
    clip: Option<Rect>,
    /// Clip inherited from the enclosing saved state.
    base_clip: Option<Rect>,
    aa: bool,
}

/// Raster canvas backed by a tiny-skia [`Pixmap`].
pub struct RasterCanvas {
    pixmap: Pixmap,
    state: State,
    stack: Vec<State>,
    text: TextEngine,
    masks: HashMap<(i32, i32, i32, i32), Mask>,
    /// Gamma applied to text coverage (1.0 = linear blending).
    text_gamma: f32,
    gamma_lut: [u8; 256],
}

impl RasterCanvas {
    pub fn new(width: u32, height: u32, background: Color, family: FontFamily, hinting: TextHinting) -> RasterCanvas {
        let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("non-zero canvas");
        pixmap.fill(to_sk(background));
        let mut c = RasterCanvas {
            pixmap,
            state: State { dx: 0.0, dy: 0.0, s: 1.0, clip: None, base_clip: None, aa: true },
            stack: Vec::new(),
            text: TextEngine::new(family, hinting),
            masks: HashMap::new(),
            text_gamma: 1.0,
            gamma_lut: [0; 256],
        };
        c.set_text_gamma(1.0);
        c
    }

    /// Adjust text coverage gamma (>1 darkens/thickens light-on-dark text).
    pub fn set_text_gamma(&mut self, g: f32) {
        self.text_gamma = g;
        for i in 0..256 {
            let v = (i as f32 / 255.0).powf(1.0 / g);
            self.gamma_lut[i] = (v * 255.0 + 0.5) as u8;
        }
    }

    /// Choose whose integer font metrics to reproduce.
    pub fn set_text_backend(&mut self, backend: TextBackend) {
        self.text.set_backend(backend);
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    pub fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    /// Encode as PNG.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        self.pixmap.encode_png().map_err(|e| e.to_string())
    }

    /// Straight (un-premultiplied) RGBA8 pixels.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixmap.data().len());
        for p in self.pixmap.pixels() {
            let c = p.demultiply();
            out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        out
    }

    fn mask_for<'a>(
        masks: &'a mut HashMap<(i32, i32, i32, i32), Mask>,
        pixmap: &Pixmap,
        clip: Option<Rect>,
    ) -> Option<&'a Mask> {
        let r = clip?;
        let key = (r.x as i32, r.y as i32, r.w as i32, r.h as i32);
        let entry = match masks.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => return Some(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(e) => e,
        };
        let mut m = Mask::new(pixmap.width(), pixmap.height())?;
        if let Some(rect) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w.max(0.0), r.h.max(0.0)) {
            let path = PathBuilder::from_rect(rect);
            m.fill_path(&path, FillRule::Winding, false, Transform::identity());
        }
        Some(entry.insert(m))
    }

    fn build(&self, path: &Path, shift: f32) -> Option<tiny_skia::Path> {
        let (ox, oy, s) = (self.state.dx + shift, self.state.dy + shift, self.state.s);
        let tx = |x: f32| x * s + ox;
        let ty = |y: f32| y * s + oy;
        let mut pb = PathBuilder::new();
        for c in &path.cmds {
            match *c {
                PathCmd::MoveTo(x, y) => pb.move_to(tx(x), ty(y)),
                PathCmd::LineTo(x, y) => pb.line_to(tx(x), ty(y)),
                PathCmd::CubicTo(a, b, cc, d, x, y) => pb.cubic_to(tx(a), ty(b), tx(cc), ty(d), tx(x), ty(y)),
                PathCmd::Close => pb.close(),
            }
        }
        pb.finish()
    }

    fn blend_glyph(&mut self, gx: i32, gy: i32, g: &crate::text::GlyphBitmap, color: Color, clip: (i32, i32, i32, i32)) {
        let w = self.pixmap.width() as i32;
        let (cx0, cy0, cx1, cy1) = clip;
        let lut = self.gamma_lut;
        let pixels = self.pixmap.pixels_mut();
        for row in 0..g.height as i32 {
            let y = gy + row;
            if y < cy0 || y >= cy1 {
                continue;
            }
            for col in 0..g.width as i32 {
                let x = gx + col;
                if x < cx0 || x >= cx1 {
                    continue;
                }
                let cov = g.coverage[(row * g.width as i32 + col) as usize];
                if cov == 0 {
                    continue;
                }
                let a = lut[cov as usize] as u32 * color.a as u32 / 255;
                let idx = (y * w + x) as usize;
                let d = pixels[idx];
                // Source-over in premultiplied space.
                let inv = 255 - a;
                let r = (color.r as u32 * a + d.red() as u32 * inv + 127) / 255;
                let gg = (color.g as u32 * a + d.green() as u32 * inv + 127) / 255;
                let b = (color.b as u32 * a + d.blue() as u32 * inv + 127) / 255;
                let al = (255 * a + d.alpha() as u32 * inv + 127) / 255;
                let al = al.min(255) as u8;
                let r = (r as u8).min(al);
                let gg = (gg as u8).min(al);
                let b = (b as u8).min(al);
                if let Some(p) = PremultipliedColorU8::from_rgba(r, gg, b, al) {
                    pixels[idx] = p;
                }
            }
        }
    }

    fn device_clip_bounds(&self) -> (i32, i32, i32, i32) {
        let (w, h) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        match self.state.clip {
            None => (0, 0, w, h),
            Some(r) => (
                (r.x as i32).max(0),
                (r.y as i32).max(0),
                ((r.x + r.w) as i32).min(w),
                ((r.y + r.h) as i32).min(h),
            ),
        }
    }
}

fn to_sk(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

impl Canvas for RasterCanvas {
    fn size(&self) -> (f32, f32) {
        (self.pixmap.width() as f32 / self.state.s, self.pixmap.height() as f32 / self.state.s)
    }

    fn scale(&mut self, s: f32) {
        self.state.s *= s;
    }

    fn save(&mut self) {
        self.stack.push(self.state);
        self.state.base_clip = self.state.clip;
    }

    fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.state.dx += dx * self.state.s;
        self.state.dy += dy * self.state.s;
    }

    fn set_clip(&mut self, r: Option<Rect>) {
        self.state.clip = match r {
            None => self.state.base_clip,
            Some(r) => {
                // Qt clip rects are integer rects; snap to whole pixels.
                let s = self.state.s;
                let x0 = (r.x * s + self.state.dx).round();
                let y0 = (r.y * s + self.state.dy).round();
                let x1 = (r.right() * s + self.state.dx).round();
                let y1 = (r.bottom() * s + self.state.dy).round();
                let dev = Rect::new(x0, y0, x1 - x0, y1 - y0);
                Some(match self.state.base_clip {
                    Some(b) => dev.intersect(&b),
                    None => dev,
                })
            }
        };
    }

    fn set_antialias(&mut self, on: bool) {
        self.state.aa = on;
    }

    fn fill_rect(&mut self, r: Rect, color: Color) {
        let p = Path::rect(r);
        self.fill_path(&p, color);
    }

    fn fill_path(&mut self, path: &Path, color: Color) {
        let Some(sk) = self.build(path, 0.0) else { return };
        let mut paint = Paint::default();
        paint.set_color(to_sk(color));
        paint.anti_alias = self.state.aa;
        let mask = Self::mask_for(&mut self.masks, &self.pixmap, self.state.clip);
        self.pixmap.fill_path(&sk, &paint, FillRule::Winding, Transform::identity(), mask);
    }

    fn stroke_path(&mut self, path: &Path, pen: &Pen) {
        if pen.color.a == 0 {
            return;
        }
        // Qt treats width 0 as a cosmetic 1 px pen.
        let lwidth = if pen.width <= 0.0 { 1.0 } else { pen.width };
        let width = lwidth * self.state.s;
        let shift = if self.state.aa { 0.0 } else if (width.round() as i32) % 2 == 1 { 0.5 } else { 0.0 };
        let Some(sk) = self.build(path, shift) else { return };
        let mut paint = Paint::default();
        paint.set_color(to_sk(pen.color));
        paint.anti_alias = self.state.aa;
        let mut stroke = Stroke {
            width,
            line_cap: match pen.cap {
                Cap::Flat => LineCap::Butt,
                Cap::Square => LineCap::Square,
                Cap::Round => LineCap::Round,
            },
            line_join: LineJoin::Miter,
            miter_limit: 2.0,
            ..Stroke::default()
        };
        if let Some(pat) = pen.style.pattern() {
            if lwidth <= 1.0 {
                // Qt draws 1 px dashed pens with its cosmetic stroker, which
                // counts the dash pattern in pixels along the major axis of
                // each segment. Emulate by dashing ourselves.
                let dashed = cosmetic_dash(path, pat);
                let Some(dsk) = self.build(&dashed, shift) else { return };
                stroke.line_cap = LineCap::Butt;
                let mask = Self::mask_for(&mut self.masks, &self.pixmap, self.state.clip);
                self.pixmap.stroke_path(&dsk, &paint, &stroke, Transform::identity(), mask);
                return;
            }
            let unit = width.max(1.0);
            let mut intervals: Vec<f32> = pat.iter().map(|v| v * unit).collect();
            // Square caps extend each dash by half a width on both ends (Qt
            // does the same), so shorten dashes/lengthen gaps accordingly to
            // keep Qt's visual pattern when caps are square.
            if pen.cap == Cap::Square {
                for (i, v) in intervals.iter_mut().enumerate() {
                    if i % 2 == 0 {
                        *v = (*v - width).max(0.01);
                    } else {
                        *v += width;
                    }
                }
            }
            stroke.dash = StrokeDash::new(intervals, 0.0);
        }
        let mask = Self::mask_for(&mut self.masks, &self.pixmap, self.state.clip);
        self.pixmap.stroke_path(&sk, &paint, &stroke, Transform::identity(), mask);
    }

    fn draw_text(&mut self, rect: Rect, align: Align, text: &str, font: Font, color: Color) {
        if text.is_empty() || color.a == 0 {
            return;
        }
        // Lay text out in device pixels so hinting happens at the final size.
        let s = self.state.s;
        let font = Font { px: font.px * s, ..font };
        let rect = Rect::new(rect.x * s, rect.y * s, rect.w * s, rect.h * s);
        let (ascent, descent) = self.text.layout_metrics(font);
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            if align.wrap && rect.w > 0.0 {
                lines.extend(wrap_words(&mut self.text, para, font, rect.w));
            } else {
                lines.push(para.to_string());
            }
        }
        let lh = ascent + descent;
        let total = lh * lines.len() as f32;
        let yoff = match align.v {
            VAlign::Top => 0.0,
            VAlign::Center => (rect.h - total) / 2.0,
            VAlign::Bottom => rect.h - total,
        };
        let clip = self.device_clip_bounds();
        for (i, line) in lines.iter().enumerate() {
            let (glyphs, width) = self.text.layout(line, font);
            let xoff = match align.h {
                HAlign::Left => 0.0,
                HAlign::Center => (rect.w - width) / 2.0,
                HAlign::Right => rect.w - width,
            };
            let baseline = (rect.y + yoff + i as f32 * lh + ascent + self.state.dy + 0.5).floor();
            let x0 = rect.x + xoff + self.state.dx;
            let int_pos = self.text.integer_positions();
            for pg in glyphs {
                let px = x0 + pg.x;
                // With integer positioning the pen snaps to the nearest pixel;
                // otherwise the sub-pixel remainder is quantized to quarters.
                let (gx, frac) = if int_pos {
                    ((px + 0.5).floor() as i32, 0.0)
                } else {
                    let q = (px * 4.0).round() / 4.0;
                    (q.floor() as i32, q - q.floor())
                };
                let g = self.text.glyph(font, pg.gid, frac);
                if g.width == 0 {
                    continue;
                }
                self.blend_glyph(gx + g.left, baseline as i32 + g.top, &g, color, clip);
            }
        }
    }

    fn text_width(&mut self, text: &str, font: Font) -> f32 {
        let s = self.state.s;
        self.text.width(text, Font { px: font.px * s, ..font }) / s
    }

    fn font_metrics(&mut self, font: Font) -> FontMetrics {
        let s = self.state.s;
        let m = self.text.metrics(Font { px: font.px * s, ..font });
        FontMetrics {
            ascent: m.ascent / s,
            descent: m.descent / s,
            height: m.height / s,
            x_height: m.x_height / s,
            cap_height: m.cap_height / s,
        }
    }
}

/// Split a path into dash segments, measuring the pattern in major-axis
/// pixels per segment (Qt cosmetic-stroker semantics). Curves are flattened.
fn cosmetic_dash(path: &Path, pattern: &[f32]) -> Path {
    let total: f32 = pattern.iter().sum();
    let mut out = Path::new();
    if total <= 0.0 {
        return path.clone();
    }
    let mut phase = 0.0f32; // position within the pattern (major-axis px)
    let mut cur = (0.0f32, 0.0f32);
    let mut start = cur;
    let emit = |a: (f32, f32), b: (f32, f32), phase: &mut f32, out: &mut Path| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let major = dx.abs().max(dy.abs());
        if major <= 0.0 {
            return;
        }
        let mut t = 0.0f32; // major-axis distance travelled
        while t < major {
            // locate pattern slot
            let mut acc = 0.0;
            let mut idx = 0;
            while idx < pattern.len() && acc + pattern[idx] <= *phase {
                acc += pattern[idx];
                idx += 1;
            }
            if idx >= pattern.len() {
                *phase -= total;
                continue;
            }
            let remain = acc + pattern[idx] - *phase;
            let step = remain.min(major - t);
            if idx % 2 == 0 {
                let t0 = t / major;
                let t1 = (t + step) / major;
                out.move_to(a.0 + dx * t0, a.1 + dy * t0).line_to(a.0 + dx * t1, a.1 + dy * t1);
            }
            t += step;
            *phase += step;
            if *phase >= total - 1e-4 {
                *phase -= total;
            }
        }
    };
    for cmd in &path.cmds {
        match *cmd {
            PathCmd::MoveTo(x, y) => {
                cur = (x, y);
                start = cur;
            }
            PathCmd::LineTo(x, y) => {
                emit(cur, (x, y), &mut phase, &mut out);
                cur = (x, y);
            }
            PathCmd::CubicTo(a, b, c2, d, x, y) => {
                let p0 = cur;
                let n = 16;
                let mut prev = p0;
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let mt = 1.0 - t;
                    let px = mt * mt * mt * p0.0 + 3.0 * mt * mt * t * a + 3.0 * mt * t * t * c2 + t * t * t * x;
                    let py = mt * mt * mt * p0.1 + 3.0 * mt * mt * t * b + 3.0 * mt * t * t * d + t * t * t * y;
                    emit(prev, (px, py), &mut phase, &mut out);
                    prev = (px, py);
                }
                cur = (x, y);
            }
            PathCmd::Close => {
                emit(cur, start, &mut phase, &mut out);
                cur = start;
            }
        }
    }
    out
}
