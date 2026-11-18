//! BUFKIT model point soundings (`.buf`), as produced for HRRR, RAP, RUC, NAM, GFS, …
//!
//! Layout (values wrap over several lines; column order is given by `SNPARM` and the
//! per-profile header, and differs between models):
//!
//! ```text
//! SNPARM = PRES;TMPC;TMWC;DWPC;THTE;DRCT;SKNT;OMEG;CFRL;HGHT
//! STNPRM = SHOW;LIFT;...
//!
//! STID = KPAH STNM = 724350 TIME = 211210/1800      <- one block per forecast hour
//! SLAT = 37.07 SLON = -88.77 SELV = 120.0
//! STIM = 0
//!
//! SHOW = 1.13 LIFT = -0.60 ...                       <- station indices (ignored)
//!
//! PRES TMPC TMWC DWPC THTE DRCT SKNT OMEG
//! CFRL HGHT
//! 993.30 18.24 17.44 16.96 327.20 176.82 7.01 0.10
//! 0.00 128.65
//! ...
//! STN YYMMDD/HHMM PMSL PRES SKTC ...                  <- surface time series (ignored)
//! ```
//!
//! `TIME` is the valid time and `STIM` the forecast hour, so init = TIME − STIM. The
//! model name is not in the content; it comes from [`crate::DecodeOptions`] or the file
//! name. If a file has no `HGHT` column, heights are integrated hypsometrically from
//! `SELV`.

use super::key_values;
use crate::error::{Error, Result};
use crate::qc::{fill_missing_heights, parse_field};
use crate::sounding::{MISSING, Sounding, SoundingMeta, SoundingSeries};
use crate::time::UtcTime;
use crate::Format;

fn is_number(tok: &str) -> bool {
    tok.parse::<f64>().is_ok()
}

/// Decode every forecast hour in a BUFKIT file.
pub fn decode(text: &str) -> Result<SoundingSeries> {
    let lines: Vec<&str> = text.lines().collect();
    let mut snparm: Option<Vec<String>> = None;
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim();
        if l.starts_with("SNPARM") {
            if let Some((_, v)) = l.split_once('=') {
                snparm = Some(
                    v.split(';').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect(),
                );
            }
            i += 1;
        } else if l.starts_with("STID") {
            let (snd, next) = decode_block(&lines, i, snparm.as_deref())?;
            out.push(snd);
            i = next;
        } else if l.starts_with("STN ") || l == "STN" {
            break; // surface time-series section
        } else {
            i += 1;
        }
    }
    if out.is_empty() {
        return Err(Error::parse(Format::Bufkit, None, "no STID profile blocks found"));
    }
    Ok(SoundingSeries::new(out))
}

fn decode_block(lines: &[&str], start: usize, snparm: Option<&[String]>) -> Result<(Sounding, usize)> {
    let mut meta = SoundingMeta::default();
    let mut stim: Option<u32> = None;
    let mut i = start;

    // Header key/value lines up to the profile header.
    let mut header: Vec<String> = Vec::new();
    while i < lines.len() {
        let l = lines[i].trim();
        let first = l.split_whitespace().next().unwrap_or("");
        if l.is_empty() {
            i += 1;
            continue;
        }
        if l.contains('=') {
            if i > start && first == "STID" {
                break; // a new block began before any data (malformed) – stop here
            }
            for (k, v) in key_values(l) {
                match k.as_str() {
                    "STID" if !v.is_empty() => meta.station_id = Some(v),
                    "STNM" if !v.is_empty() => meta.station_number = Some(v),
                    "TIME" => meta.valid_time = UtcTime::parse_yymmdd_hhmm(&v),
                    "SLAT" => meta.lat = v.parse().ok(),
                    "SLON" => meta.lon = v.parse().ok(),
                    "SELV" => meta.elevation_m = v.parse().ok(),
                    "STIM" => stim = v.parse::<f64>().ok().map(|h| h.round().max(0.0) as u32),
                    _ => {}
                }
            }
            i += 1;
            continue;
        }
        // Column header (possibly wrapped over several lines).
        if !is_number(first) {
            while i < lines.len() {
                let toks: Vec<&str> = lines[i].split_whitespace().collect();
                if toks.is_empty() || is_number(toks[0]) || lines[i].contains('=') {
                    break;
                }
                header.extend(toks.iter().map(|t| t.to_ascii_uppercase()));
                i += 1;
            }
        }
        break;
    }
    if header.is_empty() {
        header = snparm
            .map(|s| s.to_vec())
            .ok_or_else(|| Error::parse(Format::Bufkit, Some(start + 1), "no profile column header"))?;
    }

    // Data: numeric tokens until a blank or non-numeric line.
    let mut vals: Vec<f64> = Vec::new();
    while i < lines.len() {
        let toks: Vec<&str> = lines[i].split_whitespace().collect();
        if toks.is_empty() || !is_number(toks[0]) {
            break;
        }
        vals.extend(toks.iter().map(|t| parse_field(t)));
        i += 1;
    }
    let ncol = header.len();
    if vals.len() % ncol != 0 {
        return Err(Error::parse(
            Format::Bufkit,
            Some(i),
            format!("{} values is not a multiple of {} columns", vals.len(), ncol),
        ));
    }

    let idx = |name: &str| header.iter().position(|h| h == name);
    let pres_i = idx("PRES").ok_or_else(|| Error::parse(Format::Bufkit, Some(start + 1), "no PRES column"))?;
    let (hght_i, tmpc_i, dwpc_i) = (idx("HGHT"), idx("TMPC"), idx("DWPC"));
    let (drct_i, sknt_i, omeg_i) = (idx("DRCT"), idx("SKNT"), idx("OMEG"));
    let (uwnd_i, vwnd_i) = (idx("UWND"), idx("VWND"));

    meta.forecast_hour = stim;
    if let (Some(v), Some(h)) = (meta.valid_time, stim) {
        meta.init_time = Some(v.add_hours(-i64::from(h)));
    }
    let mut s = Sounding::new(meta);
    let get = |row: &[f64], c: Option<usize>| c.map(|c| row[c]).unwrap_or(MISSING);
    let mut omeg = Vec::new();
    for row in vals.chunks(ncol) {
        s.pres.push(row[pres_i]);
        s.hght.push(get(row, hght_i));
        s.tmpc.push(get(row, tmpc_i));
        s.dwpc.push(get(row, dwpc_i));
        let (mut wd, mut ws) = (get(row, drct_i), get(row, sknt_i));
        if drct_i.is_none() {
            // UWND/VWND in knots (GEMPAK convention) when direction/speed are absent
            let (u, v) = (get(row, uwnd_i), get(row, vwnd_i));
            if u.is_finite() && v.is_finite() && u.abs() < 500.0 && v.abs() < 500.0 {
                ws = u.hypot(v);
                wd = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            }
        }
        s.wdir.push(wd);
        s.wspd.push(ws);
        omeg.push(get(row, omeg_i));
    }
    if omeg_i.is_some() {
        s.omeg = Some(omeg);
    }
    if hght_i.is_none() {
        // Integration requires clean, descending pressures even for callers
        // requesting the raw file order. Map derived heights back by pressure.
        let mut column = s.clone();
        column.apply_qc();
        let selv = column.meta.elevation_m.filter(|z| (-500.0..=80_000.0).contains(z) && *z != 9999.0);
        fill_missing_heights(&mut column, selv);
        for (p, height) in s.pres.iter().zip(&mut s.hght) {
            if let Ok(i) = column.pres.binary_search_by(|q| p.total_cmp(q)) {
                *height = column.hght[i];
            }
        }
    }
    Ok((s, i))
}
