//! SPC / SHARPpy tabular text.
//!
//! ```text
//! %TITLE%
//!  OAX   140616/1900   41.32,-96.37        <- station, YYMMDD/HHMM, optional lat,lon
//!
//!    LEVEL       HGHT       TEMP       DWPT       WDIR       WSPD      [OMEG]
//! -------------------------------------------------------------------
//! %RAW%
//!  1000.00,    34.00,  -9999.00,  -9999.00,  -9999.00,  -9999.00
//!  ...
//! %END%
//! ```
//!
//! The column-name line, when present, decides the column order; otherwise the
//! classic order is assumed (6 columns, or 7 with omega last). Extra title tokens
//! (e.g. a model name) are tolerated.

use super::{Col, column_for};
use crate::error::{Error, Result};
use crate::qc::parse_field;
use crate::sounding::{MISSING, Sounding, SoundingMeta, SoundingSeries, SourceKind};
use crate::time::UtcTime;
use crate::Format;
use std::fmt::Write as _;

const DEFAULT_COLS: [Col; 7] = [Col::Pres, Col::Hght, Col::Tmpc, Col::Dwpc, Col::Wdir, Col::Wspd, Col::Omeg];

/// Decode SPC text. A file may contain several `%TITLE% … %END%` blocks; each becomes
/// one sounding in the series.
pub fn decode(text: &str) -> Result<SoundingSeries> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim();
        if l == "%TITLE%" || l == "%RAW%" {
            let (snd, next) = decode_block(&lines, i)?;
            out.push(snd);
            i = next;
        } else {
            i += 1;
        }
    }
    if out.is_empty() {
        return Err(Error::parse(Format::Spc, None, "no %RAW% section found"));
    }
    Ok(SoundingSeries::new(out))
}

fn decode_block(lines: &[&str], start: usize) -> Result<(Sounding, usize)> {
    let mut meta = SoundingMeta { source: SourceKind::Observed, ..Default::default() };
    let mut cols: Option<Vec<Option<Col>>> = None;
    let mut i = start;

    if lines[i].trim() == "%TITLE%" {
        i += 1;
        // title line = first non-empty line
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i < lines.len() && !lines[i].trim().starts_with('%') {
            parse_title(lines[i], &mut meta);
            i += 1;
        }
        while i < lines.len() && lines[i].trim() != "%RAW%" {
            let toks: Vec<&str> = lines[i].split_whitespace().collect();
            if toks.len() >= 4 && toks.iter().all(|t| t.chars().all(|c| c.is_ascii_alphabetic())) {
                cols = Some(toks.iter().map(|t| column_for(t)).collect());
            }
            i += 1;
        }
    }
    if i >= lines.len() {
        return Err(Error::parse(Format::Spc, Some(start + 1), "missing %RAW%"));
    }
    i += 1; // past %RAW%

    let mut snd = Sounding::new(meta);
    let mut rows: Vec<Vec<f64>> = Vec::new();
    while i < lines.len() {
        let l = lines[i].trim();
        if l.starts_with('%') {
            break;
        }
        i += 1;
        if l.is_empty() {
            continue;
        }
        let vals: Vec<f64> = if l.contains(',') {
            l.split(',').map(parse_field).collect()
        } else {
            l.split_whitespace().map(parse_field).collect()
        };
        rows.push(vals);
    }
    if i < lines.len() && lines[i].trim() == "%END%" {
        i += 1;
    }

    let ncol = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let cols: Vec<Option<Col>> = match cols {
        Some(c) if c.len() == ncol => c,
        _ => DEFAULT_COLS.iter().take(ncol).map(|c| Some(*c)).collect(),
    };
    if !cols.contains(&Some(Col::Pres)) {
        return Err(Error::parse(Format::Spc, None, "no pressure column"));
    }
    let has_omeg = cols.contains(&Some(Col::Omeg));
    let n = rows.len();
    snd.pres = vec![MISSING; n];
    snd.hght = vec![MISSING; n];
    snd.tmpc = vec![MISSING; n];
    snd.dwpc = vec![MISSING; n];
    snd.wdir = vec![MISSING; n];
    snd.wspd = vec![MISSING; n];
    let mut omeg = vec![MISSING; n];
    for (r, vals) in rows.iter().enumerate() {
        for (c, v) in cols.iter().zip(vals) {
            let slot = match c {
                Some(Col::Pres) => &mut snd.pres[r],
                Some(Col::Hght) => &mut snd.hght[r],
                Some(Col::Tmpc) => &mut snd.tmpc[r],
                Some(Col::Dwpc) => &mut snd.dwpc[r],
                Some(Col::Wdir) => &mut snd.wdir[r],
                Some(Col::Wspd) => &mut snd.wspd[r],
                Some(Col::Omeg) => &mut omeg[r],
                None => continue,
            };
            *slot = *v;
        }
    }
    if has_omeg {
        snd.omeg = Some(omeg);
    }
    Ok((snd, i))
}

fn parse_title(line: &str, meta: &mut SoundingMeta) {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if let Some(stid) = toks.first() {
        meta.station_id = Some(stid.to_string());
    }
    for t in toks.iter().skip(1) {
        if meta.valid_time.is_none() {
            // full token first, then SHARPpy's 11-character prefix (allows trailing junk)
            let time = UtcTime::parse_yymmdd_hhmm(t)
                .or_else(|| t.get(..11.min(t.len())).and_then(UtcTime::parse_yymmdd_hhmm));
            if let Some(time) = time {
                meta.valid_time = Some(time);
                continue;
            }
        }
        if let Some((a, b)) = t.split_once(',') {
            if let (Ok(lat), Ok(lon)) = (a.parse::<f64>(), b.parse::<f64>()) {
                meta.lat = Some(lat);
                meta.lon = Some(lon);
                continue;
            }
        }
        // anything else that names a model, e.g. "HRRR"
        let src = SourceKind::from_name(t);
        if matches!(src, SourceKind::Model(crate::Model::Hrrr | crate::Model::Rap | crate::Model::Ruc
            | crate::Model::Nam | crate::Model::NamNest | crate::Model::Gfs))
        {
            meta.source = src;
        }
    }
}

/// Options for [`write_spc_with`].
#[derive(Debug, Clone, Copy)]
pub struct SpcWriteOptions {
    /// Append an `OMEG` column (SHARPpy's own SPC reader expects exactly six columns,
    /// so this is off by default).
    pub include_omega: bool,
    /// Write `lat,lon` after the time in the title line when known.
    pub include_location: bool,
}

impl Default for SpcWriteOptions {
    fn default() -> Self {
        Self { include_omega: false, include_location: true }
    }
}

/// Encode as SHARPpy-compatible SPC text (six columns, missing = -9999.00).
pub fn write_spc(s: &Sounding) -> String {
    write_spc_with(s, SpcWriteOptions::default())
}

/// Encode as SPC text with options.
pub fn write_spc_with(s: &Sounding, opts: SpcWriteOptions) -> String {
    let stid: String = s
        .meta
        .station_id
        .as_deref()
        .unwrap_or("UNKN")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let time = s
        .meta
        .valid_time
        .or(s.meta.init_time)
        .map(|t| t.to_yymmdd_hhmm())
        .unwrap_or_else(|| "700101/0000".to_string());
    let mut out = String::new();
    out.push_str("%TITLE%\n");
    let _ = write!(out, " {stid:<5} {time}");
    if opts.include_location {
        if let (Some(lat), Some(lon)) = (s.meta.lat, s.meta.lon) {
            let _ = write!(out, " {lat:.2},{lon:.2}");
        }
    }
    out.push_str("\n\n");
    let omega = opts.include_omega && s.omeg.is_some();
    out.push_str("   LEVEL       HGHT       TEMP       DWPT       WDIR       WSPD");
    if omega {
        out.push_str("       OMEG");
    }
    out.push('\n');
    out.push_str("-------------------------------------------------------------------\n");
    out.push_str("%RAW%\n");
    let f = |v: f64| if v.is_finite() { v } else { -9999.0 };
    for i in 0..s.len() {
        let _ = write!(
            out,
            " {:8.2}, {:9.2}, {:9.2}, {:9.2}, {:9.2}, {:9.2}",
            f(s.pres[i]),
            f(s.hght[i]),
            f(s.tmpc[i]),
            f(s.dwpc[i]),
            f(s.wdir[i]),
            f(s.wspd[i])
        );
        if omega {
            let _ = write!(out, ", {:9.4}", f(s.omeg.as_ref().unwrap()[i]));
        }
        out.push('\n');
    }
    out.push_str("%END%\n");
    out
}
