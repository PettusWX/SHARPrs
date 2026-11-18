//! Individual SPC-window panels and the window compositor.

pub mod barbs;
pub mod columns;
pub mod hodo;
pub mod insets;
pub mod skewt;
pub mod small;
pub mod tables;

use crate::canvas::{Align, Canvas, CanvasExt, Font, Pen, Rect};
use crate::layout::WindowLayout;
use crate::options::Flavor;
use crate::DrawCtx;

/// Run `f` with the canvas origin at `r`'s top-left and clipped to `r`.
pub(crate) fn in_rect(c: &mut dyn Canvas, r: Rect, f: impl FnOnce(&mut dyn Canvas, f32, f32)) {
    c.save();
    c.translate(r.x, r.y);
    c.set_clip(Some(Rect::new(0.0, 0.0, r.w, r.h)));
    c.save();
    f(c, r.w, r.h);
    c.restore();
    c.restore();
}

/// Compose the whole SPC window.
pub fn draw_window(c: &mut dyn Canvas, ctx: &DrawCtx, lay: &WindowLayout) {
    let th = ctx.theme;
    let (w, h) = c.size();
    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);

    in_rect(c, lay.skewt, |c, w, h| skewt::draw(c, ctx, w, h));
    draw_brand(c, ctx, lay.brand);
    in_rect(c, lay.speed, |c, w, h| columns::draw_speed(c, ctx, w, h));
    in_rect(c, lay.advection, |c, w, h| columns::draw_advection(c, ctx, w, h));
    in_rect(c, lay.hodo, |c, w, h| hodo::draw(c, ctx, w, h));
    in_rect(c, lay.slinky, |c, w, h| small::draw_slinky(c, ctx, w, h));
    in_rect(c, lay.thetae, |c, w, h| small::draw_thetae(c, ctx, w, h));
    in_rect(c, lay.srwinds, |c, w, h| small::draw_srwinds(c, ctx, w, h));
    in_rect(c, lay.watch, |c, w, h| small::draw_watch(c, ctx, w, h));
    in_rect(c, lay.thermo, |c, w, h| tables::draw_thermo(c, ctx, w, h));
    in_rect(c, lay.kinematics, |c, w, h| tables::draw_kinematics(c, ctx, w, h));
    in_rect(c, lay.left_inset, |c, w, h| insets::draw(c, ctx, ctx.opts.left_inset, w, h));
    in_rect(c, lay.right_inset, |c, w, h| insets::draw(c, ctx, ctx.opts.right_inset, w, h));
    draw_text_area_frame(c, ctx, lay);
}

fn draw_brand(c: &mut dyn Canvas, ctx: &DrawCtx, r: Rect) {
    let Some(text) = ctx.opts.header_right.as_deref() else { return };
    match ctx.opts.flavor {
        Flavor::Pivotal => {
            let scale = r.h / 26.0;
            let font = Font::px(12.0 * scale);
            // Right-aligned with a small right margin, vertically centred in the strip.
            let rect = Rect::new(r.x, r.y, r.w - 1.0, r.h);
            c.draw_text(rect, Align::RIGHT_VCENTER, text, font, ctx.theme.fg);
        }
        Flavor::Classic => {
            // SHARPpy's brand QLabel: `font-size: 11px`, 4 px top/bottom
            // padding, AlignRight with no vertical flag (so top-aligned in
            // the padded contents rect). The styled label's contents end
            // a few pixels short of the window edge.
            let scale = r.h / 21.0;
            let font = Font::px(11.0 * scale);
            let rect = Rect::new(r.x, r.y + 4.0 * scale, r.w - 5.0 * scale, r.h - 8.0 * scale);
            c.draw_text(rect, Align::RIGHT, text, font, ctx.theme.fg);
        }
    }
}

fn draw_text_area_frame(c: &mut dyn Canvas, ctx: &DrawCtx, lay: &WindowLayout) {
    let col = ctx.theme.text_border;
    let r = lay.text_area;
    // 2 px outer border drawn inside the rect.
    c.stroke_rect(Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0), &Pen::new(col, 2.0));
    let insets = [lay.thermo, lay.kinematics, lay.left_inset, lay.right_inset];
    match ctx.opts.flavor {
        Flavor::Pivotal => {
            // 1 px border of each inset frame.
            for ir in insets {
                c.stroke_rect(Rect::new(ir.x + 0.5, ir.y + 0.5, ir.w - 1.0, ir.h - 1.0), &Pen::new(col, 1.0));
            }
        }
        Flavor::Classic => {
            // Each inset QFrame's 1 px border shows on its left and top
            // edges only: the right/bottom edges fall under the next inset's
            // left border or the outer frame.
            for ir in insets {
                c.fill_rect(Rect::new(ir.x, ir.y, 1.0, ir.h), col);
                c.fill_rect(Rect::new(ir.x, ir.y, ir.w, 1.0), col);
            }
        }
    }
}
