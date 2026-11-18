//! Iowa Environmental Mesonet RAOB JSON
//! (`https://mesonet.agron.iastate.edu/json/raob.py?ts=YYYYMMDDHHMM&station=KOUN`).
//!
//! ```json
//! {"profiles": [{"station": "KOUN", "valid": "2013-05-21T00:00:00Z",
//!   "profile": [{"pres": 964.0, "hght": 362.0, "tmpc": 22.4, "dwpc": 18.6,
//!                "drct": 0.0, "sknt": 0.0}, ...]}], "generated_at": "..."}
//! ```
//!
//! `null` values are missing. Station ids are ICAO-style (`KOUN`, `KBMX`); latitude
//! and longitude are not included and are filled from the built-in station table when
//! possible. Each entry in `profiles` becomes one sounding.

use crate::error::{Error, Result};
use crate::sounding::{MISSING, Sounding, SoundingMeta, SoundingSeries, SourceKind};
use crate::time::UtcTime;
use crate::Format;
use serde_json::Value;

pub fn decode(text: &str) -> Result<SoundingSeries> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| Error::parse(Format::IemJson, Some(e.line()), e.to_string()))?;
    let profiles = match &v {
        Value::Object(o) => o.get("profiles").and_then(Value::as_array),
        Value::Array(a) => Some(a),
        _ => None,
    }
    .ok_or_else(|| Error::parse(Format::IemJson, None, "no \"profiles\" array"))?;

    let mut out = Vec::new();
    for p in profiles {
        let meta = SoundingMeta {
            station_id: p.get("station").and_then(Value::as_str).map(str::to_string),
            valid_time: p.get("valid").and_then(Value::as_str).and_then(UtcTime::parse_iso),
            source: SourceKind::Observed,
            ..Default::default()
        };
        let mut s = Sounding::new(meta);
        let Some(levels) = p.get("profile").and_then(Value::as_array) else { continue };
        for l in levels {
            let f = |k: &str| l.get(k).and_then(Value::as_f64).unwrap_or(MISSING);
            s.pres.push(f("pres"));
            s.hght.push(f("hght"));
            s.tmpc.push(f("tmpc"));
            s.dwpc.push(f("dwpc"));
            s.wdir.push(f("drct"));
            s.wspd.push(f("sknt"));
        }
        out.push(s);
    }
    if out.iter().all(|s| s.is_empty()) {
        return Err(Error::NoData { format: Format::IemJson });
    }
    Ok(SoundingSeries::new(out))
}
