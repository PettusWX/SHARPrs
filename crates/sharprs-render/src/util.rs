//! Number formatting (SHARPpy's INT2STR / FLOAT2STR), unit conversion and
//! the display-side interpolation helpers the panels need.

/// Round half to even, like `numpy.round`.
pub fn round_half_even(v: f64) -> f64 {
    v.round_ties_even()
}

/// SHARPpy `INT2STR`: rounded integer text, `--` when missing.
pub fn int2str(v: Option<f64>) -> String {
    // Single source of truth for SHARPpy-identical rounding (ties to even,
    // NumPy semantics) lives in the physics crate.
    sharprs_core::display::int_str(v)
}

/// SHARPpy `FLOAT2STR`: Python repr of `numpy.round(v, p)` (trailing zeros
/// dropped, at least one decimal), `--` when missing.
pub fn float2str(v: Option<f64>, p: i32) -> String {
    sharprs_core::display::float_str(v, p)
}

/// Python's `repr(float)` for ordinary magnitudes.
pub fn py_float_repr(r: f64) -> String {
    let s = format!("{}", r);
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

pub fn opt(v: f64) -> Option<f64> {
    if v.is_finite() { Some(v) } else { None }
}

pub fn kts2ms(v: f64) -> f64 {
    v * 0.514444
}

pub fn ctof(c: f64) -> f64 {
    1.8 * c + 32.0
}

pub fn ftoc(f: f64) -> f64 {
    (f - 32.0) * 5.0 / 9.0
}

pub fn m2ft(m: f64) -> f64 {
    m * 3.2808399
}

/// (u, v) -> (direction deg, speed), SHARPpy `comp2vec`.
pub fn comp2vec(u: f64, v: f64) -> (f64, f64) {
    let spd = (u * u + v * v).sqrt();
    let mut dir = (-u).atan2(-v).to_degrees();
    if dir < 0.0 {
        dir += 360.0;
    }
    if spd < 1e-10 {
        dir = 0.0;
    }
    (dir, spd)
}

/// (direction deg, speed) -> (u, v), SHARPpy `vec2comp`.
pub fn vec2comp(dir: f64, spd: f64) -> (f64, f64) {
    let r = dir.to_radians();
    (-spd * r.sin(), -spd * r.cos())
}

/// Linear interpolation of `ys` at `x` over monotonic `xs` (either
/// direction). Missing (NaN) pairs are skipped. Returns NaN outside range.
pub fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    let pts: Vec<(f64, f64)> =
        xs.iter().zip(ys).filter(|(a, b)| a.is_finite() && b.is_finite()).map(|(a, b)| (*a, *b)).collect();
    if pts.len() < 2 || !x.is_finite() {
        return f64::NAN;
    }
    let increasing = pts[pts.len() - 1].0 > pts[0].0;
    for w in pts.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        let inside = if increasing { x >= x0 && x <= x1 } else { x <= x0 && x >= x1 };
        if inside {
            if x1 == x0 {
                return y0;
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    f64::NAN
}

/// `numpy.interp(x, xp, fp)` including its behaviour on non-monotonic
/// `xp`: each `x` is located with NumPy's guessed binary search (which
/// assumes `xp` is increasing) and interpolated between the bracketing
/// pair it lands on; values left/right of `xp[0]`/`xp[-1]` clamp to the end
/// values. SHARPpy feeds it raw arrays whose masked entries still hold the
/// -9999 fill value, so reproducing the search matters for those profiles.
pub fn numpy_interp(x: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    const LIKELY_IN_CACHE_SIZE: usize = 8;
    let n = xp.len().min(fp.len());
    if n == 0 {
        return vec![f64::NAN; x.len()];
    }
    if n == 1 {
        return x.iter().map(|v| if v.is_nan() { *v } else { fp[0] }).collect();
    }
    let (xp, fp) = (&xp[..n], &fp[..n]);
    // Returns the index i with xp[i] <= key < xp[i + 1] (for sorted xp),
    // -1 below the range and n above it.
    let search = |key: f64, guess: isize| -> isize {
        let len = n as isize;
        let at = |i: isize| xp[i as usize];
        if key > at(len - 1) {
            return len;
        } else if key < at(0) {
            return -1;
        }
        if len <= 4 {
            let mut i = 1;
            while i < len && key >= at(i) {
                i += 1;
            }
            return i - 1;
        }
        let guess = guess.clamp(1, len - 3);
        let (mut imin, mut imax) = (0isize, len);
        let cache = LIKELY_IN_CACHE_SIZE as isize;
        if key < at(guess) {
            if key < at(guess - 1) {
                imax = guess - 1;
                if guess > cache && key >= at(guess - cache) {
                    imin = guess - cache;
                }
            } else {
                return guess - 1;
            }
        } else if key < at(guess + 1) {
            return guess;
        } else if key < at(guess + 2) {
            return guess + 1;
        } else {
            imin = guess + 2;
            if guess < len - cache - 1 && key < at(guess + cache) {
                imax = guess + cache;
            }
        }
        while imin < imax {
            let imid = imin + ((imax - imin) >> 1);
            if key >= at(imid) {
                imin = imid + 1;
            } else {
                imax = imid;
            }
        }
        imin - 1
    };
    let mut j: isize = 0;
    x.iter()
        .map(|&xv| {
            if xv.is_nan() {
                return xv;
            }
            j = search(xv, j);
            if j == -1 {
                fp[0]
            } else if j == n as isize {
                fp[n - 1]
            } else if j == n as isize - 1 || xp[j as usize] == xv {
                fp[j as usize]
            } else {
                let i = j as usize;
                let slope = (fp[i + 1] - fp[i]) / (xp[i + 1] - xp[i]);
                let mut r = slope * (xv - xp[i]) + fp[i];
                if r.is_nan() {
                    r = slope * (xv - xp[i + 1]) + fp[i + 1];
                    if r.is_nan() && fp[i] == fp[i + 1] {
                        r = fp[i];
                    }
                }
                r
            }
        })
        .collect()
}

/// Interpolate a profile variable at pressure `p` (linear in log p).
pub fn interp_p(p: f64, pres: &[f64], var: &[f64]) -> f64 {
    let lp: Vec<f64> = pres.iter().map(|v| v.ln()).collect();
    interp(p.ln(), &lp, var)
}

/// Pressure at height `h` (m MSL), interpolating log p in height.
pub fn pres_at_hght(h: f64, pres: &[f64], hght: &[f64]) -> f64 {
    let lp: Vec<f64> = pres.iter().map(|v| v.ln()).collect();
    interp(h, hght, &lp).exp()
}
