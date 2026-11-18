//! Unit conversions and wind-vector helpers (SHARPpy's conversion factors).

use crate::numerics::sq;

/// Values whose magnitude is below this are snapped to zero (SHARPpy `TOL`).
pub const TOL: f64 = 1e-10;

#[inline]
pub fn ms_to_kts(v: f64) -> f64 {
    v * 1.94384449
}
#[inline]
pub fn kts_to_ms(v: f64) -> f64 {
    v * 0.514444
}
#[inline]
pub fn ms_to_mph(v: f64) -> f64 {
    v * 2.23694
}
#[inline]
pub fn kts_to_mph(v: f64) -> f64 {
    v * 1.15078
}
#[inline]
pub fn m_to_ft(v: f64) -> f64 {
    v * 3.2808399
}
#[inline]
pub fn ft_to_m(v: f64) -> f64 {
    v * 0.3048
}
#[inline]
pub fn in_to_cm(v: f64) -> f64 {
    v * 2.54
}

/// Wind direction (deg) / speed -> (u, v) components, SHARPpy convention
/// (meteorological direction the wind blows *from*; components in the same
/// unit as the speed). Tiny components are snapped to exactly zero.
#[inline]
pub fn vec2comp(wdir: f64, wspd: f64) -> (f64, f64) {
    let r = wdir.to_radians();
    let mut u = wspd * r.sin() * -1.;
    let mut v = wspd * r.cos() * -1.;
    if u.abs() < TOL {
        u = 0.;
    }
    if v.abs() < TOL {
        v = 0.;
    }
    (u, v)
}

/// (u, v) -> (direction deg, speed). Returns NaNs if either is NaN.
#[inline]
pub fn comp2vec(u: f64, v: f64) -> (f64, f64) {
    if u.is_nan() || v.is_nan() {
        return (f64::NAN, f64::NAN);
    }
    let mut wdir = (-u).atan2(-v).to_degrees();
    if wdir < 0. {
        wdir += 360.;
    }
    if wdir.abs() < TOL {
        wdir = 0.;
    }
    (wdir, mag(u, v))
}

/// Vector magnitude, computed as SHARPpy does (`sqrt(u**2 + v**2)` with
/// libm `pow`).
#[inline]
pub fn mag(u: f64, v: f64) -> f64 {
    (sq(u) + sq(v)).sqrt()
}

/// A horizontal wind vector in knots (u eastward, v northward).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vector {
    pub u: f64,
    pub v: f64,
}

impl Vector {
    pub fn new(u: f64, v: f64) -> Self {
        Self { u, v }
    }
    /// Build from direction (deg, from) and speed (kt).
    pub fn from_dir_speed(wdir: f64, wspd: f64) -> Self {
        let (u, v) = vec2comp(wdir, wspd);
        Self { u, v }
    }
    /// Meteorological direction (deg, from), SHARPpy `comp2vec`.
    pub fn direction(&self) -> f64 {
        comp2vec(self.u, self.v).0
    }
    /// Speed in knots.
    pub fn speed(&self) -> f64 {
        mag(self.u, self.v)
    }
    /// Speed in m/s (SHARPpy's KTS2MS factor).
    pub fn speed_ms(&self) -> f64 {
        kts_to_ms(self.speed())
    }
    pub(crate) fn from_nan(u: f64, v: f64) -> Option<Self> {
        if u.is_nan() || v.is_nan() { None } else { Some(Self { u, v }) }
    }
}
