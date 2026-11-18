//! Thermodynamic primitives.
//!
//! Units follow SHARPpy's conventions throughout: pressure in hPa,
//! temperature in °C, mixing ratio in g/kg. Every function here is a plain
//! scalar `f64 -> f64` so the renderer can call them directly when drawing
//! skew-T background lines (dry/moist adiabats, mixing-ratio lines).
//!
//! Numerics intentionally reproduce SHARPpy (and the original SPC SHARP
//! code it descends from): moist adiabats use the Wobus polynomial, the
//! saturated lift iterates a secant scheme to a 0.1 °C tolerance, the
//! vapor-pressure fit is the 8th-power polynomial, and the LCL uses the
//! empirical `lcltemp` fit. These are *not* the most accurate formulations
//! available, but matching them is what keeps CAPE/LCL/etc. identical to
//! SPC/SHARPpy output.

/// 0 °C in Kelvin.
pub const ZEROCNK: f64 = 273.15;
/// R/cp as used by SHARPpy (note: truncated value, not 2/7).
pub const ROCP: f64 = 0.28571426;
/// Standard gravity (m s^-2).
pub const G: f64 = 9.80665;
/// Ratio of gas constants Rd/Rv used in the virtual-temperature formula.
pub const EPS: f64 = 0.62197;

use crate::numerics::pow;

/// Potential temperature (°C) of air at `p_hpa`/`t_c`, referenced to
/// `p2_hpa` (SHARPpy's default reference is 1000 hPa).
#[inline]
pub fn theta(p_hpa: f64, t_c: f64, p2_hpa: f64) -> f64 {
    ((t_c + ZEROCNK) * pow(p2_hpa / p_hpa, ROCP)) - ZEROCNK
}

/// Pressure (hPa) at which a parcel with potential temperature `theta_c`
/// (referenced to 1000 hPa) has temperature `t_c`.
#[inline]
pub fn thalvl(theta_c: f64, t_c: f64) -> f64 {
    let t = t_c + ZEROCNK;
    let th = theta_c + ZEROCNK;
    1000. / pow(th / t, 1. / ROCP)
}

/// Temperature (°C) of a parcel when raised dry-adiabatically to its LCL.
#[inline]
pub fn lcltemp(t: f64, td: f64) -> f64 {
    let s = t - td;
    let dlt = s * (1.2185 + 0.001278 * t + s * (-0.00219 + 1.173e-5 * s - 0.0000052 * t));
    t - dlt
}

/// Lift a parcel dry-adiabatically to its LCL. Returns `(p_lcl, t_lcl)`.
#[inline]
pub fn drylift(p: f64, t: f64, td: f64) -> (f64, f64) {
    let t2 = lcltemp(t, td);
    let p2 = thalvl(theta(p, t, 1000.), t2);
    (p2, t2)
}

/// The Wobus function: correction (°C) between potential temperature and
/// saturated potential temperature. SHARPpy (like SHARP) ignores the slight
/// pressure dependency noted by Davies-Jones (2008); so do we.
#[inline]
pub fn wobf(t: f64) -> f64 {
    let t = t - 20.;
    if t <= 0. {
        let npol = 1.
            + t * (-8.841660499999999e-3
                + t * (1.4714143e-4
                    + t * (-9.671989000000001e-7 + t * (-3.2607217e-8 + t * (-3.8598073e-10)))));
        15.13 / pow(npol, 4.0)
    } else {
        let ppol = t
            * (4.9618922e-07
                + t * (-6.1059365e-09
                    + t * (3.9401551e-11 + t * (-1.2588129e-13 + t * (1.6688280e-16)))));
        let ppol = 1. + t * (3.6182989e-03 + t * (-1.3603273e-05 + ppol));
        (29.93 / pow(ppol, 4.0)) + (0.96 * t) - 14.8
    }
}

/// Temperature (°C) at pressure `p` of a saturated parcel whose saturated
/// potential temperature is `thetam` (°C). Secant iteration converging to
/// 0.1 °C, exactly as SHARP/SHARPpy do (a tighter tolerance would drift
/// from SPC's calibrated CAPE values).
#[inline]
pub fn satlift(p: f64, thetam: f64) -> f64 {
    if (p - 1000.).abs() - 0.001 <= 0. {
        return thetam;
    }
    satlift_k(p, thetam, pow(p / 1000., ROCP))
}

/// [`satlift`] with `pwrp = pow(p / 1000, ROCP)` supplied by the caller
/// (cached per profile level).
#[inline]
pub(crate) fn satlift_k(p: f64, thetam: f64, pwrp: f64) -> f64 {
    const CONV: f64 = 0.1;
    if (p - 1000.).abs() - 0.001 <= 0. {
        return thetam;
    }
    // first pass
    let mut t1 = (thetam + ZEROCNK) * pwrp - ZEROCNK;
    let mut e1 = wobf(t1) - wobf(thetam);
    let mut rate = 1.0;
    let mut t2 = t1 - e1 * rate;
    let mut e2 = (t2 + ZEROCNK) / pwrp - ZEROCNK;
    e2 += wobf(t2) - wobf(e2) - thetam;
    let mut eor = e2 * rate;
    // Guard against a pathological non-converging input (SHARPpy would spin
    // forever); 100 iterations is far more than ever needed in practice.
    let mut iters = 0;
    while eor.abs() - CONV > 0. && iters < 100 {
        rate = (t2 - t1) / (e2 - e1);
        t1 = t2;
        e1 = e2;
        t2 = t1 - e1 * rate;
        e2 = (t2 + ZEROCNK) / pwrp - ZEROCNK;
        e2 += wobf(t2) - wobf(e2) - thetam;
        eor = e2 * rate;
        iters += 1;
    }
    t2 - eor
}

/// Lift a saturated parcel at `p`/`t` moist-adiabatically to `p2`; returns
/// its temperature (°C).
#[inline]
pub fn wetlift(p: f64, t: f64, p2: f64) -> f64 {
    let thta = theta(p, t, 1000.);
    if thta.is_nan() || p2.is_nan() {
        return f64::NAN;
    }
    let thetam = thta - wobf(thta) + wobf(t);
    satlift(p2, thetam)
}

/// [`wetlift`] with the potential-temperature factor `k1 = pow(1000 / p,
/// ROCP)` and the target-level factor `k2 = pow(p2 / 1000, ROCP)` supplied
/// by the caller (cached per profile level). Same arithmetic, same bits.
#[inline]
pub(crate) fn wetlift_k(t: f64, k1: f64, p2: f64, k2: f64) -> f64 {
    let thta = theta_k(t, k1);
    if thta.is_nan() || p2.is_nan() {
        return f64::NAN;
    }
    let thetam = thta - wobf(thta) + wobf(t);
    satlift_k(p2, thetam, k2)
}

/// [`theta`] (reference 1000 hPa) with `k = pow(1000 / p, ROCP)` supplied.
#[inline]
pub(crate) fn theta_k(t_c: f64, k: f64) -> f64 {
    ((t_c + ZEROCNK) * k) - ZEROCNK
}

/// [`virtemp`] with `w = 0.001 * mixratio(p, td)` supplied.
#[inline]
pub(crate) fn virtemp_w(t: f64, w: f64) -> f64 {
    let tk = t + ZEROCNK;
    let vt = (tk * (1. + w / EPS) / (1. + w)) - ZEROCNK;
    if vt.is_finite() { vt } else { t }
}

/// Temperature (°C) of the parcel (p, t, td) after lifting to `lev` (hPa):
/// dry-adiabatically to the LCL, then moist-adiabatically.
#[inline]
pub fn lifted(p: f64, t: f64, td: f64, lev: f64) -> f64 {
    let (p2, t2) = drylift(p, t, td);
    wetlift(p2, t2, lev)
}

/// Saturation vapor pressure (hPa) over water at `t` (°C); SHARP polynomial.
#[inline]
pub fn vappres(t: f64) -> f64 {
    let pol = t * (1.1112018e-17 + (t * -3.0994571e-20));
    let pol = t * (2.1874425e-13 + (t * (-1.789232e-15 + pol)));
    let pol = t * (4.3884180e-09 + (t * (-2.988388e-11 + pol)));
    let pol = t * (7.8736169e-05 + (t * (-6.111796e-07 + pol)));
    let pol = 0.99999683 + (t * (-9.082695e-03 + pol));
    6.1078 / pow(pol, 8.0)
}

/// Mixing ratio (g/kg) of saturated air at `p` (hPa) and `t` (°C). Pass the
/// dewpoint as `t` to get the actual mixing ratio. Includes SHARP's
/// enhancement-factor correction.
#[inline]
pub fn mixratio(p: f64, t: f64) -> f64 {
    mixratio_e(p, t, vappres(t))
}

/// [`mixratio`] with `e = vappres(t)` supplied.
#[inline]
pub(crate) fn mixratio_e(p: f64, t: f64, e: f64) -> f64 {
    let x = 0.02 * (t - 12.5 + (7500. / p));
    let wfw = 1. + (0.0000045 * p) + (0.0014 * x * x);
    let fwesw = wfw * e;
    621.97 * (fwesw / (p - fwesw))
}

/// Temperature (°C) of air with mixing ratio `w_gkg` (g/kg) at `p` (hPa).
#[inline]
pub fn temp_at_mixrat(w_gkg: f64, p: f64) -> f64 {
    const C1: f64 = 0.0498646455;
    const C2: f64 = 2.4082965;
    const C3: f64 = 7.07475;
    const C4: f64 = 38.9114;
    const C5: f64 = 0.0915;
    const C6: f64 = 1.2035;
    let x = (w_gkg * p / (622. + w_gkg)).log10();
    (pow(10., (C1 * x) + C2) - C3 + (C4 * pow(pow(10., C5 * x) - C6, 2.))) - ZEROCNK
}

/// Temperature (°C) at which the saturation vapor pressure equals `e` (hPa)
/// (simple Clausius-Clapeyron inversion used by SHARPpy's fire module).
#[inline]
pub fn temp_at_vappres(e: f64) -> f64 {
    let l = 2.5e6;
    let r_v = 461.5;
    let t_o = 273.15;
    let e_so = 6.11;
    let a = l / r_v;
    let b = 1. / t_o;
    pow(-1. * ((1. / a) * (e / e_so).ln() - b), -1.) - ZEROCNK
}

/// Virtual temperature (°C). If `td` is NaN (missing) the dry-bulb
/// temperature is returned unchanged, matching SHARPpy.
#[inline]
pub fn virtemp(p: f64, t: f64, td: f64) -> f64 {
    let tk = t + ZEROCNK;
    let w = 0.001 * mixratio(p, td);
    let vt = (tk * (1. + w / EPS) / (1. + w)) - ZEROCNK;
    if vt.is_finite() { vt } else { t }
}

/// Relative humidity (%) from the ratio of vapor pressures.
#[inline]
pub fn relh(_p: f64, t: f64, td: f64) -> f64 {
    100. * vappres(td) / vappres(t)
}

/// Wet-bulb temperature (°C).
#[inline]
pub fn wetbulb(p: f64, t: f64, td: f64) -> f64 {
    let (p2, t2) = drylift(p, t, td);
    wetlift(p2, t2, p)
}

/// Wet-bulb potential temperature (°C).
#[inline]
pub fn thetaw(p: f64, t: f64, td: f64) -> f64 {
    let (p2, t2) = drylift(p, t, td);
    wetlift(p2, t2, 1000.)
}

/// Equivalent potential temperature (°C), SHARP-style: lift to the LCL, then
/// moist-adiabatically to 100 hPa, then dry-adiabatically back to 1000 hPa.
#[inline]
pub fn thetae(p: f64, t: f64, td: f64) -> f64 {
    let (p2, t2) = drylift(p, t, td);
    theta(100., wetlift(p2, t2, 100.), 1000.)
}

#[inline]
pub fn ctof(t: f64) -> f64 {
    1.8 * t + 32.
}
#[inline]
pub fn ftoc(t: f64) -> f64 {
    (t - 32.) * (5. / 9.)
}
#[inline]
pub fn ctok(t: f64) -> f64 {
    t + ZEROCNK
}
#[inline]
pub fn ktoc(t: f64) -> f64 {
    t - ZEROCNK
}
