//! Downdraft CAPE (DCAPE) and the downdraft parcel trace.

use crate::profile::Profile;
use crate::thermo;

/// DCAPE result.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Downdraft {
    /// Downdraft CAPE (J/kg; SHARPpy's sign convention, normally positive).
    pub dcape: Option<f64>,
    /// Downdraft parcel trace from the source level (min 100 hPa mean
    /// theta-e, centred) down to the lowest level: (pres hPa, temp °C).
    pub trace: Vec<(f64, f64)>,
    /// Surface temperature of the downdraft parcel ("DownT"), °F.
    pub downt_f: Option<f64>,
}

impl Profile {
    /// (dcape, ttrace, ptrace) with SHARPpy semantics.
    pub(crate) fn dcape_raw(&self) -> (f64, Vec<f64>, Vec<f64>) {
        let sfc_pres = self.psfc();
        let keep: Vec<usize> = (0..self.len()).filter(|&i| !self.thetae[i].is_nan() && !self.pres[i].is_nan()).collect();
        let pres: Vec<f64> = keep.iter().map(|&i| self.pres[i]).collect();
        let hght: Vec<f64> = keep.iter().map(|&i| self.hght[i]).collect();
        let tmpc: Vec<f64> = keep.iter().map(|&i| self.tmpc[i]).collect();

        // source level: centre of the lowest-mean-theta-e 100 hPa layer
        // within 400 hPa of the surface
        let mut mine = 1000.0;
        let mut minp = -999.0;
        for &p in pres.iter().filter(|&&p| p >= sfc_pres - 400.) {
            let m = self.mean_thetae_raw(Some(p), Some(p - 100.));
            if !m.is_nan() && m < mine {
                minp = p - 50.;
                mine = m;
            }
        }
        let upper = minp;
        let Some(uptr) = pres.iter().rposition(|&p| p >= upper) else {
            return (f64::NAN, vec![], vec![]);
        };
        let mut tp1 = thermo::wetbulb(upper, self.temp(upper), self.dwpt(upper));
        let mut pe1 = upper;
        let mut te1 = self.temp(pe1);
        let mut h1 = self.hght(pe1);
        let mut tote = 0.;
        let mut ttrace = vec![tp1];
        let mut ptrace = vec![upper];
        for i in (0..=uptr).rev() {
            let pe2 = pres[i];
            let te2 = tmpc[i];
            let h2 = hght[i];
            let tp2 = thermo::wetlift(pe1, tp1, pe2);
            if !te1.is_nan() && !te2.is_nan() {
                let tdef1 = (tp1 - te1) / thermo::ctok(te1);
                let tdef2 = (tp2 - te2) / thermo::ctok(te2);
                let lyre = 9.8 * (tdef1 + tdef2) / 2.0 * (h2 - h1);
                tote += lyre;
            }
            ttrace.push(tp2);
            ptrace.push(pe2);
            pe1 = pe2;
            te1 = te2;
            h1 = h2;
            tp1 = tp2;
        }
        (tote, ttrace, ptrace)
    }
}
