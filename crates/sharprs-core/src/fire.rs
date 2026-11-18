//! Fire-weather parameters: Fosberg FWI and the Haines index.

use crate::profile::Profile;
use crate::thermo;
use crate::units::kts_to_mph;

/// Which Haines index variant applies at the station elevation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HainesElevation {
    /// Surface below 305 m: low-elevation (950-850 hPa) variant.
    Low,
    /// 305-914 m: mid-elevation (850-700 hPa).
    Mid,
    /// Above 914 m: high-elevation (700-500 hPa).
    High,
}

impl Profile {
    /// Fosberg Fire Weather Index from surface T, RH and wind.
    pub(crate) fn fosberg_raw(&self) -> f64 {
        let s = self.sfc;
        let tmpf = thermo::ctof(self.tmpc[s]);
        let fmph = kts_to_mph(self.wspd[s]);
        let rh = thermo::relh(self.pres[s], self.tmpc[s], self.dwpc[s]);
        let em = if rh <= 10. {
            0.03229 + 0.281073 * rh - 0.000578 * rh * tmpf
        } else if 10. < rh && rh <= 50. {
            2.22749 + 0.160107 * rh - 0.014784 * tmpf
        } else {
            21.0606 + 0.005565 * rh * rh - 0.00035 * rh * tmpf - 0.483199 * rh
        };
        let em30 = em / 30.;
        let u_sq = fmph * fmph;
        let fmdc = 1. - 2. * em30 + 1.5 * em30 * em30 - 0.5 * em30 * em30 * em30;
        (fmdc * (1. + u_sq).sqrt()) / 0.3002
    }

    pub(crate) fn haines_elevation(&self) -> HainesElevation {
        let e = self.hght[self.sfc];
        if e < 305. {
            HainesElevation::Low
        } else if (305. ..=914.).contains(&e) {
            HainesElevation::Mid
        } else {
            HainesElevation::High
        }
    }

    /// Haines index for the given elevation variant (2-6), NaN if missing.
    pub(crate) fn haines_raw(&self, which: HainesElevation) -> f64 {
        // (bottom level, top level, moisture level, lapse thresholds, dd thresholds)
        let (pb, pt, pm, l1, l2, d1, d2) = match which {
            HainesElevation::Low => (950., 850., 850., 4., 7., 6., 9.),
            HainesElevation::Mid => (850., 700., 850., 6., 10., 6., 12.),
            HainesElevation::High => (700., 500., 700., 18., 21., 15., 20.),
        };
        let tp1 = self.temp(pb);
        let tp2 = self.temp(pt);
        let td = self.dwpt(pm);
        if tp1.is_nan() || tp2.is_nan() || td.is_nan() {
            return f64::NAN;
        }
        let lapse = tp1 - tp2;
        let tm = if which == HainesElevation::Low { tp2 } else { tp1 };
        let dd = tm - td;
        let a = if lapse < l1 {
            1.
        } else if lapse <= l2 {
            2.
        } else {
            3.
        };
        let b = if dd < d1 {
            1.
        } else if dd <= d2 {
            2.
        } else {
            3.
        };
        a + b
    }
}
