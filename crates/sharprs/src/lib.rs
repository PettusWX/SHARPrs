//! # SHARPrs
//!
//! SHARPpy-compatible sounding analysis and SPC-window rendering in pure Rust.
//!
//! This crate ties the three building blocks together:
//! * [`io`] decodes sounding files (SPC text, BUFKIT, UWyo, IEM JSON, CSV),
//! * [`core`] computes everything SHARPpy's main window shows,
//! * [`render`] draws the SPC window (Pivotal Weather look by default).
//!
//! ```no_run
//! let series = sharprs::io::read_file("hrrr_kpah.buf").unwrap();
//! let sounding = series.at_forecast_hour(9).unwrap();
//! let analyzed = sharprs::Analyzed::new(sounding).unwrap();
//! println!("MUCAPE = {:?}", analyzed.analysis.parcels.mu.cape);
//! let png = analyzed.render_png(&sharprs::render::RenderOptions::default()).unwrap();
//! std::fs::write("kpah.png", png).unwrap();
//! ```

pub use sharprs_core as core;
pub use sharprs_io as io;
pub use sharprs_render as render;

mod adapter;
mod inputs;
pub use inputs::analysis_options;
mod title;

pub use adapter::{ViewOptions, build_view};
pub use title::{TitleStyle, sounding_title};

use std::fmt;

/// Errors from the end-to-end pipeline.
#[derive(Debug)]
pub enum Error {
    Io(sharprs_io::Error),
    Profile(sharprs_core::ProfileError),
    Render(String),
    /// The requested forecast hour / time is not in the file.
    NotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Profile(e) => write!(f, "invalid profile: {e}"),
            Error::Render(e) => write!(f, "render failed: {e}"),
            Error::NotFound(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<sharprs_io::Error> for Error {
    fn from(e: sharprs_io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<sharprs_core::ProfileError> for Error {
    fn from(e: sharprs_core::ProfileError) -> Self {
        Error::Profile(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn opt(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

/// Convert a decoded sounding into the physics engine's input.
pub fn to_core_sounding(s: &sharprs_io::Sounding) -> sharprs_core::Sounding {
    let col = |v: &[f64]| v.iter().map(|&x| opt(x)).collect::<Vec<_>>();
    let meta = &s.meta;
    let valid = meta.valid_time.map(|t| sharprs_core::DateTime {
        year: t.year,
        month: t.month,
        day: t.day,
        hour: t.hour,
        minute: t.minute,
    });
    let mut out = sharprs_core::Sounding::new(col(&s.pres), col(&s.hght), col(&s.tmpc), col(&s.dwpc))
        .with_polar_wind(col(&s.wdir), col(&s.wspd))
        .with_meta(sharprs_core::Metadata {
            station: meta.station_id.clone(),
            latitude: meta.lat,
            longitude: meta.lon,
            elevation: meta.elevation_m,
            valid,
        });
    if let Some(om) = &s.omeg {
        out = out.with_omega(col(om));
    }
    out
}

/// A decoded sounding together with its full SHARPpy analysis.
#[derive(Clone, Debug)]
pub struct Analyzed {
    pub sounding: sharprs_io::Sounding,
    pub profile: sharprs_core::Profile,
    pub analysis: sharprs_core::ConvectiveAnalysis,
}

impl Analyzed {
    /// Analyze with default options (Bunkers storm motion, no user parcel).
    pub fn new(sounding: &sharprs_io::Sounding) -> Result<Self> {
        Self::with_options(sounding, &sharprs_core::AnalysisOptions::default())
    }

    pub fn with_options(sounding: &sharprs_io::Sounding, opts: &sharprs_core::AnalysisOptions) -> Result<Self> {
        let profile = sharprs_core::Profile::new(&to_core_sounding(sounding))?;
        let analysis = sharprs_core::ConvectiveAnalysis::new(&profile, opts);
        Ok(Self { sounding: sounding.clone(), profile, analysis })
    }

    /// Build the render view model with default view options.
    pub fn view(&self) -> render::SoundingView {
        self.view_with(&ViewOptions::default())
    }

    pub fn view_with(&self, vopts: &ViewOptions) -> render::SoundingView {
        build_view(&self.sounding, &self.profile, &self.analysis, vopts)
    }

    /// Render the full SPC window as PNG bytes.
    pub fn render_png(&self, opts: &render::RenderOptions) -> Result<Vec<u8>> {
        render::render_png(&self.view(), opts).map_err(Error::Render)
    }
}

/// Which sounding to pick from a multi-time file (BUFKIT).
#[derive(Clone, Copy, Debug, Default)]
pub enum Select {
    /// The first (or only) sounding.
    #[default]
    First,
    ForecastHour(u32),
    /// Index in file order.
    Index(usize),
}

/// Read a file and pick one sounding from it.
pub fn load(path: impl AsRef<std::path::Path>, select: Select) -> Result<sharprs_io::Sounding> {
    let path = path.as_ref();
    let series = sharprs_io::read_file(path)?;
    let found = match select {
        Select::First => series.first(),
        Select::ForecastHour(h) => series.at_forecast_hour(h),
        Select::Index(i) => series.get(i),
    };
    found.cloned().ok_or_else(|| {
        let avail: Vec<String> = series
            .iter()
            .filter_map(|s| s.meta.forecast_hour.map(|h| h.to_string()))
            .collect();
        Error::NotFound(format!(
            "{}: requested sounding not found ({} soundings in file{})",
            path.display(),
            series.len(),
            if avail.is_empty() { String::new() } else { format!("; forecast hours {}", avail.join(",")) }
        ))
    })
}

#[cfg(feature = "observations")]
pub mod observations;
