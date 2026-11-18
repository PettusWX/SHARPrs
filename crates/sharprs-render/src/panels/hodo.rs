//! Hodograph: dashed isotach rings with speed labels on both axes, the
//! wind profile colored by height band, storm motions, Corfidi vectors,
//! LCL-EL mean wind, effective-inflow lines and the critical angle.

use crate::canvas::{Align, Canvas, CanvasExt, Color, Font, LineStyle, Pen, Rect};
use crate::theme::WindUnits;
use crate::util::{comp2vec, int2str, interp, kts2ms};
use crate::{DrawCtx, UV};

const MEAN_WIND: Color = Color::hex("#B8860B");
const CORFIDI: Color = Color::hex("#00BFFF");
const CRIT_LINE: Color = Color::hex("#FF00FF");

struct HodoGeom {
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
    scale: f32,
    ms: bool,
}

impl HodoGeom {
    fn uv_to_pix(&self, u: f64, v: f64) -> (f32, f32) {
        let conv = |s: f64| if self.ms { kts2ms(s) } else { s };
        (self.cx + conv(u) as f32 * self.scale, self.cy - conv(v) as f32 * self.scale)
    }
}

pub fn draw(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    // Pivotal's hodographs are zoomed out to SHARPpy's maximum (200 kt across).
    let pivotal = ctx.opts.flavor == crate::Flavor::Pivotal;
    let (hodomag, ring_inc) = match (ms, pivotal) {
        (true, false) => (80.0, 5.0),
        (true, true) => (100.0, 5.0),
        (false, false) => (160.0, 10.0),
        (false, true) => (200.0, 10.0),
    };
    let g = HodoGeom { w, h, cx: w / 2.0, cy: h / 2.0, scale: w / hodomag, ms };

    let fsize = if ctx.opts.dpi > 75.0 { 7.0 } else { 9.0 };
    let label_font = ctx.pt(fsize + h * 0.0045).bold(true);
    let crit_font = ctx.pt(fsize + 2.0 + h * 0.0045).bold(true);
    let crit_height = c.font_metrics(crit_font).x_height + 5.0 + h * 0.0045;

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);

    // Rings out to the farthest corner.
    let max_uv = ((g.w / 2.0 / g.scale).hypot(g.h / 2.0 / g.scale)) as i32;
    let mut spd = ring_inc;
    while spd < max_uv as f64 + ring_inc {
        draw_ring(c, ctx, &g, spd, label_font);
        spd += ring_inc;
    }
    let axis = Pen::new(th.fg, 2.0);
    c.line(g.cx.trunc(), 0.0, g.cx.trunc(), h, &axis);
    c.line(0.0, g.cy.trunc(), w, g.cy.trunc(), &axis);
    c.frame(0.0, 0.0, w, h, &Pen::new(th.fg, 2.0));

    let v = ctx.view;
    let prof = &v.profile;
    let nwind = prof.u.iter().zip(&prof.v).filter(|(a, b)| a.is_finite() && b.is_finite()).count();
    if nwind <= 1 {
        return;
    }
    draw_profile(c, ctx, &g);
    if pivotal {
        draw_km_markers(c, ctx, &g);
    }
    draw_storm_motion(c, ctx, &g, label_font);
    draw_corfidi(c, ctx, &g, label_font);
    draw_mean_wind(c, ctx, &g, label_font);
    draw_critical_angle(c, ctx, &g, crit_font, crit_height);
}

fn draw_ring(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom, spd: f64, font: Font) {
    let th = ctx.theme;
    let vv = spd as f32 * g.scale;
    c.stroke_ellipse(g.cx, g.cy, vv, vv, &Pen::styled(th.hodo_itach, 1.0, LineStyle::Dash));
    let (off, wd, ht) = (5.0, 15.0, 15.0);
    let rects = [
        Rect::new(g.cx + off, g.cy + vv - off, wd, ht),
        Rect::new(g.cx + vv - off, g.cy + off, wd, ht),
        Rect::new(g.cx + off, g.cy - vv - off, wd, ht),
        Rect::new(g.cx - vv - off, g.cy + off, wd, ht),
    ];
    let label = int2str(Some(spd));
    for r in rects {
        // SHARPpy paints the label boxes with a zero-width background pen
        // before the text; with no brush set only the cosmetic outline shows.
        c.stroke_rect(r, &Pen::new(th.bg, 1.0));
    }
    for r in rects {
        c.draw_text_clipped(r, Align::CENTER, &label, font, th.fg);
    }
}

/// Wind profile with heights AGL, missing levels removed.
fn wind_profile(ctx: &DrawCtx) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let prof = &ctx.view.profile;
    let sfc = prof.sfc_hght();
    let mut z = Vec::new();
    let mut u = Vec::new();
    let mut v = Vec::new();
    for i in 0..prof.pres.len() {
        let (a, b, hh) = (prof.u[i], prof.v[i], prof.hght[i]);
        if a.is_finite() && b.is_finite() && hh.is_finite() {
            z.push(hh - sfc);
            u.push(a);
            v.push(b);
        }
    }
    (z, u, v)
}

fn draw_profile(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom) {
    let th = ctx.theme;
    let (z, u, v) = wind_profile(ctx);
    if z.len() < 2 {
        return;
    }
    let pts: Vec<(f32, f32)> = u.iter().zip(&v).map(|(a, b)| g.uv_to_pix(*a, *b)).collect();
    let xs: Vec<f64> = pts.iter().map(|p| p.0 as f64).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1 as f64).collect();
    let zmin = z.iter().cloned().fold(f64::MAX, f64::min);
    let zmax = z.iter().cloned().fold(f64::MIN, f64::max);
    let bnds: Vec<f64> = [0.0, 3000.0, 6000.0, 9000.0, 12000.0].iter().map(|b: &f64| b.max(zmin)).collect();
    let seg: Vec<(f32, f32)> = bnds
        .iter()
        .filter(|b| **b <= zmax)
        .map(|b| (interp(*b, &z, &xs) as f32, interp(*b, &z, &ys) as f32))
        .collect();
    let idx_of = |b: f64| z.iter().position(|zz| *zz >= b).unwrap_or(z.len());
    for i in 0..seg.len().saturating_sub(1) {
        let mut path = vec![seg[i]];
        path.extend_from_slice(&pts[idx_of(bnds[i])..idx_of(bnds[i + 1]).max(idx_of(bnds[i]))]);
        path.push(seg[i + 1]);
        c.polyline(&path, &Pen::new(th.hgt_bands[i.min(3)], 2.0));
    }
    if zmax < 12000.0 && !seg.is_empty() {
        let i = seg.len() - 1;
        let mut path = vec![seg[i]];
        for p in pts.iter().skip(idx_of(bnds[i])) {
            path.push(*p);
        }
        c.polyline(&path, &Pen::new(th.hgt_bands[i.min(3)], 2.0));
    }
}

/// Pivotal-style height markers: gray dots labelled 1..6 km AGL on the trace.
fn draw_km_markers(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom) {
    let (z, u, v) = wind_profile(ctx);
    if z.len() < 2 {
        return;
    }
    let zmax = z.iter().cloned().fold(f64::MIN, f64::max);
    let fill = Color::hex("#cccccc");
    let font = Font::px(8.0 * g.h / 410.0).bold(true);
    for km in 1..=6 {
        let zz = km as f64 * 1000.0;
        if zz > zmax {
            break;
        }
        let (uu, vv) = (interp(zz, &z, &u), interp(zz, &z, &v));
        if !(uu.is_finite() && vv.is_finite()) {
            continue;
        }
        let (x, y) = g.uv_to_pix(uu, vv);
        c.fill_path(&crate::canvas::Path::ellipse(x, y, 4.5, 4.5), fill);
        c.draw_text(Rect::new(x - 5.0, y - 5.0, 10.0, 10.0), Align::CENTER, &km.to_string(), font, Color::BLACK);
    }
}

fn vec_label(ctx: &DrawCtx, uv: UV) -> String {
    let (d, s) = comp2vec(uv.0, uv.1);
    let s = if ctx.opts.wind_units == WindUnits::MetersPerSecond { kts2ms(s) } else { s };
    format!("{}/{}", int2str(Some(d)), int2str(Some(s)))
}

fn draw_storm_motion(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom, font: Font) {
    let th = ctx.theme;
    let st = &ctx.view.storm;
    let (Some(rm), Some(lm)) = (st.storm_motion.right, st.storm_motion.left) else { return };
    let pen = Pen::new(th.fg, 1.0);
    if let (Some(br), Some(bl)) = (st.bunkers.right, st.bunkers.left) {
        for b in [br, bl] {
            let (x, y) = g.uv_to_pix(b.0, b.1);
            c.line(x - 2.0, y, x + 2.0, y, &pen);
            c.line(x, y - 2.0, x, y + 2.0, &pen);
        }
    }
    let (rx, ry) = g.uv_to_pix(rm.0, rm.1);
    let (lx, ly) = g.uv_to_pix(lm.0, lm.1);
    c.stroke_ellipse(rx, ry, 5.0, 5.0, &pen);
    c.stroke_ellipse(lx, ly, 5.0, 5.0, &pen);

    if let Some((pbot, ptop)) = ctx.view.eff_inflow {
        let prof = &ctx.view.profile;
        let wind_at = |p: f64| {
            (crate::util::interp_p(p, &prof.pres, &prof.u), crate::util::interp_p(p, &prof.pres, &prof.v))
        };
        let (ut, vt) = wind_at(ptop);
        let (ub, vb) = wind_at(pbot);
        if ut.is_finite() && ub.is_finite() {
            let (xt, yt) = g.uv_to_pix(ut, vt);
            let (xb, yb) = g.uv_to_pix(ub, vb);
            let (sx, sy) = if ctx.deviant() == crate::Deviant::Left { (lx, ly) } else { (rx, ry) };
            let ep = Pen::new(th.eff_inflow, 1.0);
            c.line(sx.trunc(), sy.trunc(), xb.trunc(), yb.trunc(), &ep);
            c.line(sx.trunc(), sy.trunc(), xt.trunc(), yt.trunc(), &ep);
        }
    }

    let (ho, vo, wd, ht) = (2.0, 5.0, 55.0, 12.0);
    let rm_rect = Rect::new(rx + ho, ry + vo, wd, ht);
    let lm_rect = Rect::new(lx + ho, ly + vo, wd, ht);
    let br = st.bunkers.right.unwrap_or(rm);
    let bl = st.bunkers.left.unwrap_or(lm);
    c.draw_text_clipped(rm_rect, Align::CENTER, &format!("{} RM", vec_label(ctx, br)), font, th.fg);
    c.draw_text_clipped(lm_rect, Align::CENTER, &format!("{} LM", vec_label(ctx, bl)), font, th.fg);
}

fn draw_corfidi(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom, font: Font) {
    let st = &ctx.view.storm;
    let (Some(up), Some(dn)) = (st.corfidi_up, st.corfidi_dn) else { return };
    let pen = Pen::new(CORFIDI, 1.0);
    let (ux, uy) = g.uv_to_pix(up.0, up.1);
    let (dx, dy) = g.uv_to_pix(dn.0, dn.1);
    c.stroke_ellipse(ux, uy, 3.0, 3.0, &pen);
    c.stroke_ellipse(dx, dy, 3.0, 3.0, &pen);
    let (ho, vo, wd, ht) = (1.0, 3.0, 60.0, 10.0);
    c.draw_text_clipped(Rect::new(ux + ho, uy + vo, wd, ht), Align::CENTER, &format!("UP={}", vec_label(ctx, up)), font, CORFIDI);
    c.draw_text_clipped(Rect::new(dx + ho, dy + vo, wd, ht), Align::CENTER, &format!("DN={}", vec_label(ctx, dn)), font, CORFIDI);
}

fn draw_mean_wind(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom, font: Font) {
    let Some((d, s)) = ctx.view.storm.mean_lcl_el else { return };
    if !(d.is_finite() && s.is_finite()) {
        return;
    }
    let (u, v) = crate::util::vec2comp(d, s);
    let (x, y) = g.uv_to_pix(u, v);
    let half = 4.0;
    let r = Rect::new((x - half).trunc(), (y + half).trunc(), 8.0, 8.0);
    c.stroke_rect(r, &Pen::new(MEAN_WIND, 2.0));
    let spd = if ctx.opts.wind_units == WindUnits::MetersPerSecond { kts2ms(s) } else { s };
    let txt = format!("{}/{}", int2str(Some(d)), int2str(Some(spd)));
    c.draw_text_clipped(Rect::new(x + 1.0, y + 5.0, 40.0, 12.0), Align::CENTER, &txt, font, MEAN_WIND);
}

fn draw_critical_angle(c: &mut dyn Canvas, ctx: &DrawCtx, g: &HodoGeom, font: Font, crit_height: f32) {
    let v = ctx.view;
    let prof = &v.profile;
    let Some((pbot, ptop)) = v.eff_inflow else { return };
    if !(pbot.is_finite() && ptop.is_finite()) || (pbot - prof.sfc_pres()).abs() > 1e-6 {
        return;
    }
    let (z, u, vv) = wind_profile(ctx);
    let (su, sv) = (interp(0.0_f64.max(z[0]), &z, &u), interp(0.0_f64.max(z[0]), &z, &vv));
    let (u5, v5) = (interp(500.0, &z, &u), interp(500.0, &z, &vv));
    if !(su.is_finite() && sv.is_finite() && u5.is_finite() && v5.is_finite()) {
        return;
    }
    let (x1, y1) = g.uv_to_pix(su, sv);
    let (x2, y2) = g.uv_to_pix(u5, v5);
    c.line(x1.trunc(), y1.trunc(), x2.trunc(), y2.trunc(), &Pen::new(CRIT_LINE, 1.0));
    let st = &v.storm.storm_motion;
    if st.right.is_none() || st.left.is_none() {
        return;
    }
    let angle = *v.storm.critical_angle.get(ctx.deviant());
    let r = Rect::new(15.0, g.h - 36.0, 100.0, crit_height + 5.0);
    let txt = format!("Critical Angle = {}\u{00B0}", int2str(angle));
    c.draw_text(r, Align::LEFT, &txt, font, ctx.theme.hodo_crit);
}
