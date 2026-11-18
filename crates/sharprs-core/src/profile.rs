//! Sounding input columns and the validated [`Profile`] with SHARPpy's
//! derived per-level arrays and log-pressure interpolation.

use crate::numerics::{self, arange, arange_len, pow};
use crate::thermo;
use crate::units::{comp2vec, vec2comp};
use std::sync::{Mutex, OnceLock};

/// A calendar date/time (UTC). Only the month is used by the physics (PW
/// climatology); the rest is carried for callers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

/// Sounding metadata.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Metadata {
    /// Station identifier (3-letter, 4-letter ICAO or 5-digit WMO). Used for
    /// the PW climatology lookup.
    pub station: Option<String>,
    /// Latitude in degrees. Negative selects Southern-Hemisphere handling
    /// (left-mover based STP/SCP, sign flips) exactly as SHARPpy does; `None`
    /// behaves like SHARPpy's masked latitude (Northern Hemisphere logic,
    /// inferred temperature advection unavailable).
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Station elevation (m). Informational only.
    pub elevation: Option<f64>,
    /// Valid time. Its month selects the PW climatology column.
    pub valid: Option<DateTime>,
}

/// Wind input columns.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindColumns {
    /// Direction (deg) and speed (kt).
    Polar { wdir: Vec<Option<f64>>, wspd: Vec<Option<f64>> },
    /// u / v components (kt).
    Components { u: Vec<Option<f64>>, v: Vec<Option<f64>> },
    /// No wind data at all.
    None,
}

/// Plain-column sounding, the input to [`Profile::new`].
///
/// Units: `pres` hPa, `hght` m MSL, `tmpc`/`dwpc` °C, winds kt,
/// `omeg` **Pa/s** (the BUFKIT `OMEG` convention SHARPpy assumes: it
/// multiplies by 10 to display µb/s and by 36 to display hPa/hr).
///
/// Levels are ordered from the bottom up (decreasing pressure). Missing
/// values are `None`; levels below ground with missing temperature are fine
/// (SHARPpy keeps them and they still participate in height interpolation).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sounding {
    pub pres: Vec<Option<f64>>,
    pub hght: Vec<Option<f64>>,
    pub tmpc: Vec<Option<f64>>,
    pub dwpc: Vec<Option<f64>>,
    pub wind: WindColumns,
    pub omeg: Option<Vec<Option<f64>>>,
    pub meta: Metadata,
}

/// Convert a raw column using a sentinel (e.g. -9999) and/or NaN for missing.
pub fn column_from_raw(raw: &[f64], missing: f64) -> Vec<Option<f64>> {
    raw.iter()
        .map(|&x| if x.is_finite() && x != missing { Some(x) } else { None })
        .collect()
}

impl Sounding {
    /// Sounding from thermodynamic columns; add winds with
    /// [`Sounding::with_polar_wind`] / [`Sounding::with_component_wind`].
    pub fn new(
        pres: Vec<Option<f64>>,
        hght: Vec<Option<f64>>,
        tmpc: Vec<Option<f64>>,
        dwpc: Vec<Option<f64>>,
    ) -> Self {
        Self { pres, hght, tmpc, dwpc, wind: WindColumns::None, omeg: None, meta: Metadata::default() }
    }

    /// Convenience constructor from raw `f64` columns where `missing`
    /// (typically -9999) or any non-finite value means missing.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        pres: &[f64],
        hght: &[f64],
        tmpc: &[f64],
        dwpc: &[f64],
        wdir: &[f64],
        wspd: &[f64],
        missing: f64,
    ) -> Self {
        Self::new(
            column_from_raw(pres, missing),
            column_from_raw(hght, missing),
            column_from_raw(tmpc, missing),
            column_from_raw(dwpc, missing),
        )
        .with_polar_wind(column_from_raw(wdir, missing), column_from_raw(wspd, missing))
    }

    pub fn with_polar_wind(mut self, wdir: Vec<Option<f64>>, wspd: Vec<Option<f64>>) -> Self {
        self.wind = WindColumns::Polar { wdir, wspd };
        self
    }
    pub fn with_component_wind(mut self, u: Vec<Option<f64>>, v: Vec<Option<f64>>) -> Self {
        self.wind = WindColumns::Components { u, v };
        self
    }
    /// Omega column in Pa/s.
    pub fn with_omega(mut self, omeg: Vec<Option<f64>>) -> Self {
        self.omeg = Some(omeg);
        self
    }
    pub fn with_meta(mut self, meta: Metadata) -> Self {
        self.meta = meta;
        self
    }
}

/// Why a sounding could not be turned into a [`Profile`].
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileError {
    /// Column lengths differ.
    LengthMismatch { column: &'static str, expected: usize, found: usize },
    /// Fewer than two levels.
    TooShort,
    /// No level has a valid temperature.
    NoTemperature,
    /// SHARPpy's ConvectiveProfile requires data below 100 hPa.
    PressureTooLow,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::LengthMismatch { column, expected, found } => {
                write!(f, "column `{column}` has {found} levels, expected {expected}")
            }
            ProfileError::TooShort => write!(f, "a sounding needs at least two levels"),
            ProfileError::NoTemperature => write!(f, "no level has a valid temperature"),
            ProfileError::PressureTooLow => {
                write!(f, "the maximum pressure must exceed 100 hPa")
            }
        }
    }
}
impl std::error::Error for ProfileError {}

/// Pre-filtered interpolation table: `xp` ascending (log10 p, or height),
/// `fp` the field, both restricted to levels where both are valid (exactly
/// SHARPpy's `generic_interp_*` masking).
#[derive(Clone, Debug, Default)]
pub(crate) struct Table {
    pub xp: Vec<f64>,
    pub fp: Vec<f64>,
}

impl Table {
    fn by_logp(logp: &[f64], field: &[f64]) -> Self {
        // SHARPpy interpolates on the reversed (bottom-up -> top-down) arrays
        let mut xp = Vec::with_capacity(logp.len());
        let mut fp = Vec::with_capacity(logp.len());
        for i in (0..logp.len()).rev() {
            if !logp[i].is_nan() && !field[i].is_nan() {
                xp.push(logp[i]);
                fp.push(field[i]);
            }
        }
        Self { xp, fp }
    }
    fn by_hght(hght: &[f64], field: &[f64]) -> Self {
        let mut xp = Vec::with_capacity(hght.len());
        let mut fp = Vec::with_capacity(hght.len());
        for i in 0..hght.len() {
            if !hght[i].is_nan() && !field[i].is_nan() {
                xp.push(hght[i]);
                fp.push(field[i]);
            }
        }
        Self { xp, fp }
    }
    #[inline]
    pub fn at(&self, x: f64) -> f64 {
        if self.xp.is_empty() {
            return f64::NAN;
        }
        numerics::interp(x, &self.xp, &self.fp)
    }
    pub fn at_many(&self, xs: &[f64]) -> Vec<f64> {
        if self.xp.is_empty() {
            return vec![f64::NAN; xs.len()];
        }
        numerics::interp_many(xs, &self.xp, &self.fp)
    }
    /// `(self.at_many(xs), other.at_many(xs))`, sharing the search when both
    /// tables have the same abscissae.
    pub fn at_many2(&self, other: &Table, xs: &[f64]) -> (Vec<f64>, Vec<f64>) {
        if self.xp.is_empty() || !same_bits(&self.xp, &other.xp) {
            return (self.at_many(xs), other.at_many(xs));
        }
        let [a, b] = numerics::interp_many_n(xs, &self.xp, [&self.fp, &other.fp]);
        (a, b)
    }
}

fn same_bits(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// The 1 hPa sampling grid from the surface, `arange(psfc, plast - 1, -1)`.
/// Every SHARPpy layer computation that samples from the surface uses a
/// prefix of exactly this array (`np.arange` builds element `i` from
/// `start` and `step` only), and the vectorised interpolation of a prefix
/// equals the prefix of the vectorised interpolation (the search guess is
/// carried forward only), so interpolated columns are shared.
#[derive(Clone, Debug, Default)]
pub(crate) struct SfcGrid {
    pub p: Vec<f64>,
    pub lp: Vec<f64>,
    uv: OnceLock<(Vec<f64>, Vec<f64>)>,
    tmpc: OnceLock<Vec<f64>>,
    dwpc: OnceLock<Vec<f64>>,
    hght: OnceLock<Vec<f64>>,
    /// Environmental term of the sub-LCL CIN integrand:
    /// `virtemp_masked(p, theta(p, tmpc, 1000), dwpc)`.
    dry_env: OnceLock<Vec<f64>>,
    /// Most-unstable-level search value `wetlift(drylift(p, t, td), 1000)`
    /// over the lowest 500 hPa (the deepest search SHARPpy does).
    mu_tw: OnceLock<Vec<f64>>,
}

/// Lazily filled intermediates shared between computations on one profile
/// (pure functions of the profile data, so caching cannot change results).
#[derive(Default)]
pub(crate) struct Cache {
    sfc: OnceLock<SfcGrid>,
    /// Memoised pressure-weighted mean winds: (pbot bits, ptop bits, u, v).
    mean_wind: Mutex<Vec<(u64, u64, f64, f64)>>,
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        let mw = self.mean_wind.lock().map(|m| m.clone()).unwrap_or_default();
        Cache { sfc: self.sfc.clone(), mean_wind: Mutex::new(mw) }
    }
}

const MEAN_WIND_MEMO_MAX: usize = 96;

#[derive(Clone, Debug, Default)]
pub(crate) struct Tables {
    pub hght: Table,
    pub tmpc: Table,
    pub dwpc: Table,
    pub vtmp: Table,
    pub u: Table,
    pub v: Table,
    pub omeg: Table,
    pub thetae: Table,
    pub theta: Table,
    pub wetbulb: Table,
    pub wvmr: Table,
    pub pres_by_hght: Table,
}

/// Interpolatable fields of a [`Profile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Height,
    Temperature,
    Dewpoint,
    VirtualTemperature,
    U,
    V,
    Omega,
    /// Equivalent potential temperature, **K** (SHARPpy keeps it in K).
    ThetaE,
    /// Potential temperature, **K**.
    Theta,
    WetBulb,
    MixingRatio,
}

/// A validated sounding with SHARPpy's derived per-level arrays.
///
/// Internally missing values are NaN; every public accessor returns
/// `Option<f64>`.
#[derive(Clone)]
pub struct Profile {
    pub(crate) pres: Vec<f64>,
    pub(crate) hght: Vec<f64>,
    pub(crate) tmpc: Vec<f64>,
    pub(crate) dwpc: Vec<f64>,
    pub(crate) wdir: Vec<f64>,
    pub(crate) wspd: Vec<f64>,
    pub(crate) u: Vec<f64>,
    pub(crate) v: Vec<f64>,
    pub(crate) omeg: Vec<f64>,
    pub(crate) has_wind: bool,
    pub(crate) logp: Vec<f64>,
    pub(crate) vtmp: Vec<f64>,
    pub(crate) wetbulb: Vec<f64>,
    /// K
    pub(crate) thetae: Vec<f64>,
    /// K
    pub(crate) theta: Vec<f64>,
    pub(crate) wvmr: Vec<f64>,
    pub(crate) relh: Vec<f64>,
    pub(crate) sfc: usize,
    pub(crate) top: usize,
    pub(crate) meta: Metadata,
    pub(crate) t: Tables,
    /// `pow(1000 / pres[i], ROCP)` (theta factor) per level.
    pub(crate) rocp_inv: Vec<f64>,
    /// `pow(pres[i] / 1000, ROCP)` (`satlift` factor) per level.
    pub(crate) rocp_fwd: Vec<f64>,
    pub(crate) cache: Cache,
}

impl std::fmt::Debug for Profile {
    // the derived format, without the (derived-data) cache
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("pres", &self.pres)
            .field("hght", &self.hght)
            .field("tmpc", &self.tmpc)
            .field("dwpc", &self.dwpc)
            .field("wdir", &self.wdir)
            .field("wspd", &self.wspd)
            .field("u", &self.u)
            .field("v", &self.v)
            .field("omeg", &self.omeg)
            .field("has_wind", &self.has_wind)
            .field("logp", &self.logp)
            .field("vtmp", &self.vtmp)
            .field("wetbulb", &self.wetbulb)
            .field("thetae", &self.thetae)
            .field("theta", &self.theta)
            .field("wvmr", &self.wvmr)
            .field("relh", &self.relh)
            .field("sfc", &self.sfc)
            .field("top", &self.top)
            .field("meta", &self.meta)
            .field("t", &self.t)
            .finish()
    }
}

fn to_nan(c: &[Option<f64>]) -> Vec<f64> {
    c.iter().map(|x| x.unwrap_or(f64::NAN)).collect()
}

/// One level of a profile, for iteration by renderers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub pres: Option<f64>,
    pub hght: Option<f64>,
    pub tmpc: Option<f64>,
    pub dwpc: Option<f64>,
    pub wdir: Option<f64>,
    pub wspd: Option<f64>,
    pub u: Option<f64>,
    pub v: Option<f64>,
    /// Pa/s
    pub omeg: Option<f64>,
    pub vtmp: Option<f64>,
    pub wetbulb: Option<f64>,
    /// K
    pub thetae: Option<f64>,
    /// K
    pub theta: Option<f64>,
    pub wvmr: Option<f64>,
    pub relh: Option<f64>,
}

#[inline]
pub(crate) fn opt(x: f64) -> Option<f64> {
    if x.is_nan() { None } else { Some(x) }
}

impl Profile {
    /// Validate a sounding and compute the derived arrays (what SHARPpy's
    /// `BasicProfile` constructor does).
    pub fn new(s: &Sounding) -> Result<Self, ProfileError> {
        let n = s.pres.len();
        let check = |name: &'static str, len: usize| {
            if len != n {
                Err(ProfileError::LengthMismatch { column: name, expected: n, found: len })
            } else {
                Ok(())
            }
        };
        check("hght", s.hght.len())?;
        check("tmpc", s.tmpc.len())?;
        check("dwpc", s.dwpc.len())?;
        if n < 2 {
            return Err(ProfileError::TooShort);
        }
        let pres = to_nan(&s.pres);
        let hght = to_nan(&s.hght);
        let tmpc = to_nan(&s.tmpc);
        let dwpc = to_nan(&s.dwpc);
        let (wdir, wspd, u, v, has_wind) = match &s.wind {
            WindColumns::Polar { wdir, wspd } => {
                check("wdir", wdir.len())?;
                check("wspd", wspd.len())?;
                let mut d = to_nan(wdir);
                let mut sp = to_nan(wspd);
                let mut u = vec![f64::NAN; n];
                let mut v = vec![f64::NAN; n];
                for i in 0..n {
                    if d[i].is_nan() || sp[i].is_nan() {
                        d[i] = f64::NAN;
                        sp[i] = f64::NAN;
                    } else {
                        (u[i], v[i]) = vec2comp(d[i], sp[i]);
                    }
                }
                let any = d.iter().any(|x| !x.is_nan());
                (d, sp, u, v, any)
            }
            WindColumns::Components { u, v } => {
                check("u", u.len())?;
                check("v", v.len())?;
                let mut uu = to_nan(u);
                let mut vv = to_nan(v);
                let mut d = vec![f64::NAN; n];
                let mut sp = vec![f64::NAN; n];
                for i in 0..n {
                    if uu[i].is_nan() || vv[i].is_nan() {
                        uu[i] = f64::NAN;
                        vv[i] = f64::NAN;
                    } else {
                        (d[i], sp[i]) = comp2vec(uu[i], vv[i]);
                    }
                }
                let any = d.iter().any(|x| !x.is_nan());
                (d, sp, uu, vv, any)
            }
            WindColumns::None => {
                let nan = vec![f64::NAN; n];
                (nan.clone(), nan.clone(), nan.clone(), nan, false)
            }
        };
        let omeg = match &s.omeg {
            Some(o) => {
                check("omeg", o.len())?;
                to_nan(o)
            }
            None => vec![f64::NAN; n],
        };
        let maxp = pres.iter().copied().filter(|p| !p.is_nan()).fold(f64::NAN, f64::max);
        if maxp.is_nan() || maxp <= 100. {
            return Err(ProfileError::PressureTooLow);
        }
        let sfc = tmpc.iter().position(|t| !t.is_nan()).ok_or(ProfileError::NoTemperature)?;
        let top = tmpc.iter().rposition(|t| !t.is_nan()).ok_or(ProfileError::NoTemperature)?;

        let logp: Vec<f64> = pres.iter().map(|p| p.log10()).collect();
        // per-level pow factors reused by every theta / moist-lift step that
        // starts or ends on a level (bit-identical to computing them inline)
        let rocp_inv: Vec<f64> = pres.iter().map(|&p| pow(1000. / p, thermo::ROCP)).collect();
        let rocp_fwd: Vec<f64> = pres.iter().map(|&p| pow(p / 1000., thermo::ROCP)).collect();
        // thetae = theta(100, wetlift(lcl, tlcl, 100), 1000)
        let k_100 = pow(100. / 1000., thermo::ROCP);
        let k_10 = pow(1000. / 100., thermo::ROCP);
        let mut vtmp = vec![f64::NAN; n];
        let mut wetbulb = vec![f64::NAN; n];
        let mut thetae = vec![f64::NAN; n];
        let mut theta = vec![f64::NAN; n];
        let mut wvmr = vec![f64::NAN; n];
        let mut relh = vec![f64::NAN; n];
        for i in 0..n {
            let (p, t, td) = (pres[i], tmpc[i], dwpc[i]);
            let e_td = thermo::vappres(td);
            let mr = thermo::mixratio_e(p, td, e_td);
            // vtmp falls back to the temperature where the dewpoint is missing
            vtmp[i] = if p.is_nan() || t.is_nan() {
                f64::NAN
            } else if td.is_nan() {
                t
            } else {
                thermo::virtemp_w(t, 0.001 * mr)
            };
            if !(p.is_nan() || t.is_nan() || td.is_nan()) {
                // drylift + the common prefix of wetlift(lcl, tlcl, p) and
                // wetlift(lcl, tlcl, 100)
                let t2 = thermo::lcltemp(t, td);
                let p2 = thermo::thalvl(thermo::theta_k(t, rocp_inv[i]), t2);
                let thta = thermo::theta(p2, t2, 1000.);
                if !thta.is_nan() {
                    let thetam = thta - thermo::wobf(thta) + thermo::wobf(t2);
                    wetbulb[i] = thermo::satlift_k(p, thetam, rocp_fwd[i]);
                    thetae[i] = thermo::ctok(thermo::theta_k(thermo::satlift_k(100., thetam, k_100), k_10));
                }
                relh[i] = 100. * e_td / thermo::vappres(t);
            }
            if !(p.is_nan() || t.is_nan()) {
                theta[i] = thermo::ctok(thermo::theta_k(t, rocp_inv[i]));
            }
            if !(p.is_nan() || td.is_nan()) {
                wvmr[i] = mr;
            }
        }
        let t = Tables {
            hght: Table::by_logp(&logp, &hght),
            tmpc: Table::by_logp(&logp, &tmpc),
            dwpc: Table::by_logp(&logp, &dwpc),
            vtmp: Table::by_logp(&logp, &vtmp),
            u: Table::by_logp(&logp, &u),
            v: Table::by_logp(&logp, &v),
            omeg: Table::by_logp(&logp, &omeg),
            thetae: Table::by_logp(&logp, &thetae),
            theta: Table::by_logp(&logp, &theta),
            wetbulb: Table::by_logp(&logp, &wetbulb),
            wvmr: Table::by_logp(&logp, &wvmr),
            pres_by_hght: Table::by_hght(&hght, &logp),
        };
        Ok(Self {
            pres,
            hght,
            tmpc,
            dwpc,
            wdir,
            wspd,
            u,
            v,
            omeg,
            has_wind,
            logp,
            vtmp,
            wetbulb,
            thetae,
            theta,
            wvmr,
            relh,
            sfc,
            top,
            meta: s.meta.clone(),
            t,
            rocp_inv,
            rocp_fwd,
            cache: Cache::default(),
        })
    }

    /// Rebuild a [`Sounding`] (wind as u/v components, which is what
    /// SHARPpy passes when it copies a modified profile).
    pub fn to_sounding(&self) -> Sounding {
        let o = |v: &Vec<f64>| v.iter().map(|&x| opt(x)).collect::<Vec<_>>();
        let mut s = Sounding::new(o(&self.pres), o(&self.hght), o(&self.tmpc), o(&self.dwpc))
            .with_meta(self.meta.clone());
        if self.has_wind {
            s = s.with_component_wind(o(&self.u), o(&self.v));
        }
        if self.omeg.iter().any(|x| !x.is_nan()) {
            s = s.with_omega(o(&self.omeg));
        }
        s
    }

    // ------------------------------------------------------------------
    // public accessors
    // ------------------------------------------------------------------
    pub fn len(&self) -> usize {
        self.pres.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pres.is_empty()
    }
    pub fn meta(&self) -> &Metadata {
        &self.meta
    }
    /// Index of the surface (lowest level with a valid temperature).
    pub fn sfc_index(&self) -> usize {
        self.sfc
    }
    /// Index of the highest level with a valid temperature.
    pub fn top_index(&self) -> usize {
        self.top
    }
    pub fn has_wind(&self) -> bool {
        self.has_wind
    }
    pub fn has_omega(&self) -> bool {
        self.omeg.iter().any(|x| !x.is_nan())
    }
    /// Surface pressure (hPa).
    pub fn sfc_pres(&self) -> f64 {
        self.pres[self.sfc]
    }
    /// Surface height (m MSL).
    pub fn sfc_hght(&self) -> Option<f64> {
        opt(self.hght[self.sfc])
    }

    /// All derived data for level `i`.
    pub fn level(&self, i: usize) -> Level {
        Level {
            pres: opt(self.pres[i]),
            hght: opt(self.hght[i]),
            tmpc: opt(self.tmpc[i]),
            dwpc: opt(self.dwpc[i]),
            wdir: opt(self.wdir[i]),
            wspd: opt(self.wspd[i]),
            u: opt(self.u[i]),
            v: opt(self.v[i]),
            omeg: opt(self.omeg[i]),
            vtmp: opt(self.vtmp[i]),
            wetbulb: opt(self.wetbulb[i]),
            thetae: opt(self.thetae[i]),
            theta: opt(self.theta[i]),
            wvmr: opt(self.wvmr[i]),
            relh: opt(self.relh[i]),
        }
    }

    /// Iterate over every level (bottom up).
    pub fn levels(&self) -> impl Iterator<Item = Level> + '_ {
        (0..self.len()).map(|i| self.level(i))
    }

    /// A whole column as options.
    pub fn column(&self, field: Field) -> Vec<Option<f64>> {
        let c = match field {
            Field::Height => &self.hght,
            Field::Temperature => &self.tmpc,
            Field::Dewpoint => &self.dwpc,
            Field::VirtualTemperature => &self.vtmp,
            Field::U => &self.u,
            Field::V => &self.v,
            Field::Omega => &self.omeg,
            Field::ThetaE => &self.thetae,
            Field::Theta => &self.theta,
            Field::WetBulb => &self.wetbulb,
            Field::MixingRatio => &self.wvmr,
        };
        c.iter().map(|&x| opt(x)).collect()
    }

    /// Pressure column.
    pub fn pressures(&self) -> Vec<Option<f64>> {
        self.pres.iter().map(|&x| opt(x)).collect()
    }

    /// Interpolate `field` to pressure `p` (hPa), linearly in log10(p) like
    /// SHARPpy. `None` outside the data range.
    pub fn interp(&self, field: Field, p: f64) -> Option<f64> {
        opt(self.interp_raw(field, p))
    }

    /// Pressure (hPa) at height `h` (m MSL).
    pub fn pres_at_hght(&self, h: f64) -> Option<f64> {
        opt(self.pres(h))
    }

    /// Height (m MSL) at pressure `p`.
    pub fn hght_at_pres(&self, p: f64) -> Option<f64> {
        opt(self.hght(p))
    }

    /// Wind components (kt) at pressure `p`.
    pub fn wind_at(&self, p: f64) -> Option<crate::units::Vector> {
        let (u, v) = self.components(p);
        crate::units::Vector::from_nan(u, v)
    }

    /// m AGL -> m MSL.
    pub fn to_msl(&self, h_agl: f64) -> f64 {
        h_agl + self.hght[self.sfc]
    }
    /// m MSL -> m AGL.
    pub fn to_agl(&self, h_msl: f64) -> f64 {
        h_msl - self.hght[self.sfc]
    }

    // ------------------------------------------------------------------
    // internal NaN-based interpolation (SHARPpy `interp.*`)
    // ------------------------------------------------------------------
    pub(crate) fn interp_raw(&self, field: Field, p: f64) -> f64 {
        let lp = p.log10();
        let tab = match field {
            Field::Height => &self.t.hght,
            Field::Temperature => &self.t.tmpc,
            Field::Dewpoint => &self.t.dwpc,
            Field::VirtualTemperature => &self.t.vtmp,
            Field::U => &self.t.u,
            Field::V => &self.t.v,
            Field::Omega => &self.t.omeg,
            Field::ThetaE => &self.t.thetae,
            Field::Theta => &self.t.theta,
            Field::WetBulb => &self.t.wetbulb,
            Field::MixingRatio => &self.t.wvmr,
        };
        tab.at(lp)
    }
    #[inline]
    pub(crate) fn temp(&self, p: f64) -> f64 {
        self.t.tmpc.at(p.log10())
    }
    #[inline]
    pub(crate) fn dwpt(&self, p: f64) -> f64 {
        self.t.dwpc.at(p.log10())
    }
    #[inline]
    pub(crate) fn vtmp_at(&self, p: f64) -> f64 {
        self.t.vtmp.at(p.log10())
    }
    #[inline]
    pub(crate) fn hght(&self, p: f64) -> f64 {
        self.t.hght.at(p.log10())
    }
    #[inline]
    pub(crate) fn omeg_at(&self, p: f64) -> f64 {
        self.t.omeg.at(p.log10())
    }
    /// `interp.pres`: 10 ** interp(h, hght, logp)
    #[inline]
    pub(crate) fn pres(&self, h: f64) -> f64 {
        pow(10., self.t.pres_by_hght.at(h))
    }
    pub(crate) fn pres_many(&self, hs: &[f64]) -> Vec<f64> {
        self.t.pres_by_hght.at_many(hs).into_iter().map(|x| pow(10., x)).collect()
    }
    #[inline]
    pub(crate) fn components(&self, p: f64) -> (f64, f64) {
        if !self.has_wind {
            return (f64::NAN, f64::NAN);
        }
        let lp = p.log10();
        (self.t.u.at(lp), self.t.v.at(lp))
    }
    pub(crate) fn components_many(&self, ps: &[f64]) -> (Vec<f64>, Vec<f64>) {
        if !self.has_wind {
            return (vec![f64::NAN; ps.len()], vec![f64::NAN; ps.len()]);
        }
        let lps: Vec<f64> = ps.iter().map(|p| p.log10()).collect();
        (self.t.u.at_many(&lps), self.t.v.at_many(&lps))
    }
    pub(crate) fn many(&self, field: Field, ps: &[f64]) -> Vec<f64> {
        let lps: Vec<f64> = ps.iter().map(|p| p.log10()).collect();
        let tab = match field {
            Field::Height => &self.t.hght,
            Field::Temperature => &self.t.tmpc,
            Field::Dewpoint => &self.t.dwpc,
            Field::VirtualTemperature => &self.t.vtmp,
            Field::U => &self.t.u,
            Field::V => &self.t.v,
            Field::Omega => &self.t.omeg,
            Field::ThetaE => &self.t.thetae,
            Field::Theta => &self.t.theta,
            Field::WetBulb => &self.t.wetbulb,
            Field::MixingRatio => &self.t.wvmr,
        };
        tab.at_many(&lps)
    }
    #[inline]
    pub(crate) fn psfc(&self) -> f64 {
        self.pres[self.sfc]
    }
    #[inline]
    pub(crate) fn plast(&self) -> f64 {
        self.pres[self.pres.len() - 1]
    }
    #[inline]
    pub(crate) fn msl(&self, h: f64) -> f64 {
        h + self.hght[self.sfc]
    }
    #[inline]
    pub(crate) fn agl(&self, h: f64) -> f64 {
        h - self.hght[self.sfc]
    }
    /// pressure at a height AGL (`interp.pres(prof, interp.to_msl(prof, h))`)
    #[inline]
    pub(crate) fn p_agl(&self, h: f64) -> f64 {
        self.pres(self.msl(h))
    }

    // ------------------------------------------------------------------
    // shared caches
    // ------------------------------------------------------------------
    pub(crate) fn sfc_grid(&self) -> &SfcGrid {
        self.cache.sfc.get_or_init(|| {
            let p = arange(self.psfc(), self.plast() - 1., -1.);
            let lp = p.iter().map(|p| p.log10()).collect();
            SfcGrid { p, lp, ..Default::default() }
        })
    }

    /// If `arange(pbot, ptop - 1, -1)` is a prefix of the surface grid,
    /// the grid and the prefix length.
    #[inline]
    pub(crate) fn sfc_prefix(&self, pbot: f64, ptop: f64) -> Option<(&SfcGrid, usize)> {
        if pbot.to_bits() != self.psfc().to_bits() {
            return None;
        }
        let n = arange_len(pbot, ptop - 1., -1.);
        let g = self.sfc_grid();
        (n <= g.p.len()).then_some((g, n))
    }

    /// `(many(Height), many(Temperature), many(Dewpoint))` at `ps`, with one
    /// log10 per point and one search when the tables share abscissae.
    pub(crate) fn many_htd(&self, ps: &[f64]) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let lps: Vec<f64> = ps.iter().map(|p| p.log10()).collect();
        let (h, t, d) = (&self.t.hght, &self.t.tmpc, &self.t.dwpc);
        if !h.xp.is_empty() && same_bits(&h.xp, &t.xp) && same_bits(&h.xp, &d.xp) {
            let [a, b, c] = numerics::interp_many_n(&lps, &h.xp, [&h.fp, &t.fp, &d.fp]);
            return (a, b, c);
        }
        (h.at_many(&lps), t.at_many(&lps), d.at_many(&lps))
    }

    /// Wind components at many pressures (one search per point for u and v).
    pub(crate) fn components_many2(&self, ps: &[f64]) -> (Vec<f64>, Vec<f64>) {
        if !self.has_wind {
            return (vec![f64::NAN; ps.len()], vec![f64::NAN; ps.len()]);
        }
        let lps: Vec<f64> = ps.iter().map(|p| p.log10()).collect();
        self.t.u.at_many2(&self.t.v, &lps)
    }

    pub(crate) fn mean_wind_memo_get(&self, pbot: f64, ptop: f64) -> Option<(f64, f64)> {
        let (a, b) = (pbot.to_bits(), ptop.to_bits());
        let m = self.cache.mean_wind.lock().ok()?;
        m.iter().find(|e| e.0 == a && e.1 == b).map(|e| (e.2, e.3))
    }
    pub(crate) fn mean_wind_memo_put(&self, pbot: f64, ptop: f64, uv: (f64, f64)) {
        if let Ok(mut m) = self.cache.mean_wind.lock() {
            if m.len() < MEAN_WIND_MEMO_MAX {
                m.push((pbot.to_bits(), ptop.to_bits(), uv.0, uv.1));
            }
        }
    }
}

impl SfcGrid {
    pub(crate) fn uv(&self, prof: &Profile) -> (&[f64], &[f64]) {
        let (u, v) = self.uv.get_or_init(|| {
            if !prof.has_wind {
                return (vec![f64::NAN; self.p.len()], vec![f64::NAN; self.p.len()]);
            }
            prof.t.u.at_many2(&prof.t.v, &self.lp)
        });
        (u, v)
    }
    pub(crate) fn tmpc(&self, prof: &Profile) -> &[f64] {
        self.tmpc.get_or_init(|| prof.t.tmpc.at_many(&self.lp))
    }
    pub(crate) fn dwpc(&self, prof: &Profile) -> &[f64] {
        self.dwpc.get_or_init(|| prof.t.dwpc.at_many(&self.lp))
    }
    pub(crate) fn hght(&self, prof: &Profile) -> &[f64] {
        self.hght.get_or_init(|| prof.t.hght.at_many(&self.lp))
    }
    pub(crate) fn dry_env(&self, prof: &Profile) -> &[f64] {
        self.dry_env.get_or_init(|| {
            let t = self.tmpc(prof);
            let d = self.dwpc(prof);
            (0..self.p.len()).map(|k| crate::parcel::dry_env_tv(self.p[k], t[k], d[k])).collect()
        })
    }
    pub(crate) fn mu_tw(&self, prof: &Profile) -> &[f64] {
        self.mu_tw.get_or_init(|| {
            let psfc = prof.psfc();
            let n = arange_len(psfc, (psfc - 500.) - 1., -1.).min(self.p.len());
            let t = self.tmpc(prof);
            let d = self.dwpc(prof);
            (0..n).map(|k| crate::layer::mu_value(self.p[k], t[k], d[k])).collect()
        })
    }
}
