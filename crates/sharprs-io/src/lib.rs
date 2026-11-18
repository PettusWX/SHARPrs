//! # sharprs-io
//!
//! Reads and writes atmospheric soundings for SHARPrs. Decoders turn files/text into the
//! format-neutral [`Sounding`] / [`SoundingSeries`] types; nothing here depends on the
//! physics crate.
//!
//! Supported inputs (auto-detected from content, or forced with [`DecodeOptions::format`]):
//!
//! * **SPC / SHARPpy text** (`%TITLE%` / `%RAW%` / `%END%`), with or without an omega column
//! * **BUFKIT** model soundings (`.buf`): every forecast hour, any column order
//! * **University of Wyoming** `TEXT:LIST` (legacy HTML `<PRE>` pages and the current
//!   WSGI site, including its m/s wind speeds)
//! * **IEM RAOB JSON** (`mesonet.agron.iastate.edu/json/raob.py`)
//! * **CSV** (header-driven, with optional `# key: value` metadata comments)
//!
//! Outputs: SPC text ([`write_spc`]) and CSV ([`write_csv`]).
//!
//! Units: hPa, m MSL, °C, degrees, knots, Pa/s. **Missing values are `f64::NAN`.**
//! All decoders run [`qc`] so profiles come back surface-up with monotonic pressure and
//! heights. See [`sounding`] for details.
//!
//! ```no_run
//! let series = sharprs_io::read_file("examples/data/bufkit/2021121018_hrrr_kpah.buf").unwrap();
//! let f09 = series.at_forecast_hour(9).unwrap();
//! println!("{} levels, {}", f09.len(), f09.meta.title());
//! print!("{}", sharprs_io::write_spc(f09));
//! ```

pub mod error;
pub mod formats;
pub mod manifest;
pub mod qc;
pub mod sounding;
pub mod stations;
pub mod time;

pub use error::{Error, Result};
pub use formats::csv::write_csv;
pub use formats::detect_format;
pub use formats::spc::{SpcWriteOptions, write_spc, write_spc_with};
pub use manifest::{Manifest, ManifestEntry};
pub use qc::QcReport;
pub use sounding::{Level, MISSING, Model, Sounding, SoundingMeta, SoundingSeries, SourceKind, is_missing};
pub use time::UtcTime;

use std::fmt;
use std::path::Path;

/// The file formats this crate understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// SPC / SHARPpy `%TITLE%`/`%RAW%` text.
    Spc,
    /// BUFKIT model sounding file.
    Bufkit,
    /// University of Wyoming TEXT:LIST (HTML or plain text).
    Uwyo,
    /// Iowa Environmental Mesonet RAOB JSON.
    IemJson,
    /// Header-driven delimited text.
    Csv,
}

impl Format {
    pub fn name(&self) -> &'static str {
        match self {
            Format::Spc => "SPC text",
            Format::Bufkit => "BUFKIT",
            Format::Uwyo => "UWyo TEXT:LIST",
            Format::IemJson => "IEM RAOB JSON",
            Format::Csv => "CSV",
        }
    }

    /// Parse a short name such as `spc`, `bufkit`/`buf`, `uwyo`, `iem`/`iem_json`, `csv`.
    pub fn from_name(s: &str) -> Option<Format> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "spc" | "sharppy" | "spc_text" => Format::Spc,
            "bufkit" | "buf" => Format::Bufkit,
            "uwyo" | "wyoming" => Format::Uwyo,
            "iem" | "iem_json" | "json" => Format::IemJson,
            "csv" => Format::Csv,
            _ => return None,
        })
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Options for [`decode`] / [`read_file_with`].
#[derive(Debug, Clone, Default)]
pub struct DecodeOptions {
    /// Force a format instead of auto-detecting.
    pub format: Option<Format>,
    /// Override/augment the source kind. BUFKIT content does not name its model, so
    /// this (or a file name like `hrrr_kpah.buf`) is how the model is known.
    pub source: Option<SourceKind>,
    /// Original file name, used for hints (model prefix, station id).
    pub file_name: Option<String>,
    /// Skip quality control (raw values, except text sentinels still parse as numbers).
    pub skip_qc: bool,
}

impl DecodeOptions {
    pub fn with_format(format: Format) -> Self {
        Self { format: Some(format), ..Default::default() }
    }
}

/// Decode text content. Always returns a series (length 1 for single soundings).
pub fn decode(text: &str, opts: &DecodeOptions) -> Result<SoundingSeries> {
    let format = match opts.format {
        Some(f) => f,
        None => detect_format(text).ok_or(Error::UnknownFormat)?,
    };
    let mut series = match format {
        Format::Spc => formats::spc::decode(text)?,
        Format::Bufkit => formats::bufkit::decode(text)?,
        Format::Uwyo => formats::uwyo::decode(text)?,
        Format::IemJson => formats::iem::decode(text)?,
        Format::Csv => formats::csv::decode(text)?,
    };
    let hinted = opts.source.clone().or_else(|| {
        opts.file_name.as_deref().and_then(source_from_file_name)
    });
    series.soundings.retain(|s| !s.is_empty());
    for s in &mut series.soundings {
        s.meta.format = Some(format);
        if let Some(h) = &hinted {
            if opts.source.is_some() || s.meta.source == SourceKind::Unknown {
                s.meta.source = h.clone();
            }
        }
        if !opts.skip_qc {
            s.apply_qc();
        }
        stations::fill_location(&mut s.meta);
    }
    series.soundings.retain(|s| !s.is_empty());
    if series.is_empty() {
        return Err(Error::NoData { format });
    }
    Ok(series)
}

/// Decode raw bytes (lossy UTF-8; Latin-1 degree signs etc. are tolerated).
pub fn decode_bytes(bytes: &[u8], opts: &DecodeOptions) -> Result<SoundingSeries> {
    decode(&String::from_utf8_lossy(bytes), opts)
}

/// Read and auto-decode a file. The file name is used as a hint (e.g. the `hrrr_`
/// prefix of BUFKIT archive files sets the model).
pub fn read_file(path: impl AsRef<Path>) -> Result<SoundingSeries> {
    read_file_with(path, &DecodeOptions::default())
}

/// Read a file with explicit options. `opts.file_name` defaults to the path's file name.
pub fn read_file_with(path: impl AsRef<Path>, opts: &DecodeOptions) -> Result<SoundingSeries> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    let mut o = opts.clone();
    if o.file_name.is_none() {
        o.file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    }
    decode_bytes(&bytes, &o)
}

/// Write a sounding as SPC text to `path` (6-column SHARPpy-compatible layout).
pub fn write_spc_file(path: impl AsRef<Path>, s: &Sounding) -> Result<()> {
    std::fs::write(path, write_spc(s))?;
    Ok(())
}

/// Guess a model from a file name such as `hrrr_kpah.buf`, `2021121018_hrrr_kpah.buf`,
/// `rap_oun.buf` or `nam4km_kokc.buf`.
pub fn source_from_file_name(name: &str) -> Option<SourceKind> {
    let stem = name.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let stem = stem.split('.').next()?;
    const MODELS: [&str; 12] = [
        "hrrr", "rap", "ruc", "nam4km", "nam3km", "namnest", "namm", "nam", "gfs3", "gfsm", "gfs", "hiresw",
    ];
    for tok in stem.split(['_', '-']) {
        if MODELS.contains(&tok) {
            return Some(SourceKind::Model(Model::from_name(tok)));
        }
    }
    None
}
