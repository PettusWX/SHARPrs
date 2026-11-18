//! SARS (Sounding Analog Retrieval System) supercell and significant-hail
//! analog matching. The SPC databases are embedded (public domain, see
//! NOTICE).

use std::sync::OnceLock;

const HAIL_DB: &str = include_str!("../data/sars_hail.txt");
const SUPERCELL_DB: &str = include_str!("../data/sars_supercell.txt");

struct HailRow {
    id: String,
    size: f64,
    mucape: f64,
    mumr: f64,
    t500: f64,
    lr75: f64,
    shr3: f64,
    shr6: f64,
    shr9: f64,
    srh3: f64,
}

struct SupercellRow {
    id: String,
    category: f64,
    category_str: String,
    mlcape: f64,
    mllcl: f64,
    srh1: f64,
    shr6: f64,
    t500: f64,
    lr75: f64,
    shr3: f64,
    shr9: f64,
    srh3: f64,
}

fn parse_rows(text: &str) -> Vec<Vec<&str>> {
    text.lines()
        .skip(1) // header
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|f| !f.is_empty())
        .collect()
}

fn f(s: &str) -> f64 {
    s.parse().unwrap_or(f64::NAN)
}

fn hail_db() -> &'static [HailRow] {
    static DB: OnceLock<Vec<HailRow>> = OnceLock::new();
    DB.get_or_init(|| {
        parse_rows(HAIL_DB)
            .into_iter()
            .map(|c| HailRow {
                id: c[0].to_string(),
                size: f(c[2]),
                mucape: f(c[3]),
                mumr: f(c[4]),
                t500: f(c[5]),
                lr75: f(c[7]),
                shr3: f(c[9]),
                shr6: f(c[10]),
                shr9: f(c[11]),
                srh3: f(c[12]),
            })
            .collect()
    })
}

fn supercell_db() -> &'static [SupercellRow] {
    static DB: OnceLock<Vec<SupercellRow>> = OnceLock::new();
    DB.get_or_init(|| {
        parse_rows(SUPERCELL_DB)
            .into_iter()
            .map(|c| SupercellRow {
                id: c[0].to_string(),
                category: f(c[1]),
                category_str: c[1].to_string(),
                mlcape: f(c[3]),
                mllcl: f(c[5]),
                srh1: f(c[6]),
                shr6: f(c[7]),
                t500: f(c[9]),
                lr75: f(c[11]),
                shr3: f(c[12]),
                shr9: f(c[13]),
                srh3: f(c[14]),
            })
            .collect()
    })
}

/// Tornado category of a supercell analog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TorCategory {
    SigTor,
    WeakTor,
    NonTor,
}

impl TorCategory {
    pub fn label(self) -> &'static str {
        match self {
            TorCategory::SigTor => "SIGTOR",
            TorCategory::WeakTor => "WEAKTOR",
            TorCategory::NonTor => "NONTOR",
        }
    }
}

/// One quality supercell analog.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SupercellAnalog {
    /// Sounding id, `YYMMDDHH.STN`.
    pub id: String,
    pub category: TorCategory,
}

/// Supercell (tornado) SARS result.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SupercellMatches {
    pub quality: Vec<SupercellAnalog>,
    pub loose_count: usize,
    /// Number of loose matches that were tornadic.
    pub tor_count: usize,
    /// Fraction (0-1) of loose matches that were tornadic.
    pub tor_prob: f64,
}

/// One quality hail analog.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HailAnalog {
    pub id: String,
    /// Reported hail size (in).
    pub size: f64,
}

/// Significant-hail SARS result.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HailMatches {
    /// Up to 15 quality matches.
    pub quality: Vec<HailAnalog>,
    pub loose_count: usize,
    /// Loose matches with >= 2 in hail.
    pub sig_count: usize,
    /// Fraction (0-1) of loose matches with significant hail.
    pub sig_prob: f64,
}

/// Supercell analog search. Units follow the database: MLCAPE J/kg, ML LCL m,
/// 500 hPa T °C, 700-500 lapse rate °C/km, shears kt, SRH m²/s².
#[allow(clippy::too_many_arguments)]
pub fn supercell_matches(
    mlcape: f64,
    mllcl: f64,
    h5temp: f64,
    lr: f64,
    shr: f64,
    srh: f64,
    shr3k: f64,
    shr9k: f64,
    srh3: f64,
) -> SupercellMatches {
    let range_mlcape = if mlcape == 0. { 0. } else { 1300. };
    let range_mlcape_t1 = mlcape * 0.25;
    let range_mllcl = 500.;
    let range_mllcl_t1 = 200.;
    let range_shr = 14.;
    let range_shr_t1 = 10.;
    let range_srh = if srh.abs() < 50. { 100. } else { srh };
    let range_srh_t1 = if srh.abs() < 100. { 50. } else { srh.abs() * 0.30 };
    let range_srh3_t1 = if srh3.abs() < 100. { 50. } else { srh3.abs() * 0.50 };
    let range_temp = 7.;
    let range_temp_t1 = 5.;
    let range_lr = 1.0;
    let range_lr_t1 = 0.8;
    let range_shr3k_t1 = 15.;
    let range_shr9k_t1 = 25.;
    let within = |x: f64, m: f64, r: f64| x >= (m - r) && x <= (m + r);

    let db = supercell_db();
    let mut loose = 0usize;
    let mut tor = 0usize;
    let mut quality = Vec::new();
    for r in db {
        if within(mlcape, r.mlcape, range_mlcape)
            && within(mllcl, r.mllcl, range_mllcl)
            && within(shr, r.shr6, range_shr)
            && within(srh, r.srh1, range_srh)
            && within(h5temp, r.t500, range_temp)
            && within(lr, r.lr75, range_lr)
        {
            loose += 1;
            if r.category > 0. {
                tor += 1;
            }
        }
        if within(mlcape, r.mlcape, range_mlcape_t1)
            && within(mllcl, r.mllcl, range_mllcl_t1)
            && within(shr, r.shr6, range_shr_t1)
            && within(srh, r.srh1, range_srh_t1)
            && within(h5temp, r.t500, range_temp_t1)
            && within(lr, r.lr75, range_lr_t1)
            && within(shr3k, r.shr3, range_shr3k_t1)
            && within(shr9k, r.shr9, range_shr9k_t1)
            && within(srh3, r.srh3, range_srh3_t1)
        {
            let category = match r.category_str.as_str() {
                "2" => TorCategory::SigTor,
                "1" => TorCategory::WeakTor,
                _ => TorCategory::NonTor,
            };
            quality.push(SupercellAnalog { id: r.id.clone(), category });
        }
    }
    let tor_prob = if loose > 0 && mlcape > 0. { tor as f64 / loose as f64 } else { 0. };
    SupercellMatches { quality, loose_count: loose, tor_count: tor, tor_prob }
}

/// Significant-hail analog search. MU mixing ratio g/kg, MUCAPE J/kg,
/// 500 hPa T °C, 700-500 lapse rate, shears **m/s**, 0-3 km SRH.
#[allow(clippy::too_many_arguments)]
pub fn hail_matches(
    mumr: f64,
    mucape: f64,
    h5_temp: f64,
    lr: f64,
    shr6: f64,
    shr9: f64,
    shr3: f64,
    srh: f64,
) -> HailMatches {
    let range_mumr = 2.0;
    let range_mumr_t1 = 2.0;
    let range_mucape = mucape * 0.30;
    let range_mucape_t1 = if mucape < 500. {
        mucape * 0.50
    } else if mucape < 2000. {
        mucape * 0.25
    } else {
        mucape * 0.20
    };
    let range_lr = 2.0;
    let range_lr_t1 = 0.4;
    let range_temp = 9.;
    let range_temp_t1 = 1.5;
    let range_shr6 = 12.;
    let range_shr6_t1 = 6.;
    let range_shr9 = 22.;
    let range_shr9_t1 = 15.;
    let range_shr3 = 10.;
    let range_shr3_t1 = 8.;
    let range_srh_t1 = if srh < 50. { 25. } else { srh * 0.5 };
    let within = |x: f64, m: f64, r: f64| x >= (m - r) && x <= (m + r);

    let mut loose = 0usize;
    let mut sig = 0usize;
    let mut quality = Vec::new();
    for r in hail_db() {
        if within(mumr, r.mumr, range_mumr)
            && within(mucape, r.mucape, range_mucape)
            && within(lr, r.lr75, range_lr)
            && within(h5_temp, r.t500, range_temp)
            && within(shr6, r.shr6, range_shr6)
            && within(shr9, r.shr9, range_shr9)
            && within(shr3, r.shr3, range_shr3)
        {
            loose += 1;
            if r.size >= 2. {
                sig += 1;
            }
        }
        if quality.len() < 15
            && within(mumr, r.mumr, range_mumr_t1)
            && within(mucape, r.mucape, range_mucape_t1)
            && within(lr, r.lr75, range_lr_t1)
            && within(h5_temp, r.t500, range_temp_t1)
            && within(shr6, r.shr6, range_shr6_t1)
            && within(shr9, r.shr9, range_shr9_t1)
            && within(shr3, r.shr3, range_shr3_t1)
            && within(srh, r.srh3, range_srh_t1)
        {
            quality.push(HailAnalog { id: r.id.clone(), size: r.size });
        }
    }
    let sig_prob = if loose > 0 && mucape > 0. { sig as f64 / loose as f64 } else { 0. };
    HailMatches { quality, loose_count: loose, sig_count: sig, sig_prob }
}
