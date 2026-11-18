//! The only thermodynamics the renderer needs (skew-T background lines),
//! delegated to `sharprs-core` so the physics lives in one place.

/// Temperature (°C) at which air at `p_hpa` has mixing ratio `w_gkg`.
pub(crate) fn temp_at_mixrat(w_gkg: f64, p_hpa: f64) -> f64 {
    sharprs_core::thermo::temp_at_mixrat(w_gkg, p_hpa)
}

/// Temperature (°C) of a dry adiabat with potential temperature
/// `theta_c` (°C, referenced to 1000 hPa) at pressure `p_hpa`.
pub(crate) fn dry_adiabat_temp(theta_c: f64, p_hpa: f64) -> f64 {
    let k = sharprs_core::thermo::ZEROCNK;
    (theta_c + k) / (1000.0 / p_hpa).powf(sharprs_core::thermo::ROCP) - k
}
