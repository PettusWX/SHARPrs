//! Text shaping-lite, hinting and glyph rasterization.
//!
//! Fonts are bundled TrueType files parsed with `skrifa`. Glyph outlines are
//! optionally grid-fitted by skrifa's port of the FreeType autohinter (the
//! same engine FreeType/Qt use on Linux for fonts without bytecode hints, such
//! as unhinted Roboto) and rasterized with an exact-area coverage rasterizer.
//! The result is close to what Qt + FreeType produce on a Linux server at
//! 96 DPI, which is what Pivotal Weather's SHARPpy output shows.

use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph_rasterizer::{Rasterizer, point};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target};
use skrifa::raw::tables::gpos::{PairPos, PositionSubtables};
use skrifa::raw::types::Tag;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::canvas::{Font, FontMetrics};

/// Bundled font families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FontFamily {
    /// Roboto 2.138 (Apache-2.0). What Pivotal Weather's Linux renderer ends
    /// up using when SHARPpy asks for "Helvetica".
    #[default]
    Roboto,
    /// Arimo (OFL-1.1), metric-compatible with Helvetica/Arial: the
    /// "classic" SHARPpy look.
    Arimo,
}

/// Glyph grid-fitting mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextHinting {
    /// FreeType autohinter, normal target (hinting in both directions,
    /// integer advances, glyphs on whole pixels).
    Full,
    /// FreeType autohinter, light target (vertical snapping only) with
    /// sub-pixel glyph positioning: what Qt 5 does with fontconfig's default
    /// `hintslight`, and what Pivotal Weather output shows.
    #[default]
    Light,
    /// Unhinted outlines with fractional positions.
    None,
}

/// Which platform text stack's font metrics to reproduce. Qt reports
/// integer metrics differently per font engine, which moves every
/// top/centre-aligned string and every metrics-derived row height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextBackend {
    /// Qt's FreeType engine (Linux, e.g. Pivotal Weather): fractional pixel
    /// sizes; hinted ascender/descender rounded outwards.
    #[default]
    FreeType,
    /// Qt's CoreText engine (macOS, stock SHARPpy desktop): point sizes
    /// resolve to whole pixels; ascent/descent/x-height are rounded to the
    /// nearest pixel.
    CoreText,
}

impl TextBackend {
    /// Pixel size Qt ends up using for a fractional request.
    pub fn resolve_px(self, px: f32) -> f32 {
        match self {
            TextBackend::FreeType => px,
            TextBackend::CoreText => px.round().max(1.0),
        }
    }
}

static ROBOTO_REGULAR: &[u8] = include_bytes!("../fonts/roboto/Roboto-Regular.ttf");
static ROBOTO_BOLD: &[u8] = include_bytes!("../fonts/roboto/Roboto-Bold.ttf");
static ARIMO_REGULAR: &[u8] = include_bytes!("../fonts/arimo/Arimo-Regular.ttf");
static ARIMO_BOLD: &[u8] = include_bytes!("../fonts/arimo/Arimo-Bold.ttf");

/// A rasterized glyph coverage mask, positioned relative to the pen
/// position on the baseline (y down).
pub(crate) struct GlyphBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    /// Coverage 0..=255, row-major.
    pub coverage: Vec<u8>,
}

/// A glyph placed on a line, with its x position relative to the line start.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PlacedGlyph {
    pub gid: GlyphId,
    pub x: f32,
}

type SizeKey = (bool, u32);

pub(crate) struct TextEngine {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
    hinting: TextHinting,
    backend: TextBackend,
    instances: HashMap<SizeKey, Option<HintingInstance>>,
    advances: HashMap<(SizeKey, u32), f32>,
    glyphs: HashMap<(SizeKey, u32, u8), Rc<GlyphBitmap>>,
    metrics: HashMap<SizeKey, FontMetrics>,
    /// GPOS `kern` pair adjustments in font units, keyed by (bold, left, right).
    kerns: HashMap<(bool, u32, u32), i32>,
}

fn size_key(font: Font) -> SizeKey {
    (font.bold, (font.px * 64.0).round().max(1.0) as u32)
}

struct PathCollector {
    cmds: Vec<Seg>,
}

#[derive(Clone, Copy)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

impl OutlinePen for PathCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Seg::Move(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Seg::Line(x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.cmds.push(Seg::Quad(cx0, cy0, x, y));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.cmds.push(Seg::Cubic(cx0, cy0, cx1, cy1, x, y));
    }
    fn close(&mut self) {
        self.cmds.push(Seg::Close);
    }
}

impl TextEngine {
    pub fn new(family: FontFamily, hinting: TextHinting) -> TextEngine {
        let (r, b) = match family {
            FontFamily::Roboto => (ROBOTO_REGULAR, ROBOTO_BOLD),
            FontFamily::Arimo => (ARIMO_REGULAR, ARIMO_BOLD),
        };
        TextEngine {
            regular: FontRef::new(r).expect("bundled regular font parses"),
            bold: FontRef::new(b).expect("bundled bold font parses"),
            hinting,
            backend: TextBackend::FreeType,
            instances: HashMap::new(),
            advances: HashMap::new(),
            glyphs: HashMap::new(),
            metrics: HashMap::new(),
            kerns: HashMap::new(),
        }
    }

    pub fn set_backend(&mut self, backend: TextBackend) {
        if self.backend != backend {
            self.backend = backend;
            self.metrics.clear();
        }
    }

    fn face(&self, bold: bool) -> &FontRef<'static> {
        if bold { &self.bold } else { &self.regular }
    }

    /// Whether glyphs are snapped to whole-pixel pen positions.
    pub fn integer_positions(&self) -> bool {
        self.hinting == TextHinting::Full
    }

    fn ensure_instance(&mut self, font: Font) {
        let key = size_key(font);
        if !self.instances.contains_key(&key) {
            let inst = match self.hinting {
                TextHinting::None => None,
                h => {
                    let mode = if h == TextHinting::Full { SmoothMode::Normal } else { SmoothMode::Light };
                    let opts = HintingOptions {
                        engine: Engine::Auto(None),
                        target: Target::Smooth {
                            mode,
                            symmetric_rendering: true,
                            preserve_linear_metrics: false,
                        },
                    };
                    let face = self.face(font.bold);
                    HintingInstance::new(
                        &face.outline_glyphs(),
                        Size::new(key.1 as f32 / 64.0),
                        LocationRef::default(),
                        opts,
                    )
                    .ok()
                }
            };
            self.instances.insert(key, inst);
        }
    }

    /// QFontMetrics-style integer metrics.
    pub fn metrics(&mut self, font: Font) -> FontMetrics {
        let key = size_key(font);
        if let Some(m) = self.metrics.get(&key) {
            return *m;
        }
        let px = key.1 as f32 / 64.0;
        let m = self.face(font.bold).metrics(Size::new(px), LocationRef::default());
        let (ascent, descent) = match self.backend {
            // FreeType rounds scaled ascender up and descender down when the
            // face is hinted; Qt then reports them as-is.
            TextBackend::FreeType => (m.ascent.ceil(), (-m.descent).ceil()),
            // QFontMetrics rounds CoreText's fractional values.
            TextBackend::CoreText => (m.ascent.round(), (-m.descent).round()),
        };
        let x_height = m.x_height.unwrap_or(px * 0.5).round();
        let cap_height = m.cap_height.unwrap_or(px * 0.7).round();
        let fm = FontMetrics { ascent, descent, height: ascent + descent, x_height, cap_height };
        self.metrics.insert(key, fm);
        fm
    }

    /// Ascent and descent Qt's text layout positions lines with. The
    /// FreeType engine works in its (rounded) integer metrics; CoreText
    /// keeps the fractional values, so centred and multi-line text lands on
    /// sub-pixel baselines that are rounded only when glyphs are drawn.
    pub fn layout_metrics(&mut self, font: Font) -> (f32, f32) {
        match self.backend {
            TextBackend::FreeType => {
                let m = self.metrics(font);
                (m.ascent, m.descent)
            }
            TextBackend::CoreText => {
                let px = size_key(font).1 as f32 / 64.0;
                let m = self.face(font.bold).metrics(Size::new(px), LocationRef::default());
                (m.ascent, -m.descent)
            }
        }
    }

    fn glyph_id(&self, bold: bool, ch: char) -> GlyphId {
        let face = self.face(bold);
        face.charmap().map(ch).unwrap_or(GlyphId::new(0))
    }

    fn draw_outline(&mut self, font: Font, gid: GlyphId) -> (Vec<Seg>, f32) {
        let key = size_key(font);
        let px = key.1 as f32 / 64.0;
        let face = self.face(font.bold).clone();
        let glyphs = face.outline_glyphs();
        let mut pen = PathCollector { cmds: Vec::new() };
        let linear_adv = face
            .glyph_metrics(Size::new(px), LocationRef::default())
            .advance_width(gid)
            .unwrap_or(0.0);
        let Some(outline) = glyphs.get(gid) else {
            return (pen.cmds, linear_adv);
        };
        self.ensure_instance(font);
        let hinted = self.instances.get(&key).and_then(|i| i.as_ref());
        let adv = match hinted {
            Some(inst) => {
                let res = outline.draw(DrawSettings::hinted(inst, false), &mut pen);
                match res {
                    Ok(m) => m.advance_width.unwrap_or(linear_adv),
                    Err(_) => linear_adv,
                }
            }
            None => {
                let _ = outline.draw(DrawSettings::unhinted(Size::new(px), LocationRef::default()), &mut pen);
                linear_adv
            }
        };
        // With sub-pixel positioning Qt lays text out with the linear
        // (unhinted) advances; with full hinting advances are whole pixels.
        // Qt stores advances as 26.6 fixed point (QFixed), truncated
        // (matches Qt's string widths to 1/64 px).
        let adv = if self.integer_positions() { adv.round() } else { (linear_adv * 64.0).floor() / 64.0 };
        (pen.cmds, adv)
    }

    fn advance(&mut self, font: Font, gid: GlyphId) -> f32 {
        let key = (size_key(font), gid.to_u32());
        if let Some(a) = self.advances.get(&key) {
            return *a;
        }
        let (_, adv) = self.draw_outline(font, gid);
        self.advances.insert(key, adv);
        adv
    }

    /// Lay out a single line (no wrapping) starting at x = 0.
    pub fn layout(&mut self, text: &str, font: Font) -> (Vec<PlacedGlyph>, f32) {
        let mut x = 0.0f32;
        let mut out: Vec<PlacedGlyph> = Vec::with_capacity(text.len());
        for ch in text.chars() {
            let gid = self.glyph_id(font.bold, ch);
            // Qt shapes with kerning on by default (QFont::kerning()), which
            // adjusts the previous glyph's advance.
            if let Some(prev) = out.last() {
                x += self.kerning(font, prev.gid, gid);
            }
            out.push(PlacedGlyph { gid, x });
            x += self.advance(font, gid);
        }
        (out, x)
    }

    /// Horizontal pair adjustment (pixels) from the font's GPOS `kern`
    /// feature.
    fn kerning(&mut self, font: Font, left: GlyphId, right: GlyphId) -> f32 {
        let key = (font.bold, left.to_u32(), right.to_u32());
        let units = match self.kerns.get(&key) {
            Some(u) => *u,
            None => {
                let u = gpos_kern(self.face(font.bold), left, right);
                self.kerns.insert(key, u);
                u
            }
        };
        if units == 0 {
            return 0.0;
        }
        let px = size_key(font).1 as f32 / 64.0;
        let upem = self.face(font.bold).head().map(|h| h.units_per_em()).unwrap_or(2048) as f32;
        let k = units as f32 * px / upem;
        if self.integer_positions() { k.round() } else { (k * 64.0).round() / 64.0 }
    }

    pub fn width(&mut self, text: &str, font: Font) -> f32 {
        self.layout(text, font).1
    }

    /// Rasterized glyph; `frac_x` is the sub-pixel pen offset in [0, 1).
    pub fn glyph(&mut self, font: Font, gid: GlyphId, frac_x: f32) -> Rc<GlyphBitmap> {
        let bin = if self.integer_positions() { 0u8 } else { ((frac_x * 4.0).round() as u8) % 4 };
        let key = (size_key(font), gid.to_u32(), bin);
        if let Some(g) = self.glyphs.get(&key) {
            return g.clone();
        }
        let (segs, _) = self.draw_outline(font, gid);
        let dx = bin as f32 / 4.0;
        let bmp = Rc::new(rasterize(&segs, dx));
        self.glyphs.insert(key, bmp.clone());
        bmp
    }
}

fn rasterize(segs: &[Seg], dx: f32) -> GlyphBitmap {
    // Bounds (font space is y-up).
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut acc = |x: f32, y: f32| {
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    };
    for s in segs {
        match *s {
            Seg::Move(x, y) | Seg::Line(x, y) => acc(x, y),
            Seg::Quad(a, b, x, y) => {
                acc(a, b);
                acc(x, y)
            }
            Seg::Cubic(a, b, c, d, x, y) => {
                acc(a, b);
                acc(c, d);
                acc(x, y)
            }
            Seg::Close => {}
        }
    }
    if x0 > x1 {
        return GlyphBitmap { left: 0, top: 0, width: 0, height: 0, coverage: Vec::new() };
    }
    let left = (x0 + dx).floor() as i32;
    let right = (x1 + dx).ceil() as i32;
    let top = (-y1).floor() as i32; // y-down
    let bottom = (-y0).ceil() as i32;
    let w = (right - left).max(1) as usize;
    let h = (bottom - top).max(1) as usize;
    let mut r = Rasterizer::new(w, h);
    let tx = |x: f32| x + dx - left as f32;
    let ty = |y: f32| -y - top as f32;
    let mut start = (0.0f32, 0.0f32);
    let mut cur = (0.0f32, 0.0f32);
    for s in segs {
        match *s {
            Seg::Move(x, y) => {
                if cur != start {
                    r.draw_line(point(tx(cur.0), ty(cur.1)), point(tx(start.0), ty(start.1)));
                }
                start = (x, y);
                cur = (x, y);
            }
            Seg::Line(x, y) => {
                r.draw_line(point(tx(cur.0), ty(cur.1)), point(tx(x), ty(y)));
                cur = (x, y);
            }
            Seg::Quad(a, b, x, y) => {
                r.draw_quad(point(tx(cur.0), ty(cur.1)), point(tx(a), ty(b)), point(tx(x), ty(y)));
                cur = (x, y);
            }
            Seg::Cubic(a, b, c, d, x, y) => {
                r.draw_cubic(
                    point(tx(cur.0), ty(cur.1)),
                    point(tx(a), ty(b)),
                    point(tx(c), ty(d)),
                    point(tx(x), ty(y)),
                );
                cur = (x, y);
            }
            Seg::Close => {
                if cur != start {
                    r.draw_line(point(tx(cur.0), ty(cur.1)), point(tx(start.0), ty(start.1)));
                }
                cur = start;
            }
        }
    }
    if cur != start {
        r.draw_line(point(tx(cur.0), ty(cur.1)), point(tx(start.0), ty(start.1)));
    }
    let mut coverage = vec![0u8; w * h];
    r.for_each_pixel(|i, a| {
        coverage[i] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    });
    GlyphBitmap { left, top, width: w as u32, height: h as u32, coverage }
}

/// Greedy word wrap of one paragraph to `max_w` pixels.
pub(crate) fn wrap_words(engine: &mut TextEngine, text: &str, font: Font, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && engine.width(&candidate, font) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = candidate;
        }
    }
    lines.push(cur);
    lines
}

/// Sum of the x-advance adjustments the GPOS `kern` lookups apply to the
/// pair (`left`, `right`): the first pair-positioning subtable of each lookup
/// that covers the pair contributes. Errors or a missing table yield 0.
fn gpos_kern(face: &FontRef<'static>, left: GlyphId, right: GlyphId) -> i32 {
    let (Ok(l), Ok(r)) = (u16::try_from(left.to_u32()), u16::try_from(right.to_u32())) else { return 0 };
    let (l, r) = (skrifa::raw::types::GlyphId16::new(l), skrifa::raw::types::GlyphId16::new(r));
    let Ok(gpos) = face.gpos() else { return 0 };
    let (Ok(features), Ok(lookups)) = (gpos.feature_list(), gpos.lookup_list()) else { return 0 };
    let kern = Tag::new(b"kern");
    let mut indices: Vec<u16> = Vec::new();
    for rec in features.feature_records() {
        if rec.feature_tag() != kern {
            continue;
        }
        if let Ok(f) = rec.feature(features.offset_data()) {
            indices.extend(f.lookup_list_indices().iter().map(|i| i.get()));
        }
    }
    indices.sort_unstable();
    indices.dedup();
    let mut total = 0i32;
    for idx in indices {
        let Ok(lookup) = lookups.lookups().get(idx as usize) else { continue };
        let Ok(PositionSubtables::Pair(subtables)) = lookup.subtables() else { continue };
        for sub in subtables.iter().flatten() {
            let adj = match &sub {
                PairPos::Format1(t) => {
                    let Some(ci) = t.coverage().ok().and_then(|c| c.get(l)) else { continue };
                    let Ok(set) = t.pair_sets().get(ci as usize) else { continue };
                    let rec = set.pair_value_records().iter().flatten().find(|rec| rec.second_glyph() == r);
                    match rec {
                        Some(rec) => rec.value_record1().x_advance().unwrap_or(0) as i32,
                        None => continue,
                    }
                }
                PairPos::Format2(t) => {
                    if t.coverage().ok().and_then(|c| c.get(l)).is_none() {
                        continue;
                    }
                    let (Ok(cd1), Ok(cd2)) = (t.class_def1(), t.class_def2()) else { continue };
                    let (c1, c2) = (cd1.get(l), cd2.get(r));
                    t.class1_records()
                        .get(c1 as usize)
                        .ok()
                        .and_then(|rec| rec.class2_records().get(c2 as usize).ok())
                        .map(|rec| rec.value_record1().x_advance().unwrap_or(0) as i32)
                        .unwrap_or(0)
                }
            };
            total += adj;
            break;
        }
    }
    total
}
