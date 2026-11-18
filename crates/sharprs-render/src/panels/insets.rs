//! Swappable insets in the bottom-right of the window: SARS analogues,
//! STP climatology box plots, conditional STP / VROT probabilities, SHIP,
//! winter and fire weather.
//!
//! Each inset draws into a pixmap offset 1 px inside its frame, sized from
//! the widget dimensions as SHARPpy does (fonts in points proportional to
//! the inset height).

use crate::canvas::{Align, Canvas, CanvasExt, Color, Font, LineStyle, Pen, Rect};
use crate::options::{Flavor, Inset};
use crate::util::{float2str, int2str, m2ft};
use crate::DrawCtx;

const RED: Color = Color::hex("#FF0000");
const LBLUE: Color = Color::hex("#06B5FF");
const LBROWN: Color = Color::hex("#996600");
const DBROWN: Color = Color::hex("#775000");
const YELLOW: Color = Color::hex("#FFFF00");
const MAGENTA: Color = Color::hex("#E700DF");
const ORANGE: Color = Color::hex("#FF4000");
const GREEN: Color = Color::hex("#00FF00");
const DGREEN: Color = Color::hex("#006000");

pub fn draw(c: &mut dyn Canvas, ctx: &DrawCtx, which: Inset, w: f32, h: f32) {
    c.save();
    c.translate(1.0, 1.0);
    c.set_antialias(true);
    match which {
        Inset::Sars => draw_sars(c, ctx, w, h),
        Inset::StpStats => draw_stp_stats(c, ctx, w, h),
        Inset::Ship => draw_ship(c, ctx, w, h),
        Inset::CondStp => draw_cond_stp(c, ctx, w, h),
        Inset::Vrot => draw_vrot(c, ctx, w, h),
        Inset::Winter => draw_winter(c, ctx, w, h),
        Inset::Fire => draw_fire(c, ctx, w, h),
    }
    c.restore();
}

// ---------------------------------------------------------------- SARS ----

fn draw_sars(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let fg = th.fg;
    let brx = w;
    let bry = h;
    let bpad = 5.0;
    let tpad = (0.03 * h).floor();
    let x1 = brx / 6.0;
    let s = h / 191.0;
    let title_font = ctx.flavored(ctx.pt((0.09 * h).floor()), 21.33, s);
    let plot_font = ctx.flavored(ctx.pt((0.07 * h).floor()), 13.33, s);
    let match_font = ctx.flavored(ctx.pt((0.06 * h).floor()), 10.0, s);
    let title_h = c.font_metrics(title_font).x_height + tpad;
    let plot_h = c.font_metrics(plot_font).x_height + tpad;
    let match_h = c.font_metrics(match_font).x_height + tpad;
    let title = match ctx.opts.flavor {
        Flavor::Pivotal => "SARS - Sounding Analogs",
        Flavor::Classic => "SARS - Sounding Analogue System",
    };
    let pen = Pen::new(fg, 1.0);
    // The SARS widget paints without antialiasing (crisp separators).
    c.set_antialias(false);
    let mut y = tpad;
    c.draw_text(Rect::qt_int(0.0, y, brx, title_h), Align::CENTER, title, title_font, fg);
    y += title_h + tpad;
    c.line(0.0, y.trunc(), brx, y.trunc(), &pen);
    c.line((brx / 2.0).trunc(), y.trunc(), (brx / 2.0).trunc(), bry, &pen);
    y += tpad;
    c.draw_text(Rect::qt_int(x1, y, x1, plot_h), Align::CENTER, "SUPERCELL", plot_font, fg);
    c.draw_text(Rect::qt_int(4.0 * x1, y, x1, plot_h), Align::CENTER, "SGFNT HAIL", plot_font, fg);
    y += title_h;
    let text_start = y;

    let d = ctx.deviant();
    let sars = &ctx.view.sars;
    for (cat, tor) in [(sars.hail.get(d), false), (sars.supercell.get(d), true)] {
        let (place, place2, sigstr) = if tor {
            (1.0, (3.0, brx / 2.0 - x1 * 0.75 + 2.0), "TOR")
        } else {
            (4.0, (x1 * 3.0 + 7.0, x1 * 5.5 - 5.0), "SIG")
        };
        // SHARPpy only reaches the loose-match / probability lines when the
        // quality-match list is non-empty; with no quality matches the
        // column shows "No Quality Matches" alone.
        if let (false, Some(n), Some(prob)) = (cat.matches.is_empty(), cat.n_loose, cat.prob) {
            if n > 0 {
                let pct = prob * 100.0;
                let col = if pct >= 50.0 { Color::hex("#FF00FF") } else { fg };
                let mut yy = bry - bpad * 3.0;
                let s = format!("SARS: {}% {sigstr}", int2str(Some(pct.round())));
                c.draw_text(Rect::qt_int(x1 * place, yy, x1, match_h), Align::CENTER, &s, match_font, col);
                yy -= match_h + bpad;
                let s = format!("({} loose matches)", n);
                c.draw_text(Rect::qt_int(x1 * place, yy, x1, match_h), Align::CENTER, &s, match_font, col);
            }
        }
        if cat.matches.is_empty() {
            c.draw_text(
                Rect::qt_int(x1 * place, bry * 2.0 / 5.0, x1, match_h),
                Align::CENTER,
                "No Quality Matches",
                match_font,
                fg,
            );
        } else {
            let mut yy = text_start;
            for m in &cat.matches {
                let col = if tor {
                    if m.kind.starts_with("SIG") {
                        RED
                    } else if m.kind.starts_with("WEAK") {
                        LBLUE
                    } else {
                        LBROWN
                    }
                } else if m.hail_size.unwrap_or(0.0) >= 2.0 {
                    RED
                } else {
                    LBLUE
                };
                let kind = if tor {
                    m.kind.strip_suffix("TOR").unwrap_or(&m.kind).to_string()
                } else {
                    m.hail_size.map(|s| format!("{s:.2}")).unwrap_or_else(|| m.kind.clone())
                };
                c.draw_text(Rect::qt_int(place2.0, yy, x1, match_h), Align::LEFT, &m.label, match_font, col);
                c.draw_text(Rect::qt_int(place2.1, yy, x1, match_h), Align::LEFT, &kind, match_font, col);
                yy += match_h;
            }
        }
    }
}

// ------------------------------------------------------- STP box plots ----

struct BoxGeom {
    plot_font: Font,
    box_font: Font,
    plot_height: f32,
    box_height: f32,
    tpad: f32,
    bpad: f32,
    hgt: f32,
    brx: f32,
    bry: f32,
}

fn box_geom(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) -> BoxGeom {
    let fsize1 = (h * 0.0512).round() + 2.0;
    let fsize2 = (h * 0.0512).round();
    let plot_font = ctx.flavored(ctx.pt(fsize1), 17.33, h / 191.0);
    let box_font = ctx.flavored(ctx.pt(fsize2), 10.67, h / 191.0);
    let plot_height = c.font_metrics(plot_font).x_height;
    let box_height = c.font_metrics(box_font).x_height + 5.0;
    let tpad = plot_height + 15.0;
    let bpad = plot_height + 2.0;
    let hgt = h - bpad;
    BoxGeom { plot_font, box_font, plot_height, box_height, tpad, bpad, hgt, brx: w, bry: hgt - bpad }
}

#[allow(clippy::too_many_arguments)]
fn draw_box_plots(
    c: &mut dyn Canvas,
    ctx: &DrawCtx,
    g: &BoxGeom,
    title: &str,
    title_rect: Rect,
    vmax: f64,
    ytexts: &[&str],
    ytick_extra: f32,
    data: &[[f64; 5]],
    xlabels: &[&str],
    width: f32,
    centers: &[f32],
    draw_median: bool,
) {
    let th = ctx.theme;
    let to_pix = |v: f64| g.bry - ((v / vmax) as f32) * (g.bry - g.tpad);
    c.draw_text(title_rect, Align::CENTER, title, g.plot_font, th.fg);
    let ytf = (0.0512 * g.hgt).round() + ytick_extra;
    let yfont = ctx.pt(ytf);
    for t in ytexts {
        if t.trim().is_empty() {
            continue;
        }
        let v: f64 = t.parse().unwrap_or(0.0);
        let y = to_pix(v).trunc();
        c.line(0.0, y, g.brx, y, &Pen::styled(th.stp_line, 1.0, LineStyle::Dash));
        c.draw_text(Rect::qt_int(0.0, y - ytf / 2.0, 20.0, ytf), Align::CENTER, t, yfont, th.fg);
    }
    let xfont = ctx.pt((0.0512 * g.hgt).round());
    let pen = Pen::new(th.stp_box, 2.0);
    for (i, row) in data.iter().enumerate() {
        let cx = centers[i];
        let y: Vec<f32> = row.iter().map(|v| to_pix(*v)).collect();
        let (l, r) = ((cx - width / 2.0).trunc(), (cx + width / 2.0).trunc());
        let cxi = cx.trunc();
        c.line(cxi, y[0].trunc(), cxi, y[1].trunc(), &pen);
        c.line(l, y[3].trunc(), r, y[3].trunc(), &pen);
        c.line(l, y[1].trunc(), r, y[1].trunc(), &pen);
        c.line(l, y[1].trunc(), l, y[3].trunc(), &pen);
        c.line(r, y[1].trunc(), r, y[3].trunc(), &pen);
        if draw_median {
            c.line(l, y[2].trunc(), r, y[2].trunc(), &pen);
        }
        c.line(cxi, y[3].trunc(), cxi, y[4].trunc(), &pen);
        let lr = Rect::new(cx - width / 2.0, g.bry + (g.bpad / 2.0).round(), width, g.bpad);
        c.draw_text(lr, Align::CENTER, xlabels[i], xfont, th.fg);
    }
}

fn stp_prob_tables(ctx: &DrawCtx) -> [(String, Option<f64>, usize); 6] {
    let v = ctx.view;
    let d = ctx.deviant();
    let si = &v.stp_inset;
    let neg = v.latitude < 0.0;
    let flip = |x: Option<f64>| if neg { x.map(|y| -y) } else { x };
    let pick = |x: Option<f64>, f: &dyn Fn(f64) -> (f64, usize)| match x {
        Some(x) if x.is_finite() => {
            let (p, a) = f(x);
            if p.is_nan() { (None, 0) } else { (Some(p), a) }
        }
        _ => (None, 0),
    };
    let cape = pick(si.mlcape, &|x| {
        if x == 0.0 {
            (0.0, 0)
        } else if x < 250.0 {
            (0.12, 1)
        } else if x < 500.0 {
            (0.14, 2)
        } else if x < 1000.0 {
            (0.16, 2)
        } else if x < 1500.0 {
            (0.15, 2)
        } else if x < 2000.0 {
            (0.13, 2)
        } else if x < 2500.0 {
            (0.14, 2)
        } else if x < 3000.0 {
            (0.18, 3)
        } else if x < 4000.0 {
            (0.20, 3)
        } else {
            (0.16, 3)
        }
    });
    let lcl = pick(si.mllcl, &|x| {
        if x < 1000.0 {
            (0.19, 3)
        } else if x < 1250.0 {
            (0.15, 2)
        } else if x < 1500.0 {
            (0.10, 1)
        } else if x < 2000.0 {
            (0.06, 0)
        } else if x < 2500.0 {
            (0.02, 0)
        } else {
            (0.0, 0)
        }
    });
    let esrh = pick(flip(*si.esrh.get(d)), &|x| {
        if x < 100.0 {
            (0.06, 0)
        } else if x < 200.0 {
            (0.08, 1)
        } else if x < 300.0 {
            (0.14, 2)
        } else if x < 400.0 {
            (0.20, 3)
        } else if x < 500.0 {
            (0.27, 3)
        } else if x < 600.0 {
            (0.38, 4)
        } else if x < 700.0 {
            (0.37, 4)
        } else {
            (0.42, 4)
        }
    });
    let ebwd = pick(si.ebwd, &|x| {
        if x == 0.0 {
            (0.0, 0)
        } else if x < 0.01 {
            (f64::NAN, 0)
        } else if x < 20.0 {
            (0.03, 0)
        } else if x < 30.0 {
            (0.05, 0)
        } else if x < 40.0 {
            (0.06, 0)
        } else if x < 50.0 {
            (0.12, 1)
        } else if x < 60.0 {
            (0.19, 3)
        } else if x < 70.0 {
            (0.27, 3)
        } else if x < 80.0 {
            (0.36, 4)
        } else {
            (0.26, 3)
        }
    });
    let stpc = pick(flip(*v.severe.stp_cin.get(d)), &|x| {
        if x < 0.1 {
            (0.06, 0)
        } else if x < 0.5 {
            (0.08, 1)
        } else if x < 1.0 {
            (0.12, 1)
        } else if x < 2.0 {
            (0.17, 2)
        } else if x < 4.0 {
            (0.25, 3)
        } else if x < 6.0 {
            (0.32, 4)
        } else if x < 8.0 {
            (0.34, 4)
        } else if x < 10.0 {
            (0.55, 5)
        } else {
            (0.58, 5)
        }
    });
    let stpf = pick(flip(*v.severe.stp_fixed.get(d)), &|x| {
        if x < 0.1 {
            (0.05, 0)
        } else if x < 0.5 {
            (0.06, 0)
        } else if x < 1.0 {
            (0.11, 1)
        } else if x < 2.0 {
            (0.17, 2)
        } else if x < 5.0 {
            (0.25, 3)
        } else if x < 7.0 {
            (0.39, 4)
        } else if x < 9.0 {
            (0.55, 5)
        } else {
            (0.59, 5)
        }
    });
    [
        ("based on CAPE: ".to_string(), cape.0, cape.1),
        ("based on LCL:".to_string(), lcl.0, lcl.1),
        ("based on ESRH:".to_string(), esrh.0, esrh.1),
        ("based on EBWD:".to_string(), ebwd.0, ebwd.1),
        ("based on STPC:".to_string(), stpc.0, stpc.1),
        ("based on STP_fixed:".to_string(), stpf.0, stpf.1),
    ]
}

fn draw_stp_stats(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let g = box_geom(c, ctx, w, h);
    let data = [
        [1.2, 2.6, 5.3, 8.3, 11.0],
        [0.2, 1.0, 2.4, 4.5, 8.4],
        [0.0, 0.6, 1.7, 3.7, 5.6],
        [0.0, 0.3, 1.2, 2.6, 4.5],
        [0.0, 0.1, 0.8, 2.0, 3.7],
        [0.0, 0.0, 0.2, 0.7, 1.7],
    ];
    let spacing = g.brx / 7.0;
    let centers: Vec<f32> = (1..=6).map(|i| spacing * i as f32).collect();
    let ytexts = ["11", "10", "9", "8", "7", "6", "5", "4", "3", "2", "1", "0"];
    draw_box_plots(
        c,
        ctx,
        &g,
        "Effective Layer STP (with CIN)",
        Rect::new(0.0, 5.0, g.brx, g.plot_height),
        11.0,
        &ytexts,
        1.0,
        &data,
        &["EF4+", "EF3", "EF2", "EF1", "EF0", "NONTOR"],
        g.brx / 14.0,
        &centers,
        true,
    );
    let to_pix = |v: f64| g.bry - ((v / 11.0) as f32) * (g.bry - g.tpad);

    // Current STPC line.
    let d = ctx.deviant();
    let mut stpc = *ctx.view.severe.stp_cin.get(d);
    if ctx.view.latitude < 0.0 {
        stpc = stpc.map(|x| -x);
    }
    if let Some(s) = stpc.filter(|s| s.is_finite()) {
        let s = s.clamp(0.0, 11.0);
        let a = th.alert;
        let col = [(8.0, 5), (4.0, 4), (2.0, 3), (1.0, 2), (0.5, 1)]
            .iter()
            .find(|(t, _)| s >= *t)
            .map(|(_, i)| a[*i])
            .unwrap_or(a[0]);
        let y = to_pix(s);
        c.line(0.0, y.trunc(), w, y.trunc(), &Pen::new(col, 1.5));
    }

    // Probability box (drawn over the line, without antialiasing).
    c.set_antialias(false);
    let left_x = g.brx / 2.0;
    let right_x = g.brx - 5.0;
    let top_y = to_pix(11.0);
    let bot_y = top_y + 8.0 * (g.box_height + 1.0);
    c.fill_rect(Rect::new(left_x, top_y, right_x - left_x, bot_y - top_y), th.bg);
    let fpen = Pen::new(th.fg, 2.0);
    c.frame(left_x.trunc(), top_y.trunc(), right_x.trunc(), bot_y.trunc(), &fpen);
    let bw = right_x - left_x - 3.0;
    let mut y1 = top_y + 2.0;
    let x1 = left_x + 3.0;
    let x2 = x1 + 0.75 * bw;
    for s in ["Prob EF2+ torn with supercell", "Sample CLIMO = .15 sigtor"] {
        c.draw_text(Rect::new(x1, y1, bw, g.box_height), Align::LEFT, s, g.box_font, th.fg);
        y1 += g.box_height + 1.0;
    }
    c.line(left_x.trunc(), (y1 - 1.0).trunc(), right_x.trunc(), (y1 - 1.0).trunc(), &fpen);
    for (label, prob, ai) in stp_prob_tables(ctx) {
        let col = th.alert[ai];
        c.draw_text(Rect::new(x1, y1, bw, g.box_height), Align::LEFT, &label, g.box_font, col);
        c.draw_text(Rect::new(x2, y1, bw, g.box_height), Align::LEFT, &float2str(prob, 2), g.box_font, col);
        y1 += g.box_height;
    }
    c.set_antialias(true);
}

fn draw_ship(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let g = box_geom(c, ctx, w, h);
    let data = [[0.2, 0.3, 0.2, 0.9, 1.2], [1.1, 1.4, 0.8, 2.8, 4.0]];
    let centers = [g.brx / 3.0, 2.0 * g.brx / 3.0];
    draw_box_plots(
        c,
        ctx,
        &g,
        "Significant Hail Param (SHIP)",
        Rect::new(1.5, 6.0, g.brx, g.plot_height),
        5.0,
        &["5", "4", "3", "2", "1", "0", " "],
        0.0,
        &data,
        &["<= 1.5\"", ">= 2.5\""],
        g.brx / 3.7,
        &centers,
        false,
    );
    if let Some(s) = ctx.view.severe.ship.filter(|s| s.is_finite()) {
        let s = s.clamp(0.0, 5.0);
        let a = th.alert;
        let col = [(5.0, 5), (2.0, 4), (1.0, 3), (0.5, 2)].iter().find(|(t, _)| s >= *t).map(|(_, i)| a[*i]).unwrap_or(a[0]);
        let y = g.bry - ((s / 5.0) as f32) * (g.bry - g.tpad);
        c.line(0.0, y.trunc(), w, y.trunc(), &Pen::new(col, 1.5));
    }
}

// ------------------------------------------- conditional probabilities ----

fn prob_axes(c: &mut dyn Canvas, ctx: &DrawCtx, brx: f32, bry: f32, tpad: f32, font: Font, fsize: f32) -> impl Fn(f64) -> f32 + use<> {
    let th = ctx.theme;
    let to_pix = move |p: f64| bry - ((p / 70.0) as f32) * (bry - tpad);
    for t in ["70", "60", "50", "40", "30", "20", "10", "0"] {
        let y = to_pix(t.parse().unwrap()).trunc();
        c.line(0.0, y, brx, y, &Pen::styled(Color::hex("#0080FF"), 1.0, LineStyle::Dash));
        c.draw_text(Rect::qt_int(0.0, y - fsize / 2.0, 20.0, fsize), Align::CENTER, t, font, th.fg);
    }
    to_pix
}

fn draw_cond_stp(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let fsize = (0.0512 * h).round();
    let plot_font = ctx.pt(fsize + 1.0);
    let plot_height = c.font_metrics(plot_font).x_height + 5.0;
    let tpad = 5.0 + 2.0 * plot_height;
    let bpad = plot_height + 5.0;
    let brx = w;
    let bry = h - bpad;
    c.draw_text(Rect::new(1.5, 2.0, brx, plot_height), Align::CENTER, "Conditional Tornado Probs based on STPC", plot_font, th.fg);
    let leg_font = ctx.pt(fsize - 1.0);
    let curves: [(&str, Color, [f64; 11]); 4] = [
        ("EF1+", Color::hex("#006600"), [15.3, 28.0, 36.1, 47.8, 52.3, 54.4, 60.3, 61.6, 61.2, 73.2, 73.4]),
        ("EF2+", Color::hex("#FFCC33"), [2.7, 6.6, 6.5, 13.8, 14.9, 17.5, 21.8, 31.5, 33.3, 46.2, 53.1]),
        ("EF3+", Color::hex("#FF0000"), [0.0, 1.2, 1.2, 2.7, 2.7, 4.1, 7.3, 10.0, 13.2, 25.8, 39.1]),
        ("EF4+", Color::hex("#FF00FF"), [0.0, 0.0, 0.0, 0.0, 0.2, 0.1, 1.5, 2.3, 6.2, 9.7, 17.2]),
    ];
    for (i, (name, col, _)) in curves.iter().enumerate() {
        let x = (2.0 + 2.0 * i as f32) * brx / 12.0;
        c.draw_text(Rect::new(x, 2.0 + plot_height, 10.0, plot_height), Align::CENTER, name, leg_font, *col);
    }
    let to_pix = prob_axes(c, ctx, brx, bry, tpad, ctx.pt(fsize), fsize);
    let xfont = ctx.pt(fsize - 2.0);
    let xl = ["0", ".01-.5", ".5-1", "1-2", "2-3", "3-4", "4-6", "6-8", "8-10", "10-12", ">12"];
    for (k, t) in xl.iter().enumerate() {
        let x = (k + 1) as f32 * brx / 12.0;
        c.draw_text(Rect::new(x - brx / 24.0, bry + bpad / 2.0, brx / 12.0, 4.0), Align::CENTER, t, xfont, th.fg);
    }
    for (_, col, vals) in &curves {
        let pts: Vec<(f32, f32)> = vals.iter().enumerate().map(|(k, v)| ((k + 1) as f32 * brx / 12.0, to_pix(*v))).collect();
        c.polyline(&pts, &Pen::new(*col, 3.0));
    }
    if let Some(s) = *ctx.view.severe.stp_cin.get(ctx.deviant()) {
        let bin = if s == 0.0 {
            0
        } else if (0.01..0.5).contains(&s) {
            1
        } else if (0.5..1.0).contains(&s) {
            2
        } else if (1.0..2.0).contains(&s) {
            3
        } else if (2.0..3.0).contains(&s) {
            4
        } else if (3.0..4.0).contains(&s) {
            5
        } else if (4.0..6.0).contains(&s) {
            6
        } else if (6.0..8.0).contains(&s) {
            7
        } else if (8.0..10.0).contains(&s) {
            8
        } else if (10.0..12.0).contains(&s) {
            9
        } else {
            10
        };
        let x = ((bin + 1) as f32 * brx / 12.0).trunc();
        c.line(x, to_pix(0.0).trunc(), x, to_pix(70.0).trunc(), &Pen::styled(th.fg, 1.5, LineStyle::Dot));
    }
}

fn draw_vrot(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let fr = 0.0512 * h;
    let plot_font = ctx.pt((fr + 1.0).round());
    let label_font = ctx.pt(fr.round());
    let plot_height = c.font_metrics(plot_font).x_height + 5.0;
    let label_height = c.font_metrics(label_font).x_height + 5.0;
    let tpad = 5.0 + plot_height + label_height;
    let bpad = label_height + 5.0;
    let brx = w;
    let bry = h - bpad;
    let v2x = |v: f64| 5.0 + brx * (v / 110.0) as f32;
    c.draw_text(Rect::new(1.5, 2.0, brx, plot_height), Align::CENTER, "Conditional EF-scale Probs based on Vrot", plot_font, th.fg);
    let leg = ctx.pt(fr);
    let curves: [(&str, Color, f64, [f64; 11], bool); 3] = [
        ("EF0-EF1", Color::hex("#006600"), 25.0, [100.0, 98.6, 95.3, 91.0, 80.2, 61.9, 42.1, 29.1, 16.3, 5.6, 0.0], true),
        ("EF2-EF3", Color::hex("#FFCC33"), 50.0, [0.0, 1.0, 4.7, 9.0, 19.3, 36.5, 51.1, 62.8, 65.1, 50.0, 25.0], true),
        ("EF4-EF5", Color::hex("#FF00FF"), 75.0, [0.0, 0.0, 0.0, 0.0, 0.5, 1.6, 6.8, 8.1, 18.6, 44.4, 75.0], false),
    ];
    for (name, col, at, _, _) in &curves {
        c.draw_text(Rect::new(v2x(*at), 4.0 + plot_height, 10.0, plot_height), Align::CENTER, name, leg, *col);
    }
    let to_pix = prob_axes(c, ctx, brx, bry, tpad, label_font, fr.round());
    let xfont = ctx.pt(0.0512 * bry);
    for t in (10..=100).step_by(10) {
        let x = v2x(t as f64);
        c.draw_text(Rect::new(x - brx / 24.0, bry + bpad / 2.0, brx / 12.0, 4.0), Align::CENTER, &t.to_string(), xfont, th.fg);
    }
    for (_, col, _, vals, clamp) in &curves {
        for k in 0..vals.len() - 1 {
            let (a, b) = (vals[k], vals[k + 1]);
            let (a2, b2) = if *clamp { (a.min(70.0), b.min(70.0)) } else { (a, b) };
            let style = if *clamp && b > 70.0 { LineStyle::Dot } else { LineStyle::Solid };
            let xa = v2x(5.0 + 10.0 * k as f64);
            let xb = v2x(15.0 + 10.0 * k as f64);
            c.line(xa, to_pix(a2), xb, to_pix(b2), &Pen::styled(*col, 2.5, style));
        }
    }
    let x = v2x(ctx.view.vrot).trunc();
    c.line(x, to_pix(0.0).trunc(), x, to_pix(70.0).trunc(), &Pen::styled(th.fg, 1.5, LineStyle::Dot));
}

// -------------------------------------------------------- winter / fire ----

fn draw_winter(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let fg = th.fg;
    let fsz = (0.0512 * h).round();
    let font = ctx.pt(fsz);
    let lh = c.font_metrics(font).x_height + 3.0;
    let brx = w;
    let pen = Pen::new(fg, 1.0);
    c.draw_text(Rect::qt_int(0.0, 3.0, w, lh), Align::CENTER, "*** DENDRITIC GROWTH ZONE (-12 TO -17 C) ***", font, th.winter_dgz);
    let d1 = (6.0 * lh + 23.0).trunc();
    c.line(0.0, d1, w, d1, &pen);
    c.line((0.48 * brx).trunc(), d1, (0.48 * brx).trunc(), (4.0 * lh + 13.0).trunc(), &pen);
    let d2 = (7.0 * lh + 31.0).trunc();
    c.line(0.0, d2, w, d2, &pen);
    let d3 = (11.0 * lh + 54.0).trunc();
    c.line(0.0, d3, w, d3, &pen);
    c.line((0.48 * brx).trunc(), d3, (0.48 * brx).trunc(), (7.0 * lh + 34.0).trunc(), &pen);
    c.draw_text(Rect::qt_int(0.0, 11.0 * lh + 58.0, w, lh), Align::CENTER, "*** BEST GUESS PRECIP TYPE ***", font, fg);
    let Some(wv) = &ctx.view.winter else { return };
    let big = ctx.pt(fsz + 5.0);
    let big_h = c.font_metrics(big).x_height + 3.0;
    c.draw_text(Rect::qt_int(0.0, 12.0 * lh + 61.0, w, big_h), Align::CENTER, &wv.precip_type, big, fg);
    let small = ctx.pt(fsz - 1.0);
    let small_h = c.font_metrics(small).x_height + 3.0;
    let tmpf = wv.sfc_tmpf.map(|t| format!("{t:.2}")).unwrap_or_else(|| "--".into());
    c.draw_text(Rect::qt_int(0.0, 13.0 * lh + 69.0, w, small_h), Align::CENTER, &format!("Based on SFC Temperature of {tmpf} F"), small, fg);
    let oprh_col = match wv.oprh {
        Some(o) if o < -0.1 && wv.dgz_meanomeg.is_some() => RED,
        _ => fg,
    };
    let oprh = if wv.dgz_meanomeg.is_none() { "N/A".to_string() } else { float2str(wv.oprh, 2) };
    c.draw_text(Rect::qt_int(0.0, 2.0 * lh + 3.0, w, lh), Align::CENTER, &format!("OPRH (Omega*PW*RH): {oprh}"), font, oprh_col);
    let init = match &wv.init_phase {
        Some((p, ft, t, st)) if *p > 100.0 => format!(
            "Inital Phase: {st} from: {} mb ({} ft msl; {} C)",
            int2str(Some(*p)),
            int2str(Some(*ft)),
            float2str(Some(*t), 1)
        ),
        _ => "Initial Phase:  No Precipitation layers found.".to_string(),
    };
    let left = |c: &mut dyn Canvas, x: f32, y: f32, s: &str| {
        c.draw_text(Rect::qt_int(x, y, brx / 10.0, lh), Align::LEFT, s, font, fg);
    };
    left(c, 5.0, 6.0 * lh + 26.0, &init);
    let ly = 3.0 * lh + 8.0;
    let depth = match (wv.dgz_zbot_ft, wv.dgz_ztop_ft) {
        (Some(b), Some(t)) => format!("Layer Depth: {} ft ({}-{} ft msl)", int2str(Some(t - b)), int2str(Some(b)), int2str(Some(t))),
        _ => "Layer Depth: -- ft".to_string(),
    };
    left(c, 5.0, ly, &depth);
    left(c, 5.0, ly + lh + 5.0, &format!("Mean Layer RH: {} %", float2str(wv.dgz_meanrh, 0)));
    left(c, 5.0, ly + 2.0 * (lh + 5.0), &format!("Mean Layer PW: {} in", float2str(wv.dgz_pw, 1)));
    left(c, brx / 2.0 + 2.0, ly + lh + 5.0, &format!("Mean Layer MixRat: {} g/kg", float2str(wv.dgz_meanq, 1)));
    let omeg = match wv.dgz_meanomeg {
        Some(o) => format!("{} ub/s", float2str(Some(o), 1)),
        None => "N/A".into(),
    };
    left(c, brx / 2.0 + 2.0, ly + 2.0 * (lh + 5.0), &format!("Mean Layer Omega: {omeg}"));
    let ey = 7.0 * lh + 34.0;
    let col_txt = |pos: f64, neg: f64, top: f64, bot: f64, name: &str| -> [String; 4] {
        if pos > 0.0 && neg < 0.0 {
            [
                name.to_string(),
                format!("P/N: {} / {} J/kg", crate::util::py_float_repr(pos.round()), crate::util::py_float_repr(neg.round())),
                format!("Melt Lyr: {}-{} mb", top as i64, bot as i64),
                format!("Frz Lyr: {}-{} mb", bot as i64, ctx.view.profile.sfc_pres() as i64),
            ]
        } else {
            [name.to_string(), String::new(), "Warm/Cold layers not found.".into(), String::new()]
        }
    };
    let lcol = col_txt(wv.tpos, wv.tneg, wv.ttop, wv.tbot, "TEMPERATURE PROFILE");
    let rcol = col_txt(wv.wpos, wv.wneg, wv.wtop, wv.wbot, "WETBULB PROFILE");
    for (i, s) in lcol.iter().enumerate() {
        left(c, 5.0, ey + i as f32 * (lh + 5.0), s);
    }
    for (i, s) in rcol.iter().enumerate() {
        left(c, brx / 2.0 + 2.0, ey + i as f32 * (lh + 5.0), s);
    }
}

fn draw_fire(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let fg = th.fg;
    let brx = w;
    let font = ctx.pt((0.0512 * h).round());
    let fos_font = ctx.pt((0.0512 * h).round() + 2.0);
    let lh = c.font_metrics(font).x_height + 3.0;
    c.draw_text(Rect::qt_int(0.0, 3.0, w, lh), Align::CENTER, "Fire Weather Parameters", fos_font, fg);
    let y = 2.0 * lh + 7.0;
    c.draw_text(Rect::qt_int(brx / 10.0, y, 0.4 * brx, lh), Align::LEFT, "Moisture", font, Color::hex("#00CC33"));
    c.draw_text(Rect::qt_int(brx / 2.0, y, 0.4 * brx, lh), Align::RIGHT, "Low-Level Wind", font, Color::hex("#0066CC"));
    let yl = (3.0 * lh + 12.0).trunc();
    c.line(0.0, yl, brx, yl, &Pen::new(fg, 1.0));
    let sep = Color::hex("#FF6633");
    c.draw_text(Rect::qt_int(0.0, 9.0 * lh + 54.0, brx, lh), Align::CENTER, "Derived Indices", font, sep);
    let yd = (10.0 * lh + 61.0).trunc();
    c.line(0.0, yd, brx, yd, &Pen::new(sep, 1.0));
    let Some(f) = &ctx.view.fire else { return };
    let row = |i: usize| 3.0 * lh + 18.0 + i as f32 * (lh + 7.0);
    let ds = |x: Option<(f64, f64)>| match x {
        Some((d, s)) => format!("{}/{}", int2str(Some(d)), int2str(Some(s))),
        None => "--/--".into(),
    };
    let maxs = f.pbl_max_wind.map(|x| x.1);
    let max_col = match maxs {
        Some(s) if s > 50.0 => MAGENTA,
        Some(s) if s > 40.0 => RED,
        Some(s) if s > 30.0 => YELLOW,
        Some(s) if s > 20.0 => fg,
        Some(s) if s > 10.0 => LBROWN,
        _ => DBROWN,
    };
    let wind_rows = [
        (format!("SFC = {}", ds(f.sfc_wind)), fg),
        (format!("0-1 km mean = {}", ds(f.mean_wind_01km)), fg),
        (format!("BL mean = {}", ds(f.mean_wind_pbl)), fg),
        (format!("BL max = {}", ds(f.pbl_max_wind)), max_col),
    ];
    for (i, (s, col)) in wind_rows.iter().enumerate() {
        c.draw_text(Rect::qt_int(brx / 2.0, row(i), 0.4 * brx, lh), Align::RIGHT, s, font, *col);
    }
    let rh_col = match f.sfc_rh {
        Some(r) if r <= 10.0 => MAGENTA,
        Some(r) if r <= 15.0 => RED,
        Some(r) if r <= 20.0 => YELLOW,
        Some(r) if r <= 30.0 => fg,
        Some(r) if r <= 35.0 => LBROWN,
        _ => DBROWN,
    };
    let pw_col = match (f.pwat, f.bplus_fire, f.sfc_rh) {
        (Some(p), Some(b), Some(r)) if p < 0.5 && b > 50.0 && r < 35.0 => RED,
        _ => fg,
    };
    let moist = [
        (format!("SFC RH = {}%", int2str(f.sfc_rh)), rh_col),
        (format!("0-1 km RH = {}%", int2str(f.rh01km)), fg),
        (format!("BL mean RH = {}%", int2str(f.pbl_rh)), fg),
        (format!("PW = {} in", float2str(f.pwat, 2)), pw_col),
    ];
    for (i, (s, col)) in moist.iter().enumerate() {
        c.draw_text(Rect::qt_int(brx / 10.0, row(i), 0.4 * brx, lh), Align::LEFT, s, font, *col);
    }
    if let Some(p) = f.pbl_h {
        let s = format!("PBL Height = {}ft / {}m", float2str(Some(m2ft(p)), 0), float2str(Some(p), 0));
        c.draw_text(Rect::qt_int(0.0, 7.0 * lh + 46.0, brx, lh), Align::CENTER, &s, font, fg);
    }
    let fos_col = match f.fosberg {
        Some(v) if v >= 70.0 => MAGENTA,
        Some(v) if v >= 60.0 => RED,
        Some(v) if v >= 50.0 => YELLOW,
        Some(v) if v >= 40.0 => fg,
        Some(v) if v >= 30.0 => LBROWN,
        _ => DBROWN,
    };
    let fos = f.fosberg.map(|v| int2str(Some(v))).unwrap_or_else(|| "M".into());
    c.draw_text(Rect::qt_int(0.0, 10.0 * lh + 68.0, brx, lh), Align::CENTER, &format!("Fosberg FWI = {fos}"), fos_font, fos_col);
    let hcol = match f.haines {
        Some(3) => GREEN,
        Some(4) => YELLOW,
        Some(5) => ORANGE,
        Some(6) => RED,
        _ => DGREEN,
    };
    use sharprs_core::HainesElevation;
    let class = match f.haines_elevation {
        Some(HainesElevation::Low) => "L",
        Some(HainesElevation::Mid) => "M",
        Some(HainesElevation::High) => "H",
        None => "--",
    };
    let hs = f.haines.map(|v| v.to_string()).unwrap_or_else(|| "--".into());
    c.draw_text(Rect::qt_int(0.0, 11.0 * lh + 75.0, brx, lh), Align::CENTER, &format!("Haines Index ({class}) = {hs}"), fos_font, hcol);
}
