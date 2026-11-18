//! Read sounding-analysis overrides used by the desktop and command-line tools.

use crate::core::{AnalysisOptions, ParcelSpec, StormMotion, Vector};
use crate::io::Sounding;

fn numbers<const N: usize>(text: &str, expected: &str) -> Result<[f64; N], String> {
    let mut result = [0.0; N];
    let mut fields = text.split(',');
    for value in &mut result {
        *value = fields
            .next()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("Expected {expected}"))?;
    }
    if fields.next().is_some() {
        return Err(format!("Expected {expected}"));
    }
    Ok(result)
}

/// Parse optional explicit parcel (hPa, °C, °C) and right/left storm vectors
/// (meteorological direction from, speed in knots, for each mover).
pub fn analysis_options(
    sounding: &Sounding,
    parcel: Option<&str>,
    motion: Option<&str>,
) -> Result<AnalysisOptions, String> {
    let user_parcel = parcel
        .map(|text| {
            let [p, t, td] = numbers(text, "parcel pressure, temperature, dewpoint")?;
            let mut pressures = sounding
                .pres
                .iter()
                .zip(&sounding.tmpc)
                .filter(|(p, t)| p.is_finite() && t.is_finite())
                .map(|(&p, _)| p);
            let surface = pressures
                .next()
                .ok_or("No temperature levels for a parcel")?;
            let top = pressures.next_back().unwrap_or(surface);
            if p <= 0.0 || !(top..=surface).contains(&p) {
                return Err(format!(
                    "Parcel pressure must be within this sounding ({top}–{surface} hPa)"
                ));
            }
            if td > t || td <= -273.15 {
                return Err(
                    "Parcel dewpoint must be above absolute zero and at or below temperature"
                        .into(),
                );
            }
            Ok(ParcelSpec::user(p, t, td))
        })
        .transpose()?;
    let storm_motion = motion
        .map(|text| {
            let [rd, rs, ld, ls] = numbers(
                text,
                "right direction, right speed, left direction, left speed",
            )?;
            if !(0.0..=360.0).contains(&rd) || !(0.0..=360.0).contains(&ld) || rs < 0.0 || ls < 0.0
            {
                return Err(String::from(
                    "Storm directions must be 0–360° and speeds must be nonnegative",
                ));
            }
            Ok(StormMotion {
                right: Vector::from_dir_speed(rd, rs),
                left: Vector::from_dir_speed(ld, ls),
            })
        })
        .transpose()?;
    Ok(AnalysisOptions {
        user_parcel,
        storm_motion,
    })
}
