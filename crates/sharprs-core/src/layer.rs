//! Layer-averaged and level-finding quantities (SHARPpy `params.mean_*`,
//! `precip_water`, `lapse_rate`, `temp_lvl`, ...).
//!
//! SHARPpy has two integration schemes for layer means: the default
//! samples every 1 hPa from the bottom (`np.arange(pbot, ptop-1, -1)`) and
//! pressure-weights the samples; the "exact" scheme uses the bounding
//! interpolated values plus every observed level (each counted twice).
//! Both are reproduced here because different parameters use different
//! schemes.

use crate::numerics::{self, arange, masked_average, masked_mean, pairwise_sum, pow};
use crate::profile::{Field, Profile};
use crate::thermo;

/// Which of SHARPpy's two layer-integration schemes to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// 1 hPa sampling, pressure-weighted (SHARPpy default).
    Sampled,
    /// Observed levels plus interpolated bounds (SHARPpy `exact=True`).
    Exact,
}

#[inline]
fn falsy(x: Option<f64>) -> bool {
    // python `not x` for a number / masked / None
    match x {
        None => true,
        Some(v) => v == 0. || v.is_nan(),
    }
}

impl Profile {
    /// Resolve (pbot, ptop) the way the SHARPpy `mean_*` helpers do.
    /// Returns None when the top is not valid.
    fn layer_bounds(&self, pbot: Option<f64>, ptop: Option<f64>, check: Field) -> Option<(f64, f64)> {
        let mut pbot = if falsy(pbot) { self.psfc() } else { pbot.unwrap() };
        let ptop = if falsy(ptop) { self.psfc() - 100. } else { ptop.unwrap() };
        let f = |p: f64| match check {
            Field::Omega => self.omeg_at(p),
            _ => self.temp(p),
        };
        if f(pbot).is_nan() {
            pbot = self.psfc();
        }
        if f(ptop).is_nan() {
            return None;
        }
        Some((pbot, ptop))
    }

    /// indices of observed levels strictly inside (ptop, pbot)
    fn inner_range(&self, pbot: f64, ptop: f64) -> std::ops::Range<usize> {
        let ind1 = self.pres.iter().position(|&p| pbot > p);
        let ind2 = self.pres.iter().rposition(|&p| ptop < p);
        match (ind1, ind2) {
            (Some(a), Some(b)) if b + 1 > a => a..b + 1,
            _ => 0..0,
        }
    }

    /// Mean potential temperature (°C) of a layer.
    pub(crate) fn mean_theta_raw(&self, pbot: Option<f64>, ptop: Option<f64>, scheme: Scheme) -> f64 {
        let Some((pbot, ptop)) = self.layer_bounds(pbot, ptop, Field::Temperature) else {
            return f64::NAN;
        };
        match scheme {
            Scheme::Exact => {
                let r = self.inner_range(pbot, ptop);
                let theta1 = thermo::theta(pbot, self.temp(pbot), 1000.);
                let theta2 = thermo::theta(ptop, self.temp(ptop), 1000.);
                let mid: Vec<f64> = r
                    .map(|i| thermo::theta(self.pres[i], self.tmpc[i], 1000.))
                    .filter(|x| !x.is_nan())
                    .collect();
                let mut v = Vec::with_capacity(2 * mid.len() + 2);
                v.push(theta1);
                v.extend_from_slice(&mid);
                v.extend_from_slice(&mid);
                v.push(theta2);
                let tott = pairwise_sum(&v) / 2.;
                let num = v.len() as f64 / 2.;
                tott / num
            }
            Scheme::Sampled => {
                let p = arange(pbot, ptop - 1., -1.);
                let t = self.many(Field::Temperature, &p);
                let th: Vec<f64> = p.iter().zip(&t).map(|(&p, &t)| thermo::theta(p, t, 1000.)).collect();
                masked_average(&th, &p)
            }
        }
    }

    /// Mean mixing ratio (g/kg) of a layer.
    pub(crate) fn mean_mixratio_raw(&self, pbot: Option<f64>, ptop: Option<f64>, scheme: Scheme) -> f64 {
        let Some((pbot, ptop)) = self.layer_bounds(pbot, ptop, Field::Temperature) else {
            return f64::NAN;
        };
        match scheme {
            Scheme::Exact => {
                let r = self.inner_range(pbot, ptop);
                let dw1 = self.dwpt(pbot);
                let dw2 = self.dwpt(ptop);
                let (mut dm, mut pm) = (Vec::new(), Vec::new());
                for i in r {
                    if !self.dwpc[i].is_nan() && !self.pres[i].is_nan() {
                        dm.push(self.dwpc[i]);
                        pm.push(self.pres[i]);
                    }
                }
                let mut d = vec![dw1];
                d.extend_from_slice(&dm);
                d.extend_from_slice(&dm);
                d.push(dw2);
                let mut p = vec![pbot];
                p.extend_from_slice(&pm);
                p.extend_from_slice(&pm);
                p.push(ptop);
                let totd = pairwise_sum(&d) / 2.;
                let totp = pairwise_sum(&p) / 2.;
                let num = d.len() as f64 / 2.;
                thermo::mixratio(totp / num, totd / num)
            }
            Scheme::Sampled => {
                if let Some((g, n)) = self.sfc_prefix(pbot, ptop) {
                    return masked_mean(&g.mixing_ratio(self)[..n]);
                }
                let p = arange(pbot, ptop - 1., -1.);
                let d = self.many(Field::Dewpoint, &p);
                let w: Vec<f64> = p.iter().zip(&d).map(|(&p, &d)| thermo::mixratio(p, d)).collect();
                masked_mean(&w)
            }
        }
    }

    /// Mean relative humidity (%) of a layer (sampled scheme only, as in
    /// SHARPpy; its exact branch is broken).
    pub(crate) fn mean_relh_raw(&self, pbot: Option<f64>, ptop: Option<f64>) -> f64 {
        let Some((pbot, ptop)) = self.layer_bounds(pbot, ptop, Field::Temperature) else {
            return f64::NAN;
        };
        if let Some((g, n)) = self.sfc_prefix(pbot, ptop) {
            let t = g.tmpc(self);
            let d = g.dwpc(self);
            let rh: Vec<_> = (0..n).map(|i| thermo::relh(g.p[i], t[i], d[i])).collect();
            return masked_average(&rh, &g.p[..n]);
        }
        let p = arange(pbot, ptop - 1., -1.);
        let t = self.many(Field::Temperature, &p);
        let d = self.many(Field::Dewpoint, &p);
        let rh: Vec<_> = (0..p.len()).map(|i| thermo::relh(p[i], t[i], d[i])).collect();
        masked_average(&rh, &p)
    }

    /// Mean theta-e (K) of a layer, sampled scheme.
    pub(crate) fn mean_thetae_raw(&self, pbot: Option<f64>, ptop: Option<f64>) -> f64 {
        let Some((pbot, ptop)) = self.layer_bounds(pbot, ptop, Field::Temperature) else {
            return f64::NAN;
        };
        let p = arange(pbot, ptop - 1., -1.);
        let te = self.many(Field::ThetaE, &p);
        masked_average(&te, &p)
    }

    /// Mean omega (Pa/s) of a layer; `None`-equivalent NaN when no omega.
    pub(crate) fn mean_omega_raw(&self, pbot: Option<f64>, ptop: Option<f64>) -> f64 {
        if !self.has_omega() {
            return f64::NAN;
        }
        let Some((pbot, ptop)) = self.layer_bounds(pbot, ptop, Field::Omega) else {
            return f64::NAN;
        };
        let p = arange(pbot, ptop - 1., -1.);
        let o = self.many(Field::Omega, &p);
        masked_average(&o, &p)
    }

    /// Precipitable water (inches) between `pbot` (default surface) and
    /// `ptop` (clipped to the top of the data), 1 hPa trapezoids.
    pub(crate) fn precip_water_raw(&self, pbot: Option<f64>, ptop: f64) -> f64 {
        let pbot = if falsy(pbot) { self.psfc() } else { pbot.unwrap() };
        let mut ptop = ptop;
        if self.plast() > ptop {
            ptop = self.plast();
        }
        let owned;
        let (p, w) = if let Some((g, n)) = self.sfc_prefix(pbot, ptop) {
            (&g.p[..n], &g.mixing_ratio(self)[..n])
        } else {
            let p = arange(pbot, ptop - 1., -1.);
            let d = self.many(Field::Dewpoint, &p);
            let w: Vec<_> = p.iter().zip(d).map(|(&p, d)| thermo::mixratio(p, d)).collect();
            owned = (p, w);
            (&owned.0[..], &owned.1[..])
        };
        if p.len() < 2 {
            return 0.;
        }
        let terms: Vec<_> = (1..p.len()).map(|i| {
            ((w[i - 1] + w[i]) / 2. * (p[i - 1] - p[i])) * 0.00040173
        }).collect();
        numerics::masked_sum(&terms)
    }

    /// Virtual-temperature lapse rate (°C/km) between two pressure levels
    /// (`pres = true`) or two heights AGL.
    pub(crate) fn lapse_rate_raw(&self, lower: f64, upper: f64, pres: bool) -> f64 {
        let (p1, p2, z1, z2);
        if pres {
            if self.plast() > upper {
                return f64::NAN;
            }
            p1 = lower;
            p2 = upper;
            z1 = self.hght(lower);
            z2 = self.hght(upper);
        } else {
            z1 = self.msl(lower);
            z2 = self.msl(upper);
            p1 = self.pres(z1);
            p2 = self.pres(z2);
        }
        let tv1 = self.vtmp_at(p1);
        let tv2 = self.vtmp_at(p2);
        (tv2 - tv1) / (z2 - z1) * -1000.
    }

    /// Maximum 2 km-deep lapse rate between 2 and 6 km AGL, 250 m steps.
    /// Returns (lapse rate, pbot, ptop).
    pub(crate) fn max_lapse_rate_raw(&self) -> (f64, f64, f64) {
        let (lower, upper, interval, depth) = (2000., 6000., 250., 2000.);
        let bot: Vec<f64> = arange(lower, upper - depth + interval, interval).iter().map(|&h| self.msl(h)).collect();
        let top: Vec<f64> = arange(lower + depth, upper + interval, interval).iter().map(|&h| self.msl(h)).collect();
        let bp = self.pres_many(&bot);
        let tp = self.pres_many(&top);
        let tvt = self.many(Field::VirtualTemperature, &tp);
        let tvb = self.many(Field::VirtualTemperature, &bp);
        let lr: Vec<f64> = tvt.iter().zip(&tvb).map(|(a, b)| (a - b) * -1000.).collect();
        match numerics::masked_argmax(&lr) {
            Some(i) => (lr[i] / depth, bp[i], tp[i]),
            None => (f64::NAN, f64::NAN, f64::NAN),
        }
    }

    /// Pressure (hPa) of the first level (from the bottom) where the
    /// temperature (or wet-bulb) profile crosses `temp`.
    pub(crate) fn temp_lvl_raw(&self, temp: f64, wetbulb: bool) -> f64 {
        let prof = if wetbulb { &self.wetbulb } else { &self.tmpc };
        if let Some(i) = prof.iter().position(|t| t - temp == 0.) {
            return self.pres[i];
        }
        let mut previous = None;
        for (&t, &lp) in prof.iter().zip(&self.logp) {
            let diff = t - temp;
            if diff.is_nan() || lp.is_nan() {
                continue;
            }
            if let Some((prev_t, prev_lp, prev_diff)) = previous {
                if diff * prev_diff < 0. {
                    return pow(10., numerics::interp_clamped(temp, &[t, prev_t], &[lp, prev_lp]));
                }
            }
            previous = Some((t, lp, diff));
        }
        f64::NAN
    }

    /// Forecast maximum temperature (°C): mix down from 100 hPa above the
    /// surface dry-adiabatically and add 2 K.
    pub(crate) fn max_temp_raw(&self) -> f64 {
        let mixlayer = self.psfc() - 100.;
        let temp = thermo::ctok(self.temp(mixlayer)) + 2.;
        thermo::ktoc(temp * pow(self.psfc() / mixlayer, thermo::ROCP))
    }

    /// Pressure of the most-unstable level (max theta-w) between pbot and
    /// ptop, sampled every 1 hPa.
    pub(crate) fn most_unstable_level_raw(&self, pbot: Option<f64>, ptop: Option<f64>) -> f64 {
        let mut pbot = if falsy(pbot) { self.psfc() } else { pbot.unwrap() };
        let ptop = if falsy(ptop) { self.psfc() - 400. } else { ptop.unwrap() };
        if self.temp(pbot).is_nan() {
            pbot = self.psfc();
        }
        if self.temp(ptop).is_nan() {
            return f64::NAN;
        }
        if let Some((g, n)) = self.sfc_prefix(pbot, ptop) {
            let estimates = g.mu_estimates(self);
            if n <= estimates.len() {
                let estimates = &estimates[..n];
                let best = estimates.iter().copied().fold(f64::NAN, f64::max);
                let t = g.tmpc(self);
                let d = g.dwpc(self);
                // Allow for roundoff from the inverse dry lift and for ties at
                // the reference tolerance. Only the exact lifts decide the level.
                let mut candidates = Vec::new();
                for (i, &estimate) in estimates.iter().enumerate() {
                    if estimate >= best - 1e-5 {
                        candidates.push((g.p[i], mu_value(g.p[i], t[i], d[i])));
                    }
                }
                let max = candidates.iter().fold(f64::NAN, |max, &(_, v)| max.max(v));
                return candidates.into_iter().find(|&(_, v)| (v - max).abs() < crate::units::TOL)
                    .map_or(f64::NAN, |(p, _)| p);
            }
        }
        let p = arange(pbot, ptop - 1., -1.);
        let t = self.many(Field::Temperature, &p);
        let d = self.many(Field::Dewpoint, &p);
        let mt: Vec<f64> = (0..p.len()).map(|i| mu_value(p[i], t[i], d[i])).collect();
        let max = mt.iter().copied().filter(|x| !x.is_nan()).fold(f64::NAN, f64::max);
        match mt.iter().position(|&m| (m - max).abs() < crate::units::TOL) {
            Some(i) => p[i],
            None => f64::NAN,
        }
    }

    /// Top of the PBL (hPa): first level where theta-v exceeds the surface
    /// theta-v by 0.5 K.
    pub(crate) fn pbl_top_raw(&self) -> f64 {
        let thetav = |i: usize| {
            if self.dwpc[i].is_nan() { f64::NAN } else { thermo::theta_k(self.vtmp[i], self.rocp_inv[i]) }
        };
        let s = thetav(self.sfc) + 0.5;
        // SHARPpy quirk: `np.where` on the masked theta-v array also sees the
        // raw data behind masked entries; where only the dewpoint is missing
        // that data is the temperature in K (~250-310), which always exceeds
        // the surface theta-v in °C, so such a level becomes the PBL top.
        let level = (0..self.len())
            .position(|i| {
                let masked_td = self.dwpc[i].is_nan() && !self.tmpc[i].is_nan() && !self.pres[i].is_nan();
                (masked_td && s < thermo::ctok(self.tmpc[i])) || s < thetav(i)
            })
            .unwrap_or(self.len() - 1);
        self.pres[level]
    }

    /// (pbot, ptop) bounding temperatures `tbot` and `ttop`, defaulting to the
    /// surface pressure where not found (SHARPpy `dgz`/`hgz`).
    pub(crate) fn temp_layer(&self, tbot: f64, ttop: f64) -> (f64, f64) {
        let mut pbot = self.temp_lvl_raw(tbot, false);
        let mut ptop = self.temp_lvl_raw(ttop, false);
        if pbot.is_nan() {
            pbot = self.psfc();
        }
        if ptop.is_nan() {
            ptop = self.psfc();
        }
        (pbot, ptop)
    }
}

/// Value maximised by the most-unstable-level search.
/// (`wetlift(.., 1000)` never iterates: satlift short-circuits at 1000 hPa.)
#[inline]
pub(crate) fn mu_value(p: f64, t: f64, td: f64) -> f64 {
    let (p2, t2) = thermo::drylift(p, t, td);
    thermo::wetlift(p2, t2, 1000.)
}

/// Potential temperature is conserved on the dry ascent to the LCL. For
/// ranking candidates this cancels the pressure exponent and its inverse.
pub(crate) fn mu_estimate(p: f64, t: f64, td: f64) -> f64 {
    let theta = thermo::theta(p, t, 1000.);
    let lcl = thermo::lcltemp(t, td);
    if (-100.0..=100.0).contains(&theta) && (-100.0..=60.0).contains(&lcl) {
        theta - thermo::wobus_estimate(theta) + thermo::wobus_estimate(lcl)
    } else {
        mu_value(p, t, td)
    }
}

/// Array-style `virtemp` (masked when the dewpoint is masked; no fallback).
#[inline]
pub(crate) fn virtemp_masked(p: f64, t: f64, td: f64) -> f64 {
    if td.is_nan() || t.is_nan() || p.is_nan() {
        return f64::NAN;
    }
    thermo::virtemp(p, t, td)
}

// --------------------------------------------------------------------------
// Public, Option-returning wrappers
// --------------------------------------------------------------------------
use crate::profile::opt;

impl Profile {
    /// Mean potential temperature (°C) of the layer; defaults are the
    /// lowest 100 hPa.
    pub fn mean_theta(&self, pbot: Option<f64>, ptop: Option<f64>, scheme: Scheme) -> Option<f64> {
        opt(self.mean_theta_raw(pbot, ptop, scheme))
    }
    /// Mean mixing ratio (g/kg).
    pub fn mean_mixratio(&self, pbot: Option<f64>, ptop: Option<f64>, scheme: Scheme) -> Option<f64> {
        opt(self.mean_mixratio_raw(pbot, ptop, scheme))
    }
    /// Pressure-weighted mean RH (%).
    pub fn mean_relh(&self, pbot: Option<f64>, ptop: Option<f64>) -> Option<f64> {
        opt(self.mean_relh_raw(pbot, ptop))
    }
    /// Pressure-weighted mean theta-e (K).
    pub fn mean_thetae(&self, pbot: Option<f64>, ptop: Option<f64>) -> Option<f64> {
        opt(self.mean_thetae_raw(pbot, ptop))
    }
    /// Pressure-weighted mean omega (Pa/s).
    pub fn mean_omega(&self, pbot: Option<f64>, ptop: Option<f64>) -> Option<f64> {
        opt(self.mean_omega_raw(pbot, ptop))
    }
    /// Precipitable water (in) from `pbot` (default sfc) to `ptop`.
    pub fn precip_water(&self, pbot: Option<f64>, ptop: f64) -> Option<f64> {
        opt(self.precip_water_raw(pbot, ptop))
    }
    /// Lapse rate (°C/km) between pressures (`pres=true`) or heights AGL.
    pub fn lapse_rate(&self, lower: f64, upper: f64, pres: bool) -> Option<f64> {
        opt(self.lapse_rate_raw(lower, upper, pres))
    }
    /// Pressure where the temperature (or wet-bulb) first equals `temp`.
    pub fn temp_level(&self, temp: f64, wetbulb: bool) -> Option<f64> {
        opt(self.temp_lvl_raw(temp, wetbulb))
    }
    /// Most-unstable parcel level between pbot and ptop.
    pub fn most_unstable_level(&self, pbot: Option<f64>, ptop: Option<f64>) -> Option<f64> {
        opt(self.most_unstable_level_raw(pbot, ptop))
    }
}
