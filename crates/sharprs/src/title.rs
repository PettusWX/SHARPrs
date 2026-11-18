//! Window title strings.

use sharprs_io::{Sounding, SourceKind, UtcTime};

/// Title convention for the top-left header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleStyle {
    /// Pivotal Weather style:
    /// `HRRR 2021-12-10 18z, F009  VALID: Sat 2021-12-11 03z  KPAH @37.06°N 88.77°W`
    #[default]
    Pivotal,
    /// SHARPpy style: `KPAH   20211211/0300  (18Z  HRRR  F009)`
    Sharppy,
}

fn weekday(t: &UtcTime) -> &'static str {
    // 1970-01-01 was a Thursday.
    let days = t.unix_minutes().div_euclid(1440);
    ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize]
}

fn ymd_h(t: &UtcTime) -> String {
    format!("{:04}-{:02}-{:02} {:02}z", t.year, t.month, t.day, t.hour)
}

fn latlon(s: &Sounding) -> String {
    match (s.meta.lat, s.meta.lon) {
        (Some(la), Some(lo)) => format!(
            "@{:.2}°{} {:.2}°{}",
            la.abs(),
            if la >= 0.0 { 'N' } else { 'S' },
            lo.abs(),
            if lo >= 0.0 { 'E' } else { 'W' }
        ),
        _ => String::new(),
    }
}

/// Build the header title for a sounding.
pub fn sounding_title(s: &Sounding, style: TitleStyle) -> String {
    let m = &s.meta;
    let stn = m.station_id.clone().unwrap_or_default();
    match style {
        TitleStyle::Pivotal => {
            let loc = latlon(s);
            let tail = [stn.as_str(), loc.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" ");
            match (&m.source, m.init_time, m.forecast_hour, m.valid_time) {
                (SourceKind::Model(model), Some(init), Some(fh), Some(valid)) => format!(
                    "{} {}, F{:03}  VALID: {} {}  {}",
                    model.name(),
                    ymd_h(&init),
                    fh,
                    weekday(&valid),
                    ymd_h(&valid),
                    tail
                ),
                (SourceKind::Model(model), _, _, Some(valid)) => {
                    format!("{}  VALID: {} {}  {}", model.name(), weekday(&valid), ymd_h(&valid), tail)
                }
                (_, _, _, Some(valid)) => {
                    format!("Observed  {} {}  {}", weekday(&valid), ymd_h(&valid), tail)
                }
                _ => tail,
            }
            .trim()
            .to_string()
        }
        TitleStyle::Sharppy => {
            let when = m
                .valid_time
                .map(|t| format!("{:04}{:02}{:02}/{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute))
                .unwrap_or_default();
            // Same strings as SHARPpy's skew-T title.
            let kind = match (&m.source, m.init_time, m.forecast_hour) {
                (SourceKind::Model(model), Some(init), Some(fh)) => {
                    format!("({:02}Z  {}  F{:03})", init.hour, model.name(), fh)
                }
                (SourceKind::Model(model), _, _) => format!("({})", model.name()),
                (SourceKind::Observed, _, _) => "(Observed)".to_string(),
                _ => "(User Selected)".to_string(),
            };
            format!("{stn}   {when}  {kind}")
        }
    }
}
