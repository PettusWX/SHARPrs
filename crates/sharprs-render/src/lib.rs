//! `sharprs-render`: draws a SHARPpy-style SPC sounding window.
//!
//! The renderer takes a [`SoundingView`] (a plain view model holding every
//! number and array the display needs) plus [`RenderOptions`] (color scheme,
//! units, fonts, inset choice, header strings) and produces an image whose
//! layout, fonts, colors and line work follow SHARPpy's main window as it
//! appears on Pivotal Weather.
//!
//! ```no_run
//! use sharprs_render::{render_png, RenderOptions, SoundingView};
//! let view = SoundingView::default(); // filled by an adapter in practice
//! let png: Vec<u8> = render_png(&view, &RenderOptions::default()).unwrap();
//! ```
//!
//! Drawing goes through the backend-neutral [`canvas::Canvas`] trait; the
//! bundled backend is [`raster::RasterCanvas`] (tiny-skia + skrifa).

pub mod canvas;
pub mod layout;
pub mod options;
pub mod panels;
pub mod raster;
pub mod text;
pub mod theme;
pub mod util;
pub mod view;

mod physics;

pub use canvas::{Canvas, Color, Rect};
pub use layout::WindowLayout;
pub use options::{Deviant, Flavor, Inset, RenderOptions};
pub use raster::RasterCanvas;
pub use text::{FontFamily, TextBackend, TextHinting};
pub use theme::{ColorScheme, PwUnits, TempUnits, Theme, WindUnits};
pub use view::*;

/// Shared state handed to every panel.
pub struct DrawCtx<'a> {
    pub view: &'a SoundingView,
    pub opts: &'a RenderOptions,
    pub theme: &'a Theme,
}

impl DrawCtx<'_> {
    /// Font from a Qt point size at the configured DPI (PyQt truncates
    /// fractional point sizes to integers; CoreText then rounds the pixel
    /// size to a whole number).
    pub fn pt(&self, pt: f32) -> canvas::Font {
        let f = canvas::Font::from_pt(pt.trunc().max(1.0), self.opts.dpi);
        canvas::Font { px: self.opts.text_backend.resolve_px(f.px), ..f }
    }
    /// SHARPpy's font for the classic flavor; for the Pivotal flavor a
    /// size (pixels at the reference 1180x800 layout) measured from Pivotal
    /// Weather output, multiplied by `scale` (panel size / reference size).
    pub fn flavored(&self, classic: canvas::Font, pivotal_px: f32, scale: f32) -> canvas::Font {
        match self.opts.flavor {
            Flavor::Pivotal => canvas::Font { px: pivotal_px * scale, bold: classic.bold },
            Flavor::Classic => classic,
        }
    }
    pub fn deviant(&self) -> Deviant {
        self.opts.deviant.unwrap_or(if self.view.latitude < 0.0 { Deviant::Left } else { Deviant::Right })
    }
}

/// Draw the full SPC window onto any canvas.
///
/// The window is designed at 1180x800. Other sizes are handled like a
/// high-DPI display: everything is scaled uniformly by
/// `min(w / 1180, h / 800)`, and any leftover extent along the other axis
/// is absorbed by stretching the panel grid (as resizing SHARPpy's window
/// would).
pub fn render_to(canvas: &mut dyn Canvas, view: &SoundingView, opts: &RenderOptions) {
    let theme = opts.theme();
    let ctx = DrawCtx { view, opts, theme: &theme };
    let base = match opts.flavor {
        Flavor::Pivotal => WindowLayout::base(),
        Flavor::Classic => WindowLayout::classic_base(),
    };
    let (w, h) = canvas.size();
    let s = (w / layout::BASE_W).min(h / layout::BASE_H);
    canvas.save();
    canvas.scale(s);
    let lay = WindowLayout::scaled_f(base, w / s, h / s);
    panels::draw_window(canvas, &ctx, &lay);
    canvas.restore();
}

/// Render to a raster canvas.
pub fn render_canvas(view: &SoundingView, opts: &RenderOptions) -> RasterCanvas {
    let theme = opts.theme();
    let mut c = RasterCanvas::new(opts.width, opts.height, theme.bg, opts.font_family, opts.hinting);
    c.set_text_gamma(opts.text_gamma);
    c.set_text_backend(opts.text_backend);
    render_to(&mut c, view, opts);
    c
}

/// Render and encode as PNG.
pub fn render_png(view: &SoundingView, opts: &RenderOptions) -> Result<Vec<u8>, String> {
    render_canvas(view, opts).encode_png()
}

/// Render to straight RGBA8 pixels (`width * height * 4` bytes).
pub fn render_rgba(view: &SoundingView, opts: &RenderOptions) -> Vec<u8> {
    render_canvas(view, opts).to_rgba()
}
