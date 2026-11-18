//! Simple header-driven delimited text (comma, semicolon, tab or whitespace).
//!
//! ```text
//! # station: OUN
//! # valid: 2013-05-20T18:00Z
//! # lat: 35.18
//! # lon: -97.44
//! # source: observed            (or a model name: hrrr, rap, ...)
//! pres,hght,tmpc,dwpc,wdir,wspd
//! 966,345,27.4,22.4,185,10
//! ```
//!
//! Recognised metadata keys (`# key: value` or `# key = value`): `station`, `name`,
//! `lat`, `lon`, `elev`/`elevation`, `valid`/`time`, `init`, `fhr`/`forecast_hour`,
//! `source`/`model`.
//!
//! Column names are case-insensitive, may carry a unit in brackets or after an
//! underscore (`wspd (m/s)`, `hght_m`, `temp_k`), and map as follows:
//! pressure `pres|pressure|p|level`; height `hght|height|z|gph`; temperature
//! `tmpc|temp|t|temperature` (`K` unit → converted); dewpoint `dwpc|dwpt|dewp|td|dewpoint`;
//! direction `wdir|drct|dir`; speed `wspd|sknt|speed|sped` (`sped` and `m/s` → converted
//! to knots); components `u|v|uwnd|vwnd` (knots unless `m/s`); omega `omeg|omega|omga`.
//! Rows may be in any order; QC sorts them surface-up.

use super::{Col, column_for};
use crate::error::{Error, Result};
use crate::qc::parse_field;
use crate::sounding::{MISSING, Sounding, SoundingMeta, SoundingSeries, SourceKind};
use crate::time::UtcTime;
use crate::Format;
use std::fmt::Write as _;

const MS_TO_KT: f64 = 1.943_844_492;

/// (target column, scale, offset) per source column.
type ColumnMap = Vec<(Option<Target>, f64, f64)>;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Std(Col),
    U,
    V,
}

fn split_name_unit(raw: &str) -> (String, String) {
    let r = raw.trim().trim_matches('"').to_ascii_lowercase();
    if let Some(open) = r.find(['(', '[']) {
        let name = r[..open].trim().to_string();
        let unit = r[open + 1..].trim_end_matches([')', ']']).trim().to_string();
        return (name, unit);
    }
    if let Some((n, u)) = r.rsplit_once('_') {
        if column_for(n).is_some() || matches!(n, "u" | "v" | "uwnd" | "vwnd" | "sped") {
            return (n.to_string(), u.to_string());
        }
    }
    (r, String::new())
}

fn delimiter(line: &str) -> Option<char> {
    [',', ';', '\t'].into_iter().find(|d| line.contains(*d))
}

fn split(line: &str, delim: Option<char>) -> Vec<&str> {
    match delim {
        Some(d) => line.split(d).collect(),
        None => line.split_whitespace().collect(),
    }
}

/// Heuristic used by format detection: first non-comment line is a header that names
/// a pressure column, followed by numeric rows.
pub(crate) fn looks_like_csv(text: &str) -> bool {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let Some(h) = lines.next() else { return false };
    let d = delimiter(h);
    let has_pres = split(h, d).iter().any(|c| matches!(column_for(&split_name_unit(c).0), Some(Col::Pres)));
    has_pres && lines.next().is_some_and(|l| split(l, d).iter().any(|t| t.trim().parse::<f64>().is_ok()))
}

pub fn decode(text: &str) -> Result<SoundingSeries> {
    let mut meta = SoundingMeta::default();
    let mut header: Option<(ColumnMap, Option<char>)> = None;
    let mut s = Sounding::new(SoundingMeta::default());
    let mut omeg = Vec::new();
    let mut has_omeg = false;
    let mut uv: Vec<(f64, f64)> = Vec::new();
    let mut has_uv = false;

    for (ln, raw) in text.lines().enumerate() {
        let l = raw.trim().trim_start_matches('\u{feff}');
        if l.is_empty() {
            continue;
        }
        if let Some(c) = l.strip_prefix('#') {
            parse_meta_comment(c, &mut meta);
            continue;
        }
        let Some((cols, d)) = &header else {
            let d = delimiter(l);
            let cols: ColumnMap = split(l, d)
                .iter()
                .map(|c| {
                    let (name, unit) = split_name_unit(c);
                    target_for(&name, &unit)
                })
                .collect();
            if !cols.iter().any(|c| c.0 == Some(Target::Std(Col::Pres))) {
                return Err(Error::parse(Format::Csv, Some(ln + 1), "header has no pressure column"));
            }
            has_omeg = cols.iter().any(|c| c.0 == Some(Target::Std(Col::Omeg)));
            has_uv = cols.iter().any(|c| c.0 == Some(Target::U));
            header = Some((cols, d));
            continue;
        };
        let fields = split(l, *d);
        let mut row = [MISSING; 7];
        let (mut u, mut v) = (MISSING, MISSING);
        for ((target, scale, offset), f) in cols.iter().zip(fields) {
            let x = parse_field(f);
            // keep sentinels unscaled so QC still recognises them as missing
            let sentinel = [-9999.0, 9999.0, -99999.0, 99999.0, -999.0].contains(&x);
            let x = if x.is_finite() && !sentinel { x * scale + offset } else { MISSING };
            match target {
                Some(Target::Std(c)) => row[*c as usize] = x,
                Some(Target::U) => u = x,
                Some(Target::V) => v = x,
                None => {}
            }
        }
        s.pres.push(row[Col::Pres as usize]);
        s.hght.push(row[Col::Hght as usize]);
        s.tmpc.push(row[Col::Tmpc as usize]);
        s.dwpc.push(row[Col::Dwpc as usize]);
        s.wdir.push(row[Col::Wdir as usize]);
        s.wspd.push(row[Col::Wspd as usize]);
        omeg.push(row[Col::Omeg as usize]);
        uv.push((u, v));
    }
    if header.is_none() {
        return Err(Error::parse(Format::Csv, None, "no header line"));
    }
    if has_uv {
        for (i, (u, v)) in uv.into_iter().enumerate() {
            if !s.wspd[i].is_finite() && u.is_finite() && v.is_finite() {
                s.wspd[i] = u.hypot(v);
                s.wdir[i] = (270.0 - v.atan2(u).to_degrees()).rem_euclid(360.0);
            }
        }
    }
    if has_omeg {
        s.omeg = Some(omeg);
    }
    s.meta = meta;
    Ok(SoundingSeries::new(vec![s]))
}

/// Map a column to (target, scale, offset) applied as `x * scale + offset`.
fn target_for(name: &str, unit: &str) -> (Option<Target>, f64, f64) {
    let ms = matches!(unit, "m/s" | "ms" | "mps" | "m s-1" | "ms-1");
    let kmh = matches!(unit, "km/h" | "kph" | "kmh");
    let speed_scale = if ms { MS_TO_KT } else if kmh { MS_TO_KT / 3.6 } else { 1.0 };
    match name {
        "sped" => return (Some(Target::Std(Col::Wspd)), if unit.is_empty() { MS_TO_KT } else { speed_scale }, 0.0),
        "u" | "uwnd" => return (Some(Target::U), speed_scale, 0.0),
        "v" | "vwnd" => return (Some(Target::V), speed_scale, 0.0),
        "tmpk" | "tempk" => return (Some(Target::Std(Col::Tmpc)), 1.0, -273.15),
        "dwpk" => return (Some(Target::Std(Col::Dwpc)), 1.0, -273.15),
        "wind_dir" | "wind_direction" | "direction" => return (Some(Target::Std(Col::Wdir)), 1.0, 0.0),
        "wind_speed" | "windspeed" => return (Some(Target::Std(Col::Wspd)), speed_scale, 0.0),
        "dew_point" | "dewpoint_c" => return (Some(Target::Std(Col::Dwpc)), 1.0, 0.0),
        _ => {}
    }
    match column_for(name) {
        Some(Col::Wspd) => (Some(Target::Std(Col::Wspd)), speed_scale, 0.0),
        Some(c @ (Col::Tmpc | Col::Dwpc)) if unit == "k" => (Some(Target::Std(c)), 1.0, -273.15),
        Some(Col::Pres) if unit == "pa" => (Some(Target::Std(Col::Pres)), 0.01, 0.0),
        Some(Col::Hght) if unit == "km" => (Some(Target::Std(Col::Hght)), 1000.0, 0.0),
        Some(c) => (Some(Target::Std(c)), 1.0, 0.0),
        None => (None, 1.0, 0.0),
    }
}

fn parse_meta_comment(c: &str, meta: &mut SoundingMeta) {
    let Some((k, v)) = c.split_once(':').or_else(|| c.split_once('=')) else { return };
    let v = v.trim();
    match k.trim().to_ascii_lowercase().as_str() {
        "station" | "stid" | "station_id" => meta.station_id = Some(v.to_string()),
        "name" | "station_name" => meta.station_name = Some(v.to_string()),
        "lat" | "latitude" => meta.lat = v.parse().ok(),
        "lon" | "longitude" => meta.lon = v.parse().ok(),
        "elev" | "elevation" | "elevation_m" => meta.elevation_m = v.parse().ok(),
        "valid" | "time" | "valid_time" => {
            meta.valid_time = UtcTime::parse_iso(v).or_else(|| UtcTime::parse_yymmdd_hhmm(v))
        }
        "init" | "init_time" => meta.init_time = UtcTime::parse_iso(v).or_else(|| UtcTime::parse_yymmdd_hhmm(v)),
        "fhr" | "forecast_hour" => meta.forecast_hour = v.parse().ok(),
        "source" | "model" | "kind" => meta.source = SourceKind::from_name(v),
        _ => {}
    }
    if let (Some(i), Some(f), None) = (meta.init_time, meta.forecast_hour, meta.valid_time) {
        meta.valid_time = Some(i.add_hours(i64::from(f)));
    }
}

/// Encode as CSV with metadata comments (readable by [`decode`]).
pub fn write_csv(s: &Sounding) -> String {
    let mut out = String::new();
    let m = &s.meta;
    if let Some(v) = &m.station_id {
        let _ = writeln!(out, "# station: {v}");
    }
    if let Some(v) = &m.station_name {
        let _ = writeln!(out, "# name: {v}");
    }
    if let Some(v) = m.lat {
        let _ = writeln!(out, "# lat: {v}");
    }
    if let Some(v) = m.lon {
        let _ = writeln!(out, "# lon: {v}");
    }
    if let Some(v) = m.elevation_m {
        let _ = writeln!(out, "# elev: {v}");
    }
    if m.source != SourceKind::Unknown {
        let _ = writeln!(out, "# source: {}", m.source);
    }
    if let Some(v) = m.init_time {
        let _ = writeln!(out, "# init: {}", v.to_iso8601());
    }
    if let Some(v) = m.forecast_hour {
        let _ = writeln!(out, "# fhr: {v}");
    }
    if let Some(v) = m.valid_time {
        let _ = writeln!(out, "# valid: {}", v.to_iso8601());
    }
    out.push_str("pres,hght,tmpc,dwpc,wdir,wspd");
    if s.omeg.is_some() {
        out.push_str(",omeg");
    }
    out.push('\n');
    let f = |v: f64| if v.is_finite() { format!("{v}") } else { String::new() };
    for i in 0..s.len() {
        let _ = write!(
            out,
            "{},{},{},{},{},{}",
            f(s.pres[i]),
            f(s.hght[i]),
            f(s.tmpc[i]),
            f(s.dwpc[i]),
            f(s.wdir[i]),
            f(s.wspd[i])
        );
        if let Some(o) = &s.omeg {
            let _ = write!(out, ",{}", f(o[i]));
        }
        out.push('\n');
    }
    out
}
