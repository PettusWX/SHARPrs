//! Render-facing view model.
//!
//! [`SoundingView`] holds every array and number the SPC-window panels show.
//! It is deliberately independent of `sharprs-core`'s profile types: an
//! adapter fills it from a computed profile (or a test builds it by hand).
//!
//! Conventions (SHARPpy's):
//! * pressure in hPa, temperature in °C, heights in meters,
//! * winds in knots, as (u, v) components unless the field says
//!   "dir/spd" (meteorological direction in degrees, speed in knots),
//! * parcel heights (LCL/LFC/EL) are **AGL**, profile heights are **MSL**,
//! * profile arrays use `f64::NAN` for missing levels, scalars use `Option`.
//!
//! Anything that is `None`/empty is simply not drawn (or shown as `--`).

/// A (u, v) wind vector in knots.
pub type UV = (f64, f64);

/// Something with a right-mover and a left-mover variant (SHARPpy computes
/// most storm-relative quantities for both; the display shows one,
/// selected by [`crate::Deviant`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RL<T> {
    pub right: T,
    pub left: T,
}

impl<T> RL<T> {
    pub fn new(right: T, left: T) -> RL<T> {
        RL { right, left }
    }
    pub fn get(&self, d: crate::Deviant) -> &T {
        match d {
            crate::Deviant::Right => &self.right,
            crate::Deviant::Left => &self.left,
        }
    }
}

/// Vertical profile arrays (all the same length, surface first).
#[derive(Clone, Debug, Default)]
pub struct ProfileView {
    pub pres: Vec<f64>,
    /// Height MSL (m).
    pub hght: Vec<f64>,
    pub tmpc: Vec<f64>,
    pub dwpc: Vec<f64>,
    /// Virtual temperature (°C), dashed trace.
    pub vtmp: Vec<f64>,
    /// Wet-bulb temperature (°C).
    pub wetbulb: Vec<f64>,
    /// Equivalent potential temperature (K), theta-e panel.
    pub thetae: Vec<f64>,
    /// Wind components (kt).
    pub u: Vec<f64>,
    pub v: Vec<f64>,
    /// Vertical velocity (Pa/s or microbar/s as SHARPpy stores it); empty if absent.
    pub omeg: Vec<f64>,
    /// Index of the surface level in the arrays (SHARPpy `prof.sfc`).
    pub sfc: usize,
}

impl ProfileView {
    pub fn sfc_hght(&self) -> f64 {
        self.hght.get(self.sfc).copied().unwrap_or(f64::NAN)
    }
    pub fn sfc_pres(&self) -> f64 {
        self.pres.get(self.sfc).copied().unwrap_or(f64::NAN)
    }
}

/// One lifted parcel, as shown in the thermo table and on the skew-T.
#[derive(Clone, Debug, Default)]
pub struct ParcelView {
    /// Row label, e.g. "SFC", "ML", "FCST", "MU", "EFF", "USER".
    pub name: String,
    /// CAPE (J/kg).
    pub bplus: Option<f64>,
    /// CIN (J/kg, negative).
    pub bminus: Option<f64>,
    /// LCL/LFC/EL heights AGL (m).
    pub lclhght: Option<f64>,
    pub lfchght: Option<f64>,
    pub elhght: Option<f64>,
    /// Lifted index at 500 hPa.
    pub li5: Option<f64>,
    /// LCL/LFC/EL pressures (hPa) for the skew-T markers.
    pub lclpres: Option<f64>,
    pub lfcpres: Option<f64>,
    pub elpres: Option<f64>,
    /// 0-3 km CAPE (J/kg).
    pub b3km: Option<f64>,
    /// Pressures and heights (MSL, m) of the 0, -20, -30 °C environmental levels.
    pub p0c: Option<f64>,
    pub pm20c: Option<f64>,
    pub pm30c: Option<f64>,
    pub hght0c: Option<f64>,
    pub hghtm20c: Option<f64>,
    pub hghtm30c: Option<f64>,
    /// Virtual-temperature parcel trace (dashed line on the skew-T).
    pub ptrace: Vec<f64>,
    pub ttrace: Vec<f64>,
}

/// Inferred temperature advection layer (Inf. Temp. Adv. column).
#[derive(Clone, Copy, Debug, Default)]
pub struct AdvectionLayer {
    /// °C/hr; `None` for layers SHARPpy leaves blank.
    pub adv: Option<f64>,
    pub ptop: f64,
    pub pbot: f64,
}

/// Storm-slinky trajectory: storm-relative displacement (m) and height (m).
#[derive(Clone, Debug, Default)]
pub struct SlinkyView {
    pub traj: Vec<(f64, f64, f64)>,
    /// Updraft tilt (degrees).
    pub tilt: Option<f64>,
    /// True if the parcel has an EL (last point drawn in magenta).
    pub has_el: bool,
}

/// Values shown in the thermodynamic part of the bottom-left table.
#[derive(Clone, Debug, Default)]
pub struct ThermoIndices {
    /// Precipitable water (inches).
    pub pwat: Option<f64>,
    /// PW anomaly flag, -3..=3 (sigma from climatology), 0 = normal.
    pub pwv_flag: i32,
    /// Mean mixing ratio (g/kg).
    pub mean_mixr: Option<f64>,
    pub low_rh: Option<f64>,
    pub mid_rh: Option<f64>,
    pub dcape: Option<f64>,
    /// Downrush temperature (°F, SHARPpy convention).
    pub drush_f: Option<f64>,
    pub k_idx: Option<f64>,
    pub totals_totals: Option<f64>,
    /// Convective temperature (°F).
    pub conv_t_f: Option<f64>,
    /// Max forecast surface temperature (°F).
    pub max_t_f: Option<f64>,
    pub esp: Option<f64>,
    /// ESP was short-circuited to zero (lapse rate or MLCAPE below the
    /// gate). SHARPpy's function returns an integer then, displayed as "0".
    pub esp_gated: bool,
    pub mmp: Option<f64>,
    pub wndg: Option<f64>,
    pub tei: Option<f64>,
    /// ML parcel 0-3 km CAPE ("3CAPE").
    pub cape3km_ml: Option<f64>,
    pub mburst: Option<f64>,
    pub sig_severe: Option<f64>,
    /// Lapse rates (°C/km): sfc-3 km, 3-6 km, 850-500, 700-500.
    pub lapserate_3km: Option<f64>,
    pub lapserate_3_6km: Option<f64>,
    pub lapserate_850_500: Option<f64>,
    pub lapserate_700_500: Option<f64>,
}

/// Composite severe parameters (bottom of the thermo table, STP inset).
#[derive(Clone, Debug, Default)]
pub struct SevereIndices {
    pub scp: RL<Option<f64>>,
    pub stp_cin: RL<Option<f64>>,
    pub stp_fixed: RL<Option<f64>>,
    pub ship: Option<f64>,
}

/// Kinematic table values. Vectors given as dir/spd are `(deg, kt)`;
/// shears are bulk shear (u, v) in kt.
#[derive(Clone, Debug, Default)]
pub struct KinematicsView {
    pub srh1km: RL<Option<f64>>,
    pub srh3km: RL<Option<f64>>,
    pub esrh: RL<Option<f64>>,
    /// EHI for the 0-1 km, 0-3 km and effective layers (Pivotal layout).
    pub ehi1km: RL<Option<f64>>,
    pub ehi3km: RL<Option<f64>>,
    pub ehi_eff: RL<Option<f64>>,
    pub shear_1km: Option<UV>,
    pub shear_3km: Option<UV>,
    pub shear_6km: Option<UV>,
    pub shear_8km: Option<UV>,
    pub shear_lcl_el: Option<UV>,
    pub shear_eff: Option<UV>,
    pub ebwd: Option<UV>,
    /// Mean winds, dir/spd.
    pub mean_1km: Option<(f64, f64)>,
    pub mean_3km: Option<(f64, f64)>,
    pub mean_6km: Option<(f64, f64)>,
    pub mean_8km: Option<(f64, f64)>,
    pub mean_lcl_el: Option<(f64, f64)>,
    pub mean_eff: Option<(f64, f64)>,
    pub mean_ebw: Option<(f64, f64)>,
    /// Storm-relative mean winds, dir/spd.
    pub srw_1km: RL<Option<(f64, f64)>>,
    pub srw_3km: RL<Option<(f64, f64)>>,
    pub srw_6km: RL<Option<(f64, f64)>>,
    pub srw_8km: RL<Option<(f64, f64)>>,
    pub srw_lcl_el: RL<Option<(f64, f64)>>,
    pub srw_4_5km: RL<Option<(f64, f64)>>,
    pub srw_eff: RL<Option<(f64, f64)>>,
    pub srw_ebw: RL<Option<(f64, f64)>>,
    /// Layer-mean SR wind (u, v) for the SR-wind panel bars.
    pub srw_0_2km: RL<Option<UV>>,
    pub srw_4_6km: RL<Option<UV>>,
    pub srw_9_11km: RL<Option<UV>>,
    /// MU parcel BRN shear (m2/s2).
    pub brn_shear: Option<f64>,
    /// 1 km and 6 km AGL winds, dir/spd (kinematic-table barbs).
    pub wind1km: Option<(f64, f64)>,
    pub wind6km: Option<(f64, f64)>,
}

/// Storm motions and related hodograph annotations.
#[derive(Clone, Debug, Default)]
pub struct StormMotionView {
    /// Bunkers right/left mover (u, v), drawn as "+" markers.
    pub bunkers: RL<Option<UV>>,
    /// Active storm motion (SHARPpy `prof.srwind`); equals Bunkers unless
    /// the user moved it. Drawn as circles and labelled "ddd/ss RM/LM".
    pub storm_motion: RL<Option<UV>>,
    /// Corfidi upshear / downshear vectors (u, v).
    pub corfidi_up: Option<UV>,
    pub corfidi_dn: Option<UV>,
    /// LCL-EL mean wind, dir/spd (the brown square).
    pub mean_lcl_el: Option<(f64, f64)>,
    /// Critical angle (degrees).
    pub critical_angle: RL<Option<f64>>,
}

/// One SARS analogue match.
#[derive(Clone, Debug, Default)]
pub struct SarsMatch {
    /// Display date, e.g. "24 May 11 21Z (OUN)".
    pub label: String,
    /// Supercell: "SIGTOR"/"WEAKTOR"/"NONTOR"; hail: size in inches as text.
    pub kind: String,
    /// Hail size (in) for coloring; None for supercell matches.
    pub hail_size: Option<f64>,
}

/// One SARS category (supercell or significant hail).
#[derive(Clone, Debug, Default)]
pub struct SarsCategory {
    pub matches: Vec<SarsMatch>,
    pub n_loose: Option<u32>,
    /// Fraction 0..1 of loose matches that verified (sigtor / sig hail).
    pub prob: Option<f64>,
}

/// SARS inset data.
#[derive(Clone, Debug, Default)]
pub struct SarsView {
    pub supercell: RL<SarsCategory>,
    pub hail: RL<SarsCategory>,
}

/// Inputs to the "Effective Layer STP" inset probability box.
#[derive(Clone, Debug, Default)]
pub struct StpInsetView {
    pub mlcape: Option<f64>,
    /// ML LCL height AGL (m).
    pub mllcl: Option<f64>,
    pub esrh: RL<Option<f64>>,
    /// Effective bulk wind difference magnitude (kt).
    pub ebwd: Option<f64>,
}

/// Winter inset values.
#[derive(Clone, Debug, Default)]
pub struct WinterView {
    pub dgz_pbot: Option<f64>,
    pub dgz_ptop: Option<f64>,
    /// DGZ bottom/top heights (ft MSL).
    pub dgz_zbot_ft: Option<f64>,
    pub dgz_ztop_ft: Option<f64>,
    pub dgz_meanrh: Option<f64>,
    pub dgz_pw: Option<f64>,
    pub dgz_meanq: Option<f64>,
    pub dgz_meanomeg: Option<f64>,
    pub oprh: Option<f64>,
    /// Initial phase: (level hPa, height ft MSL, temp °C, phase string).
    pub init_phase: Option<(f64, f64, f64, String)>,
    pub tpos: f64,
    pub tneg: f64,
    pub ttop: f64,
    pub tbot: f64,
    pub wpos: f64,
    pub wneg: f64,
    pub wtop: f64,
    pub wbot: f64,
    pub precip_type: String,
    pub sfc_tmpf: Option<f64>,
}

/// Fire-weather inset values.
#[derive(Clone, Debug, Default)]
pub struct FireView {
    /// Surface wind dir/spd.
    pub sfc_wind: Option<(f64, f64)>,
    pub mean_wind_01km: Option<(f64, f64)>,
    pub mean_wind_pbl: Option<(f64, f64)>,
    pub pbl_max_wind: Option<(f64, f64)>,
    pub sfc_rh: Option<f64>,
    pub rh01km: Option<f64>,
    pub pbl_rh: Option<f64>,
    pub pwat: Option<f64>,
    pub bplus_fire: Option<f64>,
    pub pbl_h: Option<f64>,
    pub fosberg: Option<f64>,
    pub haines: Option<i32>,
    pub haines_elevation: Option<sharprs_core::HainesElevation>,
}

/// Everything the SPC window shows for one sounding.
#[derive(Clone, Debug, Default)]
pub struct SoundingView {
    /// Skew-T title, e.g. "KOAX   20140616/1900  (User Selected)".
    pub title: String,
    pub latitude: f64,
    /// Observed soundings do not show the omega profile.
    pub observed: bool,

    pub profile: ProfileView,

    /// Up to four parcels for the thermo table, in display order.
    pub parcels: Vec<ParcelView>,
    /// Index into `parcels` of the parcel lifted on the skew-T (highlighted row).
    pub selected_parcel: usize,

    /// Downdraft parcel trace (DCAPE), magenta dashed.
    pub dcape_ptrace: Vec<f64>,
    pub dcape_ttrace: Vec<f64>,

    /// Max 2-6 km lapse-rate layer: (lapse rate °C/km, pbot, ptop).
    pub max_lapse_rate_2_6: Option<(f64, f64, f64)>,
    /// Effective inflow layer (pbot, ptop).
    pub eff_inflow: Option<(f64, f64)>,
    /// Dendritic growth zone (pbot, ptop); drawn when `RenderOptions::show_dgz`.
    pub dgz: Option<(f64, f64)>,
    /// Wet-bulb zero and freezing level pressures (DGZ mode readouts).
    pub wbz_pres: Option<f64>,
    pub frz_pres: Option<f64>,
    /// PBL top pressure; drawn when `RenderOptions::show_pbl`.
    pub ppbl_top: Option<f64>,

    pub indices: ThermoIndices,
    pub severe: SevereIndices,
    pub kinematics: KinematicsView,
    pub storm: StormMotionView,

    pub inf_temp_adv: Vec<AdvectionLayer>,
    pub slinky: RL<Option<SlinkyView>>,
    /// "Possible hazard type" (e.g. "TOR", "SVR", "NONE").
    pub watch_type: RL<Option<String>>,

    pub sars: SarsView,
    pub stp_inset: StpInsetView,
    pub winter: Option<WinterView>,
    pub fire: Option<FireView>,
    /// Rotational velocity for the VROT inset (kt); 0 if not set.
    pub vrot: f64,
}
