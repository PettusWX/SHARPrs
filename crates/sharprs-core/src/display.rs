//! Display formatting identical to SHARPpy's `INT2STR` / `FLOAT2STR`.
//!
//! SHARPpy rounds with NumPy (`np.round`: round-half-to-even after scaling
//! by a power of ten) and prints with Python's `str`. Using Rust's
//! `f64::round` (half away from zero) or `format!("{:.1}")` would show a
//! different last digit for exact ties, so the renderer should format every
//! displayed number through these helpers.

/// `np.round(x, decimals)` for float64.
pub fn np_round(x: f64, decimals: i32) -> f64 {
    fn power_of_ten(n: i32) -> f64 {
        const P10: [f64; 9] = [1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8];
        if n < 9 {
            P10[n as usize]
        } else {
            let mut r = 1e9;
            let mut k = n;
            while k > 9 {
                r *= 10.;
                k -= 1;
            }
            r
        }
    }
    if decimals >= 0 {
        let f = power_of_ten(decimals);
        (x * f).round_ties_even() / f
    } else {
        let f = power_of_ten(-decimals);
        (x / f).round_ties_even() * f
    }
}

/// Python `str(float)` for finite values in the usual display range.
fn py_float_str(x: f64) -> String {
    if x == 0. {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let a = x.abs();
    if !(1e-4..1e16).contains(&a) {
        // Python switches to exponent notation; mimic `repr`
        let s = format!("{x:e}");
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let ei: i32 = e.parse().unwrap_or(0);
        return format!("{m}e{}{:02}", if ei < 0 { '-' } else { '+' }, ei.abs());
    }
    let s = format!("{x}");
    if s.contains('.') { s } else { s + ".0" }
}

/// SHARPpy `INT2STR`: nearest integer (ties to even), `"--"` if missing.
pub fn int_str(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => {
            let r = np_round(x, 0);
            format!("{}", r as i64)
        }
        _ => "--".into(),
    }
}

/// SHARPpy `FLOAT2STR(val, precision)`, `"--"` if missing.
pub fn float_str(v: Option<f64>, precision: i32) -> String {
    match v {
        Some(x) if x.is_finite() => py_float_str(np_round(x, precision)),
        _ => "--".into(),
    }
}
