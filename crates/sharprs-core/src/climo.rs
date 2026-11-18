//! Climatologies used by the SHARPpy insets: PW anomaly flag, STP / SHIP /
//! SHERB box-and-whisker distributions, STP-component tornado
//! probabilities, conditional EF-scale probabilities by STP and by VROT.
//! See NOTICE for data provenance.

use std::sync::OnceLock;

const PW_MEAN: &str = include_str!("../data/PW-mean-inches.txt");
const PW_STDEV: &str = include_str!("../data/PW-stdev-inches.txt");

struct PwRow {
    icao: String,
    wmo: String,
    site: String,
    vals: [f64; 12],
}

fn parse_pw(text: &str) -> Vec<PwRow> {
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            let mut vals = [f64::NAN; 12];
            for (k, v) in vals.iter_mut().enumerate() {
                *v = c.get(3 + k).and_then(|s| s.trim().parse().ok()).unwrap_or(f64::NAN);
            }
            PwRow { icao: c[0].to_string(), wmo: c[1].to_string(), site: c[2].to_string(), vals }
        })
        .collect()
}

fn pw_tables() -> &'static (Vec<PwRow>, Vec<PwRow>) {
    static T: OnceLock<(Vec<PwRow>, Vec<PwRow>)> = OnceLock::new();
    T.get_or_init(|| (parse_pw(PW_MEAN), parse_pw(PW_STDEV)))
}

fn lookup<'a>(rows: &'a [PwRow], station: &str) -> Option<&'a PwRow> {
    match station.len() {
        4 => {
            let s = station.to_uppercase();
            rows.iter().find(|r| r.icao == s)
        }
        3 => {
            let s = station.to_lowercase();
            rows.iter().find(|r| r.site == s)
        }
        5 => rows.iter().find(|r| r.wmo == station),
        _ => None,
    }
}

/// Monthly PW climatology (mean, stdev; inches) for a station id
/// (3-letter, 4-letter ICAO or 5-digit WMO) and month (1-12).
pub fn pw_climatology(station: &str, month: u8) -> Option<(f64, f64)> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let (m, s) = pw_tables();
    let mean = lookup(m, station)?.vals[month as usize - 1];
    let sd = lookup(s, station)?.vals[month as usize - 1];
    Some((mean, sd))
}

/// SHARPpy's PW anomaly flag: -3..=3 standard deviations from the monthly
/// mean (sfc-300 hPa PW in inches). Returns 0 if the station is unknown.
pub fn pw_flag(pw_300: f64, station: Option<&str>, month: u8) -> i8 {
    let Some(st) = station else { return 0 };
    let Some((mean, sd)) = pw_climatology(st, month) else { return 0 };
    let s1 = (mean - sd, mean + sd);
    let s2 = (mean - 2. * sd, mean + 2. * sd);
    let s3 = (mean - 3. * sd, mean + 3. * sd);
    if pw_300 > s3.1 {
        3
    } else if pw_300 < s3.0 {
        -3
    } else if pw_300 > s2.1 {
        2
    } else if pw_300 < s2.0 {
        -2
    } else if pw_300 > s1.1 {
        1
    } else if pw_300 < s1.0 {
        -1
    } else {
        0
    }
}

/// Box-and-whisker distribution (10th, 25th, 50th, 75th, 90th percentiles).
pub type Whisker = [f64; 5];

/// Effective-layer STP (with CIN) distributions by tornado class
/// (EF4+, EF3, EF2, EF1, EF0, NONTOR).
pub const STP_BY_EF: [(&str, Whisker); 6] = [
    ("EF4+", [1.2, 2.6, 5.3, 8.3, 11.0]),
    ("EF3", [0.2, 1.0, 2.4, 4.5, 8.4]),
    ("EF2", [0.0, 0.6, 1.7, 3.7, 5.6]),
    ("EF1", [0.0, 0.3, 1.2, 2.6, 4.5]),
    ("EF0", [0.0, 0.1, 0.8, 2.0, 3.7]),
    ("NONTOR", [0.0, 0.0, 0.2, 0.7, 1.7]),
];

/// SHIP distributions for <= 1.5" and >= 2.5" hail.
pub const SHIP_BY_SIZE: [(&str, Whisker); 2] =
    [("<= 1.5\"", [0.2, 0.3, 0.2, 0.9, 1.2]), (">= 2.5\"", [1.1, 1.4, 0.8, 2.8, 4.0])];

/// SHERB distributions (3 km and effective versions; SIG, SIGTOR, NULL).
pub const SHERB_DIST: [(&str, Whisker); 6] = [
    ("3SIG", [0.25, 0.8, 1.02, 1.25, 1.95]),
    ("3SIGTOR", [0.6, 1.0, 1.2, 1.4, 1.8]),
    ("3NULL", [0.1, 0.6, 0.8, 1.05, 1.6]),
    ("ESIG", [0.0, 0.6, 1.0, 1.35, 2.3]),
    ("ESIGTOR", [0.2, 1.0, 1.25, 1.55, 2.0]),
    ("ENULL", [0.0, 0.4, 0.75, 1.0, 1.95]),
];

/// STPC bins used by the conditional-probability inset.
pub const COND_STP_BINS: [&str; 11] =
    ["0", ".01-.49", ".5-.99", "1-1.99", "2-2.99", "3-3.99", "4-5.99", "6-7.99", "8-9.99", "10-11.99", ">12"];
/// Conditional tornado probabilities (%) by STPC bin.
pub const COND_STP_EF1PLUS: [f64; 11] = [15.3, 28.0, 36.1, 47.8, 52.3, 54.4, 60.3, 61.6, 61.2, 73.2, 73.4];
pub const COND_STP_EF2PLUS: [f64; 11] = [2.7, 6.6, 6.5, 13.8, 14.9, 17.5, 21.8, 31.5, 33.3, 46.2, 53.1];
pub const COND_STP_EF3PLUS: [f64; 11] = [0., 1.2, 1.2, 2.7, 2.7, 4.1, 7.3, 10.0, 13.2, 25.8, 39.1];
pub const COND_STP_EF4PLUS: [f64; 11] = [0., 0., 0., 0., 0.2, 0.1, 1.5, 2.3, 6.2, 9.7, 17.2];

/// STPC bin index (0..=10) as SHARPpy's inset selects it.
pub fn cond_stp_bin(stpc: f64) -> usize {
    if stpc == 0. {
        0
    } else if (0.01..0.5).contains(&stpc) {
        1
    } else if (0.5..1.).contains(&stpc) {
        2
    } else if (1.0..2.).contains(&stpc) {
        3
    } else if (2.0..3.).contains(&stpc) {
        4
    } else if (3.0..4.).contains(&stpc) {
        5
    } else if (4.0..6.).contains(&stpc) {
        6
    } else if (6.0..8.).contains(&stpc) {
        7
    } else if (8.0..10.).contains(&stpc) {
        8
    } else if (10.0..12.).contains(&stpc) {
        9
    } else {
        10
    }
}

/// Conditional probabilities (%) of EF1+, EF2+, EF3+, EF4+ given STPC.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CondTorProbs {
    pub bin: usize,
    pub ef1_plus: f64,
    pub ef2_plus: f64,
    pub ef3_plus: f64,
    pub ef4_plus: f64,
}

pub fn cond_tor_probs(stpc: f64) -> CondTorProbs {
    let b = cond_stp_bin(stpc);
    CondTorProbs {
        bin: b,
        ef1_plus: COND_STP_EF1PLUS[b],
        ef2_plus: COND_STP_EF2PLUS[b],
        ef3_plus: COND_STP_EF3PLUS[b],
        ef4_plus: COND_STP_EF4PLUS[b],
    }
}

/// VROT bin centres (kt) and EF-class probabilities (%).
pub const VROT_XPTS: [f64; 11] = [5., 15., 25., 35., 45., 55., 65., 75., 85., 95., 105.];
pub const VROT_EF01: [f64; 11] = [100.0, 98.6, 95.3, 91.0, 80.2, 61.9, 42.1, 29.1, 16.3, 5.6, 0.0];
pub const VROT_EF23: [f64; 11] = [0.0, 1.0, 4.7, 9.0, 19.3, 36.5, 51.1, 62.8, 65.1, 50.0, 25.0];
pub const VROT_EF45: [f64; 11] = [0.0, 0.0, 0.0, 0.0, 0.5, 1.6, 6.8, 8.1, 18.6, 44.4, 75.0];

/// EF-scale probabilities (%) for a rotational velocity (kt):
/// (EF0-1, EF2-3, EF4-5), nearest bin as in SHARPpy.
pub fn vrot_probs(vrot: f64) -> (f64, f64, f64) {
    let mut best = 0;
    let mut bd = f64::INFINITY;
    for (i, &x) in VROT_XPTS.iter().enumerate() {
        let d = (vrot - x).abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    (VROT_EF01[best], VROT_EF23[best], VROT_EF45[best])
}

/// Probability of a significant tornado given each STP ingredient
/// (the STP inset's per-ingredient readouts). `None` where SHARPpy shows
/// nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StpIngredientProbs {
    pub mlcape: Option<f64>,
    pub mllcl: Option<f64>,
    pub esrh: Option<f64>,
    pub ebwd: Option<f64>,
    pub stpc: Option<f64>,
    pub stp_fixed: Option<f64>,
}

fn bins(x: f64, edges: &[(f64, f64)], below: f64) -> Option<f64> {
    // edges: (lower bound inclusive, prob); x below the first bound -> below
    if x.is_nan() {
        return None;
    }
    if x < edges[0].0 {
        return Some(below);
    }
    let mut p = None;
    for &(lo, prob) in edges {
        if x >= lo {
            p = Some(prob);
        }
    }
    p
}

/// SHARPpy STP inset probabilities. `ebwd_kt` in knots (SHARPpy passes the
/// knot value); hemisphere sign handling is the caller's.
pub fn stp_ingredient_probs(
    mlcape: f64,
    mllcl: f64,
    esrh: f64,
    ebwd_kt: f64,
    stpc: f64,
    stp_fixed: f64,
) -> StpIngredientProbs {
    let cape = if mlcape.is_nan() {
        None
    } else if mlcape == 0. {
        Some(0.)
    } else if mlcape > 0. {
        bins(mlcape, &[(0., 0.12), (250., 0.14), (500., 0.16), (1000., 0.15), (1500., 0.13), (2000., 0.14), (2500., 0.18), (3000., 0.20), (4000., 0.16)], 0.12)
    } else {
        None
    };
    let lcl = bins(mllcl, &[(750., 0.19), (1000., 0.15), (1250., 0.10), (1500., 0.06), (1750., 0.06), (2000., 0.02), (2500., 0.0)], 0.19);
    let srh = bins(esrh, &[(50., 0.06), (100., 0.08), (200., 0.14), (300., 0.20), (400., 0.27), (500., 0.38), (600., 0.37), (700., 0.42)], 0.06);
    let ebwd = if ebwd_kt.is_nan() {
        None
    } else if ebwd_kt == 0. {
        Some(0.)
    } else if ebwd_kt >= 0.01 {
        bins(ebwd_kt, &[(0.01, 0.03), (20., 0.05), (30., 0.06), (40., 0.12), (50., 0.19), (60., 0.27), (70., 0.36), (80., 0.26)], 0.03)
    } else {
        None
    };
    let stpc_p = bins(stpc, &[(0.1, 0.08), (0.5, 0.12), (1., 0.17), (2., 0.25), (4., 0.32), (6., 0.34), (8., 0.55), (10., 0.58)], 0.06);
    let stpf_p = bins(stp_fixed, &[(0.1, 0.06), (0.5, 0.11), (1., 0.17), (2., 0.25), (3., 0.25), (5., 0.39), (7., 0.55), (9., 0.59)], 0.05);
    StpIngredientProbs { mlcape: cape, mllcl: lcl, esrh: srh, ebwd, stpc: stpc_p, stp_fixed: stpf_p }
}
