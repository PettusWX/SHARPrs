//! The four small panels under the hodograph: storm slinky, theta-e vs.
//! pressure, storm-relative wind vs. height, and possible hazard type.

use crate::canvas::{Align, Canvas, CanvasExt, Color, LineStyle, Pen, Rect};
use crate::util::{self, comp2vec, int2str, vec2comp};
use crate::DrawCtx;

fn frame(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, bry: f32) {
    c.frame(0.0, 0.0, w, bry, &Pen::new(ctx.theme.fg, 2.0));
}

/// Storm slinky: parcel positions every few seconds of ascent, relative to
/// a storm moving with the active storm motion.
pub fn draw_slinky(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    let cx = w / 2.0;
    let cy = bry / 2.0;
    let scale = w / (7000.0 * 1.7);
    let to_pix = |x: f64, y: f64| (cx + x as f32 * scale, cy - y as f32 * scale);
    let title_font = ctx.pt((h * 0.0512).round() + 2.0);
    let title_height = c.font_metrics(title_font).x_height + 5.0;

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);
    let axis = Pen::new(Color::hex("#003366"), 2.0);
    c.line(cx.trunc(), 0.0, cx.trunc(), bry, &axis);
    c.line(0.0, cy.trunc(), w, cy.trunc(), &axis);
    frame(c, ctx, w, bry);
    let r = Rect::qt_int(5.0, bry - title_height - 2.0, 20.0, title_height);
    c.draw_text(r, Align::LEFT, "Storm Slinky", title_font, th.fg);

    // Without a trajectory (no buoyant parcel) SHARPpy skips only the
    // circles; the storm-motion vector and "-- deg" are still drawn.
    let sl = ctx.view.slinky.get(ctx.deviant()).as_ref();
    let traj: &[(f64, f64, f64)] = sl.map(|s| s.traj.as_slice()).unwrap_or(&[]);
    let has_el = sl.is_some_and(|s| s.has_el);
    let last_z = traj.last().map(|t| t.2);
    for &(x, y, z) in traj {
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let color = if has_el && Some(z) == last_z {
            Color::hex("#FF00FF")
        } else if z < 12000.0 {
            th.height_band(z)
        } else {
            continue;
        };
        let (px, py) = to_pix(x, y);
        c.stroke_ellipse(px, py, 5.0, 5.0, &Pen::new(color, 1.0));
    }
    if let Some((smu, smv)) = *ctx.view.storm.storm_motion.get(ctx.deviant()) {
        let (wdir, _) = comp2vec(smu, smv);
        let (u, v) = vec2comp(wdir, 3000.0);
        let (x1, y1) = to_pix(u, v);
        let (x0, y0) = to_pix(0.0, 0.0);
        c.line(x1.trunc(), y1.trunc(), x0.trunc(), y0.trunc(), &Pen::new(th.fg, 2.0));
    }
    let r = Rect::qt_int(w - 30.0, 2.0, 30.0, title_height);
    c.draw_text(r, Align::RIGHT, &format!("{} deg  ", int2str(sl.and_then(|s| s.tilt))), title_font, th.fg);
}

/// Theta-e vs. pressure (linear pressure axis 1025-400 hPa).
pub fn draw_thetae(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    // Pivotal's build shows only the lowest ~450 hPa (1000-550 hPa) with no
    // title; stock SHARPpy spans 1025-400 hPa.
    let pivotal = ctx.opts.flavor == crate::Flavor::Pivotal;
    let (pmax, pmin) = if pivotal { (1000.0, 550.0) } else { (1025.0, 400.0) };
    let prof = &ctx.view.profile;
    let (mut tmin, mut tmax) = (f64::MAX, f64::MIN);
    for (p, t) in prof.pres.iter().zip(&prof.thetae) {
        if *p > 400.0 && t.is_finite() {
            tmin = tmin.min(*t);
            tmax = tmax.max(*t);
        }
    }
    if tmin > tmax {
        tmin = 300.0;
        tmax = 360.0;
    } else {
        tmin -= 10.0;
        tmax += 10.0;
    }
    let p2y = |p: f64| bry - (((pmax - p) / (pmax - pmin)) as f32) * bry;
    // SHARPpy maps theta-e with the panel height (bry), not its width.
    let t2x = |t: f64| bry - (((tmax - t) / (tmax - tmin)) as f32) * bry;
    let font = ctx.pt((h * 0.0512).round());

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);
    frame(c, ctx, w, bry);
    if !pivotal {
        c.draw_text_clipped(Rect::qt_int(35.0, 15.0, 50.0, 50.0), Align::CENTER, "Theta-E\nv.\nPres", font, th.fg);
    }
    let pen = Pen::new(th.fg, 1.0);
    let label_w = if pivotal { 27.0 } else { 20.0 };
    for p in [1000.0, 900.0, 800.0, 700.0, 600.0, 500.0] {
        if p >= pmax || p <= pmin {
            continue;
        }
        let y = p2y(p).trunc();
        c.line(0.0, y, 5.0, y, &pen);
        c.line(w - 5.0, y, w, y, &pen);
        c.draw_text_clipped(Rect::qt_int(0.0, y - 20.0, label_w, 40.0), Align::RIGHT_VCENTER, &int2str(Some(p)), font, th.fg);
    }
    let mut t = 200.0;
    while t < 400.0 && !pivotal {
        let x = t2x(t).trunc();
        c.line(x, 0.0, x, 5.0, &pen);
        c.line(x, bry - 5.0, x, bry, &pen);
        c.draw_text_clipped(Rect::qt_int(x, bry - 20.0, 15.0, 20.0), Align::CENTER, &int2str(Some(t)), font, th.fg);
        t += 10.0;
    }
    // One antialiased drawLine per level pair with truncated endpoints, for
    // every pair whose lower level is below 400 hPa (the segment crossing
    // the top is clipped by the widget).
    let pts: Vec<(f64, f64)> =
        prof.pres.iter().zip(&prof.thetae).filter(|(p, t)| p.is_finite() && t.is_finite()).map(|(p, t)| (*p, *t)).collect();
    let pen = Pen::new(th.temp, 2.0);
    for s in pts.windows(2) {
        let ((p1, t1), (p2, t2)) = (s[0], s[1]);
        if p1 > 400.0 {
            c.line(t2x(t1).trunc(), p2y(p1).trunc(), t2x(t2).trunc(), p2y(p2).trunc(), &pen);
        }
    }
}

/// Storm-relative wind speed vs. height AGL (0-16 km, 0-80 kt).
pub fn draw_srwinds(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    let h2y = |hk: f64| (bry - 2.0) - (hk / 16.0) as f32 * bry;
    let s2x = |s: f64| bry - (((80.0 - s) / 80.0) as f32) * bry;
    let font = ctx.pt((h * 0.0512).round());

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);
    c.draw_text_clipped(Rect::qt_int(15.0, 5.0, 45.0, 35.0), Align::CENTER, "SR Wind\nv.\nHeight", font, th.fg);
    frame(c, ctx, w, bry);
    let x15 = s2x(15.0).trunc();
    c.line(x15, bry, x15, 0.0, &Pen::styled(th.fg, 1.0, LineStyle::Dash));
    let cl = Pen::styled(th.srw_clsc, 1.0, LineStyle::Dash);
    for s in [40.0, 70.0] {
        let x = s2x(s).trunc();
        c.line(x, h2y(8.0).trunc(), x, h2y(16.0).trunc(), &cl);
    }
    c.draw_text_clipped(Rect::qt_int(s2x(40.0) - 5.0, 2.0, 50.0, 50.0), Align::CENTER, "Classic\nSupercell", font, th.srw_clsc);
    let pen = Pen::new(th.fg, 1.0);
    for hk in [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0] {
        let y = h2y(hk).trunc();
        c.line(0.0, y, 5.0, y, &pen);
        c.line(w - 5.0, y, w, y, &pen);
        c.draw_text_clipped(Rect::qt_int(0.0, y - 20.0, 20.0, 40.0), Align::RIGHT_VCENTER, &int2str(Some(hk)), font, th.fg);
    }
    for s in (0..100).step_by(10) {
        let x = s2x(s as f64).trunc();
        c.line(x, 0.0, x, 5.0, &pen);
        c.line(x, bry - 5.0, x, bry, &pen);
    }

    let v = ctx.view;
    let prof = &v.profile;
    let Some((smu, smv)) = *v.storm.storm_motion.get(ctx.deviant()) else { return };
    // SHARPpy drops only levels with a missing wind; a missing height keeps
    // its raw -9999 fill, which np.interp then searches through as data.
    const MISSING: f64 = -9999.0;
    let mut z = Vec::new();
    let mut su = Vec::new();
    let mut sv = Vec::new();
    for i in 0..prof.pres.len() {
        let (u, vv, hh) = (prof.u[i], prof.v[i], prof.hght[i]);
        if u.is_finite() && vv.is_finite() {
            z.push(if hh.is_finite() { hh } else { MISSING });
            su.push(u - smu);
            sv.push(vv - smv);
        }
    }
    // Trailing missing heights are trimmed before interpolating.
    while z.last() == Some(&MISSING) {
        z.pop();
        su.pop();
        sv.pop();
    }
    if z.len() < 2 {
        return;
    }
    let zsfc = prof.sfc_hght();
    let ztop = z[z.len() - 1].min(16000.0);
    // np.arange(zsfc, ztop, 10)
    let steps = ((ztop - zsfc) / 10.0).ceil().max(0.0) as usize;
    let hs: Vec<f64> = (0..steps).map(|i| zsfc + 10.0 * i as f64).collect();
    let iu = util::numpy_interp(&hs, &z, &su);
    let iv = util::numpy_interp(&hs, &z, &sv);
    let pts: Vec<(f32, f32)> = hs
        .iter()
        .zip(iu.iter().zip(&iv))
        .map(|(h, (a, b))| (s2x(a.hypot(*b)).trunc(), h2y((h - zsfc) / 1000.0).trunc()))
        .collect();
    // SHARPpy strokes every 10 m step as its own antialiased drawLine with
    // integer (truncated) endpoints, so the overlapping square caps build
    // up into a bright ~2 px trace rather than one thin polyline.
    let pen = Pen::new(th.srw_trace, 1.0);
    for s in pts.windows(2) {
        if s[0].0.is_finite() && s[1].0.is_finite() {
            c.line(s[0].0, s[0].1, s[1].0, s[1].1, &pen);
        }
    }
    let k = &v.kinematics;
    for (val, h0, h1, col) in [
        (k.srw_0_2km.get(ctx.deviant()), 0.0, 2.0, th.srw_0_2),
        (k.srw_4_6km.get(ctx.deviant()), 4.0, 6.0, th.srw_4_6),
        (k.srw_9_11km.get(ctx.deviant()), 9.0, 11.0, th.srw_9_11),
    ] {
        if let Some((u, vv)) = *val {
            let spd = (u * u + vv * vv).sqrt();
            let x = s2x(spd).trunc();
            c.line(x, h2y(h0).trunc(), x, h2y(h1).trunc(), &Pen::new(col, 2.0));
        }
    }
}

/// "Psbl Haz. Type" box.
pub fn draw_watch(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let bry = h - 20.0;
    let title_font = ctx.flavored(ctx.pt((h * 0.0512).round() + 5.0), 14.67, h / 154.0);
    let plot_font = ctx.flavored(ctx.pt((h * 0.0512).round() + 4.0), 13.33, h / 154.0);
    let title_height = c.font_metrics(title_font).height;
    let pad = bry / 100.0;

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(false);
    frame(c, ctx, w, bry);
    c.draw_text(Rect::qt_int(0.0, pad * 4.0, w, title_height), Align::CENTER, "Psbl Haz. Type", title_font, th.fg);
    let y = (pad * 4.0 + title_height + 3.0).trunc();
    c.line(0.0, y, w, y, &Pen::new(th.fg, 1.0));
    c.set_antialias(true);
    if let Some(wt) = ctx.view.watch_type.get(ctx.deviant()) {
        let col = th.watch_color(wt);
        c.draw_text(Rect::qt_int(0.0, bry / 2.0, w, title_height), Align::CENTER, wt, plot_font, col);
    }
}
