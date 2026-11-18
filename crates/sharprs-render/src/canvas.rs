//! Backend-neutral drawing vocabulary.
//!
//! Panels draw through the [`Canvas`] trait using a small, Qt-flavoured model
//! (pens with widths and dash styles, axis-aligned clip rects, text laid out
//! inside rectangles with alignment flags). The raster backend in
//! [`crate::raster`] implements it on top of tiny-skia; an SVG backend could
//! implement the same trait later without touching any panel code.

/// 8-bit sRGB color with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    /// Parse `#rrggbb` (or `rrggbb`). Panics on malformed input, which is only
    /// ever used with literals from theme tables.
    pub const fn hex(s: &str) -> Color {
        let b = s.as_bytes();
        let off = if b[0] == b'#' { 1 } else { 0 };
        const fn nib(c: u8) -> u8 {
            match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => panic!("bad hex digit"),
            }
        }
        Color::rgb(
            nib(b[off]) * 16 + nib(b[off + 1]),
            nib(b[off + 2]) * 16 + nib(b[off + 3]),
            nib(b[off + 4]) * 16 + nib(b[off + 5]),
        )
    }

    pub const fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    /// `#rrggbb` string.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Axis-aligned rectangle in float pixels (top-left origin, y down).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    /// Qt's integer `QRect(x, y, w, h)` built from float arguments: PyQt
    /// truncates each argument toward zero.
    pub fn qt_int(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x.trunc(), y.trunc(), w.trunc(), h.trunc())
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }
    pub fn translated(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Dash style; lengths are expressed in multiples of the pen width, like Qt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineStyle {
    Solid,
    /// Qt::DashLine (4 on, 2 off).
    Dash,
    /// Qt::DotLine (1 on, 2 off).
    Dot,
    /// Qt::DashDotLine (4, 2, 1, 2).
    DashDot,
    /// Custom on/off pattern in pen-width units.
    Custom(&'static [f32]),
}

impl LineStyle {
    pub fn pattern(&self) -> Option<&'static [f32]> {
        match self {
            LineStyle::Solid => None,
            LineStyle::Dash => Some(&[4.0, 2.0]),
            LineStyle::Dot => Some(&[1.0, 2.0]),
            LineStyle::DashDot => Some(&[4.0, 2.0, 1.0, 2.0]),
            LineStyle::Custom(p) => Some(p),
        }
    }
}

/// Line cap (Qt's default pen cap is square).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Flat,
    Square,
    Round,
}

/// Stroke description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub color: Color,
    pub width: f32,
    pub style: LineStyle,
    pub cap: Cap,
}

impl Pen {
    pub fn new(color: Color, width: f32) -> Pen {
        Pen { color, width, style: LineStyle::Solid, cap: Cap::Square }
    }
    pub fn styled(color: Color, width: f32, style: LineStyle) -> Pen {
        Pen { color, width, style, cap: Cap::Square }
    }
    pub fn with_cap(mut self, cap: Cap) -> Pen {
        self.cap = cap;
        self
    }
}

/// Path command list (absolute coordinates).
#[derive(Clone, Debug, Default)]
pub struct Path {
    pub cmds: Vec<PathCmd>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Path {
    pub fn new() -> Path {
        Path { cmds: Vec::new() }
    }
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.cmds.push(PathCmd::MoveTo(x, y));
        self
    }
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.cmds.push(PathCmd::LineTo(x, y));
        self
    }
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> &mut Self {
        self.cmds.push(PathCmd::CubicTo(c1x, c1y, c2x, c2y, x, y));
        self
    }
    pub fn close(&mut self) -> &mut Self {
        self.cmds.push(PathCmd::Close);
        self
    }
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// Polyline through the given points; non-finite points break the line.
    pub fn polyline(pts: impl IntoIterator<Item = (f32, f32)>) -> Path {
        let mut p = Path::new();
        let mut pen_down = false;
        for (x, y) in pts {
            if !(x.is_finite() && y.is_finite()) {
                pen_down = false;
                continue;
            }
            if pen_down {
                p.line_to(x, y);
            } else {
                p.move_to(x, y);
                pen_down = true;
            }
        }
        p
    }

    pub fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Path {
        let mut p = Path::new();
        p.move_to(x1, y1).line_to(x2, y2);
        p
    }

    pub fn rect(r: Rect) -> Path {
        let mut p = Path::new();
        p.move_to(r.x, r.y)
            .line_to(r.right(), r.y)
            .line_to(r.right(), r.bottom())
            .line_to(r.x, r.bottom())
            .close();
        p
    }

    /// Ellipse centred on (cx, cy) with radii (rx, ry), built from four cubic arcs.
    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        const K: f32 = 0.552_284_8;
        let (kx, ky) = (rx * K, ry * K);
        let mut p = Path::new();
        p.move_to(cx + rx, cy)
            .cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry)
            .cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy)
            .cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry)
            .cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy)
            .close();
        p
    }
}

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Vertical text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

/// Qt-like alignment flags for text inside a rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align {
    pub h: HAlign,
    pub v: VAlign,
    /// Wrap at word boundaries to the rect width (Qt::TextWordWrap).
    pub wrap: bool,
}

impl Align {
    pub const fn new(h: HAlign, v: VAlign) -> Align {
        Align { h, v, wrap: false }
    }
    /// Qt::AlignCenter (both axes).
    pub const CENTER: Align = Align::new(HAlign::Center, VAlign::Center);
    /// Qt::AlignLeft (vertically: top).
    pub const LEFT: Align = Align::new(HAlign::Left, VAlign::Top);
    /// Qt::AlignRight (vertically: top).
    pub const RIGHT: Align = Align::new(HAlign::Right, VAlign::Top);
    pub const LEFT_VCENTER: Align = Align::new(HAlign::Left, VAlign::Center);
    pub const RIGHT_VCENTER: Align = Align::new(HAlign::Right, VAlign::Center);
    pub const CENTER_TOP: Align = Align::new(HAlign::Center, VAlign::Top);
    pub const CENTER_BOTTOM: Align = Align::new(HAlign::Center, VAlign::Bottom);
    pub const fn wrapped(mut self) -> Align {
        self.wrap = true;
        self
    }
}

/// A font request. The family (Roboto, Arimo, ...) is chosen by the canvas'
/// text engine; panels only ask for a pixel size and weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    /// Size in device pixels (em size).
    pub px: f32,
    pub bold: bool,
}

impl Font {
    /// Font from a Qt point size at the given logical DPI. Qt computes the
    /// pixel size as `pt * dpi / 72` (rounded to 1/100 px).
    pub fn from_pt(pt: f32, dpi: f32) -> Font {
        let px = ((pt * dpi * 100.0 + 50.0) / 72.0).floor() / 100.0;
        Font { px, bold: false }
    }
    pub fn px(px: f32) -> Font {
        Font { px, bold: false }
    }
    pub fn bold(mut self, b: bool) -> Font {
        self.bold = b;
        self
    }
}

/// Integer-rounded font metrics, as `QFontMetrics` reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    /// ascent + descent
    pub height: f32,
    pub x_height: f32,
    pub cap_height: f32,
}

/// The drawing surface used by every panel.
pub trait Canvas {
    /// Output size in device pixels.
    fn size(&self) -> (f32, f32);

    /// Push the current state (origin, clip, antialias flag).
    fn save(&mut self);
    /// Pop the last saved state.
    fn restore(&mut self);
    /// Move the origin by (dx, dy) in the current coordinate system.
    fn translate(&mut self, dx: f32, dy: f32);
    /// Scale subsequent drawing uniformly (like a device-pixel ratio);
    /// `size()` then reports logical units.
    fn scale(&mut self, s: f32);
    /// Set the clip to `r` (in current coordinates) intersected with the
    /// clip of the enclosing saved state. `None` resets to that parent clip.
    fn set_clip(&mut self, r: Option<Rect>);
    /// Toggle geometric antialiasing (text is always antialiased).
    fn set_antialias(&mut self, on: bool);

    fn fill_rect(&mut self, r: Rect, color: Color);
    fn fill_path(&mut self, path: &Path, color: Color);
    fn stroke_path(&mut self, path: &Path, pen: &Pen);

    /// Draw (possibly multi-line) text inside `rect` with Qt-like alignment.
    /// Text may overflow the rect (Qt::TextDontClip semantics).
    fn draw_text(&mut self, rect: Rect, align: Align, text: &str, font: Font, color: Color);
    /// Natural advance width of a single line of text.
    fn text_width(&mut self, text: &str, font: Font) -> f32;
    fn font_metrics(&mut self, font: Font) -> FontMetrics;
}

/// Convenience drawing helpers available on every canvas.
pub trait CanvasExt: Canvas {
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, pen: &Pen) {
        // Qt's raster engine plots a zero-length line drawn with a pen of
        // width <= 1 as the single pixel at that point, not as an
        // antialiased square cap.
        if x1 == x2 && y1 == y2 && pen.width <= 1.0 && pen.cap == Cap::Square {
            self.fill_rect(Rect::new(x1.floor(), y1.floor(), 1.0, 1.0), pen.color);
            return;
        }
        self.stroke_path(&Path::line(x1, y1, x2, y2), pen);
    }
    fn stroke_rect(&mut self, r: Rect, pen: &Pen) {
        self.stroke_path(&Path::rect(r), pen);
    }
    fn stroke_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, pen: &Pen) {
        self.stroke_path(&Path::ellipse(cx, cy, rx, ry), pen);
    }
    fn polyline(&mut self, pts: &[(f32, f32)], pen: &Pen) {
        let p = Path::polyline(pts.iter().copied());
        if !p.is_empty() {
            self.stroke_path(&p, pen);
        }
    }
    /// `QPainter::drawText(rect, flags, text)` without `Qt::TextDontClip`:
    /// whatever spills outside `rect` is clipped away.
    fn draw_text_clipped(&mut self, rect: Rect, align: Align, text: &str, font: Font, color: Color) {
        self.save();
        self.set_clip(Some(rect));
        self.draw_text(rect, align, text, font, color);
        self.restore();
    }
    /// Qt-style frame of four lines around (x0,y0)-(x1,y1).
    fn frame(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, pen: &Pen) {
        self.line(x0, y0, x1, y0, pen);
        self.line(x1, y0, x1, y1, pen);
        self.line(x1, y1, x0, y1, pen);
        self.line(x0, y1, x0, y0, pen);
    }
}

impl<T: Canvas + ?Sized> CanvasExt for T {}
