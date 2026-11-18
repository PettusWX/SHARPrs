//! SPC "possible hazard type" decision tree (SHARPpy `watch_type`).

/// Possible hazard type, as displayed in the SHARPpy hazard box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HazardType {
    PdsTor,
    Tor,
    MrglTor,
    Svr,
    MrglSvr,
    FlashFlood,
    Blizzard,
    ExcessiveHeat,
    None,
}

impl HazardType {
    pub fn label(self) -> &'static str {
        match self {
            HazardType::PdsTor => "PDS TOR",
            HazardType::Tor => "TOR",
            HazardType::MrglTor => "MRGL TOR",
            HazardType::Svr => "SVR",
            HazardType::MrglSvr => "MRGL SVR",
            HazardType::FlashFlood => "FLASH FLOOD",
            HazardType::Blizzard => "BLIZZARD",
            HazardType::ExcessiveHeat => "EXCESSIVE HEAT",
            HazardType::None => "NONE",
        }
    }
}

/// Inputs to the hazard-type decision tree. NaN = missing. Values already
/// selected for the deviant in question (right or left mover).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HazardInputs {
    pub lr1: f64,
    pub stp_eff: f64,
    pub stp_fixed: f64,
    pub srw_4_6km: f64,
    pub esrh: f64,
    pub srh1km: f64,
    pub latitude_negative: bool,
    pub sfc_8km_shear: f64,
    pub sfc_lcl: f64,
    pub ml_lcl: f64,
    pub ml_cinh: f64,
    pub ebotm: f64,
    pub low_rh: f64,
    pub mid_rh: f64,
    pub scp: f64,
    pub mu_cinh: f64,
    pub ship: f64,
    pub dcape: f64,
    pub sig_severe: f64,
    pub mmp: f64,
    pub wndg: f64,
    pub pw_flag: i8,
    pub upshear_speed: f64,
    pub sfc_wspd_mph: f64,
    pub sfc_tmpc: f64,
    pub precip_type_has_snow: bool,
    pub heat_index: f64,
}

pub(crate) fn possible_watch(x: &HazardInputs) -> Vec<HazardType> {
    let mut out = Vec::new();
    let (mut stp_eff, mut stp_fixed, mut esrh, mut srh1km) = (x.stp_eff, x.stp_fixed, x.esrh, x.srh1km);
    if x.latitude_negative {
        stp_eff = -stp_eff;
        stp_fixed = -stp_fixed;
        esrh = -esrh;
        srh1km = -srh1km;
    }
    let ebot0 = x.ebotm == 0.;
    let tor = if stp_eff >= 3.
        && stp_fixed >= 3.
        && srh1km >= 200.
        && esrh >= 200.
        && x.srw_4_6km >= 15.0
        && x.sfc_8km_shear > 45.0
        && x.sfc_lcl < 1000.
        && x.ml_lcl < 1200.
        && x.lr1 >= 5.0
        && x.ml_cinh > -50.
        && ebot0
    {
        Some(HazardType::PdsTor)
    } else if (stp_eff >= 3. || stp_fixed >= 4.) && x.ml_cinh > -125. && ebot0 {
        Some(HazardType::Tor)
    } else if (stp_eff >= 1. || stp_fixed >= 1.)
        && (x.srw_4_6km >= 15.0 || x.sfc_8km_shear >= 40.)
        && x.ml_cinh > -50.
        && ebot0
    {
        Some(HazardType::Tor)
    } else if (stp_eff >= 1. || stp_fixed >= 1.)
        && ((x.low_rh + x.mid_rh) / 2. >= 60.)
        && x.lr1 >= 5.0
        && x.ml_cinh > -50.
        && ebot0
    {
        Some(HazardType::Tor)
    } else if (stp_eff >= 1. || stp_fixed >= 1.) && x.ml_cinh > -150. && ebot0 {
        Some(HazardType::MrglTor)
    } else if (stp_eff >= 0.5 && esrh >= 150.) || (stp_fixed >= 0.5 && srh1km >= 150. && x.ml_cinh > -50. && ebot0) {
        // SHARPpy operator precedence: A or (B and C and D)
        Some(HazardType::MrglTor)
    } else {
        None
    };
    out.extend(tor);

    let scp = x.scp;
    if (stp_fixed >= 1.0 || scp >= 4.0 || stp_eff >= 1.0) && x.mu_cinh >= -50. {
        out.push(HazardType::Svr);
    } else if scp >= 2.0 && (x.ship >= 1.0 || x.dcape >= 750.) && x.mu_cinh >= -50. {
        out.push(HazardType::Svr);
    } else if x.sig_severe >= 30000. && x.mmp >= 0.6 && x.mu_cinh >= -50. {
        out.push(HazardType::Svr);
    } else if x.mu_cinh >= -75.0 && (x.wndg >= 0.5 || x.ship >= 0.5 || scp >= 0.5) {
        out.push(HazardType::MrglSvr);
    }

    if x.pw_flag >= 2 && x.upshear_speed < 25. {
        out.push(HazardType::FlashFlood);
    }
    if x.sfc_wspd_mph > 35. && x.sfc_tmpc <= 0. && x.precip_type_has_snow {
        out.push(HazardType::Blizzard);
    }
    if x.heat_index > 105. {
        out.push(HazardType::ExcessiveHeat);
    }
    out.push(HazardType::None);
    out
}
