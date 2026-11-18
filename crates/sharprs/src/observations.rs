//! Retrieve radiosonde observations from the Iowa Environmental Mesonet.
use crate::io::{self, DecodeOptions, Format, SoundingSeries, UtcTime};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Most recent scheduled 00Z or 12Z observation cycle.
pub fn current_cycle() -> UtcTime {
    let minutes = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 60;
    UtcTime::from_unix_minutes((minutes / 720 * 720) as i64)
}

/// Fetch the selected station and time. Also return the original JSON so a
/// caller can save a reproducible input with its station and valid time.
pub fn fetch(station: &str, time: UtcTime) -> Result<(SoundingSeries, Vec<u8>), String> {
    let station = station.trim().to_ascii_uppercase();
    if !(3..=4).contains(&station.len()) || !station.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Enter a 3- or 4-character sounding station ID, such as KOAX".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let url = format!(
        "https://mesonet.agron.iastate.edu/json/raob.py?station={station}&ts={}",
        time.to_iso8601()
    );
    let response = agent
        .get(&url)
        .call()
        .and_then(|mut r| {
            r.body_mut()
                .with_config()
                .limit(8 * 1024 * 1024)
                .read_to_vec()
        })
        .map_err(|e| format!("IEM request for {station} at {time} failed · {e}"))?;
    let series = io::decode_bytes(&response, &DecodeOptions::with_format(Format::IemJson))
        .map_err(|e| format!("IEM · {station} at {time} · {e}"))?;
    Ok((series, response))
}
