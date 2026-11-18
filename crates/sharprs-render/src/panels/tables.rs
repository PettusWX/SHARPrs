//! Bottom-left text tables: parcel/thermodynamic indices and kinematics.
//!
//! Both tables size their font from the widget height (6% of it, in
//! pixels) and lay rows out at `x-height + 3% of height` spacing, as
//! SHARPpy does. Content is drawn with a one-pixel offset inside the
//! widget's 1 px frame.

use crate::canvas::{Align, Canvas, CanvasExt, Color, Font, Pen, Rect};
use crate::options::Flavor;
use crate::panels::barbs::draw_barb;
use crate::theme::{PwUnits, TempUnits, WindUnits};
use crate::util::{float2str, ftoc, int2str, kts2ms};
use crate::{DrawCtx, UV};

struct TableMetrics {
    font: Font,
    tpad: f32,
    lh: f32,
    /// Extra space under the header row (Pivotal's build adds the font descent).
    header_extra: f32,
}

fn table_metrics(c: &mut dyn Canvas, ctx: &DrawCtx, h: f32) -> TableMetrics {
    let font = Font::px((0.06 * h).floor());
    let tpad = (0.03 * h).floor();
    let fm = c.font_metrics(font);
    let header_extra = if ctx.opts.flavor == Flavor::Pivotal { fm.descent } else { 0.0 };
    TableMetrics { font, tpad, lh: fm.x_height + tpad, header_extra }
}

fn text(c: &mut dyn Canvas, r: Rect, align: Align, s: &str, f: Font, col: Color) {
    c.draw_text(r, align, s, f, col);
}

/// Parcel table + thermodynamic indices + lapse rates + severe composites.
pub fn draw_thermo(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let v = ctx.view;
    let m = table_metrics(c, ctx, h);
    let lh = m.lh;
    c.save();
    c.translate(1.0, 1.0);
    c.set_antialias(false);
    let brx = w;
    let bry = h;
    let fg = th.fg;
    let pen = Pen::new(fg, 1.0);

    // Header.
    let x1 = brx / 8.0;
    for (i, t) in ["PCL", "CAPE", "CINH", "LCL", "LI", "LFC", "EL"].iter().enumerate() {
        text(c, Rect::qt_int(x1 * i as f32, 1.0, x1 * 2.0, lh), Align::CENTER, t, m.font, fg);
    }
    let vspace = lh + m.header_extra;
    c.line(0.0, vspace, brx, vspace, &pen);
    let ylast = vspace;

    // Parcel rows.
    let y0 = ylast + m.tpad;
    let sel = v.selected_parcel;
    let mut bounds = Vec::new();
    let mut y1 = y0;
    for (i, p) in v.parcels.iter().take(4).enumerate() {
        let col = if i == sel { th.pcl_sel } else { fg };
        let cape = int2str(p.bplus);
        let cin = int2str(p.bminus);
        let lcl = int2str(p.lclhght);
        let cape_i: Option<i64> = cape.parse().ok();
        let cin_i: Option<i64> = cin.parse().ok();
        let lcl_i: Option<i64> = lcl.parse().ok();
        let cape_col = match cape_i {
            Some(c) if i == sel && c >= 4000 => th.alert[5],
            Some(c) if i == sel && c >= 3000 => th.alert[4],
            Some(c) if i == sel && c >= 2000 => th.alert[3],
            _ => fg,
        };
        let cin_col = match (cape_i, cin_i) {
            (Some(ca), Some(ci)) if ca > 0 && i == sel && ci >= -50 => th.pcl_cin_lo,
            (Some(ca), Some(ci)) if ca > 0 && i == sel && ci >= -100 => th.pcl_cin_md,
            (Some(ca), Some(_)) if ca > 0 && i == sel => th.pcl_cin_hi,
            _ => fg,
        };
        let lcl_col = match lcl_i {
            Some(l) if i == sel && p.name == "ML" && l < 1000 => th.pcl_cin_lo,
            Some(l) if i == sel && p.name == "ML" && l < 1500 => th.pcl_cin_md,
            Some(l) if i == sel && p.name == "ML" && l < 2000 => th.pcl_cin_hi,
            _ => fg,
        };
        let cells = [
            (p.name.clone(), col),
            (cape, cape_col),
            (cin, cin_col),
            (lcl, lcl_col),
            (int2str(p.li5), fg),
            (int2str(p.lfchght), fg),
            (int2str(p.elhght), fg),
        ];
        bounds.push((y1, y1 + lh));
        for (k, (s, cc)) in cells.iter().enumerate() {
            text(c, Rect::qt_int(x1 * k as f32, y1, x1 * 2.0, lh), Align::CENTER, s, m.font, *cc);
        }
        y1 += lh;
    }
    let ylast = y0 + lh * 4.0;
    c.line(0.0, (ylast + 2.0).trunc(), brx, (ylast + 2.0).trunc(), &pen);
    if let Some(&(b0, b1)) = bounds.get(sel) {
        // SHARPpy uses a 0.5 px pen here; Qt's raster engine paints it as a
        // 1 px line at half coverage.
        let bp = Pen::new(Color::hex("#996633").with_alpha(128), 1.0);
        let xr = (x1 * 6.0 + x1 * 2.0 - 1.0).trunc();
        c.line(2.0, (b0 - 1.0).trunc(), xr, (b0 - 1.0).trunc(), &bp);
        c.line(2.0, (b0 - 1.0).trunc(), 2.0, b1.trunc(), &bp);
        c.line(2.0, b1.trunc(), xr, b1.trunc(), &bp);
        c.line(xr, (b0 - 1.0).trunc(), xr, b1.trunc(), &bp);
    }

    // Indices (three columns).
    let ind = &v.indices;
    let x1 = brx / 10.0;
    let rpad = 5.0;
    let flag = ind.pwv_flag.clamp(-3, 3);
    let pw_col = match flag {
        -3 => th.pwat_m[0],
        -2 => th.pwat_m[1],
        -1 => th.pwat_m[2],
        1 => th.pwat_p[0],
        2 => th.pwat_p[1],
        3 => th.pwat_p[2],
        _ => fg,
    };
    let sigma = ["(<3\u{03C3})", "(2-3\u{03C3})", "(1-2\u{03C3})", "", "(1-2\u{03C3})", "(2-3\u{03C3})", "(>3\u{03C3})"];
    let dist = sigma[(flag + 3) as usize];
    let (pw_disp, pw_unit) = match ctx.opts.pw_units {
        PwUnits::Centimeters => (float2str(ind.pwat.map(|p| p * 2.54), 1), "cm"),
        PwUnits::Inches => (float2str(ind.pwat, 2), "in"),
    };
    let (tu, tconv): (&str, fn(f64) -> f64) = match ctx.opts.temp_units {
        TempUnits::Fahrenheit => ("F", |x| x),
        TempUnits::Celsius => ("C", ftoc),
    };
    let col1 = [
        (format!("PW = {pw_disp}{pw_unit} {dist}"), pw_col),
        (format!("MeanW = {}g/kg", float2str(ind.mean_mixr, 1)), fg),
        (format!("LowRH = {}%", int2str(ind.low_rh)), fg),
        (format!("MidRH = {}%", int2str(ind.mid_rh)), fg),
        (format!("DCAPE = {}", int2str(ind.dcape)), fg),
        (format!("DownT = {}{tu}", int2str(ind.drush_f.map(tconv))), fg),
    ];
    let col2 = [
        format!("K = {}", int2str(ind.k_idx)),
        format!("TT = {}", int2str(ind.totals_totals)),
        format!("ConvT = {}{tu}", int2str(ind.conv_t_f.map(tconv))),
        format!("maxT = {}{tu}", int2str(ind.max_t_f.map(tconv))),
        if ind.esp_gated { "ESP = 0".to_string() } else { format!("ESP = {}", float2str(ind.esp, 1)) },
        format!("MMP = {}", float2str(ind.mmp, 2)),
    ];
    let col3 = [
        format!("WNDG = {}", float2str(ind.wndg, 1)),
        format!("TEI = {}", int2str(ind.tei)),
        format!("3CAPE = {}", int2str(ind.cape3km_ml)),
        format!("MBURST = {}", int2str(ind.mburst)),
        String::new(),
        format!("SigSvr = {} m3/s3", int2str(ind.sig_severe)),
    ];
    let yi = ylast + m.tpad;
    for (i, (s, cc)) in col1.iter().enumerate() {
        text(c, Rect::qt_int(rpad, yi + lh * i as f32, x1 * 4.0, lh), Align::LEFT, s, m.font, *cc);
    }
    for (i, s) in col2.iter().enumerate() {
        text(c, Rect::qt_int(x1 * 3.5, yi + lh * i as f32, x1 * 4.0, lh), Align::LEFT, s, m.font, fg);
    }
    for (i, s) in col3.iter().enumerate() {
        text(c, Rect::qt_int(x1 * 6.0, yi + lh * i as f32, x1 * 4.0, lh), Align::LEFT, s, m.font, fg);
    }
    let ylast = yi + lh * 6.0;
    c.line(0.0, (ylast + 2.0).trunc(), brx, (ylast + 2.0).trunc(), &pen);
    c.line((x1 * 7.0 - 5.0).trunc(), (ylast + 2.0).trunc(), (x1 * 7.0 - 5.0).trunc(), bry, &pen);

    // Lapse rates.
    let yl = ylast + m.tpad;
    let lr = [
        ("Sfc-3km AGL LR = ", ind.lapserate_3km),
        ("3-6km AGL LR = ", ind.lapserate_3_6km),
        ("850-500mb LR = ", ind.lapserate_850_500),
        ("700-500mb LR = ", ind.lapserate_700_500),
    ];
    for (i, (l, val)) in lr.iter().enumerate() {
        let s = format!("{l}{} C/km", float2str(*val, 1));
        text(c, Rect::qt_int(rpad, yl + lh * i as f32, x1 * 8.0, lh), Align::LEFT, &s, m.font, fg);
    }

    // Severe composites (bold, alert-colored).
    let sev = &v.severe;
    let d = ctx.deviant();
    let neg = v.latitude < 0.0;
    let flip = |x: Option<f64>| if neg { x.map(|v| -v) } else { x };
    let scp = flip(*sev.scp.get(d));
    let stpc = flip(*sev.stp_cin.get(d));
    let stpf = flip(*sev.stp_fixed.get(d));
    let a = th.alert;
    let scp_col = |x: f64| {
        if x >= 19.95 {
            a[5]
        } else if x >= 11.95 {
            a[4]
        } else if x >= 1.95 {
            a[2]
        } else if x >= 0.45 {
            a[1]
        } else if x >= -0.45 {
            a[0]
        } else {
            th.alert_lscp
        }
    };
    let stpc_col = |x: f64| [(8.0, 5), (4.0, 4), (2.0, 3), (1.0, 2), (0.5, 1)].iter().find(|(t, _)| x >= *t).map(|(_, i)| a[*i]).unwrap_or(a[0]);
    let stpf_col = |x: f64| [(7.0, 5), (5.0, 4), (2.0, 3), (1.0, 2), (0.5, 1)].iter().find(|(t, _)| x >= *t).map(|(_, i)| a[*i]).unwrap_or(a[0]);
    let ship_col = |x: f64| [(5.0, 5), (2.0, 4), (1.0, 3), (0.5, 2)].iter().find(|(t, _)| x >= *t).map(|(_, i)| a[*i]).unwrap_or(a[0]);
    type ColorRule<'a> = &'a dyn Fn(f64) -> Color;
    // The last flag says whether SHARPpy's if/elif chain for that row ends
    // in an `else`. Missing values compare false everywhere, so rows
    // without one keep the previous row's pen (Qt yellow before the first
    // row), and rows with one fall to the lowest alert colour.
    let rows: [(&str, Option<f64>, Option<f64>, ColorRule, bool); 4] = [
        ("Supercell = ", *sev.scp.get(d), scp, &scp_col, false),
        ("STP (cin) = ", *sev.stp_cin.get(d), stpc, &stpc_col, false),
        ("STP (fix) = ", *sev.stp_fixed.get(d), stpf, &stpf_col, true),
        ("SHIP = ", sev.ship, sev.ship, &ship_col, true),
    ];
    let bold = m.font.bold(true);
    let mut prev = Color::hex("#FFFF00");
    for (i, (label, shown, val, cf, has_else)) in rows.iter().enumerate() {
        let col = match val {
            Some(x) if x.is_finite() => cf(*x),
            _ if *has_else => a[0],
            _ => prev,
        };
        prev = col;
        let s = format!("{label}{}", float2str(*shown, 1));
        text(c, Rect::qt_int(x1 * 7.0, yl + lh * i as f32, x1 * 8.0, lh), Align::LEFT, &s, bold, col);
    }
    c.restore();
}

fn wind_str(ctx: &DrawCtx, ds: Option<(f64, f64)>, unit: bool) -> String {
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    let unit_s = if ms { " m/s" } else { " kt" };
    let conv = |s: f64| if ms { kts2ms(s) } else { s };
    let s = match ds {
        Some((d, s)) if d.is_finite() && s.is_finite() => format!("{}/{}", int2str(Some(d)), int2str(Some(conv(s)))),
        _ => "--/--".to_string(),
    };
    if unit { format!("{s}{unit_s}") } else { s }
}

fn shear_str(ctx: &DrawCtx, uv: Option<UV>) -> String {
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    int2str(uv.map(|(u, v)| {
        let m = (u * u + v * v).sqrt();
        if ms { kts2ms(m) } else { m }
    }))
}

/// Kinematic parameter table with the 1 km / 6 km barbs.
pub fn draw_kinematics(c: &mut dyn Canvas, ctx: &DrawCtx, w: f32, h: f32) {
    let th = ctx.theme;
    let v = ctx.view;
    let k = &v.kinematics;
    let d = ctx.deviant();
    let m = table_metrics(c, ctx, h);
    let lh = m.lh;
    let fg = th.fg;
    let pivotal = ctx.opts.flavor == Flavor::Pivotal;
    c.save();
    c.translate(1.0, 1.0);
    c.set_antialias(false);
    let brx = w;
    let (lpad, rpad) = (5.0, 5.0);
    let x1 = brx / 10.0;
    let ms = ctx.opts.wind_units == WindUnits::MetersPerSecond;
    let unit = if ms { "m/s" } else { "kt" };

    // Column headers and the x-positions (in units of x1) of each value column.
    let (headers, cols): (Vec<(f32, String)>, Vec<f32>) = if pivotal {
        (
            vec![
                (2.0, "EHI".into()),
                (3.5, "SRH".into()),
                (5.0, "BlkShr".into()),
                (7.0, "MnWind".into()),
                (8.5, "SRW".into()),
            ],
            vec![2.0, 3.5, 5.0, 7.0, 8.72],
        )
    } else {
        (
            vec![
                (2.5, "SRH (m2/s2)".into()),
                (5.0, format!("Shear ({unit})")),
                (7.0, "MnWind".into()),
                (9.0 - rpad / x1, "SRW".into()),
            ],
            vec![3.0, 5.0, 7.0, 9.0],
        )
    };
    for (pos, t) in &headers {
        text(c, Rect::qt_int(x1 * pos, 3.0, x1, lh), Align::CENTER, t, m.font, fg);
    }

    // Row labels.
    let mut y1 = lh + m.tpad;
    let label_rows = |c: &mut dyn Canvas, labels: &[&str], y1: &mut f32| {
        for l in labels {
            text(c, Rect::qt_int(lpad, *y1, x1, lh), Align::LEFT, l, m.font, fg);
            *y1 += lh;
        }
    };
    let row0 = y1;
    let eff_label = if pivotal { "Eff Inflow" } else { "Eff Inflow Layer" };
    label_rows(c, &["SFC-1km", "SFC-3km", eff_label], &mut y1);
    y1 += m.tpad;
    let row1 = y1;
    label_rows(c, &["SFC-6km", "SFC-8km", "LCL-EL (Cloud Layer)", "Eff Shear (EBWD)"], &mut y1);
    y1 += m.tpad;
    let row2 = y1;
    label_rows(c, &["BRN Shear = ", "4-6km SR Wind = "], &mut y1);
    y1 += m.tpad;
    let row3 = y1;
    let barby = y1 + m.tpad;
    label_rows(c, &["...Storm Motion Vectors...", "Bunkers Right = ", "Bunkers Left = ", "Corfidi Downshear = ", "Corfidi Upshear = "], &mut y1);
    // Header rule: SHARPpy leaves `ylast` at one row height after the last
    // label block and draws the separator 3 px below that.
    c.line(0.0, (lh + 3.0).trunc(), brx, (lh + 3.0).trunc(), &Pen::new(fg, 1.0));

    // Values.
    let put = |c: &mut dyn Canvas, col: f32, y: f32, s: &str, color: Color| {
        text(c, Rect::qt_int(x1 * col, y, x1, lh), Align::RIGHT, s, m.font, color);
    };
    let has_eff = v.eff_inflow.is_some();
    let eff_or_dash = |s: Option<(f64, f64)>| if has_eff { s } else { None };
    let ehi = |x: &crate::RL<Option<f64>>| float2str(*x.get(d), 1);
    let rows_a: [(Vec<String>, f32); 3] = [
        (
            vec![ehi(&k.ehi1km), int2str(*k.srh1km.get(d)), shear_str(ctx, k.shear_1km), wind_str(ctx, k.mean_1km, false), wind_str(ctx, *k.srw_1km.get(d), false)],
            row0,
        ),
        (
            vec![ehi(&k.ehi3km), int2str(*k.srh3km.get(d)), shear_str(ctx, k.shear_3km), wind_str(ctx, k.mean_3km, false), wind_str(ctx, *k.srw_3km.get(d), false)],
            row0 + lh,
        ),
        (
            vec![
                ehi(&k.ehi_eff),
                int2str(*k.esrh.get(d)),
                shear_str(ctx, k.shear_eff),
                wind_str(ctx, eff_or_dash(k.mean_eff), false),
                wind_str(ctx, eff_or_dash(*k.srw_eff.get(d)), false),
            ],
            row0 + 2.0 * lh,
        ),
    ];
    for (vals, y) in rows_a.iter() {
        let vals: Vec<&String> = if pivotal { vals.iter().collect() } else { vals.iter().skip(1).collect() };
        for (s, col) in vals.iter().zip(&cols) {
            put(c, *col, *y, s, fg);
        }
    }
    let lower_cols: Vec<f32> = if pivotal { vec![cols[2], cols[3], cols[4]] } else { vec![5.0, 7.0, 9.0] };
    let rows_b = [
        [shear_str(ctx, k.shear_6km), wind_str(ctx, k.mean_6km, false), wind_str(ctx, *k.srw_6km.get(d), false)],
        [shear_str(ctx, k.shear_8km), wind_str(ctx, k.mean_8km, false), wind_str(ctx, *k.srw_8km.get(d), false)],
        [shear_str(ctx, k.shear_lcl_el), wind_str(ctx, k.mean_lcl_el, false), wind_str(ctx, *k.srw_lcl_el.get(d), false)],
        [
            shear_str(ctx, k.ebwd),
            wind_str(ctx, eff_or_dash(k.mean_ebw), false),
            wind_str(ctx, eff_or_dash(*k.srw_ebw.get(d)), false),
        ],
    ];
    for (i, vals) in rows_b.iter().enumerate() {
        for (s, col) in vals.iter().zip(&lower_cols) {
            put(c, *col, row1 + lh * i as f32, s, fg);
        }
    }
    let mid_col = if pivotal { cols[2] } else { 5.0 };
    put(c, mid_col, row2, &format!("{} m2/s2", int2str(k.brn_shear)), fg);
    put(c, mid_col, row2 + lh, &wind_str(ctx, *k.srw_4_5km.get(d), true), fg);

    let st = &v.storm;
    let dirspd = |x: Option<UV>| x.map(|(u, v)| crate::util::comp2vec(u, v));
    let ys = row3 + lh;
    put(c, mid_col, ys, &wind_str(ctx, dirspd(st.bunkers.right), true), Color::hex("#0099CC"));
    put(c, mid_col, ys + lh, &wind_str(ctx, dirspd(st.bunkers.left), true), Color::hex("#FF6666"));
    put(c, mid_col, ys + 2.0 * lh, &wind_str(ctx, dirspd(st.corfidi_dn), true), fg);
    put(c, mid_col, ys + 3.0 * lh, &wind_str(ctx, dirspd(st.corfidi_up), true), fg);

    // 1 km & 6 km barbs.
    c.set_antialias(true);
    let blue = Color::hex("#0A74C6");
    // SHARPpy positions this label just below the Bunkers rows.
    let label_y = ys + 2.0 * lh + m.tpad;
    let rlab = Rect::qt_int(x1 * 7.0, label_y, x1 * 2.0, lh);
    text(c, rlab, Align::CENTER, "1km & 6km AGL\nWind Barbs", m.font, blue);
    let conv = |s: f64| if ms { kts2ms(s) } else { s };
    let shemis = v.latitude < 0.0;
    if let Some((dd, ss)) = k.wind1km {
        draw_barb(c, x1 * 8.0, barby, dd, conv(ss), Color::hex("#AA0000"), shemis);
    }
    if let Some((dd, ss)) = k.wind6km {
        draw_barb(c, x1 * 8.0, barby, dd, conv(ss), blue, shemis);
    }
    let _ = th;
    c.restore();
}
