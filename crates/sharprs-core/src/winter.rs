//! Winter-weather analysis: dendritic growth zone statistics, initial
//! precipitation phase, warm/cold layer energies and SHARPpy's best-guess
//! precipitation type.

use crate::profile::Profile;
use crate::thermo;
use crate::units::kts_to_mph;

/// Initial precipitation phase (SHARPpy `init_phase`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Phase {
    Rain,
    FreezingRain,
    ZrSnowMix,
    Snow,
}

impl Phase {
    /// SHARPpy's label.
    pub fn label(self) -> &'static str {
        match self {
            Phase::Rain => "Rain",
            Phase::FreezingRain => "Freezing Rain",
            Phase::ZrSnowMix => "ZR/S Mix",
            Phase::Snow => "Snow",
        }
    }
    /// SHARPpy's numeric phase code (0 rain, 1 freezing/mixed, 3 snow).
    pub fn code(self) -> i32 {
        match self {
            Phase::Rain => 0,
            Phase::FreezingRain | Phase::ZrSnowMix => 1,
            Phase::Snow => 3,
        }
    }
}

/// Initial phase: the level near the top of the highest saturated layer
/// below 5 km.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InitialPhase {
    /// hPa
    pub pres: f64,
    /// Temperature at that level, °C
    pub tmpc: f64,
    pub phase: Phase,
}

/// SHARPpy's best-guess precipitation type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PrecipType {
    /// No precipitating layer found.
    NoPrecip,
    Rain,
    Snow,
    SleetAndSnow,
    Sleet,
    FreezingRainDrizzle,
    FreezingRain,
    Unknown,
}

impl PrecipType {
    /// SHARPpy's label (including its trailing period).
    pub fn label(self) -> &'static str {
        match self {
            PrecipType::NoPrecip => "None.",
            PrecipType::Rain => "Rain.",
            PrecipType::Snow => "Snow.",
            PrecipType::SleetAndSnow => "Sleet and Snow.",
            PrecipType::Sleet => "Sleet.",
            PrecipType::FreezingRainDrizzle => "Freezing Rain/Drizzle.",
            PrecipType::FreezingRain => "Freezing Rain.",
            PrecipType::Unknown => "Unknown.",
        }
    }
}

/// Positive / negative energy of the warm layer aloft (J/kg) and its
/// bounds (hPa). Zero when no warm layer over a cold layer exists.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PosNeg {
    pub pos: f64,
    pub neg: f64,
    pub top: f64,
    pub bot: f64,
}

impl Profile {
    /// SHARPpy `init_phase`: returns (plevel, phase code, tmp, label) with
    /// NaN/-1 when no precipitating layer is found.
    pub(crate) fn init_phase_raw(&self) -> Option<InitialPhase> {
        let n = self.len();
        let avail = self.omeg.iter().filter(|&&o| o < 0.1).count();
        let agl: Vec<f64> = self.hght.iter().map(|&h| self.agl(h)).collect();
        let below: Vec<usize> = (0..n)
            .filter(|&i| agl[i] < 5000. && agl[i] >= 0. && (avail < 5 || self.omeg[i] <= 0.))
            .collect();
        let mut new_pres = Vec::new();
        for &i in &below {
            let rh = thermo::relh(self.pres[i], self.tmpc[i], self.dwpc[i]);
            if rh > 80. {
                new_pres.push(self.pres[i] + 50.);
            }
        }
        let mut top_most = None;
        for (k, &p) in new_pres.iter().enumerate() {
            let rh = thermo::relh(p, self.temp(p), self.dwpt(p));
            if rh > 80. {
                top_most = Some(k);
            }
        }
        let k = top_most?;
        let plevel = new_pres[k] - 25.;
        let tmp = self.temp(plevel);
        let phase = if tmp > 0. {
            Phase::Rain
        } else if tmp <= 0. && tmp > -5. {
            Phase::FreezingRain
        } else if tmp <= -5. && tmp > -9. {
            Phase::ZrSnowMix
        } else if tmp <= -9. {
            Phase::Snow
        } else {
            return None;
        };
        Some(InitialPhase { pres: plevel, tmpc: tmp, phase })
    }

    /// Warm-layer energy from `start` (hPa) down to the surface, using the
    /// temperature (`wetbulb = false`) or wet-bulb profile.
    pub(crate) fn posneg_raw(&self, start: f64, wetbulb: bool) -> Option<PosNeg> {
        if self.temp(500.).is_nan() && self.temp(850.).is_nan() {
            return None;
        }
        let lptr = self.sfc;
        let upper = start;
        let uptr = (0..self.len()).rev().find(|&i| self.pres[i] > upper).unwrap_or(0);
        let tfun = |p: f64| {
            if wetbulb {
                thermo::wetbulb(p, self.temp(p), self.dwpt(p))
            } else {
                self.temp(p)
            }
        };
        let mut h1 = self.hght(upper);
        let mut te1 = tfun(upper);
        let (mut warm, mut cold) = (false, false);
        let (mut totp, mut totn) = (0., 0.);
        let (mut ptop, mut pbot) = (0., 0.);
        let mut i = uptr as isize;
        while i >= lptr as isize {
            let iu = i as usize;
            let pe2 = self.pres[iu];
            let h2 = self.hght[iu];
            let te2 = tfun(pe2);
            let tdef1 = (0. - te1) / thermo::ctok(te1);
            let tdef2 = (0. - te2) / thermo::ctok(te2);
            let lyre = 9.8 * (tdef1 + tdef2) / 2.0 * (h2 - h1);
            if te2 > 0. && !warm {
                warm = true;
                ptop = pe2;
            }
            if te2 < 0. && warm && !cold {
                cold = true;
                pbot = pe2;
            }
            if warm {
                if lyre > 0. {
                    totp += lyre;
                } else {
                    totn += lyre;
                }
            }
            h1 = h2;
            te1 = te2;
            i -= 1;
        }
        Some(if warm && cold {
            PosNeg { pos: totp, neg: totn, top: ptop, bot: pbot }
        } else {
            PosNeg { pos: 0., neg: 0., top: 0., bot: 0. }
        })
    }

    /// SHARPpy `best_guess_precip`.
    pub(crate) fn best_guess_precip_raw(&self, init: Option<&InitialPhase>, tpos: f64, tneg: f64) -> PrecipType {
        let sfc_t = self.tmpc[self.sfc];
        let (phase, lvl, itmp) = match init {
            None => return PrecipType::NoPrecip,
            Some(ip) => (ip.phase.code(), ip.pres, ip.tmpc),
        };
        if phase == 0 && tneg >= 0. && sfc_t > 0. {
            PrecipType::Rain
        } else if phase == 3 && tpos <= 0. && sfc_t <= 0. {
            PrecipType::Snow
        } else if phase == 1 && tpos <= 0. && sfc_t > 0. {
            PrecipType::Rain
        } else if phase == 1 && tpos <= 0. && sfc_t <= 0. {
            if self.agl(self.hght(lvl)) >= 3000. {
                if itmp <= -4. { PrecipType::SleetAndSnow } else { PrecipType::Sleet }
            } else {
                PrecipType::FreezingRainDrizzle
            }
        } else if phase == 3 && tpos <= 0. && sfc_t > 0. {
            if sfc_t > 4. { PrecipType::Rain } else { PrecipType::Snow }
        } else if tpos > 0. {
            let x1 = tpos;
            let y1 = -tneg;
            let y2 = (0.62 * x1) + 60.0;
            if y1 > y2 {
                PrecipType::Sleet
            } else if sfc_t <= 0. {
                PrecipType::FreezingRain
            } else {
                PrecipType::Rain
            }
        } else {
            PrecipType::Unknown
        }
    }

    /// Wind chill (°F) at the surface (SHARPpy formula).
    pub(crate) fn wind_chill_raw(&self) -> f64 {
        let t = thermo::ctof(self.tmpc[self.sfc]);
        let w = kts_to_mph(self.wspd[self.sfc]);
        35.74 + (0.6215 * t) - (35.75 * crate::numerics::pow(w, 0.16))
            + 0.4275 * (t) * crate::numerics::pow(w, 0.16)
    }
}
