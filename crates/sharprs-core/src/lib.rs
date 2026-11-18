//! `sharprs-core`: the sounding physics and parameter engine for SHARPrs.
//!
//! The numbers produced here are designed to be identical to SHARPpy's
//! (the SPC sounding analysis program), including its documented quirks.
//! SHARPpy served as the numerical reference only; the implementation is
//! independent.
//!
//! # Quick start
//! ```
//! use sharprs_core::{Sounding, Profile, ConvectiveAnalysis};
//! let p = [1000., 925., 850., 700., 500., 300., 200.];
//! let h = [100., 760., 1490., 3100., 5800., 9500., 12000.];
//! let t = [25., 20., 15., 5., -12., -40., -55.];
//! let td = [20., 16., 10., -5., -25., -50., -65.];
//! let wd = [180., 200., 220., 240., 260., 270., 270.];
//! let ws = [10., 25., 35., 40., 50., 70., 80.];
//! let snd = Sounding::from_raw(&p, &h, &t, &td, &wd, &ws, -9999.);
//! let prof = Profile::new(&snd).unwrap();
//! let a = ConvectiveAnalysis::compute(&prof);
//! assert!(a.parcels.mu.cape.unwrap() > 0.);
//! ```
//!
//! # Conventions
//! * Pressure hPa, height m (MSL for profile heights, AGL for parcel and
//!   layer heights as in SHARPpy), temperature °C, wind kt, omega Pa/s.
//! * Missing values are `Option::None` in the public API.
//! * [`thermo`] holds the scalar thermodynamic primitives used to draw skew-T
//!   background lines.

// Formulas intentionally mirror the reference arithmetic (operation order,
// branch structure, NaN behaviour of explicit comparisons instead of
// `clamp`), so these style lints are silenced crate-wide.
#![allow(
    clippy::excessive_precision,
    clippy::neg_multiply,
    clippy::manual_clamp,
    clippy::if_same_then_else,
    clippy::manual_range_contains,
    clippy::needless_range_loop
)]

pub mod analysis;
pub mod display;
pub mod climo;
pub mod downdraft;
pub mod fire;
pub mod hazard;
pub mod indices;
pub mod layer;
mod numerics;
pub mod parcel;
pub mod profile;
pub mod sars;
pub mod thermo;
pub mod units;
pub mod winds;
pub mod winter;

pub use analysis::{
    AnalysisOptions, ConvectiveAnalysis, Deviant, DeviantSevere, EffectiveLayer, FireWeather, Kinematics,
    MaxLapseRate, Parcels, Sars, Severe, StormRelative, ThermoIndices, Winter,
};
pub use downdraft::Downdraft;
pub use fire::HainesElevation;
pub use hazard::HazardType;
pub use indices::{AdvectionLayer, Slinky};
pub use layer::Scheme;
pub use parcel::{Parcel, ParcelKind, ParcelSpec, TracePoint};
pub use profile::{DateTime, Field, Level, Metadata, Profile, ProfileError, Sounding, WindColumns, column_from_raw};
pub use units::Vector;
pub use winds::{Corfidi, Helicity, StormMotion};
pub use winter::{InitialPhase, Phase, PosNeg, PrecipType};
