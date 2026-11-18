//! Skew-T log-p diagram.
//!
//! Coordinates follow SHARPpy's: log-pressure vertical axis from 1050 to
//! 100 hPa, isotherms skewed by 100/3 degrees, and fixed pads around the
//! plot area for pressure labels (left), temperature labels (bottom) and
//! wind barbs (right).

use crate::canvas::{Align, Canvas, CanvasExt, Color, Font, HAlign, LineStyle, Pen, Rect, VAlign};
use crate::options::Flavor;
use crate::panels::barbs::draw_barb;
use crate::physics;
use crate::theme::{TempUnits, WindUnits};
use crate::util::{self, comp2vec, int2str, float2str, interp_p, kts2ms, m2ft, pres_at_hght};
use crate::DrawCtx;

const PMAX: f64 = 1050.0;
const PMIN: f64 = 100.0;
const XSKEW_DEG: f64 = 100.0 / 3.0;

/// Fixed colors SHARPpy hard-codes in the skew-T.
const SIG_TEMP_LEVEL: Color = Color::hex("#0A63FF");
const DCAPE_TRACE: Color = Color::hex("#FF00FF");
const OMEGA_AXIS: Color = Color::hex("#FF00FF");
const OMEGA_UP: Color = Color::hex("#FF6666");
const OMEGA_DOWN: Color = Color::hex("#0066CC");

/// Geometry of the skew-T inside its widget rectangle.
pub struct SkewGeom {
    pub lpad: f64,
    pub rpad: f64,
    pub tpad: f64,
    pub bpad: f64,
    pub brx: f64,
    pub bry: f64,
    pub bltmpc: f64,
    pub brtmpc: f64,
    /// Omega axis bounds (temperatures at 1000 hPa for +10 / -10).
    pub omega_plus10: f64,
    pub omega_minus10: f64,
    log_pmax: f64,
    log_pmin: f64,
    yrange: f64,
}

impl SkewGeom {
    pub fn new(w: f64, h: f64, flavor: Flavor) -> SkewGeom {
        // Pivotal's build trims the barb gutter and starts the temperature
        // axis at -40 C; stock SHARPpy uses a 65 px gutter and -50 C.
        let (rpad, bltmpc) = match flavor {
            Flavor::Pivotal => (51.0, -40.0),
            Flavor::Classic => (65.0, -50.0),
        };
        let (lpad, tpad, bpad) = (30.0, 25.0, 20.0);
        let brtmpc = 50.0;
        let shift = bltmpc + 50.0;
        SkewGeom {
            lpad,
            rpad,
            tpad,
            bpad,
            brx: w - rpad,
            bry: h - bpad,
            bltmpc,
            brtmpc,
            omega_plus10: -49.0 + shift,
            omega_minus10: -41.0 + shift,
            log_pmax: PMAX.ln(),
            log_pmin: PMIN.ln(),
            yrange: XSKEW_DEG.to_radians().tan() * (brtmpc - bltmpc),
        }
    }

    pub fn p_to_y(&self, p: f64) -> f64 {
        let scl1 = self.log_pmax - self.log_pmin;
        let scl2 = self.log_pmax - p.ln();
        self.bry - (scl2 / scl1) * (self.bry - self.tpad)
    }

    /// Pressure at a pointer position inside the plotted temperature area.
    pub fn pressure_at(&self, x: f64, y: f64) -> Option<f64> {
        if !(self.lpad..=self.brx).contains(&x) || !(self.tpad..=self.bry).contains(&y) { return None; }
        let fraction = (self.bry - y) / (self.bry - self.tpad);
        Some((self.log_pmax - fraction * (self.log_pmax - self.log_pmin)).exp())
    }

    pub fn t_to_x(&self, t: f64, p: f64) -> f64 {
        let xrange = self.brtmpc - self.bltmpc;
        let scl1 = self.brtmpc - ((self.bry - self.p_to_y(p)) / (self.bry - self.tpad)) * self.yrange;
        self.brx - ((scl1 - t) / xrange) * (self.brx - self.lpad)
    }

    fn barbx(&self) -> f64 {
        self.brx + self.rpad / 2.0
    }
}

/// Font sizes used by the skew-T (pixels).
struct SkewFonts {
    title: Font,
    label: Font,
    tick_label: Font,
    env_trace: Font,
    in_plot: Font,
    esrh: Font,
    hght: Font,
    esrh_height: f32,
    title_height: f32,
}

fn fonts(c: &mut dyn Canvas, ctx: &DrawCtx, hgt: f32) -> SkewFonts {
    // SHARPpy picks base sizes by display DPI (points).
    let (fsize, fsizet) = if ctx.opts.dpi > 75.0 { (6.0, 10.0) } else { (7.0, 14.0) };
    let mut f = SkewFonts {
        title: ctx.pt(fsizet + hgt * 0.006),
        label: ctx.pt(fsize + 2.0 + hgt * 0.0045).bold(true),
        tick_label: ctx.pt(fsize + 2.0 + hgt * 0.0045).bold(true),
        env_trace: ctx.pt(11.0 + hgt * 0.0045),
        in_plot: ctx.pt(fsize + hgt * 0.0045),
        esrh: ctx.pt(fsize + 2.0 + hgt * 0.0045),
        hght: ctx.pt(9.0 + hgt * 0.0045),
        esrh_height: 0.0,
        title_height: 0.0,
    };
    // Pivotal's build uses a smaller title and pressure labels and a larger
    // lapse-rate/inflow annotation font (sizes measured at 1180x800).
    let s = hgt / 570.0;
    f.title = ctx.flavored(f.title, 13.33, s);
    f.label = ctx.flavored(f.label, 10.67, s);
    f.esrh = ctx.flavored(f.esrh, 14.67, s);
    let em = c.font_metrics(f.esrh);
    f.esrh_height = em.x_height + 9.0 + hgt * 0.0045;
    let tm = c.font_metrics(f.title);
    f.title_height = tm.x_height + 15.0 + hgt * 0.003;
    f
}

/// Draw the skew-T into a `w` x `h` widget whose origin is the canvas origin.
pub fn draw(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let g = SkewGeom::new(w as f64, h as f64, ctx.opts.flavor);
    let hgt = (h as f64 - g.bpad) as f32;
    let fnt = fonts(c, ctx, hgt);
    let clip = Rect::new(g.lpad as f32, g.tpad as f32, (g.brx + g.rpad - g.lpad + 1.0) as f32, (g.bry - g.tpad + 1.0) as f32);

    c.fill_rect(Rect::new(0.0, 0.0, w, h), th.bg);
    c.set_antialias(true);

    // ---- background ------------------------------------------------------
    c.set_clip(Some(clip));
    let mut t = g.bltmpc - 100.0;
    while t <= g.brtmpc {
        let x1 = g.t_to_x(t, PMAX).trunc() as f32;
        let x2 = g.t_to_x(t, PMIN).trunc() as f32;
        let color = if t as i64 == 0 || t as i64 == -20 { th.skew_itherm_hgz } else { th.skew_itherm };
        let pen = Pen::styled(color, 1.0, LineStyle::Custom(&[4.0, 2.0]));
        c.line(x1, g.bry.trunc() as f32, x2, g.tpad as f32, &pen);
        t += 10.0;
    }
    let mut theta = g.bltmpc;
    let ad_pen = Pen::new(th.skew_adiab, 1.0);
    while theta < 80.0 {
        let mut pts = Vec::new();
        let mut p = PMAX;
        while p >= PMIN - 0.01 {
            let tt = physics::dry_adiabat_temp(theta, p);
            pts.push((g.t_to_x(tt, p) as f32, g.p_to_y(p) as f32));
            p -= 10.0;
        }
        c.polyline(&pts, &ad_pen);
        theta += 20.0;
    }
    // SHARPpy's `[2] + np.arange(4, 33, 4)` broadcasts to 6, 10, ..., 34.
    for w_gkg in (4..=32).step_by(4).map(|v| (v + 2) as f64) {
        draw_mixing_ratio(c, ctx, &g, &fnt, w_gkg, 600.0);
    }
    // Frame: blank out the margins, then the 2 px border.
    c.set_clip(None);
    let bg = th.bg;
    c.fill_rect(Rect::new(-0.5, -0.5, g.lpad as f32 + 1.0, g.bry as f32 + 1.0), bg);
    let ybot = g.p_to_y(PMAX).trunc() as f32;
    c.fill_rect(Rect::new(-0.5, ybot - 0.5, g.brx as f32 + 1.0, g.bry as f32 + 1.0), bg);
    c.fill_rect(Rect::new(g.brx as f32 - 0.5, -0.5, (w as f64 - g.brx + 1.0) as f32, ybot + 1.0), bg);
    let fpen = Pen::new(th.fg, 2.0);
    let (l, tp, r, b) = (g.lpad as f32, g.tpad as f32, (g.brx + g.rpad) as f32, g.bry as f32);
    c.frame(l, tp, r, b, &fpen);
    for p in [1000.0, 850.0, 700.0, 500.0, 300.0, 200.0, 100.0] {
        draw_isobar(c, ctx, &g, &fnt, p, true);
    }
    let mut t = g.bltmpc;
    while t <= g.brtmpc {
        let x1 = g.t_to_x(t, PMAX);
        if x1 >= g.lpad && x1 <= g.brx + 0.001 {
            let r = Rect::qt_int((x1 - 10.0) as f32, (g.bry + 2.0) as f32, 20.0, 20.0);
            c.draw_text_clipped(r, Align::CENTER, &int2str(Some(t)), fnt.tick_label, th.fg);
        }
        t += 10.0;
    }
    let mut p = PMAX;
    while p >= PMIN - 0.01 {
        draw_isobar(c, ctx, &g, &fnt, p, false);
        p -= 50.0;
    }

    // ---- data -----------------------------------------------------------
    let v = ctx.view;
    let prof = &v.profile;
    draw_title(c, ctx, &g, &fnt);
    if prof.pres.is_empty() {
        return;
    }
    c.set_clip(Some(clip));
    let sfc_units_f = ctx.opts.temp_units == TempUnits::Fahrenheit;
    // drawTrace's `label` defaults to True, so stock SHARPpy also boxes the
    // surface wet-bulb value (partly covered by the temperature and dew
    // point labels drawn later); Pivotal's build doesn't show it.
    let wetb_label = ctx.opts.flavor == Flavor::Classic;
    if ctx.opts.show_wet_bulb {
        draw_trace(c, ctx, &g, &fnt, &prof.pres, &prof.wetbulb, th.wetb, 1.0, LineStyle::Solid, wetb_label, sfc_units_f);
    }
    draw_trace(c, ctx, &g, &fnt, &prof.pres, &prof.tmpc, th.temp, 3.0, LineStyle::Solid, true, sfc_units_f);
    if ctx.opts.show_virtual_temp {
        draw_trace(c, ctx, &g, &fnt, &prof.pres, &prof.vtmp, th.temp, 1.0, LineStyle::Dash, false, sfc_units_f);
    }

    let dgz_mode = ctx.opts.show_dgz && v.dgz.is_some_and(|(b, t)| b != t);
    if dgz_mode {
        draw_dgz(c, ctx, &g, &fnt);
    } else {
        let drew_lr = draw_max_lapse_rate(c, ctx, &g, &fnt);
        // Stock SHARPpy never sets a font for the 0/-20/-30 C labels: they
        // inherit the lapse-rate font when that annotation was drawn, else
        // the surface-trace label font. Pivotal's build uses the height font.
        let level_font = match ctx.opts.flavor {
            Flavor::Pivotal => fnt.hght,
            Flavor::Classic if drew_lr => fnt.esrh,
            Flavor::Classic => fnt.env_trace,
        };
        draw_temp_levels(c, ctx, &g, level_font);
    }
    draw_trace(c, ctx, &g, &fnt, &prof.pres, &prof.dwpc, th.dewp, 3.0, LineStyle::Solid, true, sfc_units_f);

    for hm in [0.0, 1000.0, 3000.0, 6000.0, 9000.0, 12000.0, 15000.0] {
        draw_height(c, ctx, &g, &fnt, hm);
    }
    if let Some(pcl) = v.parcels.get(v.selected_parcel) {
        if ctx.opts.show_parcel_trace {
            draw_parcel_trace(c, &g, &pcl.ptrace, &pcl.ttrace, th.fg);
        }
        if ctx.opts.show_downdraft {
            draw_parcel_trace(c, &g, &v.dcape_ptrace, &v.dcape_ttrace, DCAPE_TRACE);
        }
    }
    if ctx.opts.show_pbl {
        draw_pbl(c, ctx, &g, &fnt);
    }
    draw_parcel_levels(c, ctx, &g, &fnt);

    // Barbs are drawn without antialiasing, clipped to the plot plus the
    // bottom pad.
    c.set_antialias(false);
    c.set_clip(Some(Rect::new(clip.x, clip.y, clip.w, clip.h + g.bpad as f32)));
    if ctx.opts.show_wind_barbs {
        draw_barbs(c, ctx, &g);
    }
    c.set_clip(Some(clip));
    c.set_antialias(true);

    let drew_eff = draw_effective_layer(c, ctx, &g, &fnt);
    let plot_omega = ctx.opts.show_omega.unwrap_or(!v.observed);
    if plot_omega && !prof.omeg.is_empty() {
        // SHARPpy labels the omega axis with whatever font is still set on
        // the painter: the inflow-layer font when that layer was drawn,
        // otherwise the height font from the parcel-level markers.
        let label_font = if drew_eff { fnt.esrh } else { fnt.hght };
        draw_omega(c, ctx, &g, label_font);
    }
    c.set_clip(None);
}

fn draw_mixing_ratio(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts, w: f64, pmin: f64) {
    let th = ctx.theme;
    let t1 = physics::temp_at_mixrat(w, PMAX);
    let t2 = physics::temp_at_mixrat(w, pmin);
    if !(t1.is_finite() && t2.is_finite()) {
        return;
    }
    let x1 = g.t_to_x(t1, PMAX) as f32;
    let y1 = g.p_to_y(PMAX) as f32;
    let x2 = g.t_to_x(t2, pmin) as f32;
    let y2 = g.p_to_y(pmin) as f32;
    let r = Rect::new(x2 - 5.0, y2 - 10.0, 10.0, 10.0);
    c.fill_rect(Rect::new(r.x - 0.5, r.y - 0.5, r.w + 1.0, r.h + 1.0), th.bg);
    let pen = Pen::new(th.skew_mixr, 1.0);
    c.line(x1.trunc(), y1.trunc(), x2.trunc(), y2.trunc(), &pen);
    c.draw_text_clipped(r, Align::new(HAlign::Center, VAlign::Bottom), &int2str(Some(w)), f.in_plot, th.skew_mixr);
}

fn draw_isobar(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts, p: f64, labeled: bool) {
    let th = ctx.theme;
    let y = g.p_to_y(p).trunc();
    if y < g.tpad || y > g.bry {
        return;
    }
    let pen = Pen::new(th.fg, 1.0);
    let yf = y as f32;
    if labeled {
        c.line(g.lpad as f32, yf, g.brx.trunc() as f32, yf, &pen);
        let r = Rect::new(1.0, yf - 20.0, (g.lpad - 4.0) as f32, 40.0);
        c.draw_text_clipped(r, Align::RIGHT_VCENTER, &int2str(Some(p)), f.label, th.fg);
    } else {
        let off = 5.0;
        c.line(g.lpad as f32, yf, (g.lpad + off) as f32, yf, &pen);
        let xr = (g.brx + g.rpad) as f32;
        c.line(xr - off as f32, yf, xr, yf, &pen);
    }
}

fn draw_title(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) {
    let title = ctx.opts.header_left.clone().unwrap_or_else(|| ctx.view.title.clone());
    let r = Rect::qt_int(g.lpad as f32, 2.0, 150.0, f.title_height);
    c.set_clip(None);
    c.draw_text(r, Align::LEFT, &title, f.title, ctx.theme.fg);
}

#[allow(clippy::too_many_arguments)]
fn draw_trace(
    c: &mut dyn Canvas,
    ctx: &DrawCtx,
    g: &SkewGeom,
    f: &SkewFonts,
    pres: &[f64],
    data: &[f64],
    color: Color,
    width: f32,
    style: LineStyle,
    label: bool,
    fahrenheit: bool,
) {
    let pts: Vec<(f32, f32)> = pres
        .iter()
        .zip(data)
        .filter(|(p, t)| p.is_finite() && t.is_finite())
        .map(|(p, t)| (g.t_to_x(*t, *p) as f32, g.p_to_y(*p) as f32))
        .collect();
    let first = pres.iter().zip(data).find(|(p, t)| p.is_finite() && t.is_finite());
    if pts.len() < 2 {
        return;
    }
    c.polyline(&pts, &Pen::styled(color, width, style));
    if label {
        let (_, t0) = first.unwrap();
        let (x0, y0) = pts[0];
        let val = if fahrenheit { util::ctof(*t0) } else { *t0 };
        c.set_clip(None);
        let r = Rect::new(x0 - 8.0, y0 + 4.0, 16.0, 12.0);
        c.fill_rect(Rect::new(r.x - 0.5, r.y - 0.5, r.w + 1.0, r.h + 1.0), ctx.theme.bg);
        c.draw_text_clipped(r, Align::CENTER, &int2str(Some(val)), f.env_trace, color);
        let clip = Rect::new(g.lpad as f32, g.tpad as f32, (g.brx + g.rpad - g.lpad + 1.0) as f32, (g.bry - g.tpad + 1.0) as f32);
        c.set_clip(Some(clip));
    }
}

fn draw_parcel_trace(c: &mut dyn Canvas, g: &SkewGeom, ptrace: &[f64], ttrace: &[f64], color: Color) {
    let pts: Vec<(f32, f32)> = ptrace
        .iter()
        .zip(ttrace)
        .filter(|(p, t)| p.is_finite() && t.is_finite())
        .map(|(p, t)| (g.t_to_x(*t, *p) as f32, g.p_to_y(*p) as f32))
        .collect();
    if pts.len() >= 2 {
        c.polyline(&pts, &Pen::styled(color, 1.0, LineStyle::Dash));
    }
}

fn agl_pres(ctx: &DrawCtx, h_agl: f64) -> f64 {
    let prof = &ctx.view.profile;
    pres_at_hght(h_agl + prof.sfc_hght(), &prof.pres, &prof.hght)
}

fn hght_agl_at(ctx: &DrawCtx, p: f64) -> f64 {
    let prof = &ctx.view.profile;
    interp_p(p, &prof.pres, &prof.hght) - prof.sfc_hght()
}

fn draw_height(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts, h: f64) {
    let p = agl_pres(ctx, h);
    if !p.is_finite() {
        return;
    }
    let y = g.p_to_y(p).trunc() as f32;
    let lp = g.lpad as f32;
    let col = ctx.theme.skew_hgt;
    c.line(lp, y, lp + 10.0, y, &Pen::new(col, 1.0));
    let r = Rect::new(lp + 15.0, y - 20.0, lp + 15.0, 40.0);
    c.draw_text_clipped(r, Align::LEFT_VCENTER, &format!("{} km", int2str(Some(h / 1000.0))), f.hght, col);
}

fn draw_sig_level(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts, p: Option<f64>, color: Color, id: &str) {
    let Some(p) = p.filter(|v| v.is_finite()) else { return };
    let z = m2ft(interp_p(p, &ctx.view.profile.pres, &ctx.view.profile.hght));
    let x0 = g.t_to_x(37.0, 1000.0) as f32;
    let x1 = g.t_to_x(45.0, 1000.0) as f32;
    let y = g.p_to_y(p) as f32;
    c.line(x0.trunc(), y.trunc(), x1.trunc(), y.trunc(), &Pen::new(color, 1.0));
    let l0 = g.t_to_x(20.0, 1000.0) as f32;
    let l1 = g.t_to_x(36.0, 1000.0) as f32;
    let r = Rect::new(l0, y - 3.0, l1 - l0, 4.0);
    c.draw_text(r, Align::RIGHT, &format!("{id}{}'", int2str(Some(z))), f.hght, color);
}

fn draw_dgz(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) {
    let v = ctx.view;
    let Some((pbot, ptop)) = v.dgz else { return };
    let mut pres = Vec::new();
    let mut p = ptop;
    while p < pbot {
        pres.push(p);
        p += 5.0;
    }
    pres.reverse();
    let tmpc: Vec<f64> = pres.iter().map(|p| interp_p(*p, &v.profile.pres, &v.profile.tmpc)).collect();
    draw_trace(c, ctx, g, f, &pres, &tmpc, ctx.theme.skew_dgz, 3.0, LineStyle::Solid, false, false);
    let dgz_col = Color::hex("#F5D800");
    draw_sig_level(c, ctx, g, f, Some(pbot), dgz_col, "");
    draw_sig_level(c, ctx, g, f, Some(ptop), dgz_col, "");
    draw_sig_level(c, ctx, g, f, v.wbz_pres, ctx.theme.dewp, "WBZ=");
    draw_sig_level(c, ctx, g, f, v.frz_pres, Color::hex("#FFA500"), "FRZ=");
}

fn draw_pbl(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) {
    let Some(p) = ctx.view.ppbl_top.filter(|p| p.is_finite()) else { return };
    let x0 = g.t_to_x(37.0, 1000.0).trunc() as f32;
    let x1 = g.t_to_x(41.0, 1000.0).trunc() as f32;
    let y = g.p_to_y(p) as f32;
    let gray = Color::hex("#a0a0a4");
    c.line(x0, y.trunc(), x1, y.trunc(), &Pen::new(gray, 2.0));
    c.draw_text(Rect::new(x0, y + 6.0, x1 - x0, 4.0), Align::CENTER, "PBL", f.hght, gray);
}

fn draw_parcel_levels(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) {
    let v = ctx.view;
    let Some(pcl) = v.parcels.get(v.selected_parcel) else { return };
    let th = ctx.theme;
    let x0 = g.t_to_x(37.0, 1000.0) as f32;
    let x1 = g.t_to_x(41.0, 1000.0) as f32;
    let (xa, xb) = (x0.trunc(), x1.trunc());
    if let Some(p) = pcl.lclpres.filter(|p| p.is_finite()) {
        let y = g.p_to_y(p) as f32;
        c.line(xa, y.trunc(), xb, y.trunc(), &Pen::new(th.skew_lcl_mkr, 2.0));
        c.draw_text(Rect::new(x0, y + 6.0, x1 - x0, 4.0), Align::CENTER, "LCL", f.hght, th.skew_lcl_mkr);
    }
    if let Some(p) = pcl.lfcpres.filter(|p| p.is_finite()) {
        let y = g.p_to_y(p) as f32;
        c.line(xa, y.trunc(), xb, y.trunc(), &Pen::new(th.skew_lfc_mkr, 2.0));
        c.draw_text(Rect::new(x0, y - 8.0, x1 - x0, 4.0), Align::CENTER, "LFC", f.hght, th.skew_lfc_mkr);
    }
    if let Some(p) = pcl.elpres.filter(|p| p.is_finite()) {
        if Some(p) != pcl.lclpres {
            let y = g.p_to_y(p) as f32;
            c.line(xa, y.trunc(), xb, y.trunc(), &Pen::new(th.skew_el_mkr, 2.0));
            c.draw_text(Rect::new(x0, y - 8.0, x1 - x0, 4.0), Align::CENTER, "EL", f.hght, th.skew_el_mkr);
        }
    }
}

fn draw_temp_levels(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, font: Font) {
    let v = ctx.view;
    let Some(pcl) = v.parcels.get(v.selected_parcel) else { return };
    // Pivotal's build draws these two degrees left of the LCL/LFC/EL markers.
    let t0 = if ctx.opts.flavor == Flavor::Pivotal { 35.0 } else { 37.0 };
    let x0 = g.t_to_x(t0, 1000.0) as f32;
    let x1 = g.t_to_x(t0 + 4.0, 1000.0) as f32;
    for (p, h, label) in [
        (pcl.p0c, pcl.hght0c, "0 C"),
        (pcl.pm20c, pcl.hghtm20c, "-20 C"),
        (pcl.pm30c, pcl.hghtm30c, "-30 C"),
    ] {
        let Some(p) = p.filter(|p| p.is_finite()) else { continue };
        let y = g.p_to_y(p) as f32;
        c.line(x0.trunc(), y.trunc(), x1.trunc(), y.trunc(), &Pen::new(SIG_TEMP_LEVEL, 2.0));
        let txt = format!("{label}={}'", int2str(h.map(m2ft)));
        c.draw_text(Rect::new(x0, y - 12.0, x1 - x0, 4.0), Align::LEFT, &txt, font, SIG_TEMP_LEVEL);
    }
}

/// Returns whether the annotation was drawn.
fn draw_max_lapse_rate(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) -> bool {
    let v = ctx.view;
    let Some((lr, pbot, ptop)) = v.max_lapse_rate_2_6 else { return false };
    if !(lr.is_finite() && pbot.is_finite() && ptop.is_finite()) || lr < 4.5 {
        return false;
    }
    let th = ctx.theme;
    let vt = interp_p(pbot, &v.profile.pres, &v.profile.vtmp);
    let x1 = g.t_to_x(vt + 5.0, pbot) as f32;
    let y1 = g.p_to_y(pbot) as f32;
    let y2 = g.p_to_y(ptop) as f32;
    let r = Rect::new(x1 - 15.0, y2 - f.esrh_height, 50.0, f.esrh_height);
    c.fill_rect(Rect::new(r.x - 0.5, r.y - 0.5, r.w + 1.0, r.h + 1.0), th.bg);
    let color = if lr >= 8.0 {
        th.alert[5]
    } else if lr >= 7.0 {
        th.alert[4]
    } else if lr >= 6.0 {
        th.alert[1]
    } else {
        th.alert[0]
    };
    let pen = Pen::new(color, 1.5);
    let ll = 10.0;
    c.line((x1 - ll).trunc(), y1.trunc(), (x1 + ll).trunc(), y1.trunc(), &pen);
    c.line((x1 - ll).trunc(), y2.trunc(), (x1 + ll).trunc(), y2.trunc(), &pen);
    c.line(x1.trunc(), y1.trunc(), x1.trunc(), y2.trunc(), &pen);
    c.set_clip(None);
    c.draw_text(r, Align::LEFT, &format!("{} C/km", float2str(Some(lr), 1)), f.esrh, color);
    let clip = Rect::new(g.lpad as f32, g.tpad as f32, (g.brx + g.rpad - g.lpad + 1.0) as f32, (g.bry - g.tpad + 1.0) as f32);
    c.set_clip(Some(clip));
    true
}

fn draw_barbs(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom) {
    let prof = &ctx.view.profile;
    if prof.u.is_empty() {
        return;
    }
    let psfc = prof.sfc_pres();
    let ptop = prof.pres.iter().rev().find(|p| p.is_finite()).copied().unwrap_or(PMIN);
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    let shemis = ctx.view.latitude < 0.0;
    let mut p = psfc;
    while p > ptop {
        if p >= PMIN {
            let u = interp_p(p, &prof.pres, &prof.u);
            let vv = interp_p(p, &prof.pres, &prof.v);
            if u.is_finite() && vv.is_finite() {
                let (d, s) = comp2vec(u, vv);
                let s = if ms { kts2ms(s) } else { s };
                let y = g.p_to_y(p) as f32;
                draw_barb(c, g.barbx() as f32, y, d, s, ctx.theme.fg, shemis);
            }
        }
        p -= 40.0;
    }
}

/// Returns whether the layer was drawn.
fn draw_effective_layer(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, f: &SkewFonts) -> bool {
    let v = ctx.view;
    let Some((pbot, ptop)) = v.eff_inflow else { return false };
    if !(pbot.is_finite() && ptop.is_finite()) {
        return false;
    }
    let th = ctx.theme;
    let x1 = g.t_to_x(-20.0, 1000.0) as f32;
    let x2 = g.t_to_x(-33.0, 1000.0) as f32;
    let y1 = g.p_to_y(pbot) as f32;
    let y2 = g.p_to_y(ptop) as f32;
    let r1 = Rect::new(x2, y1 + 4.0, 25.0, f.esrh_height);
    let r2 = Rect::new(x2, y2 - f.esrh_height, 50.0, f.esrh_height);
    let r3 = Rect::new(x1 - 15.0, y2 - f.esrh_height, 50.0, f.esrh_height);
    let text_bot = if (v.profile.sfc_pres() - pbot).abs() < 1e-6 {
        "SFC".to_string()
    } else {
        format!("{}m", int2str(Some(hght_agl_at(ctx, pbot))))
    };
    let text_top = format!("{}m", int2str(Some(hght_agl_at(ctx, ptop))));
    for r in [r1, r2, r3] {
        c.fill_rect(Rect::new(r.x - 0.5, r.y - 0.5, r.w + 1.0, r.h + 1.0), th.bg);
    }
    let pen = Pen::new(th.eff_inflow, 2.0);
    let len = 15.0;
    c.line((x1 - len).trunc(), y1.trunc(), (x1 + len).trunc(), y1.trunc(), &pen);
    c.line((x1 - len).trunc(), y2.trunc(), (x1 + len).trunc(), y2.trunc(), &pen);
    c.line(x1.trunc(), y1.trunc(), x1.trunc(), y2.trunc(), &pen);
    c.set_clip(None);
    c.draw_text(r1, Align::LEFT, &text_bot, f.esrh, th.eff_inflow);
    let clip = Rect::new(g.lpad as f32, g.tpad as f32, (g.brx + g.rpad - g.lpad + 1.0) as f32, (g.bry - g.tpad + 1.0) as f32);
    c.set_clip(Some(clip));
    c.draw_text(r2, Align::LEFT, &text_top, f.esrh, th.eff_inflow);
    let esrh = *v.kinematics.esrh.get(ctx.deviant());
    c.draw_text(r3, Align::LEFT, &format!("{} m2s2", int2str(esrh)), f.esrh, th.eff_inflow);
    true
}

fn draw_omega(c: &mut dyn Canvas, ctx: &DrawCtx, g: &SkewGeom, label_font: Font) {
    let prof = &ctx.view.profile;
    let x_m10 = g.t_to_x(g.omega_minus10, 1000.0) as f32;
    let x_p10 = g.t_to_x(g.omega_plus10, 1000.0) as f32;
    let x0 = g.t_to_x((g.omega_plus10 + g.omega_minus10) / 2.0, 1000.0) as f32;
    let y1 = g.p_to_y(1000.0) as f32;
    let y2 = g.p_to_y(111.0) as f32;
    let dd = Pen::styled(OMEGA_AXIS, 1.0, LineStyle::DashDot);
    c.line(x_m10.trunc(), y1.trunc(), x_m10.trunc(), y2.trunc(), &dd);
    c.line(x_p10.trunc(), y1.trunc(), x_p10.trunc(), y2.trunc(), &dd);
    c.line(x0.trunc(), y1.trunc(), x0.trunc(), y2.trunc(), &Pen::new(OMEGA_AXIS, 1.0));
    c.draw_text(Rect::new(x_p10, y2 - 18.0, x_m10 - x_p10, 4.0), Align::CENTER, "OMEGA", label_font, OMEGA_AXIS);
    c.draw_text(Rect::new(x_m10 - 3.0, y2 - 7.0, 5.0, 4.0), Align::CENTER, "-10", label_font, OMEGA_AXIS);
    c.draw_text(Rect::new(x_p10 - 3.0, y2 - 7.0, 5.0, 4.0), Align::CENTER, "+10", label_font, OMEGA_AXIS);
    let to_x = |om: f64| -> f32 {
        let om = om as f32;
        if om > 0.0 {
            ((x0 - x_p10) / (0.0 - 10.0)) * om + x0
        } else if om < 0.0 {
            ((x0 - x_m10) / 10.0) * om + x0
        } else {
            x0
        }
    };
    for (p, om) in prof.pres.iter().zip(&prof.omeg) {
        if !om.is_finite() || !p.is_finite() || *p < 111.0 {
            continue;
        }
        let y = g.p_to_y(*p) as f32;
        let pen = if *om > 0.0 {
            Pen::new(OMEGA_DOWN, 1.5)
        } else if *om < 0.0 {
            Pen::new(OMEGA_UP, 1.5)
        } else {
            Pen::new(OMEGA_AXIS, 1.0)
        };
        // drawLine(float...) resolves to the int overload in PySide2.
        c.line(x0.trunc(), y.trunc(), to_x(om * 10.0).trunc(), y.trunc(), &pen);
    }
}
