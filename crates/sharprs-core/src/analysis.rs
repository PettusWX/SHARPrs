//! One-call computation of everything SHARPpy's main window displays
//! (the equivalent of SHARPpy's `ConvectiveProfile`).

use crate::climo;
use crate::downdraft::Downdraft;
use crate::fire::HainesElevation;
use crate::hazard::{self, HazardInputs, HazardType};
use crate::indices::{self, AdvectionLayer, Slinky};
use crate::layer::Scheme;
use crate::parcel::{Parcel, ParcelKind, ParcelSpec, RawParcel};
use crate::profile::{Profile, opt};
use crate::sars::{self, HailMatches, SupercellMatches};
use crate::thermo;
use crate::units::{Vector, comp2vec, kts_to_mph, kts_to_ms, mag, ms_to_kts};
use crate::winds::{Corfidi, Helicity, StormMotion};
use crate::winter::{InitialPhase, PosNeg, PrecipType};

/// Options for [`ConvectiveAnalysis::new`].
#[derive(Clone, Debug, Default)]
pub struct AnalysisOptions {
    /// User storm motion (SHARPpy "modify storm motion"). When `None` the
    /// Bunkers motion is used (effective-layer version if an effective
    /// inflow layer exists).
    pub storm_motion: Option<StormMotion>,
    /// Optional user parcel to lift (SHARPpy's USER parcel slot, which is
    /// empty until the user lifts one from the skew-T).
    pub user_parcel: Option<ParcelSpec>,
}

/// A value computed for both the right- and left-moving supercell.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Deviant<T> {
    pub right: T,
    pub left: T,
}

impl<T> Deviant<T> {
    /// The value SHARPpy shows by default: left mover in the Southern
    /// Hemisphere, right mover otherwise.
    pub fn selected(&self, southern_hemisphere: bool) -> &T {
        if southern_hemisphere { &self.left } else { &self.right }
    }
}

/// The six SHARPpy parcels.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Parcels {
    pub sfc: Parcel,
    pub fcst: Parcel,
    pub ml: Parcel,
    pub mu: Parcel,
    /// Effective-inflow-layer parcel (equals the surface parcel when there is
    /// no effective layer, as in SHARPpy).
    pub eff: Parcel,
    pub user: Option<Parcel>,
}

impl Parcels {
    pub fn get(&self, kind: ParcelKind) -> Option<&Parcel> {
        match kind {
            ParcelKind::Surface => Some(&self.sfc),
            ParcelKind::Forecast => Some(&self.fcst),
            ParcelKind::MixedLayer => Some(&self.ml),
            ParcelKind::MostUnstable => Some(&self.mu),
            ParcelKind::Effective => Some(&self.eff),
            ParcelKind::User => self.user.as_ref(),
        }
    }
}

/// Effective inflow layer.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EffectiveLayer {
    /// hPa
    pub pbot: f64,
    pub ptop: f64,
    /// m AGL
    pub hbot: f64,
    pub htop: f64,
}

/// Storm-relative quantities for one storm motion.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StormRelative {
    /// The storm motion used (kt).
    pub storm: Vector,
    /// Storm-relative pressure-weighted mean winds (kt) for the layers the
    /// kinematics panel shows.
    pub srw_1km: Option<Vector>,
    pub srw_3km: Option<Vector>,
    pub srw_6km: Option<Vector>,
    pub srw_8km: Option<Vector>,
    pub srw_4_5km: Option<Vector>,
    pub srw_lcl_el: Option<Vector>,
    pub srw_eff: Option<Vector>,
    pub srw_ebw: Option<Vector>,
    /// Layers used by the SR-wind-vs-height inset.
    pub srw_0_2km: Option<Vector>,
    pub srw_4_6km: Option<Vector>,
    pub srw_9_11km: Option<Vector>,
    pub srh_1km: Option<Helicity>,
    pub srh_3km: Option<Helicity>,
    /// Effective SRH.
    pub esrh: Option<Helicity>,
    pub critical_angle: Option<f64>,
}

/// Kinematic panel values (winds in kt).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Kinematics {
    pub wind_1km: Option<Vector>,
    pub wind_6km: Option<Vector>,
    pub shear_1km: Option<Vector>,
    pub shear_3km: Option<Vector>,
    pub shear_6km: Option<Vector>,
    pub shear_8km: Option<Vector>,
    pub shear_9km: Option<Vector>,
    pub shear_lcl_el: Option<Vector>,
    pub shear_eff: Option<Vector>,
    /// Effective bulk wind difference (effective bottom to half the MU EL).
    pub ebwd: Option<Vector>,
    pub mean_1km: Option<Vector>,
    pub mean_3km: Option<Vector>,
    pub mean_6km: Option<Vector>,
    pub mean_8km: Option<Vector>,
    pub mean_lcl_el: Option<Vector>,
    pub mean_eff: Option<Vector>,
    pub mean_ebw: Option<Vector>,
    pub corfidi: Option<Corfidi>,
    /// Bunkers motion (effective-layer method when possible).
    pub bunkers: Option<StormMotion>,
    /// Non-parcel (0-6 km) Bunkers motion.
    pub bunkers_non_parcel: Option<StormMotion>,
    pub storm_relative: Deviant<StormRelative>,
}

/// Lapse-rate layer with the maximum 2 km lapse rate between 2 and 6 km.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MaxLapseRate {
    /// °C/km
    pub rate: f64,
    pub pbot: f64,
    pub ptop: f64,
}

/// Thermodynamic indices.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThermoIndices {
    /// Precipitable water, sfc-400 hPa (in).
    pub pwat: Option<f64>,
    pub k_index: Option<f64>,
    pub totals_totals: Option<f64>,
    pub cross_totals: Option<f64>,
    pub vertical_totals: Option<f64>,
    /// Mean mixing ratio of the lowest 100 hPa (g/kg).
    pub mean_mixr: Option<f64>,
    /// Mean RH sfc-100 hPa above (%).
    pub low_rh: Option<f64>,
    /// Mean RH 150-350 hPa above the surface (%).
    pub mid_rh: Option<f64>,
    /// °C/km
    pub lapse_rate_0_3km: Option<f64>,
    pub lapse_rate_3_6km: Option<f64>,
    pub lapse_rate_850_500: Option<f64>,
    pub lapse_rate_700_500: Option<f64>,
    pub lapse_rate_0_1km: Option<f64>,
    pub max_lapse_rate: Option<MaxLapseRate>,
    /// Convective temperature (°F).
    pub conv_t: Option<f64>,
    /// Forecast max temperature (°F).
    pub max_t: Option<f64>,
    /// Theta-e index (K).
    pub tei: Option<f64>,
    /// Freezing level / wet-bulb zero (hPa, m AGL).
    pub frz_pres: Option<f64>,
    pub frz_hght: Option<f64>,
    pub wbz_pres: Option<f64>,
    pub wbz_hght: Option<f64>,
    /// PBL top (hPa).
    pub pbl_top: Option<f64>,
    /// Hail growth zone (-10 to -30 °C), hPa.
    pub hgz: (f64, f64),
}

/// Composite severe parameters.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviantSevere {
    pub stp_fixed: Option<f64>,
    pub stp_cin: Option<f64>,
    pub scp: Option<f64>,
    /// EHI with the surface parcel, 0-1 / 0-3 km SRH.
    pub ehi_sfc_1km: Option<f64>,
    pub ehi_sfc_3km: Option<f64>,
    pub ehi_ml_1km: Option<f64>,
    pub ehi_ml_3km: Option<f64>,
    /// EHI with MUCAPE and effective SRH.
    pub ehi_eff: Option<f64>,
    /// STP inset per-ingredient probabilities.
    pub stp_probs: climo::StpIngredientProbs,
    /// Conditional EF probabilities for this STPC.
    pub cond_tor_probs: Option<climo::CondTorProbs>,
    /// Large hail parameter.
    pub lhp: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Severe {
    pub deviant: Deviant<DeviantSevere>,
    pub ship: Option<f64>,
    /// Effective-layer SHERB.
    pub sherbe: Option<f64>,
    /// 0-3 km SHERB.
    pub sherb: Option<f64>,
    pub esp: Option<f64>,
    pub mmp: Option<f64>,
    pub wndg: Option<f64>,
    /// MLCAPE * 0-6 km shear (m³/s³).
    pub sig_severe: Option<f64>,
    pub mburst: Option<f64>,
    pub dcp: Option<f64>,
    pub sweat: Option<f64>,
    pub thetae_diff: Option<f64>,
    pub precip_eff: Option<f64>,
}

/// Fire-weather inset values.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FireWeather {
    pub fosberg: Option<f64>,
    pub haines_elevation: HainesElevation,
    pub haines_low: Option<f64>,
    pub haines_mid: Option<f64>,
    pub haines_high: Option<f64>,
    /// PBL top (hPa) and depth (m AGL).
    pub pbl_top: Option<f64>,
    pub pbl_depth: Option<f64>,
    pub sfc_rh: Option<f64>,
    pub rh_0_1km: Option<f64>,
    pub pbl_rh: Option<f64>,
    pub mean_wind_0_1km: Option<Vector>,
    pub mean_wind_pbl: Option<Vector>,
    /// Max wind in the PBL and its pressure (SHARPpy layer-index quirk kept).
    pub pbl_max_wind: Option<(Vector, f64)>,
    /// MU (lowest 500 hPa) CAPE shown in the fire inset.
    pub mu_cape: Option<f64>,
}

/// Winter-weather inset values.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Winter {
    /// Dendritic growth zone (-12 to -17 °C), hPa (surface pressure if not
    /// found, as in SHARPpy).
    pub dgz_pbot: f64,
    pub dgz_ptop: f64,
    pub dgz_mean_rh: Option<f64>,
    /// in
    pub dgz_pw: Option<f64>,
    pub dgz_mean_mixr: Option<f64>,
    /// µb/s (= 10 * Pa/s); `None` without omega data.
    pub dgz_mean_omega: Option<f64>,
    /// Omega * PW * RH product in the DGZ.
    pub oprh: Option<f64>,
    pub initial_phase: Option<InitialPhase>,
    pub temp_energy: Option<PosNeg>,
    pub wetbulb_energy: Option<PosNeg>,
    /// SHARPpy best-guess precipitation type (`.label()` gives its text).
    pub precip_type: PrecipType,
    pub wind_chill: Option<f64>,
}

/// SARS analog results.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sars {
    pub hail: Deviant<HailMatches>,
    pub supercell: Deviant<SupercellMatches>,
}

/// Everything SHARPpy's main window shows for one sounding.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConvectiveAnalysis {
    pub southern_hemisphere: bool,
    pub parcels: Parcels,
    pub effective_layer: Option<EffectiveLayer>,
    /// Storm motion used for storm-relative quantities.
    pub storm_motion: Option<StormMotion>,
    pub kinematics: Kinematics,
    pub thermo: ThermoIndices,
    pub severe: Severe,
    pub downdraft: Downdraft,
    pub fire: FireWeather,
    pub winter: Winter,
    pub sars: Sars,
    pub hazards: Deviant<Vec<HazardType>>,
    /// PW climatology anomaly flag (-3..=3), `None` without station/month.
    pub pw_flag: Option<i8>,
    /// Construction-time MU trajectory using the Bunkers right mover. The GUI
    /// trajectory follows the selected parcel and live storm motion separately.
    pub slinky: Option<Slinky>,
    pub temp_advection: Vec<AdvectionLayer>,
}

fn vec_opt(u: f64, v: f64) -> Option<Vector> {
    Vector::from_nan(u, v)
}
fn hel_opt(h: (f64, f64, f64)) -> Option<Helicity> {
    if h.0.is_nan() { None } else { Some(Helicity { total: h.0, positive: h.1, negative: h.2 }) }
}

/// Internal intermediate state shared between the steps.
struct Work {
    mu: RawParcel,
    sfc: RawParcel,
    ml: RawParcel,
    eff_layer: Option<(f64, f64)>,
    ebotm: f64,
    etopm: f64,
    storm: (f64, f64, f64, f64),
}

impl ConvectiveAnalysis {
    /// Compute everything with default options.
    pub fn compute(prof: &Profile) -> Self {
        Self::new(prof, &AnalysisOptions::default())
    }

    /// Compute everything.
    pub fn new(prof: &Profile, opts: &AnalysisOptions) -> Self {
        Self::new_impl(prof, opts, |_| {})
    }

    /// [`ConvectiveAnalysis::new`] with wall-clock timings of each internal
    /// stage (for profiling; not part of the stable API).
    #[doc(hidden)]
    pub fn new_timed(prof: &Profile, opts: &AnalysisOptions) -> (Self, Vec<(&'static str, std::time::Duration)>) {
        let mut marks = Vec::with_capacity(32);
        let mut last = std::time::Instant::now();
        let a = Self::new_impl(prof, opts, |stage| {
            let now = std::time::Instant::now();
            marks.push((stage, now - last));
            last = now;
        });
        (a, marks)
    }

    #[inline(always)]
    fn new_impl(prof: &Profile, opts: &AnalysisOptions, mut mark: impl FnMut(&'static str)) -> Self {
        let lat = prof.meta.latitude.filter(|l| !l.is_nan());
        let south = lat.is_some_and(|l| l < 0.);

        // ---------------- parcels ---------------------------------------
        let mu = prof.lift_raw(&prof.parcel_spec(ParcelKind::MostUnstable));
        let sfc = if mu.pres == prof.psfc() {
            let mut s = mu.clone();
            s.kind = ParcelKind::Surface;
            s
        } else {
            prof.lift_raw(&prof.parcel_spec(ParcelKind::Surface))
        };
        mark("parcel mu+sfc");
        let fcst = prof.lift_raw(&prof.parcel_spec(ParcelKind::Forecast));
        mark("parcel fcst");
        let ml = prof.lift_raw(&prof.parcel_spec(ParcelKind::MixedLayer));
        mark("parcel ml");
        let (ebot, etop) = prof.effective_inflow_layer_raw(mu.bplus, mu.bminus);
        mark("effective layer");
        let eff_layer = if ebot.is_nan() || etop.is_nan() { None } else { Some((ebot, etop)) };
        let (eff, ebotm, etopm) = match eff_layer {
            None => {
                let mut e = sfc.clone();
                e.kind = ParcelKind::Effective;
                (e, f64::NAN, f64::NAN)
            }
            Some((pb, pt)) => {
                let ebotm = prof.agl(prof.hght(pb));
                let etopm = prof.agl(prof.hght(pt));
                let mtha = prof.mean_theta_raw(Some(pb), Some(pt), Scheme::Sampled);
                let mmr = prof.mean_mixratio_raw(Some(pb), Some(pt), Scheme::Sampled);
                let effpres = (pb + pt) / 2.;
                let spec = ParcelSpec {
                    kind: ParcelKind::Effective,
                    pres: effpres,
                    tmpc: thermo::theta(1000., mtha, effpres),
                    dwpc: thermo::temp_at_mixrat(mmr, effpres),
                };
                (prof.lift_raw(&spec), ebotm, etopm)
            }
        };
        mark("parcel eff");
        let user = opts.user_parcel.as_ref().map(|s| prof.lift(s));

        // ---------------- storm motion -----------------------------------
        let bunkers = match eff_layer {
            None => prof.non_parcel_bunkers_raw(),
            Some((pb, _)) => prof.bunkers_raw(mu.bplus, mu.elhght, pb),
        };
        let storm = match &opts.storm_motion {
            Some(sm) => (sm.right.u, sm.right.v, sm.left.u, sm.left.v),
            None => bunkers,
        };
        let mut w = Work { mu, sfc, ml, eff_layer, ebotm, etopm, storm: bunkers };
        mark("parcel user + bunkers");

        let initial = Self::kinematics(prof, &w, bunkers);
        let sars = Self::sars(prof, &w, &initial);
        w.storm = storm;
        let kinematics = if opts.storm_motion.is_some() {
            Self::kinematics(prof, &w, bunkers)
        } else {
            initial
        };
        mark("kinematics");
        let thermo_ix = Self::thermo_indices(prof);
        mark("thermo indices");
        let fire = Self::fire(prof);
        mark("fire");
        let winter = Self::winter(prof);
        mark("winter");
        let (dcape, dt, dp) = prof.dcape_raw();
        let downdraft = Downdraft {
            dcape: opt(dcape),
            trace: dp.iter().zip(&dt).filter(|(p, t)| !p.is_nan() && !t.is_nan()).map(|(&p, &t)| (p, t)).collect(),
            downt_f: dt.last().and_then(|&t| opt(thermo::ctof(t))),
        };
        mark("dcape");
        let severe = Self::severe(prof, &w, &kinematics, &thermo_ix, dcape, south);
        mark("severe");

        let pw_flag = match (&prof.meta.station, prof.meta.valid) {
            (Some(st), Some(dt)) => {
                let pw300 = prof.precip_water_raw(None, 300.);
                Some(climo::pw_flag(pw300, Some(st.as_str()), dt.month))
            }
            _ => None,
        };

        mark("pw flag");
        let temp_advection = prof.inferred_temp_adv_raw(lat);
        mark("temp advection");

        let parcels = Parcels {
            sfc: w.sfc.to_public(),
            fcst: fcst.to_public(),
            ml: w.ml.to_public(),
            mu: w.mu.to_public(),
            eff: eff.to_public(),
            user,
        };
        let slinky = prof.parcel_trajectory(&parcels.mu, Vector::new(bunkers.0, bunkers.1));
        mark("slinky");
        let effective_layer = eff_layer.map(|(pbot, ptop)| EffectiveLayer { pbot, ptop, hbot: ebotm, htop: etopm });
        let storm_motion = (!storm.0.is_nan() && !storm.2.is_nan()).then(|| StormMotion {
            right: Vector::new(storm.0, storm.1),
            left: Vector::new(storm.2, storm.3),
        });

        mark("assemble");
        let mut out = ConvectiveAnalysis {
            southern_hemisphere: south,
            parcels,
            effective_layer,
            storm_motion,
            kinematics,
            thermo: thermo_ix,
            severe,
            downdraft,
            fire,
            winter,
            sars,
            hazards: Deviant { right: vec![], left: vec![] },
            pw_flag,
            slinky,
            temp_advection,
        };
        out.hazards = Deviant {
            right: out.hazard_types(prof, &w, false),
            left: out.hazard_types(prof, &w, true),
        };
        mark("hazards");
        out
    }

    /// Storm-slinky for an arbitrary parcel and storm motion.
    pub fn slinky_for(prof: &Profile, parcel_spec: &ParcelSpec, storm: Vector) -> Option<Slinky> {
        prof.parcel_trajectory(&prof.lift(parcel_spec), storm)
    }

    /// Hazard types shown by default (left mover in the Southern Hemisphere).
    pub fn hazard_types_selected(&self) -> &[HazardType] {
        self.hazards.selected(self.southern_hemisphere)
    }

    // ----------------------------------------------------------------------
    fn kinematics(prof: &Profile, w: &Work, bunkers: (f64, f64, f64, f64)) -> Kinematics {
        let sfc = prof.psfc();
        let hs = [1000., 3000., 4000., 5000., 6000., 8000., 9000.].map(|h| prof.msl(h));
        let ps = prof.pres_many(&hs);
        let (p1km, p3km, p4km, p5km, p6km, p8km, p9km) = (ps[0], ps[1], ps[2], ps[3], ps[4], ps[5], ps[6]);
        let comp = |p: f64| {
            let (u, v) = prof.components(p);
            vec_opt(u, v)
        };
        let shear = |a: f64, b: f64| {
            let (u, v) = prof.wind_shear_raw(a, b);
            vec_opt(u, v)
        };
        let mean = |a: f64, b: f64| {
            let (u, v) = prof.mean_wind_raw(a, b, 0., 0.);
            vec_opt(u, v)
        };
        let mu = &w.mu;
        let (mean_eff, mean_ebw, shear_eff, ebwd, elh) = match w.eff_layer {
            None => (None, None, None, None, f64::NAN),
            Some((pb, pt)) => {
                let depth = (mu.elhght - w.ebotm) / 2.;
                let elh = prof.pres(prof.msl(w.ebotm + depth));
                (mean(pb, pt), mean(pb, elh), shear(pb, pt), shear(pb, elh), elh)
            }
        };
        let p2km = prof.p_agl(2000.);
        let p4kmb = prof.p_agl(4000.);
        let p9kmb = prof.p_agl(9000.);
        let p11km = prof.p_agl(11000.);
        let sr = |stu: f64, stv: f64| -> StormRelative {
            let srw = |a: f64, b: f64| {
                let (u, v) = prof.mean_wind_raw(a, b, stu, stv);
                vec_opt(u, v)
            };
            let (srw_eff, srw_ebw, esrh, crit) = match w.eff_layer {
                None => (None, None, None, None),
                Some((pb, pt)) => (
                    srw(pb, pt),
                    srw(pb, elh),
                    hel_opt(prof.helicity_raw(w.ebotm, w.etopm, stu, stv)),
                    opt(prof.critical_angle_raw(stu, stv)),
                ),
            };
            StormRelative {
                storm: Vector::new(stu, stv),
                srw_1km: srw(sfc, p1km),
                srw_3km: srw(sfc, p3km),
                srw_6km: srw(sfc, p6km),
                srw_8km: srw(sfc, p8km),
                srw_4_5km: srw(p4km, p5km),
                srw_lcl_el: srw(mu.lclpres, mu.elpres),
                srw_eff,
                srw_ebw,
                srw_0_2km: srw(sfc, p2km),
                srw_4_6km: srw(p4kmb, p6km),
                srw_9_11km: srw(p9kmb, p11km),
                srh_1km: hel_opt(prof.helicity_raw(0., 1000., stu, stv)),
                srh_3km: hel_opt(prof.helicity_raw(0., 3000., stu, stv)),
                esrh,
                critical_angle: crit,
            }
        };
        let (cu, cv, du, dv) = prof.corfidi_raw();
        let np = prof.non_parcel_bunkers_raw();
        let sm = |t: (f64, f64, f64, f64)| {
            Some(StormMotion { right: vec_opt(t.0, t.1)?, left: vec_opt(t.2, t.3)? })
        };
        Kinematics {
            wind_1km: comp(p1km),
            wind_6km: comp(p6km),
            shear_1km: shear(sfc, p1km),
            shear_3km: shear(sfc, p3km),
            shear_6km: shear(sfc, p6km),
            shear_8km: shear(sfc, p8km),
            shear_9km: shear(sfc, p9km),
            shear_lcl_el: shear(mu.lclpres, mu.elpres),
            shear_eff,
            ebwd,
            mean_1km: mean(sfc, p1km),
            mean_3km: mean(sfc, p3km),
            mean_6km: mean(sfc, p6km),
            mean_8km: mean(sfc, p8km),
            mean_lcl_el: mean(mu.lclpres, mu.elpres),
            mean_eff,
            mean_ebw,
            corfidi: vec_opt(cu, cv).zip(vec_opt(du, dv)).map(|(a, b)| Corfidi { upshear: a, downshear: b }),
            bunkers: sm(bunkers),
            bunkers_non_parcel: sm(np),
            storm_relative: Deviant { right: sr(w.storm.0, w.storm.1), left: sr(w.storm.2, w.storm.3) },
        }
    }

    fn thermo_indices(prof: &Profile) -> ThermoIndices {
        let frz = prof.temp_lvl_raw(0., false);
        let wbz = prof.temp_lvl_raw(0., true);
        let mlr = prof.max_lapse_rate_raw();
        ThermoIndices {
            pwat: opt(prof.precip_water_raw(None, 400.)),
            k_index: opt(prof.k_index_raw()),
            totals_totals: opt(prof.t_totals_raw()),
            cross_totals: opt(prof.c_totals_raw()),
            vertical_totals: opt(prof.v_totals_raw()),
            mean_mixr: opt(prof.mean_mixr_sfc100()),
            low_rh: opt(prof.mean_relh_raw(None, None)),
            mid_rh: opt(prof.mean_relh_raw(Some(prof.psfc() - 150.), Some(prof.psfc() - 350.))),
            lapse_rate_0_3km: opt(prof.lapse_rate_raw(0., 3000., false)),
            lapse_rate_3_6km: opt(prof.lapse_rate_raw(3000., 6000., false)),
            lapse_rate_850_500: opt(prof.lapse_rate_raw(850., 500., true)),
            lapse_rate_700_500: opt(prof.lapse_rate_raw(700., 500., true)),
            lapse_rate_0_1km: opt(prof.lapse_rate_raw(0., 1000., false)),
            max_lapse_rate: (!mlr.0.is_nan()).then_some(MaxLapseRate { rate: mlr.0, pbot: mlr.1, ptop: mlr.2 }),
            conv_t: opt(thermo::ctof(prof.convective_temp_raw())),
            max_t: opt(thermo::ctof(prof.max_temp_raw())),
            tei: opt(prof.tei_raw()),
            frz_pres: opt(frz),
            frz_hght: opt(prof.agl(prof.hght(frz))),
            wbz_pres: opt(wbz),
            wbz_hght: opt(prof.agl(prof.hght(wbz))),
            pbl_top: opt(prof.pbl_top_raw()),
            hgz: prof.temp_layer(-10., -30.),
        }
    }

    fn fire(prof: &Profile) -> FireWeather {
        let psfc = prof.psfc();
        let ppbl = prof.pbl_top_raw();
        let p1km = prof.p_agl(1000.);
        let pbl_h = prof.agl(prof.hght(ppbl));
        let (mu, mv) = prof.mean_wind_raw(psfc, p1km, 0., 0.);
        let (pu, pv) = prof.mean_wind_raw(psfc, ppbl, 0., 0.);
        let (xu, xv, xp) = prof.max_wind_raw(0., pbl_h);
        let mu500 = prof.mu_spec(500.);
        let (bplus, _) = prof.cape_fast(mu500.pres, mu500.tmpc, mu500.dwpc, false);
        FireWeather {
            fosberg: opt(prof.fosberg_raw()),
            haines_elevation: prof.haines_elevation(),
            haines_low: opt(prof.haines_raw(HainesElevation::Low)),
            haines_mid: opt(prof.haines_raw(HainesElevation::Mid)),
            haines_high: opt(prof.haines_raw(HainesElevation::High)),
            pbl_top: opt(ppbl),
            pbl_depth: opt(pbl_h),
            sfc_rh: opt(thermo::relh(psfc, prof.tmpc[prof.sfc], prof.dwpc[prof.sfc])),
            rh_0_1km: opt(prof.mean_relh_raw(Some(psfc), Some(p1km))),
            pbl_rh: opt(prof.mean_relh_raw(Some(psfc), Some(ppbl))),
            mean_wind_0_1km: vec_opt(mu, mv),
            mean_wind_pbl: vec_opt(pu, pv),
            pbl_max_wind: vec_opt(xu, xv).map(|v| (v, xp)),
            mu_cape: opt(bplus),
        }
    }

    fn winter(prof: &Profile) -> Winter {
        let (pbot, ptop) = prof.temp_layer(-12., -17.);
        let rh = prof.mean_relh_raw(Some(pbot), Some(ptop));
        let pw = prof.precip_water_raw(Some(pbot), ptop);
        let q = prof.mean_mixratio_raw(Some(pbot), Some(ptop), Scheme::Sampled);
        let om = prof.mean_omega_raw(Some(pbot), Some(ptop)) * 10.;
        let oprh = om * pw * (rh / 100.);
        let init = prof.init_phase_raw();
        let start = init.map(|i| i.pres).unwrap_or(-9999.);
        let tpn = prof.posneg_raw(start, false);
        let wpn = prof.posneg_raw(start, true);
        let (tpos, tneg) = tpn.map(|x| (x.pos, x.neg)).unwrap_or((f64::NAN, f64::NAN));
        let precip_type = prof.best_guess_precip_raw(init.as_ref(), tpos, tneg);
        Winter {
            dgz_pbot: pbot,
            dgz_ptop: ptop,
            dgz_mean_rh: opt(rh),
            dgz_pw: opt(pw),
            dgz_mean_mixr: opt(q),
            dgz_mean_omega: opt(om),
            oprh: opt(oprh),
            initial_phase: init,
            temp_energy: tpn,
            wetbulb_energy: wpn,
            precip_type,
            wind_chill: opt(prof.wind_chill_raw()),
        }
    }

    fn severe(
        prof: &Profile,
        w: &Work,
        k: &Kinematics,
        th: &ThermoIndices,
        dcape: f64,
        south: bool,
    ) -> Severe {
        let s6 = k.shear_6km.map(|v| (v.u, v.v)).unwrap_or((f64::NAN, f64::NAN));
        let shr6_ms = kts_to_ms(mag(s6.0, s6.1));
        let ebwd = k.ebwd.map(|v| (v.u, v.v));
        let ebwspd = ebwd.map(|(u, v)| mag(u, v)).unwrap_or(f64::NAN);
        let lr75 = prof.lapse_rate_raw(700., 500., true);
        let dev = |sr: &StormRelative| -> DeviantSevere {
            let srh1 = sr.srh_1km.map(|h| h.total).unwrap_or(f64::NAN);
            let srh3 = sr.srh_3km.map(|h| h.total).unwrap_or(f64::NAN);
            let esrh = sr.esrh.map(|h| h.total).unwrap_or(f64::NAN);
            let stp_fixed = indices::stp_fixed(w.sfc.bplus, w.sfc.lclhght, srh1, shr6_ms);
            let (scp, stp_cin) = if w.eff_layer.is_none() {
                (0., 0.)
            } else {
                let scp = indices::scp(w.mu.bplus, esrh, kts_to_ms(ebwspd));
                let e = if south { -esrh } else { esrh };
                let mut stpc = indices::stp_cin(w.ml.bplus, e, kts_to_ms(ebwspd), w.ml.lclhght, w.ml.bminus);
                if south {
                    stpc = -stpc;
                }
                (scp, stpc)
            };
            let sgn = if south { -1. } else { 1. };
            let stp_probs = climo::stp_ingredient_probs(
                w.ml.bplus,
                w.ml.lclhght,
                sgn * esrh,
                if w.eff_layer.is_none() { f64::NAN } else { ebwspd },
                sgn * stp_cin,
                sgn * stp_fixed,
            );
            DeviantSevere {
                stp_fixed: opt(stp_fixed),
                stp_cin: opt(stp_cin),
                scp: opt(scp),
                ehi_sfc_1km: opt(indices::ehi(w.sfc.bplus, srh1)),
                ehi_sfc_3km: opt(indices::ehi(w.sfc.bplus, srh3)),
                ehi_ml_1km: opt(indices::ehi(w.ml.bplus, srh1)),
                ehi_ml_3km: opt(indices::ehi(w.ml.bplus, srh3)),
                ehi_eff: if w.eff_layer.is_some() { opt(indices::ehi(w.mu.bplus, esrh)) } else { None },
                stp_probs,
                cond_tor_probs: (!stp_cin.is_nan()).then(|| climo::cond_tor_probs(stp_cin)),
                lhp: opt(prof.lhp_raw(&w.mu, s6, lr75, sr.storm.u, sr.storm.v)),
            }
        };
        let deviant = Deviant { right: dev(&k.storm_relative.right), left: dev(&k.storm_relative.left) };
        let sherbe = match ebwd {
            Some(e) => prof.sherbe_raw(e),
            None => f64::NAN,
        };
        let lr03 = th.lapse_rate_0_3km.unwrap_or(f64::NAN);
        let esp = if lr03 < 7. || w.ml.bplus < 250. { 0. } else { (w.ml.b3km / 50.) * ((lr03 - 7.0) / (1.0)) };
        let pwat = th.pwat.unwrap_or(f64::NAN);
        let mean6 = k.mean_6km.map(|v| v.speed()).unwrap_or(f64::NAN);
        let dcp = (dcape / 980.) * (w.mu.bplus / 2000.) * (mag(s6.0, s6.1) / 20.) * (mean6 / 16.);
        Severe {
            deviant,
            ship: opt(prof.ship_raw(&w.mu, s6)),
            sherbe: opt(sherbe),
            sherb: opt(prof.sherb3_raw()),
            esp: opt(esp),
            mmp: opt(prof.mmp_raw(w.mu.bplus)),
            wndg: opt(prof.wndg_raw(&w.ml)),
            sig_severe: opt(w.ml.bplus * shr6_ms),
            mburst: opt(prof.mburst_raw(&w.sfc, lr03, dcape, pwat)),
            dcp: opt(dcp),
            sweat: opt(prof.sweat_raw()),
            thetae_diff: opt(prof.thetae_diff_raw()),
            precip_eff: opt(prof.precip_eff_raw(pwat)),
        }
    }

    fn sars(prof: &Profile, w: &Work, k: &Kinematics) -> Sars {
        let sp = |v: Option<Vector>| kts_to_ms(v.map(|v| v.speed()).unwrap_or(f64::NAN));
        let s6 = sp(k.shear_6km);
        let s3 = sp(k.shear_3km);
        let s9 = sp(k.shear_9km);
        let h500t = prof.temp(500.);
        let lr = prof.lapse_rate_raw(700., 500., true);
        let mumr = thermo::mixratio(w.mu.pres, w.mu.dwpc);
        let tot = |h: Option<Helicity>| h.map(|h| h.total).unwrap_or(f64::NAN);
        let r = &k.storm_relative.right;
        let l = &k.storm_relative.left;
        let hail = |srh3: f64| sars::hail_matches(mumr, w.mu.bplus, h500t, lr, s6, s9, s3, srh3);
        let sup = |srh1: f64, srh3: f64| {
            sars::supercell_matches(
                w.ml.bplus,
                w.ml.lclhght,
                h500t,
                lr,
                ms_to_kts(s6),
                srh1,
                ms_to_kts(s3),
                ms_to_kts(s9),
                srh3,
            )
        };
        Sars {
            hail: Deviant { right: hail(tot(r.srh_3km)), left: hail(-tot(l.srh_3km)) },
            supercell: Deviant {
                right: sup(tot(r.srh_1km), tot(r.srh_3km)),
                left: sup(-tot(l.srh_1km), -tot(l.srh_3km)),
            },
        }
    }

    fn hazard_types(&self, prof: &Profile, w: &Work, left: bool) -> Vec<HazardType> {
        let sr = if left { &self.kinematics.storm_relative.left } else { &self.kinematics.storm_relative.right };
        let ds = if left { &self.severe.deviant.left } else { &self.severe.deviant.right };
        let f = |x: Option<f64>| x.unwrap_or(f64::NAN);
        let tot = |h: Option<Helicity>| h.map(|h| h.total).unwrap_or(f64::NAN);
        let s8 = self.kinematics.shear_8km.map(|v| v.speed()).unwrap_or(f64::NAN);
        let ups = self.kinematics.corfidi.map(|c| comp2vec(c.upshear.u, c.upshear.v).1).unwrap_or(f64::NAN);
        let sfc_t = prof.tmpc[prof.sfc];
        let temp_f = thermo::ctof(sfc_t);
        // SHARPpy quirk: RH for the heat index is computed with T in °F
        let rh = thermo::relh(prof.psfc(), temp_f, prof.dwpc[prof.sfc]);
        let x = HazardInputs {
            lr1: prof.lapse_rate_raw(0., 1000., false),
            stp_eff: f(ds.stp_cin),
            stp_fixed: f(ds.stp_fixed),
            srw_4_6km: sr.srw_4_6km.map(|v| v.speed()).unwrap_or(f64::NAN),
            esrh: tot(sr.esrh),
            srh1km: tot(sr.srh_1km),
            latitude_negative: self.southern_hemisphere,
            sfc_8km_shear: s8,
            sfc_lcl: w.sfc.lclhght,
            ml_lcl: w.ml.lclhght,
            ml_cinh: w.ml.bminus,
            ebotm: w.ebotm,
            low_rh: f(self.thermo.low_rh),
            mid_rh: f(self.thermo.mid_rh),
            scp: f(ds.scp),
            mu_cinh: w.mu.bminus,
            ship: f(self.severe.ship),
            dcape: f(self.downdraft.dcape),
            sig_severe: f(self.severe.sig_severe),
            mmp: f(self.severe.mmp),
            wndg: f(self.severe.wndg),
            pw_flag: self.pw_flag.unwrap_or(0),
            upshear_speed: ups,
            sfc_wspd_mph: kts_to_mph(prof.wspd[prof.sfc]),
            sfc_tmpc: sfc_t,
            precip_type_has_snow: self.winter.precip_type.label().contains("Snow"),
            heat_index: indices::heat_index(temp_f, rh),
        };
        hazard::possible_watch(&x)
    }
}

// --------------------------------------------------------------------------
// profile modification hooks
// --------------------------------------------------------------------------
impl Profile {
    /// SHARPpy "modify surface": replace the surface temperature and/or
    /// dewpoint. Returns a new profile (rebuilt from u/v winds, as SHARPpy's
    /// copy does).
    pub fn with_surface(&self, tmpc: Option<f64>, dwpc: Option<f64>) -> Result<Profile, crate::ProfileError> {
        let mut s = self.to_sounding();
        if let Some(t) = tmpc {
            s.tmpc[self.sfc] = Some(t);
        }
        if let Some(d) = dwpc {
            s.dwpc[self.sfc] = Some(d);
        }
        Profile::new(&s)
    }

    /// SHARPpy "modify surface" with the *mix* option: the surface potential
    /// temperature and mixing ratio are mixed upward through every level whose
    /// potential temperature is not warmer than the new surface value.
    pub fn with_mixed_surface(&self, tmpc: f64, dwpc: f64) -> Result<Profile, crate::ProfileError> {
        let mut s = self.to_sounding();
        let theta = thermo::ctok(thermo::theta(self.psfc(), tmpc, 1000.));
        let mut th = self.theta.clone();
        th[self.sfc] = theta;
        let mixrat = thermo::mixratio(self.psfc(), dwpc);
        for i in 0..self.len() {
            if th[i] <= theta {
                let p = self.pres[i];
                let t = thermo::ktoc(theta / crate::numerics::pow(1000. / p, thermo::ROCP));
                s.tmpc[i] = opt(t);
                s.dwpc[i] = opt(thermo::temp_at_mixrat(mixrat, p));
            }
        }
        Profile::new(&s)
    }

    /// SHARPpy "interpolate profile": resample every `dp` hPa (default 25)
    /// from the surface to the top of the temperature data.
    pub fn interpolated(&self, dp: f64) -> Result<Profile, crate::ProfileError> {
        let pres = crate::numerics::arange(self.psfc(), self.pres[self.top], -dp.abs());
        let o = |v: Vec<f64>| v.into_iter().map(opt).collect::<Vec<_>>();
        let tmpc = self.many(crate::Field::Temperature, &pres);
        let dwpc = self.many(crate::Field::Dewpoint, &pres);
        let hght = self.many(crate::Field::Height, &pres);
        let mut s = crate::Sounding::new(
            pres.iter().map(|&p| Some(p)).collect(),
            o(hght),
            o(tmpc),
            o(dwpc),
        )
        .with_meta(self.meta.clone());
        if self.has_omega() {
            s = s.with_omega(o(self.many(crate::Field::Omega, &pres)));
        }
        let (u, v) = self.components_many(&pres);
        if self.has_wind {
            s = s.with_component_wind(o(u), o(v));
        }
        Profile::new(&s)
    }
}
