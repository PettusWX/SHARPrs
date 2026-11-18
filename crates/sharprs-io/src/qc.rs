//! Quality control applied by every decoder.
//!
//! Steps, in order:
//! 1. Out-of-range / sentinel values (`-9999`, `9999.0`, `-999`, blanks already NaN, …)
//!    become NaN. Every column has a plausibility range, so sentinels are caught without
//!    special-casing each spelling.
//! 2. Levels without a valid pressure are dropped.
//! 3. Levels are stable-sorted surface-up (decreasing pressure).
//! 4. Levels with the same pressure are merged (first non-missing value wins per column).
//! 5. Leading "below ground" levels that carry no temperature (e.g. the 1000 hPa
//!    mandatory level reported under a high-elevation station) are dropped.
//! 6. Levels whose height breaks monotonic increase with decreasing pressure are dropped
//!    (the minimal set, via longest increasing subsequence).
//! 7. Dewpoints warmer than the temperature are clamped to the temperature.
//! 8. A negative wind speed or a direction outside 0..=360 removes that wind.

use crate::sounding::{MISSING, Sounding};

/// Counts of what [`run`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QcReport {
    pub input_levels: usize,
    pub output_levels: usize,
    /// Individual values set to missing because they were sentinels or implausible.
    pub values_set_missing: usize,
    pub dropped_no_pressure: usize,
    pub merged_duplicate_pressure: usize,
    pub dropped_below_ground: usize,
    pub dropped_nonmonotonic_height: usize,
    pub dewpoints_clamped: usize,
    /// `true` if the input was not already ordered surface-up.
    pub reordered: bool,
}

impl QcReport {
    /// `true` if QC changed nothing besides sentinel handling.
    pub fn is_clean(&self) -> bool {
        self.dropped_no_pressure == 0
            && self.merged_duplicate_pressure == 0
            && self.dropped_nonmonotonic_height == 0
            && self.dewpoints_clamped == 0
    }
}

const PRES: (f64, f64) = (0.01, 1100.0);
const HGHT: (f64, f64) = (-500.0, 80_000.0);
const TMPC: (f64, f64) = (-120.0, 70.0);
const DWPC: (f64, f64) = (-160.0, 70.0);
const WDIR: (f64, f64) = (0.0, 360.0);
const WSPD: (f64, f64) = (0.0, 500.0);
const OMEG: (f64, f64) = (-200.0, 200.0);

/// Normalise one value: NaN if non-finite or outside `range` (inclusive).
#[inline]
pub fn clean_value(v: f64, range: (f64, f64)) -> f64 {
    if v.is_finite() && v >= range.0 && v <= range.1 { v } else { MISSING }
}

/// Parse a numeric field as found in text formats: blank, `M`, `NA`, `nan`, `-`, `//`
/// and unparsable strings become NaN. Sentinels like -9999 are handled later by QC.
pub fn parse_field(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return MISSING;
    }
    match t.parse::<f64>() {
        Ok(v) => v,
        Err(_) => MISSING,
    }
}

#[derive(Clone, Copy)]
struct Row {
    p: f64,
    z: f64,
    t: f64,
    td: f64,
    wd: f64,
    ws: f64,
    w: f64,
}

/// Run the QC pipeline on `s` in place and return a report.
pub fn run(s: &mut Sounding) -> QcReport {
    let n = s.pres.len();
    let mut rep = QcReport { input_levels: n, ..Default::default() };
    let has_omeg = s.omeg.is_some();
    let om = s.omeg.take().unwrap_or_else(|| vec![MISSING; n]);

    let mut set_missing = 0usize;
    let mut c = |v: f64, r: (f64, f64)| {
        let out = clean_value(v, r);
        if v.is_finite() && !out.is_finite() {
            set_missing += 1;
        }
        out
    };

    let mut rows: Vec<Row> = Vec::with_capacity(n);
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        let mut r = Row {
            p: c(s.pres[i], PRES),
            z: c(s.hght[i], HGHT),
            t: c(s.tmpc[i], TMPC),
            td: c(s.dwpc[i], DWPC),
            wd: c(s.wdir[i], WDIR),
            ws: c(s.wspd[i], WSPD),
            w: c(om[i], OMEG),
        };
        // A wind needs both components.
        if !(r.wd.is_finite() && r.ws.is_finite()) {
            r.wd = MISSING;
            r.ws = MISSING;
        }
        if r.p.is_finite() {
            rows.push(r);
        } else {
            rep.dropped_no_pressure += 1;
        }
    }
    rep.values_set_missing = set_missing;

    // 3. sort surface-up (stable)
    if rows.windows(2).any(|w| w[1].p > w[0].p) {
        rep.reordered = true;
        rows.sort_by(|a, b| b.p.total_cmp(&a.p));
    }

    // 4. merge duplicates
    let mut merged: Vec<Row> = Vec::with_capacity(rows.len());
    for r in rows {
        if let Some(last) = merged.last_mut() {
            if (last.p - r.p).abs() < 1e-3 {
                let fill = |a: &mut f64, b: f64| {
                    if !a.is_finite() {
                        *a = b;
                    }
                };
                fill(&mut last.z, r.z);
                fill(&mut last.t, r.t);
                fill(&mut last.td, r.td);
                if !last.ws.is_finite() {
                    last.wd = r.wd;
                    last.ws = r.ws;
                }
                fill(&mut last.w, r.w);
                rep.merged_duplicate_pressure += 1;
                continue;
            }
        }
        merged.push(r);
    }
    let mut rows = merged;

    // 5. leading below-ground levels without temperature
    if let Some(first_t) = rows.iter().position(|r| r.t.is_finite()) {
        if first_t > 0 {
            rep.dropped_below_ground = first_t;
            rows.drain(..first_t);
        }
    }

    // 6. monotonic heights (keep a longest strictly increasing subsequence of the
    //    levels that have a height; levels without height are always kept)
    let with_z: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].z.is_finite()).collect();
    let keep = longest_increasing(&with_z.iter().map(|&i| rows[i].z).collect::<Vec<_>>());
    if keep.len() < with_z.len() {
        let mut drop = vec![false; rows.len()];
        let mut k = 0;
        for (j, &i) in with_z.iter().enumerate() {
            if k < keep.len() && keep[k] == j {
                k += 1;
            } else {
                drop[i] = true;
            }
        }
        rep.dropped_nonmonotonic_height = drop.iter().filter(|d| **d).count();
        let mut idx = 0;
        rows.retain(|_| {
            let d = drop[idx];
            idx += 1;
            !d
        });
    }

    // 7. dewpoint <= temperature
    for r in rows.iter_mut() {
        if r.t.is_finite() && r.td.is_finite() && r.td > r.t {
            r.td = r.t;
            rep.dewpoints_clamped += 1;
        }
    }

    s.pres = rows.iter().map(|r| r.p).collect();
    s.hght = rows.iter().map(|r| r.z).collect();
    s.tmpc = rows.iter().map(|r| r.t).collect();
    s.dwpc = rows.iter().map(|r| r.td).collect();
    s.wdir = rows.iter().map(|r| r.wd).collect();
    s.wspd = rows.iter().map(|r| r.ws).collect();
    s.omeg = has_omeg.then(|| rows.iter().map(|r| r.w).collect());
    rep.output_levels = s.pres.len();

    if s.meta.elevation_m.is_none() {
        if let Some(z) = s.hght.first().copied().filter(|z| z.is_finite()) {
            s.meta.elevation_m = Some(z);
        }
    }
    rep
}

/// Indices (into `v`) of one longest strictly increasing subsequence, ascending.
fn longest_increasing(v: &[f64]) -> Vec<usize> {
    if v.is_empty() {
        return Vec::new();
    }
    // tails[k] = index of the smallest tail of an increasing subsequence of length k+1
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; v.len()];
    for i in 0..v.len() {
        let pos = tails.partition_point(|&j| v[j] < v[i]);
        if pos > 0 {
            prev[i] = tails[pos - 1];
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut k = *tails.last().unwrap();
    loop {
        out.push(k);
        if prev[k] == usize::MAX {
            break;
        }
        k = prev[k];
    }
    out.reverse();
    out
}

/// Fill missing heights by integrating the hypsometric equation upward (and downward)
/// from the nearest level with a known height, using virtual temperature where a
/// dewpoint is available. `surface_hght` seeds the integration when no level has a
/// height. Used by decoders for sources that omit heights.
pub fn fill_missing_heights(s: &mut Sounding, surface_hght: Option<f64>) -> usize {
    const RD: f64 = 287.04;
    const G: f64 = 9.80665;
    let n = s.len();
    if n == 0 {
        return 0;
    }
    if !s.hght.iter().any(|z| z.is_finite()) {
        match surface_hght {
            Some(z) => s.hght[0] = z,
            None => return 0,
        }
    }
    let tv = |i: usize| -> Option<f64> {
        let t = s.tmpc[i];
        if !t.is_finite() {
            return None;
        }
        let tk = t + 273.15;
        let td = s.dwpc[i];
        if td.is_finite() {
            let e = 6.112 * (17.67 * td / (td + 243.5)).exp();
            let q = 0.622 * e / (s.pres[i] - 0.378 * e);
            Some(tk * (1.0 + 0.61 * q))
        } else {
            Some(tk)
        }
    };
    let tvs: Vec<Option<f64>> = (0..n).map(tv).collect();
    let mut filled = 0;
    let layer = |a: usize, b: usize| -> Option<f64> {
        let tm = match (tvs[a], tvs[b]) {
            (Some(x), Some(y)) => 0.5 * (x + y),
            (Some(x), None) | (None, Some(x)) => x,
            _ => return None,
        };
        Some(RD * tm / G * (s.pres[a] / s.pres[b]).ln())
    };
    // upward
    let start = s.hght.iter().position(|z| z.is_finite()).unwrap();
    let mut new = s.hght.clone();
    for i in start + 1..n {
        if !new[i].is_finite() && new[i - 1].is_finite() {
            if let Some(dz) = layer(i - 1, i) {
                new[i] = new[i - 1] + dz;
                filled += 1;
            }
        }
    }
    for i in (0..start).rev() {
        if !new[i].is_finite() && new[i + 1].is_finite() {
            if let Some(dz) = layer(i, i + 1) {
                new[i] = new[i + 1] - dz;
                filled += 1;
            }
        }
    }
    s.hght = new;
    filled
}
