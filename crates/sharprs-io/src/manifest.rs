//! Reader for `examples/data/manifest.json`, the example-sounding gallery index.

use crate::error::{Error, Result};
use crate::sounding::Sounding;
use crate::{DecodeOptions, Format, SourceKind, read_file_with};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The whole manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub description: String,
    pub soundings: Vec<ManifestEntry>,
    /// Directory the manifest was loaded from (entry `file`s are relative to it).
    #[serde(skip)]
    pub base_dir: PathBuf,
}

/// One gallery entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    /// Path relative to the manifest's directory.
    pub file: String,
    /// `bufkit`, `iem_json`, `uwyo`, `spc` or `csv`.
    pub format: String,
    pub title: String,
    /// `hrrr`, `rap`, `ruc`, … or `observed`.
    pub kind: String,
    /// `severe` or `edge` (non-severe robustness case).
    #[serde(default)]
    pub category: String,
    pub event: String,
    pub station: String,
    #[serde(default)]
    pub station_name: String,
    #[serde(default)]
    pub init_time: Option<String>,
    /// Forecast hour to plot (0 for observations).
    #[serde(default)]
    pub forecast_hour: u32,
    pub valid_time: String,
    pub source_url: String,
    pub note: String,
    #[serde(default)]
    pub attribution: Option<String>,
}

impl Manifest {
    /// Load a manifest from a JSON file.
    pub fn load(path: impl AsRef<Path>) -> Result<Manifest> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let mut m: Manifest = serde_json::from_str(&text).map_err(|e| Error::Json(e.to_string()))?;
        m.base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Option<&ManifestEntry> {
        self.soundings.iter().find(|e| e.id == id)
    }

    /// Absolute/relative path to an entry's data file.
    pub fn path_of(&self, e: &ManifestEntry) -> PathBuf {
        self.base_dir.join(&e.file)
    }

    /// Decode the entry and return the sounding at its `forecast_hour`.
    pub fn load_sounding(&self, e: &ManifestEntry) -> Result<Sounding> {
        let opts = DecodeOptions {
            format: Format::from_name(&e.format),
            source: Some(SourceKind::from_name(&e.kind)),
            ..Default::default()
        };
        let series = read_file_with(self.path_of(e), &opts)?;
        let fmt = opts.format.unwrap_or(Format::Spc);
        let mut s = if series.len() == 1 && e.forecast_hour == 0 {
            series.into_first()
        } else {
            series.at_forecast_hour(e.forecast_hour).cloned()
        }
        .ok_or(Error::NoData { format: fmt })?;
        if s.meta.station_name.is_none() && !e.station_name.is_empty() {
            s.meta.station_name = Some(e.station_name.clone());
        }
        Ok(s)
    }
}
