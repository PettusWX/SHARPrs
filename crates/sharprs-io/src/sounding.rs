//! Format-neutral sounding types.
//!
//! # Units and missing values
//!
//! | column | unit |
//! |--------|------|
//! | `pres` | hPa |
//! | `hght` | m above mean sea level |
//! | `tmpc` | °C |
//! | `dwpc` | °C |
//! | `wdir` | degrees, meteorological (direction wind blows *from*) |
//! | `wspd` | knots |
//! | `omeg` | Pa s⁻¹ (pressure vertical velocity, negative = rising) |
//!
//! **Missing values are `f64::NAN`** in every column (see [`MISSING`] and
//! [`is_missing`]). After quality control `pres` is never missing and levels are ordered
//! surface-up (strictly decreasing pressure). The row view [`Level`] exposes the same data
//! as `Option<f64>` for convenience.

use crate::qc::QcReport;
use crate::time::UtcTime;
use crate::Format;
use std::fmt;

/// The sentinel used for missing values in [`Sounding`] columns.
pub const MISSING: f64 = f64::NAN;

/// `true` if `v` represents a missing value (NaN or infinite).
#[inline]
pub fn is_missing(v: f64) -> bool {
    !v.is_finite()
}

#[inline]
fn opt(v: f64) -> Option<f64> {
    if v.is_finite() { Some(v) } else { None }
}

/// Numerical weather prediction model that produced a forecast sounding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Model {
    Hrrr,
    Rap,
    Ruc,
    Nam,
    /// NAM CONUS nest (BUFKIT archives call it `nam4km`/`namm`).
    NamNest,
    Gfs,
    Other(String),
}

impl Model {
    /// Parse a model name or file-name prefix such as `hrrr`, `RAP`, `nam4km`, `gfs3`.
    pub fn from_name(name: &str) -> Model {
        let n = name.trim().to_ascii_lowercase();
        match n.as_str() {
            "hrrr" | "hrrrx" => Model::Hrrr,
            "rap" | "rr" => Model::Rap,
            "ruc" | "ruc2" => Model::Ruc,
            "nam" | "eta" => Model::Nam,
            "nam4km" | "nam3km" | "namnest" | "namm" | "nam_nest" => Model::NamNest,
            "gfs" | "gfs3" | "gfsm" | "avn" => Model::Gfs,
            _ => Model::Other(name.trim().to_ascii_uppercase()),
        }
    }

    /// Short upper-case display name, e.g. `"HRRR"`.
    pub fn name(&self) -> &str {
        match self {
            Model::Hrrr => "HRRR",
            Model::Rap => "RAP",
            Model::Ruc => "RUC",
            Model::Nam => "NAM",
            Model::NamNest => "NAM3KM",
            Model::Gfs => "GFS",
            Model::Other(s) => s,
        }
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where a sounding came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum SourceKind {
    /// A radiosonde (or other) observation.
    Observed,
    /// A model forecast/analysis point sounding.
    Model(Model),
    /// Not known (e.g. CSV without metadata, or BUFKIT text without a file name).
    #[default]
    Unknown,
}

impl SourceKind {
    pub fn is_observed(&self) -> bool {
        matches!(self, SourceKind::Observed)
    }
    pub fn model(&self) -> Option<&Model> {
        match self {
            SourceKind::Model(m) => Some(m),
            _ => None,
        }
    }
    /// Parse `observed`/`obs`/`raob` or any model name.
    pub fn from_name(name: &str) -> SourceKind {
        match name.trim().to_ascii_lowercase().as_str() {
            "" | "unknown" => SourceKind::Unknown,
            "observed" | "obs" | "raob" | "radiosonde" => SourceKind::Observed,
            _ => SourceKind::Model(Model::from_name(name)),
        }
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::Observed => f.write_str("Observed"),
            SourceKind::Model(m) => write!(f, "{m}"),
            SourceKind::Unknown => f.write_str("Unknown"),
        }
    }
}

/// Descriptive metadata for one sounding.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundingMeta {
    /// Station identifier as written in the source (e.g. `"OUN"`, `"KPAH"`, `"72357"`).
    pub station_id: Option<String>,
    /// Human-readable station name, when the source provides one.
    pub station_name: Option<String>,
    /// WMO / BUFKIT station number when known (e.g. `"72357"`, `"724350"`).
    pub station_number: Option<String>,
    /// Degrees north.
    pub lat: Option<f64>,
    /// Degrees east (negative in the western hemisphere).
    pub lon: Option<f64>,
    /// Station / model-terrain elevation, m MSL.
    pub elevation_m: Option<f64>,
    pub source: SourceKind,
    /// Model initialization time (`None` for observations).
    pub init_time: Option<UtcTime>,
    /// Forecast hour relative to `init_time` (`Some(0)` is an analysis; `None` for obs).
    pub forecast_hour: Option<u32>,
    /// Valid (observation) time.
    pub valid_time: Option<UtcTime>,
    /// The decoder that produced this sounding.
    pub format: Option<Format>,
}

impl SoundingMeta {
    /// A short title like `"HRRR F09 KPAH valid 2021-12-11 03:00Z"`.
    pub fn title(&self) -> String {
        let stn = self.station_id.as_deref().unwrap_or("UNKNOWN");
        let when = self.valid_time.map(|t| t.to_string()).unwrap_or_else(|| "unknown time".into());
        match (&self.source, self.forecast_hour) {
            (SourceKind::Model(m), Some(f)) => format!("{m} F{f:02} {stn} valid {when}"),
            (SourceKind::Model(m), None) => format!("{m} {stn} valid {when}"),
            (SourceKind::Observed, _) => format!("Observed {stn} {when}"),
            (SourceKind::Unknown, _) => format!("{stn} {when}"),
        }
    }
}

/// One row of a sounding with explicit `Option`s for missing values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Level {
    pub pres: f64,
    pub hght: Option<f64>,
    pub tmpc: Option<f64>,
    pub dwpc: Option<f64>,
    pub wdir: Option<f64>,
    pub wspd: Option<f64>,
    pub omeg: Option<f64>,
}

/// A single vertical profile stored column-wise (struct of arrays).
///
/// All column vectors always have the same length. `omeg` is `None` when the source has
/// no vertical-velocity column at all; otherwise it has the same length as the others
/// (with NaN where missing).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sounding {
    pub meta: SoundingMeta,
    pub pres: Vec<f64>,
    pub hght: Vec<f64>,
    pub tmpc: Vec<f64>,
    pub dwpc: Vec<f64>,
    pub wdir: Vec<f64>,
    pub wspd: Vec<f64>,
    pub omeg: Option<Vec<f64>>,
    /// What quality control changed while decoding.
    pub qc: QcReport,
}

impl Sounding {
    /// An empty sounding with the given metadata.
    pub fn new(meta: SoundingMeta) -> Self {
        Self { meta, ..Default::default() }
    }

    /// Build from rows. `omeg` column is kept only if any row has omega.
    pub fn from_levels(meta: SoundingMeta, levels: &[Level]) -> Self {
        let mut s = Self::new(meta);
        let has_omeg = levels.iter().any(|l| l.omeg.is_some());
        if has_omeg {
            s.omeg = Some(Vec::with_capacity(levels.len()));
        }
        for l in levels {
            s.push(*l);
        }
        s
    }

    /// Append a level (no QC is applied).
    pub fn push(&mut self, l: Level) {
        let f = |o: Option<f64>| o.unwrap_or(MISSING);
        self.pres.push(l.pres);
        self.hght.push(f(l.hght));
        self.tmpc.push(f(l.tmpc));
        self.dwpc.push(f(l.dwpc));
        self.wdir.push(f(l.wdir));
        self.wspd.push(f(l.wspd));
        match (&mut self.omeg, l.omeg) {
            (Some(o), v) => o.push(f(v)),
            (None, Some(v)) => {
                let mut o = vec![MISSING; self.pres.len() - 1];
                o.push(v);
                self.omeg = Some(o);
            }
            (None, None) => {}
        }
    }

    pub fn len(&self) -> usize {
        self.pres.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pres.is_empty()
    }

    /// Row `i` as a [`Level`].
    pub fn level(&self, i: usize) -> Level {
        Level {
            pres: self.pres[i],
            hght: opt(self.hght[i]),
            tmpc: opt(self.tmpc[i]),
            dwpc: opt(self.dwpc[i]),
            wdir: opt(self.wdir[i]),
            wspd: opt(self.wspd[i]),
            omeg: self.omeg.as_ref().and_then(|o| opt(o[i])),
        }
    }

    /// Iterate rows surface-up.
    pub fn levels(&self) -> impl Iterator<Item = Level> + '_ {
        (0..self.len()).map(|i| self.level(i))
    }

    /// The lowest level (after QC this is the surface).
    pub fn surface(&self) -> Option<Level> {
        (!self.is_empty()).then(|| self.level(0))
    }

    /// `true` if any non-missing omega value is present.
    pub fn has_omega(&self) -> bool {
        self.omeg.as_ref().is_some_and(|o| o.iter().any(|v| v.is_finite()))
    }

    /// Number of levels with a valid temperature.
    pub fn n_temperature(&self) -> usize {
        self.tmpc.iter().filter(|v| v.is_finite()).count()
    }

    /// Re-run quality control in place (decoders already do this).
    pub fn apply_qc(&mut self) {
        let report = crate::qc::run(self);
        self.qc = report;
    }
}

/// A time series of soundings at one location (e.g. all forecast hours in a BUFKIT
/// file, or several observation times in one UWyo page). Ordered as in the source,
/// which is chronological for every supported format.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundingSeries {
    pub soundings: Vec<Sounding>,
}

impl SoundingSeries {
    pub fn new(soundings: Vec<Sounding>) -> Self {
        Self { soundings }
    }
    pub fn len(&self) -> usize {
        self.soundings.len()
    }
    pub fn is_empty(&self) -> bool {
        self.soundings.is_empty()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Sounding> {
        self.soundings.iter()
    }
    pub fn get(&self, i: usize) -> Option<&Sounding> {
        self.soundings.get(i)
    }
    pub fn first(&self) -> Option<&Sounding> {
        self.soundings.first()
    }
    /// The sounding for forecast hour `fhr`. For a single observed sounding, `fhr == 0`
    /// returns it.
    pub fn at_forecast_hour(&self, fhr: u32) -> Option<&Sounding> {
        self.soundings
            .iter()
            .find(|s| s.meta.forecast_hour == Some(fhr))
            .or_else(|| {
                (fhr == 0 && self.soundings.len() == 1 && self.soundings[0].meta.forecast_hour.is_none())
                    .then(|| &self.soundings[0])
            })
    }
    /// The sounding whose valid time equals `t`.
    pub fn at_valid_time(&self, t: UtcTime) -> Option<&Sounding> {
        self.soundings.iter().find(|s| s.meta.valid_time == Some(t))
    }
    /// The sounding valid closest to `t`.
    pub fn nearest_valid_time(&self, t: UtcTime) -> Option<&Sounding> {
        self.soundings
            .iter()
            .filter_map(|s| s.meta.valid_time.map(|v| ((v.unix_minutes() - t.unix_minutes()).abs(), s)))
            .min_by_key(|(d, _)| *d)
            .map(|(_, s)| s)
    }
    /// Consume and return the first sounding.
    pub fn into_first(self) -> Option<Sounding> {
        self.soundings.into_iter().next()
    }
}

impl IntoIterator for SoundingSeries {
    type Item = Sounding;
    type IntoIter = std::vec::IntoIter<Sounding>;
    fn into_iter(self) -> Self::IntoIter {
        self.soundings.into_iter()
    }
}

impl<'a> IntoIterator for &'a SoundingSeries {
    type Item = &'a Sounding;
    type IntoIter = std::slice::Iter<'a, Sounding>;
    fn into_iter(self) -> Self::IntoIter {
        self.soundings.iter()
    }
}
