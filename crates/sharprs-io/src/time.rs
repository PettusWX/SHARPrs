//! A tiny UTC timestamp type (minute resolution), so the public API does not force a
//! date/time crate on downstream users.

use std::fmt;

/// A UTC calendar time with minute resolution.
///
/// Ordering is chronological. Construct with [`UtcTime::new`] (validated) or parse
/// one of the formats found in sounding files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    ((y + i64::from(m <= 2)) as i32, m, d)
}

impl UtcTime {
    /// Validated constructor. Returns `None` for impossible dates/times.
    pub fn new(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> Option<Self> {
        if !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
        {
            return None;
        }
        Some(Self { year, month, day, hour, minute })
    }

    /// Minutes since 1970-01-01 00:00 UTC.
    pub fn unix_minutes(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 1440
            + i64::from(self.hour) * 60
            + i64::from(self.minute)
    }

    /// Inverse of [`UtcTime::unix_minutes`].
    pub fn from_unix_minutes(m: i64) -> Self {
        let days = m.div_euclid(1440);
        let rem = m.rem_euclid(1440);
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day, hour: (rem / 60) as u8, minute: (rem % 60) as u8 }
    }

    /// Shift by a (possibly negative) number of hours.
    pub fn add_hours(&self, hours: i64) -> Self {
        Self::from_unix_minutes(self.unix_minutes() + hours * 60)
    }

    /// Shift by a (possibly negative) number of minutes.
    pub fn add_minutes(&self, minutes: i64) -> Self {
        Self::from_unix_minutes(self.unix_minutes() + minutes)
    }

    /// Whole hours from `earlier` to `self` (truncated toward zero).
    pub fn hours_since(&self, earlier: &UtcTime) -> i64 {
        (self.unix_minutes() - earlier.unix_minutes()) / 60
    }

    /// Parse the GEMPAK/SPC style `YYMMDD/HHMM` (also accepts `YYYYMMDD/HHMM` and a
    /// missing `/HHMM`). Two-digit years 70..=99 map to 19xx, the rest to 20xx.
    pub fn parse_yymmdd_hhmm(s: &str) -> Option<Self> {
        let s = s.trim();
        let (date, time) = match s.split_once('/') {
            Some((d, t)) => (d, t),
            None => (s, "0000"),
        };
        if !date.bytes().all(|b| b.is_ascii_digit()) || !time.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (year, rest) = match date.len() {
            6 => {
                let yy: i32 = date[..2].parse().ok()?;
                (if yy >= 70 { 1900 + yy } else { 2000 + yy }, &date[2..])
            }
            8 => (date[..4].parse().ok()?, &date[4..]),
            _ => return None,
        };
        let month: u8 = rest[..2].parse().ok()?;
        let day: u8 = rest[2..4].parse().ok()?;
        let (hour, minute) = match time.len() {
            4 => (time[..2].parse().ok()?, time[2..].parse().ok()?),
            2 => (time.parse().ok()?, 0),
            _ => return None,
        };
        Self::new(year, month, day, hour, minute)
    }

    /// Parse ISO-8601-ish strings: `2013-05-21T00:00:00Z`, `2013-05-21 00:00`,
    /// `2013-05-21T00Z`, `2013-05-21` and compact `201305210000`.
    pub fn parse_iso(s: &str) -> Option<Self> {
        let s = s.trim().trim_end_matches('Z').trim_end_matches("+00:00");
        if !s.is_ascii() { return None; }
        if s.len() >= 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
            let year: i32 = s[..4].parse().ok()?;
            let month: u8 = s[5..7].parse().ok()?;
            let day: u8 = s[8..10].parse().ok()?;
            let rest = s[10..].trim_start_matches(['T', ' ']);
            let mut parts = rest.split(':');
            let hour: u8 = match parts.next() {
                Some(h) if !h.is_empty() => h.trim().parse().ok()?,
                _ => 0,
            };
            let minute: u8 = match parts.next() {
                Some(m) if !m.is_empty() => m.trim().parse().ok()?,
                _ => 0,
            };
            return Self::new(year, month, day, hour, minute);
        }
        if (s.len() == 12 || s.len() == 10) && s.bytes().all(|b| b.is_ascii_digit()) {
            let minute = if s.len() == 12 { s[10..12].parse().ok()? } else { 0 };
            return Self::new(
                s[..4].parse().ok()?,
                s[4..6].parse().ok()?,
                s[6..8].parse().ok()?,
                s[8..10].parse().ok()?,
                minute,
            );
        }
        None
    }

    /// Parse a three-letter English month abbreviation (case-insensitive).
    pub(crate) fn month_from_abbrev(s: &str) -> Option<u8> {
        let up = s.trim().to_ascii_uppercase();
        let up = up.get(..3)?;
        MONTHS.iter().position(|m| *m == up).map(|i| i as u8 + 1)
    }

    /// `YYMMDD/HHMM` as used by SPC text and BUFKIT.
    pub fn to_yymmdd_hhmm(&self) -> String {
        format!(
            "{:02}{:02}{:02}/{:02}{:02}",
            self.year.rem_euclid(100),
            self.month,
            self.day,
            self.hour,
            self.minute
        )
    }

    /// `YYYY-MM-DDTHH:MM:00Z`
    pub fn to_iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:00Z",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

impl fmt::Display for UtcTime {
    /// `YYYY-MM-DD HH:MMZ`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}
