//! Wind barbs in SHARPpy's style: a 25 px staff pointing into the wind,
//! hollow triangular flags for 50 kt, 10 px full barbs, 5 px half barbs,
//! features spaced 4 px apart walking back toward the station, and a 3 px
//! circle for calm.

use crate::canvas::{Canvas, CanvasExt, Color, Path, Pen};

/// Draw one barb at (x, y). `wdir` in degrees, `wspd` in display units.
pub fn draw_barb(c: &mut dyn Canvas, x: f32, y: f32, wdir: f64, wspd: f64, color: Color, shemis: bool) {
    if !wdir.is_finite() || !wspd.is_finite() {
        return;
    }
    let pen = Pen::new(color, 1.0);
    let mut spd = (crate::util::round_half_even(wspd / 5.0) * 5.0) as i64;
    if spd <= 0 {
        c.stroke_ellipse(x, y, 3.0, 3.0, &pen);
        return;
    }
    // Build in a local frame where the staff runs along +x, then rotate by
    // (wdir - 90) degrees about the station (Qt's rotate is clockwise on screen).
    let ang = ((wdir - 90.0) as f32).to_radians();
    let (sa, ca) = ang.sin_cos();
    let tf = |lx: f32, ly: f32| (x + lx * ca - ly * sa, y + lx * sa + ly * ca);
    let side = if shemis { -1.0 } else { 1.0 };
    let mut segs: Vec<((f32, f32), (f32, f32))> = Vec::new();
    segs.push(((0.0, 0.0), (25.0, 0.0)));
    let mut px = 25.0f32;
    while spd >= 50 {
        segs.push(((px, 0.0), (px, side * 10.0)));
        segs.push(((px, side * 10.0), (px - 4.0, 0.0)));
        px -= 6.0;
        spd -= 50;
    }
    while spd >= 10 {
        segs.push(((px, 0.0), (px, side * 10.0)));
        px -= 4.0;
        spd -= 10;
    }
    while spd >= 5 {
        segs.push(((px, 0.0), (px, side * 5.0)));
        px -= 4.0;
        spd -= 5;
    }
    let mut path = Path::new();
    for (a, b) in segs {
        let (ax, ay) = tf(a.0, a.1);
        let (bx, by) = tf(b.0, b.1);
        path.move_to(ax, ay).line_to(bx, by);
    }
    c.stroke_path(&path, &pen);
}
