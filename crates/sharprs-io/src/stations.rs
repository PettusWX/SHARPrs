//! A small built-in table of North American radiosonde sites, used to fill latitude /
//! longitude / elevation for sources that only give a station id (IEM JSON, SPC text
//! without a location). Coordinates are approximate (±0.05°), which is ample for
//! sounding analysis (Coriolis terms, titles, maps).

use crate::sounding::SoundingMeta;

/// One radiosonde site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Three-letter identifier, e.g. `"OUN"`.
    pub id: &'static str,
    /// WMO block/station number, e.g. `"72357"`.
    pub wmo: &'static str,
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
}

macro_rules! st {
    ($id:literal, $wmo:literal, $name:literal, $lat:literal, $lon:literal, $elev:literal) => {
        Station { id: $id, wmo: $wmo, name: $name, lat: $lat, lon: $lon, elevation_m: $elev }
    };
}

/// CONUS + nearby radiosonde network (approximate).
pub const STATIONS: &[Station] = &[
    st!("ABQ", "72365", "Albuquerque, NM", 35.04, -106.62, 1619.0),
    st!("ABR", "72659", "Aberdeen, SD", 45.46, -98.41, 397.0),
    st!("ALB", "72518", "Albany, NY", 42.69, -73.83, 93.0),
    st!("AMA", "72363", "Amarillo, TX", 35.23, -101.71, 1095.0),
    st!("APX", "72634", "Gaylord, MI", 44.91, -84.72, 448.0),
    st!("BIS", "72764", "Bismarck, ND", 46.77, -100.76, 503.0),
    st!("BMX", "72230", "Birmingham, AL", 33.17, -86.77, 178.0),
    st!("BNA", "72327", "Nashville, TN", 36.25, -86.56, 180.0),
    st!("OHX", "72327", "Nashville, TN", 36.25, -86.56, 180.0),
    st!("BOI", "72681", "Boise, ID", 43.57, -116.21, 874.0),
    st!("BRO", "72250", "Brownsville, TX", 25.92, -97.42, 7.0),
    st!("BUF", "72528", "Buffalo, NY", 42.94, -78.72, 218.0),
    st!("CAR", "72712", "Caribou, ME", 46.87, -68.02, 191.0),
    st!("CHH", "74494", "Chatham, MA", 41.67, -69.97, 16.0),
    st!("CHS", "72208", "Charleston, SC", 32.90, -80.03, 15.0),
    st!("CRP", "72251", "Corpus Christi, TX", 27.78, -97.51, 14.0),
    st!("DDC", "72451", "Dodge City, KS", 37.76, -99.97, 790.0),
    st!("DNR", "72469", "Denver, CO", 39.77, -104.87, 1611.0),
    st!("DRT", "72261", "Del Rio, TX", 29.37, -100.92, 313.0),
    st!("DTX", "72632", "Detroit/White Lake, MI", 42.70, -83.47, 329.0),
    st!("DVN", "74455", "Davenport, IA", 41.61, -90.58, 229.0),
    st!("EPZ", "72364", "Santa Teresa, NM", 31.87, -106.70, 1252.0),
    st!("FFC", "72215", "Peachtree City, GA", 33.36, -84.57, 246.0),
    st!("FGZ", "72376", "Flagstaff, AZ", 35.23, -111.82, 2179.0),
    st!("FWD", "72249", "Fort Worth, TX", 32.83, -97.30, 196.0),
    st!("GGW", "72768", "Glasgow, MT", 48.21, -106.63, 693.0),
    st!("GJT", "72476", "Grand Junction, CO", 39.12, -108.53, 1475.0),
    st!("GRB", "72645", "Green Bay, WI", 44.48, -88.13, 209.0),
    st!("GSO", "72317", "Greensboro, NC", 36.10, -79.94, 277.0),
    st!("GYX", "74389", "Gray, ME", 43.89, -70.26, 125.0),
    st!("IAD", "72403", "Sterling, VA", 38.98, -77.49, 88.0),
    st!("ILN", "72426", "Wilmington, OH", 39.42, -83.82, 317.0),
    st!("ILX", "74560", "Lincoln, IL", 40.15, -89.34, 178.0),
    st!("INL", "72747", "International Falls, MN", 48.57, -93.38, 361.0),
    st!("JAN", "72235", "Jackson, MS", 32.32, -90.08, 91.0),
    st!("JAX", "72206", "Jacksonville, FL", 30.50, -81.70, 9.0),
    st!("KEY", "72201", "Key West, FL", 24.55, -81.79, 2.0),
    st!("LBF", "72562", "North Platte, NE", 41.13, -100.68, 849.0),
    st!("LCH", "72240", "Lake Charles, LA", 30.12, -93.22, 5.0),
    st!("LIX", "72233", "Slidell, LA", 30.34, -89.83, 8.0),
    st!("LZK", "72340", "Little Rock, AR", 34.83, -92.26, 172.0),
    st!("MAF", "72265", "Midland, TX", 31.95, -102.19, 873.0),
    st!("MFL", "72202", "Miami, FL", 25.75, -80.38, 4.0),
    st!("MHX", "72305", "Newport, NC", 34.78, -76.88, 11.0),
    st!("MPX", "72649", "Chanhassen, MN", 44.85, -93.57, 287.0),
    st!("NKX", "72293", "San Diego, CA", 32.85, -117.12, 128.0),
    st!("OAK", "72493", "Oakland, CA", 37.73, -122.22, 3.0),
    st!("OAX", "72558", "Omaha/Valley, NE", 41.32, -96.37, 350.0),
    st!("OKX", "72501", "Upton, NY", 40.87, -72.86, 20.0),
    st!("OTX", "72786", "Spokane, WA", 47.68, -117.63, 728.0),
    st!("OUN", "72357", "Norman, OK", 35.18, -97.44, 357.0),
    st!("PIT", "72520", "Pittsburgh, PA", 40.53, -80.22, 360.0),
    st!("REV", "72489", "Reno, NV", 39.57, -119.80, 1516.0),
    st!("RIW", "72672", "Riverton, WY", 43.06, -108.48, 1688.0),
    st!("RNK", "72318", "Blacksburg, VA", 37.20, -80.41, 648.0),
    st!("SGF", "72440", "Springfield, MO", 37.23, -93.40, 394.0),
    st!("SHV", "72248", "Shreveport, LA", 32.45, -93.84, 84.0),
    st!("SLC", "72572", "Salt Lake City, UT", 40.77, -111.95, 1289.0),
    st!("SLE", "72694", "Salem, OR", 44.92, -123.02, 61.0),
    st!("TBW", "72210", "Tampa Bay, FL", 27.70, -82.40, 13.0),
    st!("TFX", "72776", "Great Falls, MT", 47.46, -111.38, 1130.0),
    st!("TLH", "72214", "Tallahassee, FL", 30.45, -84.30, 53.0),
    st!("TOP", "72456", "Topeka, KS", 39.07, -95.63, 268.0),
    st!("TUS", "72274", "Tucson, AZ", 32.23, -110.96, 751.0),
    st!("UNR", "72662", "Rapid City, SD", 44.07, -103.21, 1029.0),
    st!("VEF", "72388", "Las Vegas, NV", 36.05, -115.18, 697.0),
    st!("WAL", "72402", "Wallops Island, VA", 37.93, -75.47, 13.0),
];

/// Look up by three-letter id (`OUN`), ICAO-style id (`KOUN`) or WMO number (`72357`).
pub fn lookup(id: &str) -> Option<&'static Station> {
    let id = id.trim().to_ascii_uppercase();
    if id.chars().all(|c| c.is_ascii_digit()) {
        return STATIONS.iter().find(|s| s.wmo == id);
    }
    let short = if id.len() == 4 && (id.starts_with('K') || id.starts_with('P')) { &id[1..] } else { &id[..] };
    STATIONS.iter().find(|s| s.id == short)
}

/// Fill missing lat/lon/elevation/name for observed soundings from the table. Model
/// soundings are left alone (their files carry exact grid-point coordinates).
pub(crate) fn fill_location(meta: &mut SoundingMeta) {
    if meta.source.model().is_some() {
        return;
    }
    let st = meta
        .station_id
        .as_deref()
        .and_then(lookup)
        .or_else(|| meta.station_number.as_deref().and_then(lookup));
    let Some(st) = st else { return };
    if meta.lat.is_none() || meta.lon.is_none() {
        meta.lat = Some(st.lat);
        meta.lon = Some(st.lon);
    }
    if meta.elevation_m.is_none() {
        meta.elevation_m = Some(st.elevation_m);
    }
    if meta.station_name.is_none() {
        meta.station_name = Some(st.name.to_string());
    }
    if meta.station_number.is_none() {
        meta.station_number = Some(st.wmo.to_string());
    }
    // the current UWyo site only gives the WMO number; prefer the familiar id
    if meta.station_id.as_deref().is_none_or(|s| s.chars().all(|c| c.is_ascii_digit())) {
        meta.station_id = Some(st.id.to_string());
    }
}
