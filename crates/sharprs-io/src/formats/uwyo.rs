//! University of Wyoming `TEXT:LIST` soundings.
//!
//! Two generations are supported:
//!
//! * **Legacy** (`cgi-bin/sounding`, retired): `<H2>72357 OUN Norman Observations at 12Z
//!   22 Dec 2018</H2>`, a fixed-width `<PRE>` table with `SKNT` (knots), then a
//!   "Station information and sounding indices" `<PRE>` block (`Station number: 72357`,
//!   `Station latitude: 35.18`, `Observation time: 181222/1200`, …). One page may hold
//!   several soundings.
//! * **Current** (`wsgi/sounding?…&type=TEXT:LIST`): `<H1>Observations for Station 72357
//!   at 18 UTC 20 May 2013</H1>`, station name in `<H3>`, `Latitude: … Longitude: …`,
//!   a fixed-width table whose wind speed column is `SPED` in **m/s**, and an indices
//!   HTML table with `SLAT`/`SLON`/`SELV` rows.
//!
//! The table is parsed by column position (fields are right-aligned under 7-character
//! headers), so blank cells become missing values instead of shifting columns. Plain
//! text copies of the page (no HTML) decode the same way.

use crate::error::{Error, Result};
use crate::qc::parse_field;
use crate::sounding::{MISSING, Sounding, SoundingMeta, SoundingSeries, SourceKind};
use crate::time::UtcTime;
use crate::Format;

const MS_TO_KT: f64 = 1.943_844_492;

/// Index of the column-name line of the first table (`PRES   HGHT   TEMP …`), if any.
pub(crate) fn find_table_header(text: &str) -> Option<usize> {
    text.lines().position(is_header_line)
}

fn is_header_line(l: &str) -> bool {
    let t: Vec<&str> = l.split_whitespace().collect();
    t.len() >= 4 && t[0] == "PRES" && t[1] == "HGHT" && t.contains(&"TEMP")
}

fn is_dashes(l: &str) -> bool {
    let t = l.trim();
    t.len() > 20 && t.chars().all(|c| c == '-')
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&")
}

/// Decode every table on the page.
pub fn decode(text: &str) -> Result<SoundingSeries> {
    let lines: Vec<&str> = text.lines().collect();
    let headers: Vec<usize> = (0..lines.len()).filter(|&i| is_header_line(lines[i])).collect();
    if headers.is_empty() {
        return Err(Error::parse(Format::Uwyo, None, "no PRES/HGHT/TEMP table found"));
    }
    let mut out = Vec::new();
    for (k, &h) in headers.iter().enumerate() {
        let prev_end = if k == 0 { 0 } else { headers[k - 1] + 1 };
        let next = headers.get(k + 1).copied().unwrap_or(lines.len());
        let mut meta = SoundingMeta { source: SourceKind::Observed, ..Default::default() };
        // title lines before the table
        for l in &lines[prev_end..h] {
            parse_title(&strip_tags(l), &mut meta);
        }
        let (snd, end) = parse_table(&lines, h, meta)?;
        let mut snd = snd;
        // metadata after the table, up to the next table's title
        let stop = if k + 1 < headers.len() { title_start(&lines, end, next) } else { lines.len() };
        parse_trailer(&lines[end..stop], &mut snd.meta);
        out.push(snd);
    }
    Ok(SoundingSeries::new(out))
}

/// Where the title section for the table at `next` begins (first `<H2>`/`<H1>` after `from`).
fn title_start(lines: &[&str], from: usize, next: usize) -> usize {
    (from..next)
        .find(|&i| {
            let u = lines[i].to_ascii_uppercase();
            u.contains("<H2>") || u.contains("<H1>") || u.contains("OBSERVATIONS AT") || u.contains("OBSERVATIONS FOR")
        })
        .unwrap_or(next)
}

fn parse_table(lines: &[&str], h: usize, meta: SoundingMeta) -> Result<(Sounding, usize)> {
    let header = lines[h];
    // column spans: names are right-aligned, so each field ends where its name ends
    let mut spans: Vec<(String, usize, usize)> = Vec::new();
    let mut prev_end = 0;
    let bytes = header.as_bytes();
    let mut j = 0;
    while j < bytes.len() {
        if bytes[j].is_ascii_whitespace() {
            j += 1;
            continue;
        }
        let s = j;
        while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        spans.push((header[s..j].to_string(), prev_end, j));
        prev_end = j;
    }
    // Some plain-text exports use tab-delimited cells, including empty cells.
    let tabular = header.trim().split('\t').count() == spans.len() && header.contains('\t');
    // units line (if present) just below the header
    let units: Vec<String> = lines
        .get(h + 1)
        .filter(|l| !is_dashes(l))
        .map(|l| if tabular {
            l.trim_start_matches(' ').split('\t').map(|s| s.trim().to_ascii_lowercase()).collect()
        } else {
            spans.iter().map(|(_, a, b)| slice(l, *a, *b).trim().to_ascii_lowercase()).collect()
        })
        .unwrap_or_default();
    let col = |name: &str| spans.iter().position(|(n, _, _)| n == name);
    let pres_c = col("PRES").ok_or_else(|| Error::parse(Format::Uwyo, Some(h + 1), "missing PRES header"))?;
    let hght_c = col("HGHT");
    let tmpc_c = col("TEMP");
    let dwpc_c = col("DWPT");
    let drct_c = col("DRCT");
    let (wspd_c, wspd_factor) = match (col("SKNT"), col("SPED")) {
        (Some(c), _) => (Some(c), 1.0),
        (None, Some(c)) => {
            let u = units.get(c).map(String::as_str).unwrap_or("m/s");
            (Some(c), if u.contains("knot") || u == "kt" { 1.0 } else { MS_TO_KT })
        }
        _ => (None, 1.0),
    };

    // skip units + dashes
    let mut i = h + 1;
    let mut dashes = 0;
    while i < lines.len() && i < h + 4 {
        if is_dashes(lines[i]) {
            dashes += 1;
            i += 1;
            break;
        }
        i += 1;
    }
    if dashes == 0 {
        i = h + 1;
    }

    let mut s = Sounding::new(meta);
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim();
        if t.is_empty() || t.starts_with('<') || !t.as_bytes()[0].is_ascii_digit() {
            break;
        }
        let cells = tabular.then(|| l.trim_start_matches(' ').split('\t').collect::<Vec<_>>());
        let field = |c: Option<usize>| -> f64 {
            match c {
                Some(c) if cells.is_some() => cells.as_ref().and_then(|row| row.get(c)).map_or(MISSING, |v| parse_field(v)),
                Some(c) => {
                    let (_, a, b) = &spans[c];
                    parse_field(slice(l, *a, *b))
                }
                None => MISSING,
            }
        };
        s.pres.push(field(Some(pres_c)));
        s.hght.push(field(hght_c));
        s.tmpc.push(field(tmpc_c));
        s.dwpc.push(field(dwpc_c));
        s.wdir.push(field(drct_c));
        s.wspd.push(field(wspd_c) * wspd_factor);
        i += 1;
    }
    if s.is_empty() {
        return Err(Error::parse(Format::Uwyo, Some(h + 1), "table has no rows"));
    }
    Ok((s, i))
}

fn slice(l: &str, a: usize, b: usize) -> &str {
    let b = b.min(l.len());
    if a >= b { "" } else { l.get(a..b).unwrap_or("") }
}

/// Title lines: legacy `72357 OUN Norman Observations at 12Z 22 Dec 2018` or current
/// `Observations for Station 72357 at 18 UTC 20 May 2013`, station name lines, and
/// `Latitude: 35.180 Longitude: -97.440`.
fn parse_title(l: &str, meta: &mut SoundingMeta) {
    let t = l.trim();
    if t.is_empty() {
        return;
    }
    if let Some(pos) = t.find("Observations at") {
        // legacy: "<wmo> <id> <name...> Observations at 12Z 22 Dec 2018"
        let head: Vec<&str> = t[..pos].split_whitespace().collect();
        if let Some(n) = head.first() {
            meta.station_number = Some(n.to_string());
        }
        if let Some(id) = head.get(1) {
            meta.station_id = Some(id.to_string());
        }
        if head.len() > 2 {
            meta.station_name = Some(head[2..].join(" "));
        }
        meta.valid_time = parse_obs_time(&t[pos + "Observations at".len()..]);
    } else if let Some(rest) = t.strip_prefix("Observations for Station") {
        // current: "72357 at 18 UTC 20 May 2013"
        if let Some((stn, when)) = rest.split_once(" at ") {
            let stn = stn.trim().to_string();
            meta.station_id = Some(stn.clone());
            meta.station_number = Some(stn);
            meta.valid_time = parse_obs_time(when);
        }
    } else if t.starts_with("Latitude:") {
        let toks: Vec<&str> = t.split_whitespace().collect();
        for w in toks.windows(2) {
            match w[0] {
                "Latitude:" => meta.lat = w[1].parse().ok(),
                "Longitude:" => meta.lon = w[1].parse().ok(),
                _ => {}
            }
        }
    } else if meta.valid_time.is_some()
        && meta.station_name.is_none()
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && t.contains(',')
        && t.len() < 80
    {
        // current site: "NORMAN/MAX WESTHEIMER A, OK., USA"
        meta.station_name = Some(t.to_string());
    }
}

/// `12Z 22 Dec 2018` or `18 UTC 20 May 2013`.
fn parse_obs_time(s: &str) -> Option<UtcTime> {
    let toks: Vec<&str> = s.split_whitespace().filter(|t| *t != "UTC").collect();
    if toks.len() < 4 {
        return None;
    }
    let hhmm = toks[0].trim_end_matches('Z');
    let (hour, minute) = if hhmm.len() == 4 {
        (hhmm[..2].parse().ok()?, hhmm[2..].parse().ok()?)
    } else {
        (hhmm.parse().ok()?, 0)
    };
    let day: u8 = toks[1].parse().ok()?;
    let month = UtcTime::month_from_abbrev(toks[2])?;
    let year: i32 = toks[3].parse().ok()?;
    UtcTime::new(year, month, day, hour, minute)
}

/// Metadata after the table: legacy "Station …: value" lines or the current site's
/// `<TD>SLAT</TD>…<TD>35.18</TD>` indices table.
fn parse_trailer(lines: &[&str], meta: &mut SoundingMeta) {
    let text = lines.join("\n");
    for l in text.lines() {
        let t = strip_tags(l);
        let t = t.trim();
        if let Some((k, v)) = t.split_once(':') {
            let v = v.trim();
            match k.trim() {
                "Station identifier" => meta.station_id = Some(v.to_string()),
                "Station number" => meta.station_number = Some(v.to_string()),
                "Observation time" => {
                    if let Some(tm) = UtcTime::parse_yymmdd_hhmm(v) {
                        meta.valid_time = Some(tm);
                    }
                }
                "Station latitude" => meta.lat = v.parse().ok().or(meta.lat),
                "Station longitude" => meta.lon = v.parse().ok().or(meta.lon),
                "Station elevation" => meta.elevation_m = v.parse().ok().or(meta.elevation_m),
                _ => {}
            }
        }
    }
    // current site: rows of <TD>KEY</TD><TD>desc</TD><TD ...>value</TD>
    let cells: Vec<String> = text
        .split("<TD")
        .skip(1)
        .map(|c| strip_tags(&format!("<TD{c}")).trim().to_string())
        .collect();
    for (k, c) in cells.iter().enumerate() {
        let val = || cells.get(k + 2).and_then(|v| v.parse::<f64>().ok());
        match c.as_str() {
            "SLAT" => meta.lat = val().or(meta.lat),
            "SLON" => meta.lon = val().or(meta.lon),
            "SELV" => meta.elevation_m = val().or(meta.elevation_m),
            _ => {}
        }
    }
}
