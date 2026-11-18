//! Parcel definition and lifting: LCL/LFC/EL/MPL, CAPE/CIN and the many
//! partial integrals SHARPpy reports.
//!
//! The integration scheme is SHARP's: below the LCL the CIN is integrated in
//! 1 hPa steps using *virtual potential temperature* differences (SHARPpy
//! quirk: `virtemp` is fed potential temperatures there); above the LCL the
//! parcel is lifted moist-adiabatically level by level and buoyancy is
//! integrated with the trapezoid rule on the observed levels, in virtual
//! temperature. Only negative areas below 500 hPa count towards CIN.

use crate::layer::{Scheme, virtemp_masked};
use crate::numerics::{arange, arange_len, masked_argmin, pairwise_sum, sq};
use crate::profile::{Field, Profile, opt};
use crate::thermo::{self, G};
use crate::units::{kts_to_ms, mag};

/// The standard SHARPpy parcel types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ParcelKind {
    /// Surface-based parcel.
    Surface,
    /// Forecast surface parcel (max temperature, mean 100 hPa mixing ratio).
    Forecast,
    /// Most-unstable parcel in the lowest 300 hPa.
    MostUnstable,
    /// 100 hPa mixed-layer parcel.
    MixedLayer,
    /// Mean of the effective inflow layer.
    Effective,
    /// User-defined parcel.
    User,
}

impl ParcelKind {
    /// SHARPpy's `DefineParcel` flag (drives the BRN layer choice).
    fn flag(self) -> u8 {
        match self {
            ParcelKind::Surface => 1,
            ParcelKind::Forecast => 2,
            ParcelKind::MostUnstable => 3,
            ParcelKind::MixedLayer => 4,
            ParcelKind::Effective | ParcelKind::User => 5,
        }
    }
}

/// Starting conditions of a lifted parcel.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParcelSpec {
    pub kind: ParcelKind,
    /// hPa
    pub pres: f64,
    /// °C
    pub tmpc: f64,
    /// °C
    pub dwpc: f64,
}

impl ParcelSpec {
    /// A user parcel at an arbitrary level with explicit T/Td.
    pub fn user(pres: f64, tmpc: f64, dwpc: f64) -> Self {
        Self { kind: ParcelKind::User, pres, tmpc, dwpc }
    }
}

/// One point of a parcel trace (virtual temperature).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TracePoint {
    /// hPa
    pub pres: f64,
    /// Parcel virtual temperature, °C
    pub vtmp: f64,
}

/// A lifted parcel. Heights are m AGL, pressures hPa, energies J/kg.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Parcel {
    pub kind: ParcelKind,
    /// Origin pressure / temperature / dewpoint.
    pub pres: Option<f64>,
    pub tmpc: Option<f64>,
    pub dwpc: Option<f64>,
    pub lcl_pres: Option<f64>,
    pub lcl_hght: Option<f64>,
    pub lfc_pres: Option<f64>,
    pub lfc_hght: Option<f64>,
    pub el_pres: Option<f64>,
    pub el_hght: Option<f64>,
    /// Maximum parcel level.
    pub mpl_pres: Option<f64>,
    pub mpl_hght: Option<f64>,
    /// CAPE.
    pub cape: Option<f64>,
    /// CIN (<= 0).
    pub cinh: Option<f64>,
    /// CAPE below the freezing level.
    pub cape_fzl: Option<f64>,
    /// 0-3 km CAPE.
    pub cape_3km: Option<f64>,
    /// 0-6 km CAPE.
    pub cape_6km: Option<f64>,
    /// Pressures of the environmental 0/-10/-20/-30 °C levels.
    pub p0c: Option<f64>,
    pub pm10c: Option<f64>,
    pub pm20c: Option<f64>,
    pub pm30c: Option<f64>,
    /// Heights (m **MSL**, as SHARPpy stores them) of those levels.
    pub hght0c: Option<f64>,
    pub hghtm10c: Option<f64>,
    pub hghtm20c: Option<f64>,
    pub hghtm30c: Option<f64>,
    /// CAPE up to the -10/-20/-30 °C levels (the skew-T "sampling" readout).
    pub wm10c: Option<f64>,
    pub wm20c: Option<f64>,
    pub wm30c: Option<f64>,
    /// 500 hPa lifted index.
    pub li5: Option<f64>,
    /// 300 hPa lifted index.
    pub li3: Option<f64>,
    /// BRN shear term (m²/s², 0.5 * shear²) and its components (kt).
    pub brn_shear: Option<f64>,
    pub brn_u: Option<f64>,
    pub brn_v: Option<f64>,
    /// Bulk Richardson number.
    pub brn: Option<f64>,
    /// Maximum lifted index (most negative parcel-env difference, sign flipped).
    pub limax: Option<f64>,
    pub limax_pres: Option<f64>,
    /// Cap strength (°C) and its pressure.
    pub cap: Option<f64>,
    pub cap_pres: Option<f64>,
    /// Minimum buoyancy (°C) below 500 hPa and its pressure.
    pub bmin: Option<f64>,
    pub bmin_pres: Option<f64>,
    /// Parcel virtual-temperature trace from the origin through the LCL and
    /// every observed level above it (missing levels skipped).
    pub trace: Vec<TracePoint>,
}

/// NaN-based working copy of SHARPpy's `Parcel` object.
#[derive(Clone, Debug)]
pub(crate) struct RawParcel {
    pub kind: ParcelKind,
    pub pres: f64,
    pub tmpc: f64,
    pub dwpc: f64,
    pub lclpres: f64,
    pub lclhght: f64,
    pub lfcpres: f64,
    pub lfchght: f64,
    pub elpres: f64,
    pub elhght: f64,
    pub mplpres: f64,
    pub mplhght: f64,
    pub bplus: f64,
    pub bminus: f64,
    pub bfzl: f64,
    pub b3km: f64,
    pub b6km: f64,
    pub p0c: f64,
    pub pm10c: f64,
    pub pm20c: f64,
    pub pm30c: f64,
    pub hght0c: f64,
    pub hghtm10c: f64,
    pub hghtm20c: f64,
    pub hghtm30c: f64,
    pub wm10c: f64,
    pub wm20c: f64,
    pub wm30c: f64,
    pub li5: f64,
    pub li3: f64,
    pub brnshear: f64,
    pub brnu: f64,
    pub brnv: f64,
    pub brn: f64,
    pub limax: f64,
    pub limaxpres: f64,
    pub cap: f64,
    pub cappres: f64,
    pub bmin: f64,
    pub bminpres: f64,
    pub ptrace: Vec<f64>,
    pub ttrace: Vec<f64>,
}

const NAN: f64 = f64::NAN;

impl RawParcel {
    fn empty(spec: &ParcelSpec) -> Self {
        Self {
            kind: spec.kind,
            pres: spec.pres,
            tmpc: spec.tmpc,
            dwpc: spec.dwpc,
            lclpres: NAN,
            lclhght: NAN,
            lfcpres: NAN,
            lfchght: NAN,
            elpres: NAN,
            elhght: NAN,
            mplpres: NAN,
            mplhght: NAN,
            bplus: NAN,
            bminus: NAN,
            bfzl: NAN,
            b3km: NAN,
            b6km: NAN,
            p0c: NAN,
            pm10c: NAN,
            pm20c: NAN,
            pm30c: NAN,
            hght0c: NAN,
            hghtm10c: NAN,
            hghtm20c: NAN,
            hghtm30c: NAN,
            wm10c: NAN,
            wm20c: NAN,
            wm30c: NAN,
            li5: NAN,
            li3: NAN,
            brnshear: NAN,
            brnu: NAN,
            brnv: NAN,
            brn: NAN,
            limax: NAN,
            limaxpres: NAN,
            cap: NAN,
            cappres: NAN,
            bmin: NAN,
            bminpres: NAN,
            ptrace: Vec::new(),
            ttrace: Vec::new(),
        }
    }

    pub(crate) fn to_public(&self) -> Parcel {
        let trace = self
            .ptrace
            .iter()
            .zip(&self.ttrace)
            .filter(|(p, t)| !p.is_nan() && !t.is_nan())
            .map(|(&pres, &vtmp)| TracePoint { pres, vtmp })
            .collect();
        Parcel {
            kind: self.kind,
            pres: opt(self.pres),
            tmpc: opt(self.tmpc),
            dwpc: opt(self.dwpc),
            lcl_pres: opt(self.lclpres),
            lcl_hght: opt(self.lclhght),
            lfc_pres: opt(self.lfcpres),
            lfc_hght: opt(self.lfchght),
            el_pres: opt(self.elpres),
            el_hght: opt(self.elhght),
            mpl_pres: opt(self.mplpres),
            mpl_hght: opt(self.mplhght),
            cape: opt(self.bplus),
            cinh: opt(self.bminus),
            cape_fzl: opt(self.bfzl),
            cape_3km: opt(self.b3km),
            cape_6km: opt(self.b6km),
            p0c: opt(self.p0c),
            pm10c: opt(self.pm10c),
            pm20c: opt(self.pm20c),
            pm30c: opt(self.pm30c),
            hght0c: opt(self.hght0c),
            hghtm10c: opt(self.hghtm10c),
            hghtm20c: opt(self.hghtm20c),
            hghtm30c: opt(self.hghtm30c),
            wm10c: opt(self.wm10c),
            wm20c: opt(self.wm20c),
            wm30c: opt(self.wm30c),
            li5: opt(self.li5),
            li3: opt(self.li3),
            brn_shear: opt(self.brnshear),
            brn_u: opt(self.brnu),
            brn_v: opt(self.brnv),
            brn: opt(self.brn),
            limax: opt(self.limax),
            limax_pres: opt(self.limaxpres),
            cap: opt(self.cap),
            cap_pres: opt(self.cappres),
            bmin: opt(self.bmin),
            bmin_pres: opt(self.bminpres),
            trace,
        }
    }
}

/// virtual temperature of a saturated parcel
#[inline]
fn vt_sat(p: f64, t: f64) -> f64 {
    thermo::virtemp(p, t, t)
}

/// buoyancy term (Tv_parcel - Tv_env) / Tv_env[K]
#[inline]
fn tdef(p: f64, tp: f64, te: f64) -> f64 {
    tdef_vs(vt_sat(p, tp), te)
}

/// [`tdef`] given the parcel's saturated virtual temperature.
#[inline]
fn tdef_vs(vs: f64, te: f64) -> f64 {
    (vs - te) / thermo::ctok(te)
}

/// Environmental virtual "potential" temperature of the sub-LCL CIN
/// integrand at sample `p` (SHARPpy feeds theta to `virtemp` there).
#[inline]
pub(crate) fn dry_env_tv(p: f64, t: f64, td: f64) -> f64 {
    let env_theta = thermo::theta(p, t, 1000.);
    virtemp_masked(p, env_theta, td)
}

/// Parcel moisture term of the sub-LCL CIN integrand on the surface grid
/// for one mixing ratio: `w_k = 0.001 * mixratio(p_k, temp_at_mixrat(blmr,
/// p_k))` (NaN where the dewpoint is masked). Lets repeated ascents with the
/// same moisture (the convective-temperature iteration) skip the
/// transcendental work.
#[derive(Default)]
pub(crate) struct DryParcelCol {
    blmr: u64,
    /// `None` where `virtemp_masked` masks the sample
    w: Vec<Option<f64>>,
}

impl DryParcelCol {
    /// `w` for sample `p` of mixing ratio `blmr` (None = masked).
    #[inline]
    fn compute(blmr: f64, p: f64) -> Option<f64> {
        let td = thermo::temp_at_mixrat(blmr, p);
        if td.is_nan() || p.is_nan() { None } else { Some(0.001 * thermo::mixratio(p, td)) }
    }
}

impl Profile {
    // ------------------------------------------------------------------
    // parcel definitions (SHARPpy DefineParcel)
    // ------------------------------------------------------------------

    /// Starting conditions for one of the standard parcels. `Effective`
    /// requires the effective inflow layer and is produced by the analysis;
    /// here it falls back to the surface parcel like SHARPpy does.
    pub fn parcel_spec(&self, kind: ParcelKind) -> ParcelSpec {
        match kind {
            ParcelKind::Surface | ParcelKind::Effective | ParcelKind::User => ParcelSpec {
                kind,
                pres: self.psfc(),
                tmpc: self.tmpc[self.sfc],
                dwpc: self.dwpc[self.sfc],
            },
            ParcelKind::Forecast => {
                let tmpc = self.max_temp_raw();
                let pres = self.psfc();
                let mmr = self.mean_mixratio_raw(Some(pres), Some(pres - 100.), Scheme::Exact);
                ParcelSpec { kind, pres, tmpc, dwpc: thermo::temp_at_mixrat(mmr, pres) }
            }
            ParcelKind::MostUnstable => self.mu_spec(300.),
            ParcelKind::MixedLayer => self.mixed_layer_spec(self.psfc(), 100.),
        }
    }

    /// Most-unstable parcel searched over the lowest `depth` hPa.
    pub(crate) fn mu_spec(&self, depth: f64) -> ParcelSpec {
        let pbot = self.psfc();
        let pres = self.most_unstable_level_raw(Some(pbot), Some(pbot - depth));
        ParcelSpec { kind: ParcelKind::MostUnstable, pres, tmpc: self.temp(pres), dwpc: self.dwpt(pres) }
    }

    /// Mixed-layer parcel: mean theta / mixing ratio of the `depth` hPa above
    /// `pbot` (SHARPpy exact scheme), placed at `pbot`.
    pub fn mixed_layer_spec(&self, pbot: f64, depth: f64) -> ParcelSpec {
        let ptop = pbot - depth;
        let mtheta = self.mean_theta_raw(Some(pbot), Some(ptop), Scheme::Exact);
        let tmpc = thermo::theta(1000., mtheta, pbot);
        let mmr = self.mean_mixratio_raw(Some(pbot), Some(ptop), Scheme::Exact);
        ParcelSpec { kind: ParcelKind::MixedLayer, pres: pbot, tmpc, dwpc: thermo::temp_at_mixrat(mmr, pbot) }
    }

    /// User parcel at `pres` using the interpolated environment T/Td (the
    /// skew-T "lift parcel from this level" tool).
    pub fn user_spec_at(&self, pres: f64) -> ParcelSpec {
        ParcelSpec::user(pres, self.temp(pres), self.dwpt(pres))
    }

    /// Lift a parcel through the whole profile.
    pub fn lift(&self, spec: &ParcelSpec) -> Parcel {
        self.lift_raw(spec).to_public()
    }

    // ------------------------------------------------------------------
    // full lifting (SHARPpy parcelx)
    // ------------------------------------------------------------------
    pub(crate) fn lift_raw(&self, spec: &ParcelSpec) -> RawParcel {
        let mut pcl = RawParcel::empty(spec);
        let (pres, tmpc, dwpc) = (spec.pres, spec.tmpc, spec.dwpc);
        let n = self.len();
        let mut cap_strength = -9999.;
        let mut cap_strengthpres = -9999.;
        let mut li_max = -9999.;
        let mut li_maxpres = -9999.;
        let mut totp = 0.;
        let mut totn;
        let mut tote = 0.;

        let mut pbot = self.psfc();
        let ptop = self.plast();
        if pbot > pres {
            pbot = pres;
        }
        let mut pe1 = pbot;
        let tp1_0 = thermo::virtemp(pres, tmpc, dwpc);
        let mut ptrace = vec![pe1];
        let mut ttrace = vec![tp1_0];

        // dry ascent to the LCL
        let (mut pe2, mut tp2) = thermo::drylift(pres, tmpc, dwpc);
        if pe2.is_nan() {
            return pcl;
        }
        let blupper = pe2;
        let mut h2 = self.hght(pe2);
        pcl.lclpres = if self.psfc() < pe2 { self.psfc() } else { pe2 };
        pcl.lclhght = self.agl(h2);
        ptrace.push(pe2);
        ttrace.push(vt_sat(pe2, tp2));

        totn = self.dry_cin(pbot, blupper, pres, dwpc, pe2, tp2);

        if pbot > pe2 {
            pbot = pe2;
        }

        // environmental temperature levels
        let p0c = self.temp_lvl_raw(0., false);
        let pm10c = self.temp_lvl_raw(-10., false);
        let pm20c = self.temp_lvl_raw(-20., false);
        let pm30c = self.temp_lvl_raw(-30., false);
        let hgt0c = self.hght(p0c);
        let hgtm10c = self.hght(pm10c);
        let hgtm20c = self.hght(pm20c);
        let hgtm30c = self.hght(pm30c);
        pcl.p0c = p0c;
        pcl.pm10c = pm10c;
        pcl.pm20c = pm20c;
        pcl.pm30c = pm30c;
        pcl.hght0c = hgt0c;
        pcl.hghtm10c = hgtm10c;
        pcl.hghtm20c = hgtm20c;
        pcl.hghtm30c = hgtm30c;

        if pbot < self.plast() {
            return pcl;
        }

        let Some(lptr) = self.pres.iter().position(|&p| pbot >= p) else {
            return pcl;
        };
        let uptr = self.pres.iter().rposition(|&p| ptop <= p).unwrap_or(0);

        pe1 = pbot;
        let mut h1 = self.hght(pe1);
        let mut te1 = self.vtmp_at(pe1);
        let mut tp1 = thermo::wetlift(pe2, tp2, pe1);
        let mut lyre = 0.;
        let mut lyrlast;
        let mut pelast;

        let m = n - lptr;
        let mut ptraces = vec![NAN; m];
        let mut ttraces = vec![NAN; m];
        let mut te2;
        // theta factor pow(1000 / pe1, ROCP) and buoyancy of the previous
        // level, carried from level to level (pe1/tp1/te1 are not modified
        // after being advanced below)
        let mut k1 = crate::numerics::pow(1000. / pe1, thermo::ROCP);
        let mut tdef1 = tdef(pe1, tp1, te1);

        for i in lptr..n {
            if self.tmpc[i].is_nan() {
                continue;
            }
            pe2 = self.pres[i];
            h2 = self.hght[i];
            te2 = self.vtmp[i];
            tp2 = thermo::wetlift_k(tp1, k1, pe2, self.rocp_fwd[i]);
            let vs2 = vt_sat(pe2, tp2);
            let tdef2 = tdef_vs(vs2, te2);
            ptraces[i - lptr] = pe2;
            ttraces[i - lptr] = vs2;
            lyrlast = lyre;
            lyre = G * (tdef1 + tdef2) / 2. * (h2 - h1);
            if lyre > 0. {
                totp += lyre;
            } else if pe2 > 500. {
                totn += lyre;
            }
            let mli = vs2 - te2;
            if mli > li_max {
                li_max = mli;
                li_maxpres = pe2;
            }
            let mcap = te2 - mli;
            if mcap > cap_strength {
                cap_strength = mcap;
                cap_strengthpres = pe2;
            }
            tote += lyre;
            pelast = pe1;
            pe1 = pe2;
            te1 = te2;
            tp1 = tp2;
            tdef1 = tdef2;
            k1 = self.rocp_inv[i];

            // top of the sounding: finish CAPE/CIN, integrating to ptop
            if i >= uptr && pcl.bplus.is_nan() {
                let pe3 = pe1;
                let h3 = h2;
                let te3 = te1;
                let tp3 = tp1;
                let mut lyrf = lyre;
                if lyrf > 0. {
                    pcl.bplus = totp - lyrf;
                    pcl.bminus = totn;
                } else {
                    pcl.bplus = totp;
                    pcl.bminus = if pe2 > 500. { totn + lyrf } else { totn };
                }
                pe2 = ptop;
                h2 = self.hght(pe2);
                te2 = self.vtmp_at(pe2);
                tp2 = thermo::wetlift_k(tp3, k1, pe2, self.rocp_fwd[n - 1]);
                debug_assert_eq!(tdef1.to_bits(), tdef(pe3, tp3, te3).to_bits());
                let tdef3 = tdef1;
                let tdef2 = tdef(pe2, tp2, te2);
                lyrf = G * (tdef3 + tdef2) / 2. * (h2 - h3);
                if lyrf > 0. {
                    pcl.bplus += lyrf;
                } else if pe2 > 500. {
                    pcl.bminus += lyrf;
                }
                if pcl.bplus == 0. {
                    pcl.bminus = 0.;
                }
            }

            // partial CAPE up to the freezing / -10 / -20 / -30 °C levels
            // (SHARPpy's freezing-level CAPE compares p0c with the previous
            // level, the others compare with the LCL pressure)
            let fz = [
                (0., p0c, hgt0c, 0usize),
                (-10., pm10c, hgtm10c, 1),
                (-20., pm20c, hgtm20c, 2),
                (-30., pm30c, hgtm30c, 3),
            ];
            for (tlim, plev, hlev, which) in fz {
                let slot = match which {
                    0 => pcl.bfzl,
                    1 => pcl.wm10c,
                    2 => pcl.wm20c,
                    _ => pcl.wm30c,
                };
                if !(te2 < tlim && slot.is_nan()) {
                    continue;
                }
                let pe3 = pelast;
                let h3 = self.hght(pe3);
                let te3 = self.vtmp_at(pe3);
                let tp3 = thermo::wetlift(pe1, tp1, pe3);
                let mut val = if lyre > 0. { totp - lyre } else { totp };
                let skip = if which == 0 { plev.is_nan() || plev > pe3 } else { plev.is_nan() || plev > pcl.lclpres };
                if skip {
                    val = 0.;
                } else if !pe2.is_nan() {
                    te2 = self.vtmp_at(pe2);
                    tp2 = thermo::wetlift(pe3, tp3, pe2);
                    let tdef3 = tdef(pe3, tp3, te3);
                    let tdef2 = tdef(pe2, tp2, te2);
                    let lyrf = G * (tdef3 + tdef2) / 2. * (hlev - h3);
                    if lyrf > 0. {
                        val += lyrf;
                    }
                }
                match which {
                    0 => pcl.bfzl = val,
                    1 => pcl.wm10c = val,
                    2 => pcl.wm20c = val,
                    _ => pcl.wm30c = val,
                }
            }

            // 0-3 km and 0-6 km CAPE
            for (depth, is3) in [(3000., true), (6000., false)] {
                let slot = if is3 { pcl.b3km } else { pcl.b6km };
                if pcl.lclhght < depth {
                    if self.agl(h1) <= depth && self.agl(h2) >= depth && slot.is_nan() {
                        let pe3 = pelast;
                        let h3 = self.hght(pe3);
                        let te3 = self.vtmp_at(pe3);
                        let tp3 = thermo::wetlift(pe1, tp1, pe3);
                        let mut val = if lyre > 0. { totp - lyre } else { totp };
                        let h4 = self.msl(depth);
                        let pe4 = self.pres(h4);
                        if !pe2.is_nan() {
                            te2 = self.vtmp_at(pe4);
                            tp2 = thermo::wetlift(pe3, tp3, pe4);
                            let tdef3 = tdef(pe3, tp3, te3);
                            let tdef2 = tdef(pe4, tp2, te2);
                            let lyrf = G * (tdef3 + tdef2) / 2. * (h4 - h3);
                            if lyrf > 0. {
                                val += lyrf;
                            }
                        }
                        if is3 {
                            pcl.b3km = val;
                        } else {
                            pcl.b6km = val;
                        }
                    }
                } else if is3 {
                    pcl.b3km = 0.;
                } else {
                    pcl.b6km = 0.;
                }
            }

            h1 = h2;

            // LFC: buoyancy turns positive
            if lyre >= 0. && lyrlast <= 0. {
                let tp3 = tp1;
                pe2 = pe1;
                let mut pe3 = pelast;
                let pv = |pe3: f64| {
                    let t = thermo::wetlift(pe2, tp3, pe3);
                    vt_sat(pe3, t)
                };
                if self.vtmp_at(pe3) < pv(pe3) {
                    pcl.lfcpres = pe3;
                    pcl.lfchght = self.agl(self.hght(pe3));
                    pcl.elpres = NAN;
                    pcl.elhght = NAN;
                    pcl.mplpres = NAN;
                } else {
                    while self.vtmp_at(pe3) > pv(pe3) && pe3 > 0. {
                        pe3 -= 5.;
                    }
                    if pe3 > 0. {
                        pcl.lfcpres = pe3;
                        pcl.lfchght = self.agl(self.hght(pe3));
                        tote = 0.;
                        li_max = -9999.;
                        if cap_strength < 0. {
                            cap_strength = 0.;
                        }
                        pcl.cap = cap_strength;
                        pcl.cappres = cap_strengthpres;
                        pcl.elpres = NAN;
                        pcl.elhght = NAN;
                        pcl.mplpres = NAN;
                    }
                }
                if pcl.lfcpres >= pcl.lclpres {
                    pcl.lfcpres = pcl.lclpres;
                    pcl.lfchght = pcl.lclhght;
                }
            }

            // EL: buoyancy turns negative
            if lyre <= 0. && lyrlast >= 0. {
                let tp3 = tp1;
                pe2 = pe1;
                let mut pe3 = pelast;
                let pv = |pe3: f64| {
                    let t = thermo::wetlift(pe2, tp3, pe3);
                    vt_sat(pe3, t)
                };
                while self.vtmp_at(pe3) < pv(pe3) {
                    pe3 -= 5.;
                }
                pcl.elpres = pe3;
                pcl.elhght = self.agl(self.hght(pcl.elpres));
                pcl.mplpres = NAN;
                pcl.limax = -li_max;
                pcl.limaxpres = li_maxpres;
            }

            // MPL: where the negative area above the EL cancels the CAPE
            if tote < 0. && pcl.mplpres.is_nan() && !pcl.elpres.is_nan() {
                let mut pe3 = pelast;
                let h3 = self.hght(pe3);
                let mut te3 = self.vtmp_at(pe3);
                let mut tp3 = thermo::wetlift(pe1, tp1, pe3);
                let mut totx = tote - lyre;
                pe2 = pelast;
                let mut guard = 0;
                let mut tdef3 = if totx > 0. { tdef(pe3, tp3, te3) } else { NAN };
                while totx > 0. && guard < 100_000 {
                    guard += 1;
                    pe2 -= 1.;
                    let lp2 = pe2.log10();
                    te2 = self.t.vtmp.at(lp2);
                    tp2 = thermo::wetlift(pe3, tp3, pe2);
                    h2 = self.t.hght.at(lp2);
                    let tdef2 = tdef(pe2, tp2, te2);
                    // SHARPpy quirk: h3 is never advanced inside this loop
                    let lyrf = G * (tdef3 + tdef2) / 2. * (h2 - h3);
                    totx += lyrf;
                    tp3 = tp2;
                    te3 = te2;
                    pe3 = pe2;
                    tdef3 = tdef2;
                }
                pcl.mplpres = pe2;
                pcl.mplhght = self.agl(self.hght(pe2));
            }

            if self.pres[i] <= 500. && pcl.li5.is_nan() {
                let a = self.vtmp_at(500.);
                let b = thermo::wetlift(pe1, tp1, 500.);
                pcl.li5 = a - vt_sat(500., b);
            }
            if self.pres[i] <= 300. && pcl.li3.is_nan() {
                let a = self.vtmp_at(300.);
                let b = thermo::wetlift(pe1, tp1, 300.);
                pcl.li3 = a - vt_sat(300., b);
            }
        }

        if pcl.bplus.is_nan() {
            pcl.bplus = totp;
        }
        self.bulk_rich(&mut pcl);
        if pcl.bplus.floor() == 0. {
            pcl.bminus = 0.;
        }

        ptrace.extend_from_slice(&ptraces);
        ttrace.extend_from_slice(&ttraces);

        // minimum buoyancy below 500 hPa
        let idx: Vec<usize> = (0..ptrace.len()).filter(|&k| !ttrace[k].is_nan() && ptrace[k] >= 500.).collect();
        if !idx.is_empty() {
            let ps: Vec<f64> = idx.iter().map(|&k| ptrace[k]).collect();
            let env = self.many(Field::VirtualTemperature, &ps);
            let b: Vec<f64> = idx.iter().zip(&env).map(|(&k, e)| ttrace[k] - e).collect();
            if let Some(j) = masked_argmin(&b) {
                pcl.bmin = b[j];
                pcl.bminpres = ps[j];
            }
        }
        pcl.ptrace = ptrace;
        pcl.ttrace = ttrace;
        pcl
    }

    /// CIN below the LCL, 1 hPa steps, SHARPpy's virtual-potential-temperature
    /// formulation.
    fn dry_cin(&self, pbot: f64, blupper: f64, pres: f64, dwpc: f64, pe2: f64, tp2: f64) -> f64 {
        self.dry_cin_cached(pbot, blupper, pres, dwpc, pe2, tp2, None)
    }

    /// [`Profile::dry_cin`]; on the surface grid the environmental term is
    /// shared and (with `col`) the parcel moisture term is reused across
    /// calls with the same mixing ratio.
    #[allow(clippy::too_many_arguments)]
    fn dry_cin_cached(
        &self,
        pbot: f64,
        blupper: f64,
        pres: f64,
        dwpc: f64,
        pe2: f64,
        tp2: f64,
        col: Option<&mut DryParcelCol>,
    ) -> f64 {
        let theta_parcel = thermo::theta(pe2, tp2, 1000.);
        let blmr = thermo::mixratio(pres, dwpc);
        let sfc = self.sfc_prefix(pbot, blupper);
        let n = match sfc {
            Some((_, n)) => n,
            None => arange_len(pbot, blupper - 1., -1.),
        };
        if n < 2 {
            return 0.;
        }
        // environment: sample pressures, heights and the env term
        let owned;
        let (pp, hh, env): (&[f64], &[f64], &[f64]) = match sfc {
            Some((g, n)) => (&g.p[..n], &g.hght(self)[..n], &g.dry_env(self)[..n]),
            None => {
                let pp = arange(pbot, blupper - 1., -1.);
                let (hh, tt, dd) = self.many_htd(&pp);
                let env: Vec<f64> = (0..pp.len()).map(|k| dry_env_tv(pp[k], tt[k], dd[k])).collect();
                owned = (pp, hh, env);
                (&owned.0[..], &owned.1[..], &owned.2[..])
            }
        };
        // parcel moisture term, cached per mixing ratio on the surface grid
        let col = match (sfc, col) {
            (Some(_), Some(c)) => {
                if c.blmr != blmr.to_bits() {
                    c.blmr = blmr.to_bits();
                    c.w.clear();
                }
                while c.w.len() < n {
                    let k = c.w.len();
                    c.w.push(DryParcelCol::compute(blmr, pp[k]));
                }
                Some(&c.w[..n])
            }
            _ => None,
        };
        let tdef = |k: usize| {
            let w = match col {
                Some(c) => c[k],
                None => DryParcelCol::compute(blmr, pp[k]),
            };
            // virtemp_masked(p, theta_parcel, td): masked if any input is
            let tmp1 = match w {
                Some(w) if !theta_parcel.is_nan() => thermo::virtemp_w(theta_parcel, w),
                _ => f64::NAN,
            };
            let tv_env = env[k];
            (tmp1 - tv_env) / thermo::ctok(tv_env)
        };
        // select_sum(lyre, < 0): the negative layers, pairwise-summed in order
        let mut neg = Vec::with_capacity(n);
        let mut t0 = tdef(0);
        for k in 0..n - 1 {
            let t1 = tdef(k + 1);
            let lyre = G * (t0 + t1) / 2. * (hh[k + 1] - hh[k]);
            if lyre < 0. {
                neg.push(lyre);
            }
            t0 = t1;
        }
        let totn = pairwise_sum(&neg);
        if totn.is_nan() { 0. } else { totn }
    }

    /// SHARPpy `bulk_rich`: BRN shear term from the 0-500 m and 0-6 km mean
    /// winds of the parcel's layer.
    fn bulk_rich(&self, pcl: &mut RawParcel) {
        let flag = pcl.kind.flag();
        let (pbot, ptop);
        if (1..4).contains(&flag) {
            ptop = self.p_agl(6000.);
            pbot = self.psfc();
        } else {
            let h0 = self.hght(pcl.pres);
            let mut pb = self.pres(h0 - 500.);
            // SHARPpy quirk: a *valid* pbot is replaced by the surface
            if !pb.is_nan() {
                pb = self.psfc();
            }
            pbot = pb;
            let h1 = self.hght(pbot);
            ptop = self.pres(h1 + 6000.);
        }
        if pbot.is_nan() || ptop.is_nan() {
            return;
        }
        let p = self.pres(self.hght(pbot) + 500.);
        let (mnlu, mnlv) = self.mean_wind_raw(pbot, p, 0., 0.);
        let (mnuu, mnuv) = self.mean_wind_raw(pbot, ptop, 0., 0.);
        if pcl.bplus.is_nan() || mnlu.is_nan() || mnuu.is_nan() {
            return;
        }
        let dx = mnuu - mnlu;
        let dy = mnuv - mnlv;
        pcl.brnu = dx;
        pcl.brnv = dy;
        pcl.brnshear = sq(kts_to_ms(mag(dx, dy))) / 2.;
        pcl.brn = pcl.bplus / pcl.brnshear;
    }

    // ------------------------------------------------------------------
    // fast CAPE/CIN only (SHARPpy params.cape)
    // ------------------------------------------------------------------

    /// CAPE and CIN only (no levels or traces), used by the effective
    /// inflow layer search and the convective temperature iteration.
    /// `trunc` stops the moist integration at 500 hPa (SHARPpy quirk: it then
    /// adds a single layer from there to the top of the data).
    pub(crate) fn cape_fast(&self, pres: f64, tmpc: f64, dwpc: f64, trunc: bool) -> (f64, f64) {
        self.cape_fast_impl(pres, tmpc, dwpc, trunc, None)
    }

    /// Share the sub-LCL moisture column across convective-temperature trials.
    fn cape_fast_impl(
        &self,
        pres: f64,
        tmpc: f64,
        dwpc: f64,
        trunc: bool,
        col: Option<&mut DryParcelCol>,
    ) -> (f64, f64) {
        let mut bplus = NAN;
        let mut bminus = NAN;
        let mut totp = 0.;
        let mut totn;
        let mut pbot = self.psfc();
        let ptop = self.plast();
        if pbot > pres {
            pbot = pres;
        }
        if self.vtmp_at(pbot).is_nan() || self.vtmp_at(ptop).is_nan() {
            return (bplus, bminus);
        }
        let (pe2, tp2) = thermo::drylift(pres, tmpc, dwpc);
        if pe2.is_nan() {
            return (bplus, bminus);
        }
        totn = self.dry_cin_cached(pbot, pe2, pres, dwpc, pe2, tp2, col);
        if pbot > pe2 {
            pbot = pe2;
        }
        if pbot < self.plast() {
            return (bplus, bminus);
        }
        let n = self.len();
        let Some(lptr) = self.pres.iter().position(|&p| pbot > p) else {
            return (bplus, bminus);
        };
        let uptr = self.pres.iter().rposition(|&p| ptop < p).unwrap_or(0);
        // state at the previous level pe1: height, parcel temperature, theta
        // factor pow(1000 / pe1, ROCP) and buoyancy (carried level to level)
        let pe1 = pbot;
        let mut h1 = self.hght(pe1);
        let mut tp1 = tp2;
        let mut k1 = crate::numerics::pow(1000. / pe1, thermo::ROCP);
        let mut tdef1 = tdef(pe1, tp1, self.vtmp_at(pe1));
        for i in lptr..n {
            if self.tmpc[i].is_nan() {
                continue;
            }
            let pe2 = self.pres[i];
            let h2 = self.hght[i];
            let te2 = self.vtmp[i];
            let tp2 = thermo::wetlift_k(tp1, k1, pe2, self.rocp_fwd[i]);
            let tdef2 = tdef(pe2, tp2, te2);
            let lyre = G * (tdef1 + tdef2) / 2. * (h2 - h1);
            if lyre > 0. {
                totp += lyre;
            } else if pe2 > 500. {
                totn += lyre;
            }
            h1 = h2;
            tp1 = tp2;
            tdef1 = tdef2;
            k1 = self.rocp_inv[i];
            if (trunc && pe2 <= 500.) || (i >= uptr && bplus.is_nan()) {
                let (h3, tp3) = (h1, tp1);
                let mut lyrf = lyre;
                if lyrf > 0. {
                    bplus = totp - lyrf;
                    bminus = totn;
                } else {
                    bplus = totp;
                    bminus = if pe2 > 500. { totn + lyrf } else { totn };
                }
                let pe2 = ptop;
                let h2 = self.hght(pe2);
                let te2 = self.vtmp_at(pe2);
                let tp2 = thermo::wetlift_k(tp3, k1, pe2, self.rocp_fwd[n - 1]);
                lyrf = G * (tdef1 + tdef(pe2, tp2, te2)) / 2. * (h2 - h3);
                if lyrf > 0. {
                    bplus += lyrf;
                } else if pe2 > 500. {
                    bminus += lyrf;
                }
                if bplus == 0. {
                    bminus = 0.;
                }
                break;
            }
        }
        (bplus, bminus)
    }

    /// Effective inflow layer (pbot, ptop) in hPa: the first contiguous
    /// layer, from the surface up, of parcels with CAPE >= 100 J/kg and
    /// CIN > -250 J/kg.
    pub(crate) fn effective_inflow_layer_raw(&self, mucape: f64, mucinh: f64) -> (f64, f64) {
        let (ecape, ecinh) = (100., -250.);
        let mut pbot = NAN;
        let mut ptop = NAN;
        if mucape != 0. && mucape >= ecape && mucinh > ecinh {
            let energies = |i: usize| self.cape_fast(self.pres[i], self.tmpc[i], self.dwpc[i], false);
            let Some(bptr) = self.first_level(self.sfc..self.top, |i| {
                let (cape, cin) = energies(i);
                cape >= ecape && cin > ecinh
            }) else {
                return (NAN, NAN);
            };
            pbot = self.pres[bptr];
            if pbot.is_nan() {
                return (NAN, NAN);
            }
            let top = self.first_level(bptr + 1..self.top, |i| {
                // SHARPpy quirk: `if not dwpc or not tmpc` also skips levels
                // where T or Td is exactly 0 °C
                if self.dwpc[i].is_nan() || self.tmpc[i].is_nan() || self.dwpc[i] == 0. || self.tmpc[i] == 0. {
                    return false;
                }
                // A masked result satisfies neither inequality on its own.
                let (cape, cin) = energies(i);
                cape < ecape || cin <= ecinh
            });
            if let Some(i) = top {
                let mut j = 1;
                while self.dwpc[i - j].is_nan() && self.tmpc[i - j].is_nan() {
                    j += 1;
                }
                ptop = self.pres[i - j];
                if ptop > pbot {
                    ptop = pbot;
                }
            }
        }
        (pbot, ptop)
    }

    /// First index in `range` satisfying `pred` (evaluated in order). For
    /// large (high-resolution) profiles the predicate (a full parcel ascent)
    /// is evaluated speculatively in parallel blocks; the result is the same
    /// as the sequential scan.
    fn first_level(&self, range: std::ops::Range<usize>, pred: impl Fn(usize) -> bool + Sync) -> Option<usize> {
        const PARALLEL_MIN_LEVELS: usize = 400;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
        if self.len() < PARALLEL_MIN_LEVELS || threads < 2 {
            return range.into_iter().find(|&i| pred(i));
        }
        let mut start = range.start;
        while start < range.end {
            let end = (start + threads).min(range.end);
            let hits: Vec<bool> = std::thread::scope(|s| {
                let pred = &pred;
                let handles: Vec<_> = (start..end).map(|i| s.spawn(move || pred(i))).collect();
                handles.into_iter().map(|h| h.join().unwrap_or(false)).collect()
            });
            if let Some(k) = hits.iter().position(|&h| h) {
                return Some(start + k);
            }
            start = end;
        }
        None
    }

    /// Convective temperature (°C): surface temperature needed to remove
    /// all CIN, using the mean 100 hPa mixing ratio.
    pub(crate) fn convective_temp_raw(&self) -> f64 {
        let mincinh = 0.;
        let mmr = self.mean_mixratio_raw(None, None, Scheme::Sampled);
        let pres = self.psfc();
        let mut tmpc = self.tmpc[self.sfc];
        let dwpc = thermo::temp_at_mixrat(mmr, pres);
        // every ascent below has the same moisture: share the sub-LCL term
        let mut col = DryParcelCol::default();
        let mut cape = |t: f64| self.cape_fast_impl(pres, t, dwpc, true, Some(&mut col));
        let (bp, bm) = cape(tmpc + 25.);
        if bp == 0. || bm.is_nan() || bm < mincinh {
            return NAN;
        }
        let excess = dwpc - tmpc;
        if excess > 0. {
            tmpc = tmpc + excess + 4.;
        }
        let (bp, mut bm) = cape(tmpc);
        if bp == 0. || bm.is_nan() {
            bm = NAN;
        }
        let mut guard = 0;
        while (bm.is_nan() || bm < mincinh) && guard < 1000 {
            guard += 1;
            if bm < -100. {
                tmpc += 2.;
            } else {
                tmpc += 0.5;
            }
            let (bp, b) = cape(tmpc);
            bm = if bp == 0. { NAN } else { b };
        }
        tmpc
    }
}
