//! User-facing render options (SHARPpy preferences plus Pivotal-style extras).

use crate::text::{FontFamily, TextBackend, TextHinting};
use crate::theme::{ColorScheme, PwUnits, TempUnits, Theme, WindUnits};

/// Which storm mover the display follows (SHARPpy's "deviant" toggle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Deviant {
    #[default]
    Right,
    Left,
}

/// The swappable bottom-right insets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Inset {
    /// "SARS - Sounding Analogs".
    Sars,
    /// "Effective Layer STP (with CIN)" box-and-whisker.
    StpStats,
    /// Conditional tornado probabilities based on STPC.
    CondStp,
    /// Winter weather.
    Winter,
    /// Fire weather.
    Fire,
    /// Significant hail parameter.
    Ship,
    /// EF-scale probabilities based on rotational velocity.
    Vrot,
}

/// Small text/label differences between SHARPpy releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Flavor {
    /// The SHARPpy build Pivotal Weather runs: kinematic table with an EHI
    /// column, "SARS - Sounding Analogs", header strings above the panels.
    #[default]
    Pivotal,
    /// SHARPpy 1.4 as shipped (the bundled sample_*.png look).
    Classic,
}

/// Everything that controls how a [`crate::SoundingView`] is drawn.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RenderOptions {
    /// Output size in pixels. The SPC window is designed for 1180x800;
    /// other sizes scale the layout.
    pub width: u32,
    pub height: u32,
    pub scheme: ColorScheme,
    /// Replace the scheme's colors entirely (e.g. user-customized prefs).
    pub theme_override: Option<Theme>,
    pub temp_units: TempUnits,
    pub wind_units: WindUnits,
    pub pw_units: PwUnits,
    pub font_family: FontFamily,
    pub hinting: TextHinting,
    /// Whose font metrics (pixel-size and ascent/descent rounding) to
    /// reproduce: FreeType (Linux/Pivotal) or CoreText (macOS desktop).
    pub text_backend: TextBackend,
    /// Logical DPI used to convert SHARPpy's point sizes to pixels
    /// (Pivotal's Linux renderer runs at 96).
    pub dpi: f32,
    /// Top-left header; `None` uses `SoundingView::title`.
    pub header_left: Option<String>,
    /// Top-right header (SHARPpy's brand label).
    pub header_right: Option<String>,
    pub left_inset: Inset,
    pub right_inset: Inset,
    /// Explicit storm mover. By default the sounding latitude selects the mover.
    pub deviant: Option<Deviant>,
    /// Draw the dendritic growth zone instead of the max lapse rate / temp levels.
    pub show_dgz: bool,
    /// Draw the PBL top marker.
    pub show_pbl: bool,
    pub show_wet_bulb: bool,
    pub show_virtual_temp: bool,
    pub show_parcel_trace: bool,
    pub show_downdraft: bool,
    pub show_wind_barbs: bool,
    /// Force the omega profile on/off; `None` follows `!view.observed`.
    pub show_omega: Option<bool>,
    pub flavor: Flavor,
    /// Gamma applied to text coverage when compositing (1.0 = linear).
    /// 2.0 reproduces the stroke weight of Qt/FreeType text in Pivotal
    /// Weather output.
    pub text_gamma: f32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: 1180,
            height: 800,
            scheme: ColorScheme::Standard,
            theme_override: None,
            temp_units: TempUnits::Fahrenheit,
            wind_units: WindUnits::Knots,
            pw_units: PwUnits::Inches,
            font_family: FontFamily::Roboto,
            hinting: TextHinting::Light,
            text_backend: TextBackend::FreeType,
            dpi: 96.0,
            header_left: None,
            header_right: Some(format!("SHARPrs v{}", env!("CARGO_PKG_VERSION"))),
            left_inset: Inset::Sars,
            right_inset: Inset::StpStats,
            deviant: None,
            show_dgz: false,
            show_pbl: false,
            show_wet_bulb: true,
            show_virtual_temp: true,
            show_parcel_trace: true,
            show_downdraft: true,
            show_wind_barbs: true,
            show_omega: None,
            flavor: Flavor::Pivotal,
            text_gamma: 2.0,
        }
    }
}

impl RenderOptions {
    /// The SHARPpy 1.4 desktop look: Helvetica-metric font (Arimo), classic labels.
    pub fn classic() -> RenderOptions {
        // The bundled SHARPpy screenshots come from macOS, where Qt maps
        // one point to one pixel (72 logical DPI).
        RenderOptions {
            font_family: FontFamily::Arimo,
            flavor: Flavor::Classic,
            dpi: 72.0,
            text_backend: TextBackend::CoreText,
            // CoreText neither hints nor snaps outlines, and its smoothing is
            // lighter than FreeType's on Linux (gamma tuned against real
            // SHARPpy renders on macOS).
            hinting: TextHinting::None,
            text_gamma: 1.4,
            ..Default::default()
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme_override.clone().unwrap_or_else(|| Theme::for_scheme(self.scheme))
    }
}
