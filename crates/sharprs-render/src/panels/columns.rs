//! The two narrow columns between the skew-T and the hodograph:
//! wind speed vs. pressure and inferred temperature advection.

use crate::canvas::{Align, Canvas, CanvasExt, Color, LineStyle, Pen, Rect};
use crate::theme::WindUnits;
use crate::util::{float2str, kts2ms};
use crate::DrawCtx;

const PMAX: f64 = 1050.0;
const PMIN: f64 = 100.0;

fn pres_to_pix(p: f64, bry: f32) -> f32 {
    let scl1 = PMAX.ln() - PMIN.ln();
    let scl2 = PMAX.ln() - p.ln();
    bry - ((scl2 / scl1) as f32) * bry
}

/// Wind speed column ("Wind Speed (knots)").
pub fn draw_speed(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    let (smax, delta, unit) = if ms { (80.0, 10.0, "m/s") } else { (140.0, 20.0, "knots") };
    let s2x = |s: f64| w * (s / smax) as f32;
    let label_font = ctx.pt((w * 0.12).round() + 2.0);
    let title_font = ctx.pt((w * 0.12).round() + 1.0);

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);
    c.frame(0.0, 0.0, w, bry, &Pen::new(th.fg, 2.0));
    let mut s = 0.0;
    while s < smax {
        let x = s2x(s).trunc();
        c.line(x, bry.trunc(), x, 0.0, &Pen::styled(th.spd_itach, 1.0, LineStyle::Dash));
        if s > 0.0 && (s as i64) % ((2.0 * delta) as i64) == 0 {
            let x0 = s2x(s - delta);
            let r = Rect::qt_int(x0, bry + 5.0, s2x(s + delta) - x0, 10.0);
            c.draw_text_clipped(r, Align::CENTER, &format!("{}", s as i64), label_font, th.fg);
        }
        s += delta;
    }
    let r = Rect::qt_int(2.0, 2.0, w, 30.0);
    c.draw_text_clipped(r, Align::LEFT.wrapped(), &format!("Wind Speed\n({unit})"), title_font, th.fg);

    let prof = &ctx.view.profile;
    let sfc = prof.sfc_hght();
    for i in 0..prof.pres.len() {
        let (u, v, z, p) = (prof.u[i], prof.v[i], prof.hght[i], prof.pres[i]);
        if !(u.is_finite() && v.is_finite() && z.is_finite() && p.is_finite()) {
            continue;
        }
        let mut spd = (u * u + v * v).sqrt();
        if ms {
            spd = kts2ms(spd);
        }
        let y = pres_to_pix(p, bry).trunc();
        let color = th.height_band(z - sfc);
        c.line(0.0, y, s2x(spd).trunc(), y, &Pen::new(color, 2.0));
    }
}

/// Inferred temperature advection column ("Inf. Temp. Adv. (C/hr)").
pub fn draw_advection(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    let adv_to_pix = |a: f64| (w / 2.0) * (1.0 + (a / 13.0) as f32);
    let font = ctx.pt((w * 0.12).round() + 3.0);

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);
    c.frame(0.0, 0.0, w, bry, &Pen::new(th.fg, 2.0));
    let x0 = adv_to_pix(0.0).trunc();
    c.line(x0, bry.trunc(), x0, 0.0, &Pen::styled(th.fg, 1.0, LineStyle::Dash));
    let title = match ctx.opts.flavor {
        crate::Flavor::Pivotal => "Inf. Temp. Adv.",
        crate::Flavor::Classic => "Inf. Temp. Adv. (C/hr)",
    };
    // Qt clips rect-drawn text to the rect unless TextDontClip is given, so
    // a wrapped line that falls below the 40 px box (SHARPpy's "(C/hr)")
    // is cut off.
    let title_rect = Rect::qt_int(2.0, 2.0, w - 2.0, 40.0);
    c.save();
    c.set_clip(Some(title_rect));
    c.draw_text(title_rect, Align::LEFT.wrapped(), title, font, th.fg);
    c.restore();

    let has_wind = ctx.view.profile.u.iter().any(|u| u.is_finite());
    if !has_wind {
        return;
    }
    // The layer boxes are painted straight onto the widget in paintEvent,
    // without SHARPpy's usual antialiasing hint: crisp 1 px lines.
    c.set_antialias(false);
    for layer in &ctx.view.inf_temp_adv {
        let Some(adv) = layer.adv.filter(|a| a.is_finite()) else { continue };
        let (color, label_x) = if adv > 0.0 {
            (Color::hex("#FF0000"), adv_to_pix(8.0) - 5.0)
        } else if adv < 0.0 {
            (Color::hex("#3399CC"), adv_to_pix(-8.0))
        } else {
            (th.fg, adv_to_pix(8.0) - 5.0)
        };
        let pt = pres_to_pix(layer.ptop, bry);
        let pb = pres_to_pix(layer.pbot, bry);
        let xa = adv_to_pix(adv);
        let pen = Pen::new(color, 1.0);
        let (pt_i, pb_i, xa_i) = (pt.trunc(), pb.trunc(), xa.trunc());
        c.line(xa_i, pt_i, xa_i, pb_i, &pen);
        c.line(x0, pb_i, xa_i, pb_i, &pen);
        c.line(xa_i, pt_i, x0, pt_i, &pen);
        c.line(x0, pt_i, x0, pb_i, &pen);
        let r = Rect::qt_int(label_x, (pt + pb) / 2.0, 5.0, 8.0);
        c.draw_text(r, Align::CENTER, &float2str(Some(adv), 1), font, color);
    }
}
