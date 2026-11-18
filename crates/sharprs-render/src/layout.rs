//! Window layout: where each panel sits.
//!
//! The reference geometry is measured from Pivotal Weather's 1180x800
//! output (which is SHARPpy's SPCWindow grid at that size): skew-T on the
//! left, wind-speed / temperature-advection columns, hodograph with four
//! small panels beneath, and the bordered text area along the bottom with
//! four insets. Other output sizes scale these rectangles proportionally;
//! each panel derives its fonts from its own size, as SHARPpy's widgets do.

use crate::canvas::Rect;

pub const BASE_W: f32 = 1180.0;
pub const BASE_H: f32 = 800.0;

/// Pixel rectangles of every panel (widget rectangles, as Qt would place them).
#[derive(Clone, Copy, Debug)]
pub struct WindowLayout {
    pub skewt: Rect,
    /// Strip above the hodograph holding the right-hand header text.
    pub brand: Rect,
    pub speed: Rect,
    pub advection: Rect,
    pub hodo: Rect,
    pub slinky: Rect,
    pub thetae: Rect,
    pub srwinds: Rect,
    pub watch: Rect,
    /// Outer rectangle of the bottom text area (2 px border).
    pub text_area: Rect,
    pub thermo: Rect,
    pub kinematics: Rect,
    pub left_inset: Rect,
    pub right_inset: Rect,
}

impl WindowLayout {
    /// The 1180x800 reference layout.
    pub fn base() -> WindowLayout {
        WindowLayout {
            skewt: Rect::new(9.0, 4.0, 581.0, 590.0),
            brand: Rect::new(590.0, 4.0, 581.0, 26.0),
            speed: Rect::new(590.0, 30.0, 60.0, 564.0),
            advection: Rect::new(650.0, 30.0, 40.0, 564.0),
            hodo: Rect::new(690.0, 30.0, 481.0, 410.0),
            slinky: Rect::new(690.0, 440.0, 120.0, 154.0),
            thetae: Rect::new(810.0, 440.0, 120.0, 154.0),
            srwinds: Rect::new(930.0, 440.0, 121.0, 154.0),
            watch: Rect::new(1051.0, 440.0, 120.0, 154.0),
            text_area: Rect::new(9.0, 596.0, 1162.0, 195.0),
            thermo: Rect::new(11.0, 598.0, 290.0, 191.0),
            kinematics: Rect::new(301.0, 598.0, 289.0, 191.0),
            left_inset: Rect::new(590.0, 598.0, 290.0, 191.0),
            right_inset: Rect::new(880.0, 598.0, 289.0, 191.0),
        }
    }

    /// Stock SHARPpy's 1180x800 window, as Qt lays out SPCWidget's grid:
    /// 1 px window margins and 2 px vertical spacing; the skew-T fills the
    /// left column above the text area; the right column holds the brand
    /// label (11 px font + 4 px top/bottom padding = 21 px) above the
    /// hodograph frame. Measured from real SHARPpy renders at 1180x800.
    ///
    /// The four text-area insets share the 1174 px inside the 2 px frame
    /// (294/293/294/293). Each paints its content pixmap at (1, 1) at full
    /// widget size, which covers its own right/bottom 1 px border, so only
    /// left/top borders show (see `draw_text_area_frame`).
    pub fn classic_base() -> WindowLayout {
        WindowLayout {
            skewt: Rect::new(1.0, 1.0, 589.0, 597.0),
            brand: Rect::new(590.0, 1.0, 589.0, 21.0),
            speed: Rect::new(590.0, 22.0, 61.0, 576.0),
            advection: Rect::new(651.0, 22.0, 41.0, 576.0),
            hodo: Rect::new(692.0, 22.0, 487.0, 419.0),
            slinky: Rect::new(692.0, 441.0, 121.0, 157.0),
            thetae: Rect::new(813.0, 441.0, 122.0, 157.0),
            srwinds: Rect::new(935.0, 441.0, 122.0, 157.0),
            watch: Rect::new(1057.0, 441.0, 122.0, 157.0),
            text_area: Rect::new(1.0, 600.0, 1178.0, 199.0),
            thermo: Rect::new(3.0, 602.0, 294.0, 195.0),
            kinematics: Rect::new(297.0, 602.0, 293.0, 195.0),
            left_inset: Rect::new(590.0, 602.0, 294.0, 195.0),
            right_inset: Rect::new(884.0, 602.0, 293.0, 195.0),
        }
    }

    /// Layout for an arbitrary output size (scaled from the reference and
    /// snapped to whole pixels, like Qt's integer widget geometry).
    pub fn for_size(width: u32, height: u32) -> WindowLayout {
        Self::scaled(WindowLayout::base(), width, height)
    }

    /// Scale a 1180x800 reference layout to another output size.
    pub fn scaled(b: WindowLayout, width: u32, height: u32) -> WindowLayout {
        Self::scaled_f(b, width as f32, height as f32)
    }

    /// Scale a reference layout to a (possibly fractional) logical size.
    pub fn scaled_f(b: WindowLayout, width: f32, height: f32) -> WindowLayout {
        let sx = width / BASE_W;
        let sy = height / BASE_H;
        let s = |r: Rect| {
            let x0 = (r.x * sx).round();
            let y0 = (r.y * sy).round();
            let x1 = (r.right() * sx).round();
            let y1 = (r.bottom() * sy).round();
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        };
        WindowLayout {
            skewt: s(b.skewt),
            brand: s(b.brand),
            speed: s(b.speed),
            advection: s(b.advection),
            hodo: s(b.hodo),
            slinky: s(b.slinky),
            thetae: s(b.thetae),
            srwinds: s(b.srwinds),
            watch: s(b.watch),
            text_area: s(b.text_area),
            thermo: s(b.thermo),
            kinematics: s(b.kinematics),
            left_inset: s(b.left_inset),
            right_inset: s(b.right_inset),
        }
    }
}
