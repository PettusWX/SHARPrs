//! Kinematic calculations: layer-mean winds, bulk shear, storm-relative
//! helicity, Bunkers / Corfidi motions, critical angle.

use crate::numerics::{self, arange, masked_average2, masked_mean, select_sum, sq};
use crate::profile::{Profile, opt};
use crate::units::{Vector, kts_to_ms, mag, ms_to_kts, vec2comp};

/// Storm-relative helicity (m²/s²): total, positive and negative parts.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Helicity {
    pub total: f64,
    pub positive: f64,
    pub negative: f64,
}

/// Right- and left-moving storm motions (kt).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StormMotion {
    pub right: Vector,
    pub left: Vector,
}

/// Corfidi MCS vectors (kt).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Corfidi {
    pub upshear: Vector,
    pub downshear: Vector,
}

/// 7.5 m/s Bunkers deviation, in knots.
fn bunkers_d() -> f64 {
    ms_to_kts(7.5)
}

impl Profile {
    /// Pressure-weighted mean wind (kt), 1 hPa sampling; minus (stu, stv).
    pub(crate) fn mean_wind_raw(&self, pbot: f64, ptop: f64, stu: f64, stv: f64) -> (f64, f64) {
        if pbot.is_nan() || ptop.is_nan() || !self.has_wind {
            return (f64::NAN, f64::NAN);
        }
        // `masked_average(..) - stu`: the average does not depend on the
        // storm motion, so it is memoised per layer
        let (mu, mv) = match self.mean_wind_memo_get(pbot, ptop) {
            Some(m) => m,
            None => {
                let m = match self.sfc_prefix(pbot, ptop) {
                    Some((g, n)) => {
                        let (u, v) = g.uv(self);
                        masked_average2(&u[..n], &v[..n], &g.p[..n])
                    }
                    None => {
                        let ps = arange(pbot, ptop - 1., -1.);
                        let (u, v) = self.components_many2(&ps);
                        masked_average2(&u, &v, &ps)
                    }
                };
                self.mean_wind_memo_put(pbot, ptop, m);
                m
            }
        };
        (mu - stu, mv - stv)
    }

    /// Non-pressure-weighted mean wind (kt), 1 hPa sampling.
    pub(crate) fn mean_wind_npw_raw(&self, pbot: f64, ptop: f64, stu: f64, stv: f64) -> (f64, f64) {
        if pbot.is_nan() || ptop.is_nan() || !self.has_wind {
            return (f64::NAN, f64::NAN);
        }
        if let Some((g, n)) = self.sfc_prefix(pbot, ptop) {
            let (u, v) = g.uv(self);
            return (masked_mean(&u[..n]) - stu, masked_mean(&v[..n]) - stv);
        }
        let ps = arange(pbot, ptop - 1., -1.);
        let (u, v) = self.components_many2(&ps);
        (masked_mean(&u) - stu, masked_mean(&v) - stv)
    }

    /// Bulk wind difference (kt) between two pressure levels.
    pub(crate) fn wind_shear_raw(&self, pbot: f64, ptop: f64) -> (f64, f64) {
        if pbot.is_nan() || ptop.is_nan() || !self.has_wind {
            return (f64::NAN, f64::NAN);
        }
        let (ub, vb) = self.components(pbot);
        let (ut, vt) = self.components(ptop);
        (ut - ub, vt - vb)
    }

    /// Storm-relative helicity between two heights AGL (m), using every
    /// observed wind level plus the interpolated bounds (SHARPpy `exact`).
    pub(crate) fn helicity_raw(&self, lower: f64, upper: f64, stu: f64, stv: f64) -> (f64, f64, f64) {
        let nan = (f64::NAN, f64::NAN, f64::NAN);
        if !self.has_wind || lower.is_nan() || upper.is_nan() || stu.is_nan() || stv.is_nan() {
            return nan;
        }
        if lower == upper {
            return (0., 0., 0.);
        }
        let plower = self.pres(self.msl(lower));
        let pupper = self.pres(self.msl(upper));
        if plower.is_nan() || pupper.is_nan() {
            return nan;
        }
        let ind1 = self.pres.iter().position(|&p| plower >= p);
        let ind2 = self.pres.iter().rposition(|&p| pupper <= p);
        let (u1, v1) = self.components(plower);
        let (u2, v2) = self.components(pupper);
        let mut u = vec![u1];
        let mut v = vec![v1];
        if let (Some(a), Some(b)) = (ind1, ind2) {
            if b + 1 > a {
                u.extend(self.u[a..=b].iter().copied().filter(|x| !x.is_nan()));
                v.extend(self.v[a..=b].iter().copied().filter(|x| !x.is_nan()));
            }
        }
        u.push(u2);
        v.push(v2);
        let sru: Vec<f64> = u.iter().map(|x| kts_to_ms(x - stu)).collect();
        let srv: Vec<f64> = v.iter().map(|x| kts_to_ms(x - stv)).collect();
        let layers: Vec<f64> = (0..sru.len() - 1).map(|i| (sru[i + 1] * srv[i]) - (sru[i] * srv[i + 1])).collect();
        let phel = select_sum(&layers, |x| x > 0.);
        let nhel = select_sum(&layers, |x| x < 0.);
        (phel + nhel, phel, nhel)
    }

    /// Maximum wind between two heights AGL; returns (u, v, pressure).
    ///
    /// SHARPpy quirk preserved: the returned pressure is indexed with the
    /// position *within the layer* rather than within the profile.
    pub(crate) fn max_wind_raw(&self, lower: f64, upper: f64) -> (f64, f64, f64) {
        let nan = (f64::NAN, f64::NAN, f64::NAN);
        if !self.has_wind || lower.is_nan() || upper.is_nan() {
            return nan;
        }
        let plower = self.pres(self.msl(lower));
        let pupper = self.pres(self.msl(upper));
        if plower.is_nan() || pupper.is_nan() {
            return nan;
        }
        let Some(ind1) = self.pres.iter().position(|&p| plower > p || numerics::isclose(plower, p)) else {
            return nan;
        };
        let Some(ind2) = self.pres.iter().rposition(|&p| pupper < p || numerics::isclose(pupper, p)) else {
            return nan;
        };
        if ind2 < ind1 || ind1 == ind2 {
            let (u, v) = if self.wdir[ind1].is_nan() {
                (f64::NAN, f64::NAN)
            } else {
                vec2comp(self.wdir[ind1], self.wspd[ind1])
            };
            return (u, v, self.pres[ind1]);
        }
        let arr: Vec<f64> = self.wspd[ind1..=ind2]
            .iter()
            .map(|&x| if x.is_nan() { f64::NAN } else { x })
            .collect();
        let inds = numerics::np_argsort(&arr);
        let valid: Vec<usize> = inds.into_iter().filter(|&i| !self.wspd[ind1 + i].is_nan()).rev().collect();
        let Some(&first) = valid.first() else {
            return nan;
        };
        let (u, v) = vec2comp(self.wdir[ind1 + first], self.wspd[ind1 + first]);
        (u, v, self.pres[first])
    }

    /// Bunkers motion without a parcel (0-6 km non-pressure-weighted mean
    /// wind and 0-6 km shear). Returns (rstu, rstv, lstu, lstv).
    pub(crate) fn non_parcel_bunkers_raw(&self) -> (f64, f64, f64, f64) {
        if !self.has_wind {
            return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        }
        let d = bunkers_d();
        let p6km = self.p_agl(6000.);
        let (mnu6, mnv6) = self.mean_wind_npw_raw(self.psfc(), p6km, 0., 0.);
        let (shru, shrv) = self.wind_shear_raw(self.psfc(), p6km);
        let tmp = d / mag(shru, shrv);
        (mnu6 + (tmp * shrv), mnv6 - (tmp * shru), mnu6 - (tmp * shrv), mnv6 + (tmp * shru))
    }

    /// SHARPpy's "experimental" non-parcel Bunkers (0-500 m to 5.5-6 km
    /// shear, pressure-weighted means).
    pub(crate) fn non_parcel_bunkers_experimental_raw(&self) -> (f64, f64, f64, f64) {
        if !self.has_wind {
            return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        }
        let d = bunkers_d();
        let psfc = self.psfc();
        let p500m = self.p_agl(500.);
        let p5500m = self.p_agl(5500.);
        let p6000m = self.p_agl(6000.);
        let (mnu500, mnv500) = self.mean_wind_raw(psfc, p500m, 0., 0.);
        let (mnu55, mnv55) = self.mean_wind_raw(p5500m, p6000m, 0., 0.);
        let shru = mnu55 - mnu500;
        let shrv = mnv55 - mnv500;
        let (mnu6, mnv6) = self.mean_wind_raw(psfc, p6000m, 0., 0.);
        let tmp = d / mag(shru, shrv);
        (mnu6 + (tmp * shrv), mnv6 - (tmp * shru), mnu6 - (tmp * shrv), mnv6 + (tmp * shru))
    }

    /// Bunkers motion using the effective inflow layer and the MU parcel EL
    /// (falls back to the non-parcel method when MUCAPE <= 100 J/kg or no EL).
    pub(crate) fn bunkers_raw(&self, mucape: f64, muel_agl: f64, pbot: f64) -> (f64, f64, f64, f64) {
        let d = bunkers_d();
        let base = self.agl(self.hght(pbot));
        if mucape > 100. && !muel_agl.is_nan() {
            let depth = muel_agl - base;
            let htop = base + (depth * (65. / 100.));
            let ptop = self.pres(self.msl(htop));
            let (mnu, mnv) = self.mean_wind_raw(pbot, ptop, 0., 0.);
            let (sru, srv) = self.wind_shear_raw(pbot, ptop);
            let srmag = mag(sru, srv);
            let uchg = d / srmag * srv;
            let vchg = d / srmag * sru;
            (mnu + uchg, mnv - vchg, mnu - uchg, mnv + vchg)
        } else {
            self.non_parcel_bunkers_raw()
        }
    }

    /// Corfidi upshear/downshear vectors: (upu, upv, dnu, dnv).
    pub(crate) fn corfidi_raw(&self) -> (f64, f64, f64, f64) {
        if !self.has_wind {
            return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        }
        let (mnu1, mnv1) = if self.psfc() < 850. {
            self.mean_wind_npw_raw(self.psfc(), 300., 0., 0.)
        } else {
            self.mean_wind_npw_raw(850., 300., 0., 0.)
        };
        let p15 = self.p_agl(1500.);
        let (mnu2, mnv2) = self.mean_wind_npw_raw(self.psfc(), p15, 0., 0.);
        let upu = mnu1 - mnu2;
        let upv = mnv1 - mnv2;
        (upu, upv, mnu1 + upu, mnv1 + upv)
    }

    /// Critical angle (deg) between the 0-500 m shear vector and the
    /// surface storm-relative inflow.
    pub(crate) fn critical_angle_raw(&self, stu: f64, stv: f64) -> f64 {
        if !self.has_wind || stu.is_nan() || stv.is_nan() {
            return f64::NAN;
        }
        let p500 = self.p_agl(500.);
        let (u500, v500) = self.components(p500);
        let (su, sv) = self.components(self.psfc());
        let (v1u, v1v) = (u500 - su, v500 - sv);
        let (v2u, v2v) = (stu - su, stv - sv);
        let m1 = (sq(v1u) + sq(v1v)).sqrt();
        let m2 = (sq(v2u) + sq(v2v)).sqrt();
        let dot = v1u * v2u + v1v * v2v;
        (dot / (m1 * m2)).acos().to_degrees()
    }
}

// --------------------------------------------------------------------------
// public wrappers
// --------------------------------------------------------------------------
impl Profile {
    /// Pressure-weighted mean wind (kt) in a pressure layer.
    pub fn mean_wind(&self, pbot: f64, ptop: f64) -> Option<Vector> {
        let (u, v) = self.mean_wind_raw(pbot, ptop, 0., 0.);
        Vector::from_nan(u, v)
    }
    /// Non-pressure-weighted mean wind (kt).
    pub fn mean_wind_npw(&self, pbot: f64, ptop: f64) -> Option<Vector> {
        let (u, v) = self.mean_wind_npw_raw(pbot, ptop, 0., 0.);
        Vector::from_nan(u, v)
    }
    /// Storm-relative pressure-weighted mean wind (kt).
    pub fn sr_wind(&self, pbot: f64, ptop: f64, storm: Vector) -> Option<Vector> {
        let (u, v) = self.mean_wind_raw(pbot, ptop, storm.u, storm.v);
        Vector::from_nan(u, v)
    }
    /// Bulk shear vector (kt) between two pressure levels.
    pub fn wind_shear(&self, pbot: f64, ptop: f64) -> Option<Vector> {
        let (u, v) = self.wind_shear_raw(pbot, ptop);
        Vector::from_nan(u, v)
    }
    /// Storm-relative helicity between heights AGL (m).
    pub fn helicity(&self, lower_agl: f64, upper_agl: f64, storm: Vector) -> Option<Helicity> {
        let (t, p, n) = self.helicity_raw(lower_agl, upper_agl, storm.u, storm.v);
        if t.is_nan() {
            None
        } else {
            Some(Helicity { total: t, positive: p, negative: n })
        }
    }
    /// Critical angle (deg) for a storm motion.
    pub fn critical_angle(&self, storm: Vector) -> Option<f64> {
        opt(self.critical_angle_raw(storm.u, storm.v))
    }
    /// Corfidi MCS motion vectors.
    pub fn corfidi(&self) -> Option<Corfidi> {
        let (a, b, c, d) = self.corfidi_raw();
        Some(Corfidi { upshear: Vector::from_nan(a, b)?, downshear: Vector::from_nan(c, d)? })
    }
    /// Non-parcel Bunkers storm motion.
    pub fn bunkers_non_parcel(&self) -> Option<StormMotion> {
        let (a, b, c, d) = self.non_parcel_bunkers_raw();
        Some(StormMotion { right: Vector::from_nan(a, b)?, left: Vector::from_nan(c, d)? })
    }
    /// SHARPpy's experimental non-parcel Bunkers motion.
    pub fn bunkers_experimental(&self) -> Option<StormMotion> {
        let (a, b, c, d) = self.non_parcel_bunkers_experimental_raw();
        Some(StormMotion { right: Vector::from_nan(a, b)?, left: Vector::from_nan(c, d)? })
    }
    /// Maximum wind between two heights AGL: (u, v, pressure). The pressure
    /// reproduces SHARPpy's layer-relative indexing quirk.
    pub fn max_wind(&self, lower_agl: f64, upper_agl: f64) -> (Option<f64>, Option<f64>, Option<f64>) {
        let (u, v, p) = self.max_wind_raw(lower_agl, upper_agl);
        (opt(u), opt(v), opt(p))
    }
    /// Storm-relative wind speed (kt) at every level with wind, paired with
    /// height AGL (m), for the SR-wind-vs-height panel.
    pub fn sr_wind_profile(&self, storm: Vector) -> Vec<(f64, f64)> {
        (0..self.len())
            .filter(|&i| !self.u[i].is_nan() && !self.hght[i].is_nan())
            .map(|i| (self.agl(self.hght[i]), mag(self.u[i] - storm.u, self.v[i] - storm.v)))
            .collect()
    }
    /// Wind speed (kt) at every level with wind, paired with height AGL (m).
    pub fn wind_speed_profile(&self) -> Vec<(f64, f64)> {
        (0..self.len())
            .filter(|&i| !self.u[i].is_nan() && !self.hght[i].is_nan())
            .map(|i| (self.agl(self.hght[i]), mag(self.u[i], self.v[i])))
            .collect()
    }
}
