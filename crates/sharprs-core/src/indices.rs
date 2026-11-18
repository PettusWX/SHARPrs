//! Stability indices and composite parameters (SHARPpy formulations).
//!
//! The pure formula functions are public so callers can evaluate them for
//! arbitrary inputs; the profile-dependent ones are computed by
//! [`crate::ConvectiveAnalysis`].

use crate::layer::Scheme;
use crate::numerics::{arange, masked_argmax, masked_argmin, pow, sq};
use crate::parcel::RawParcel;
use crate::profile::{Field, Profile};
use crate::thermo;
use crate::units::{comp2vec, kts_to_ms, mag};

/// Significant Tornado Parameter, effective-layer version with CIN.
/// Inputs: MLCAPE (J/kg), effective SRH (m²/s²), effective bulk wind
/// difference (m/s), ML LCL (m AGL), MLCIN (J/kg).
pub fn stp_cin(mlcape: f64, esrh: f64, ebwd: f64, mllcl: f64, mlcinh: f64) -> f64 {
    let cape_term = mlcape / 1500.;
    let eshr_term = esrh / 150.;
    let ebwd_term = if ebwd < 12.5 {
        0.
    } else if ebwd > 30. {
        1.5
    } else {
        ebwd / 20.
    };
    let lcl_term = if mllcl < 1000. {
        1.0
    } else if mllcl > 2000. {
        0.0
    } else {
        (2000. - mllcl) / 1000.
    };
    let cinh_term = if mlcinh > -50. {
        1.0
    } else if mlcinh < -200. {
        0.
    } else {
        (mlcinh + 200.) / 150.
    };
    let v = cape_term * eshr_term * ebwd_term * lcl_term * cinh_term;
    // np.maximum propagates NaN
    if v.is_nan() { v } else { v.max(0.) }
}

/// Significant Tornado Parameter, fixed layer. SBCAPE (J/kg), SB LCL (m),
/// 0-1 km SRH, 0-6 km bulk shear (m/s).
pub fn stp_fixed(sbcape: f64, sblcl: f64, srh01: f64, bwd6: f64) -> f64 {
    let lcl_term = if sblcl < 1000. {
        1.0
    } else if sblcl > 2000. {
        0.0
    } else {
        (2000. - sblcl) / 1000.
    };
    let bwd6 = if bwd6 > 30. {
        30.
    } else if bwd6 < 12.5 {
        0.0
    } else {
        bwd6
    };
    let bwd6_term = bwd6 / 20.;
    let cape_term = sbcape / 1500.;
    let srh_term = srh01 / 150.;
    cape_term * lcl_term * srh_term * bwd6_term
}

/// Supercell Composite Parameter. MUCAPE, effective SRH, EBWD (m/s).
pub fn scp(mucape: f64, srh: f64, ebwd: f64) -> f64 {
    let ebwd = if ebwd > 20. {
        20.
    } else if ebwd < 10. {
        0.
    } else {
        ebwd
    };
    (mucape / 1000.) * (srh / 50.) * (ebwd / 20.)
}

/// Energy-Helicity Index from CAPE and SRH.
pub fn ehi(cape: f64, srh: f64) -> f64 {
    (srh * cape) / 160000.
}

/// NWS heat index (°F) from temperature (°F) and RH (%), SHARPpy version.
pub fn heat_index(temp: f64, rh: f64) -> f64 {
    if temp < 40. {
        return temp;
    }
    let hi = 0.5 * (temp + 61.0 + ((temp - 68.0) * 1.2) + (rh * 0.094));
    let avg = (hi + temp) / 2.;
    if avg < 80. {
        return hi;
    }
    let mut heat_index = -42.379 + (2.04901523 * temp) + (10.14333127 * rh)
        - (0.22475541 * temp * rh)
        - (6.83783e-3 * pow(temp, 2.))
        - (5.481717e-2 * pow(rh, 2.))
        + (1.22874e-3 * rh * pow(temp, 2.))
        + (8.5282e-4 * temp * pow(rh, 2.))
        - (1.99e-6 * pow(rh, 2.) * pow(temp, 2.));
    if rh < 13. && temp > 80. && temp < 112. {
        let adjustment = ((13. - rh) / 4.) * ((17. - (temp - 95.).abs()) / 17.).sqrt();
        heat_index -= adjustment;
    } else if rh > 85. && temp > 80. && temp < 87. {
        let adjustment = ((rh - 85.) / 10.) * ((87. - temp) / 5.);
        heat_index += adjustment;
    }
    heat_index
}

/// Inferred temperature advection for one layer.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdvectionLayer {
    /// Bottom / top pressure of the layer (hPa).
    pub pbot: f64,
    pub ptop: f64,
    /// Inferred temperature advection (°C/hr); `None` if unavailable.
    pub advection: Option<f64>,
}

/// Storm-slinky trajectory and updraft tilt.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Slinky {
    /// (x, y, z) positions (m; z is AGL) every 25 s from the LFC.
    pub trajectory: Vec<[f64; 3]>,
    /// Updraft tilt (deg from horizontal).
    pub tilt: Option<f64>,
}

/// `np.where(masked_array <= thr)` semantics: masked entries are selected
/// when their underlying (missing-flag) data satisfies the test, which for
/// SHARPpy's negative missing flags means always for `<=`.
#[inline]
fn le_or_missing(x: f64, thr: f64) -> bool {
    x.is_nan() || x <= thr
}

impl Profile {
    pub(crate) fn k_index_raw(&self) -> f64 {
        let t8 = self.temp(850.);
        let t7 = self.temp(700.);
        let t5 = self.temp(500.);
        let td7 = self.dwpt(700.);
        let td8 = self.dwpt(850.);
        t8 - t5 + td8 - (t7 - td7)
    }
    pub(crate) fn c_totals_raw(&self) -> f64 {
        self.dwpt(850.) - self.temp(500.)
    }
    pub(crate) fn v_totals_raw(&self) -> f64 {
        self.temp(850.) - self.temp(500.)
    }
    pub(crate) fn t_totals_raw(&self) -> f64 {
        self.c_totals_raw() + self.v_totals_raw()
    }

    /// Theta-e index: max - min theta-e in the lowest 400 hPa.
    pub(crate) fn tei_raw(&self) -> f64 {
        let top = self.psfc() - 400.;
        let vals: Vec<f64> = (0..self.len())
            .filter(|&i| self.pres[i] >= top)
            .map(|i| self.thetae[i])
            .filter(|x| !x.is_nan())
            .collect();
        if vals.is_empty() {
            return f64::NAN;
        }
        let mx = vals.iter().copied().fold(f64::MIN, f64::max);
        let mn = vals.iter().copied().fold(f64::MAX, f64::min);
        mx - mn
    }

    /// SHARPpy `thetae_diff`: max - min theta-e in the lowest 3 km, zero if
    /// the maximum is above the minimum.
    pub(crate) fn thetae_diff_raw(&self) -> f64 {
        // SHARPpy quirk: `np.where` on a masked comparison also selects levels
        // whose height is *missing* (their raw data is the missing flag,
        // which is <= 3000), so those levels take part too.
        let idx: Vec<usize> = (0..self.len()).filter(|&i| le_or_missing(self.agl(self.hght[i]), 3000.)).collect();
        let te: Vec<f64> = idx.iter().map(|&i| self.thetae[i]).collect();
        let (Some(a), Some(b)) = (masked_argmax(&te), masked_argmin(&te)) else {
            return f64::NAN;
        };
        let maxe_pres = self.pres[idx[a]];
        let mine_pres = self.pres[idx[b]];
        if maxe_pres < mine_pres { 0. } else { te[a] - te[b] }
    }

    /// Inferred temperature advection in 100 hPa layers (thermal wind).
    pub(crate) fn inferred_temp_adv_raw(&self, lat: Option<f64>) -> Vec<AdvectionLayer> {
        if !self.has_wind {
            return vec![];
        }
        let omega = (2. * std::f64::consts::PI) / 86164.;
        let Some(last100) = (0..self.len()).rev().find(|&i| self.pres[i] >= 100.) else {
            return vec![];
        };
        let pressures = arange(self.psfc(), self.pres[last100], -100.);
        if pressures.len() < 2 {
            return vec![];
        }
        let temps: Vec<f64> = self.many(Field::Temperature, &pressures).into_iter().map(thermo::ctok).collect();
        let heights = self.many(Field::Height, &pressures);
        let (us, vs) = self.components_many(&pressures);
        let dirs: Vec<f64> = us.iter().zip(&vs).map(|(&u, &v)| comp2vec(u, v).0).collect();
        let bounds = |i: usize| (pressures[i - 1], pressures[i]);
        let Some(lat) = lat.filter(|l| !l.is_nan()) else {
            return (1..pressures.len())
                .map(|i| AdvectionLayer { pbot: bounds(i).0, ptop: bounds(i).1, advection: None })
                .collect();
        };
        let f = 2. * omega * lat.to_radians().sin();
        let multiplier = (f / 9.81) * (std::f64::consts::PI / 180.);
        (1..pressures.len())
            .map(|i| {
                let (bottom_pres, top_pres) = bounds(i);
                let btemp = temps[i - 1];
                let ttemp = temps[i];
                let bhght = heights[i - 1];
                let thght = heights[i];
                let bottom_wdir = dirs[i - 1];
                let mut top_wdir = dirs[i];
                // SHARPpy quirk: "average" temperature is (Tt + Tb) * 2
                let avg_temp = (ttemp + btemp) * 2.;
                let (mu, mv) = self.mean_wind_raw(bottom_pres, top_pres, 0., 0.);
                let mean_wspd = kts_to_ms(comp2vec(mu, mv).1);
                let modd = 180. - bottom_wdir;
                top_wdir += modd;
                if top_wdir < 0. {
                    top_wdir += 360.;
                } else if top_wdir >= 360. {
                    top_wdir -= 360.;
                }
                let d_theta = top_wdir - 180.;
                let t_adv = multiplier * sq(mean_wspd) * avg_temp * (d_theta / (thght - bhght));
                let adv = t_adv * 60. * 60.;
                AdvectionLayer { pbot: bottom_pres, ptop: top_pres, advection: crate::profile::opt(adv) }
            })
            .collect()
    }

    /// Storm-slinky: integrate a parcel from its LFC with 25 s steps.
    pub fn parcel_trajectory(&self, pcl: &crate::Parcel, storm: crate::Vector) -> Option<Slinky> {
        if pcl.cape.is_some_and(|cape| cape < 1e-3) {
            return None;
        }
        let g = 9.8;
        let delta_t = 25.;
        let (mut x_0, mut y_0) = (0., 0.);
        let mut z_0 = pcl.lfc_hght.unwrap_or(f64::NAN);
        let mut p_0 = pcl.lfc_pres.unwrap_or(f64::NAN);
        let mut t_0 = 0.;
        let mut w_0 = 5.;
        let mut pos = vec![[x_0, y_0, z_0]];
        let elhght = pcl.el_hght.unwrap_or(self.hght[self.len() - 1]);
        let (xp, fp): (Vec<_>, Vec<_>) = pcl.trace.iter().rev()
            .filter(|t| !t.pres.is_nan() && !t.vtmp.is_nan())
            .map(|t| (t.pres.log10(), t.vtmp)).unzip();
        let mut guard = 0;
        while z_0 < elhght && guard < 10_000 {
            guard += 1;
            let t_1 = delta_t + t_0;
            let env_tempv = self.vtmp_at(p_0) + 273.15;
            let pcl_tempv = crate::numerics::interp(p_0.log10(), &xp, &fp) + 273.15;
            let accel = g * ((pcl_tempv - env_tempv) / env_tempv);
            let z_1 = (0.5 * accel * pow(t_1 - t_0, 2.)) + (w_0 * (t_1 - t_0)) + z_0;
            let w_1 = accel * (t_1 - t_0) + w_0;
            let (u, v) = self.components(p_0);
            let u_0 = kts_to_ms(u - storm.u);
            let v_0 = kts_to_ms(v - storm.v);
            let x_1 = u_0 * (t_1 - t_0) + x_0;
            let y_1 = v_0 * (t_1 - t_0) + y_0;
            pos.push([x_1, y_1, z_1]);
            z_0 = z_1;
            y_0 = y_1;
            x_0 = x_1;
            t_0 = t_1;
            p_0 = self.pres(self.msl(z_1));
            w_0 = w_1;
        }
        let last = pos[pos.len() - 1];
        let r = (sq(last[0]) + sq(last[1])).sqrt();
        let theta = last[2].atan2(r).to_degrees();
        Some(Slinky { trajectory: pos, tilt: crate::profile::opt(theta) })
    }

    /// Effective-layer-free SHERB (0-3 km shear version).
    pub(crate) fn sherb3_raw(&self) -> f64 {
        let lr03 = self.lapse_rate_raw(0., 3000., false);
        let lr75 = self.lapse_rate_raw(700., 500., true);
        let p3km = self.p_agl(3000.);
        let (su, sv) = self.wind_shear_raw(self.psfc(), p3km);
        let shear = kts_to_ms(mag(su, sv));
        (shear / 26.) * (lr03 / 5.2) * (lr75 / 5.6)
    }

    /// Effective SHERB from the effective bulk wind difference (kt).
    pub(crate) fn sherbe_raw(&self, ebwd: (f64, f64)) -> f64 {
        let lr03 = self.lapse_rate_raw(0., 3000., false);
        let lr75 = self.lapse_rate_raw(700., 500., true);
        let shear = kts_to_ms(mag(ebwd.0, ebwd.1));
        (shear / 27.) * (lr03 / 5.2) * (lr75 / 5.6)
    }

    /// Significant Hail Parameter.
    pub(crate) fn ship_raw(&self, mu: &RawParcel, sfc_6km_shear: (f64, f64)) -> f64 {
        let mucape = mu.bplus;
        let mut mumr = thermo::mixratio(mu.pres, mu.dwpc);
        let frz_lvl = self.hght(self.temp_lvl_raw(0., false));
        let mut h5_temp = self.temp(500.);
        let lr75 = self.lapse_rate_raw(700., 500., true);
        let mut shr06 = kts_to_ms(mag(sfc_6km_shear.0, sfc_6km_shear.1));
        if shr06 > 27. {
            shr06 = 27.;
        } else if shr06 < 7. {
            shr06 = 7.;
        }
        if mumr > 13.6 {
            mumr = 13.6;
        } else if mumr < 11. {
            mumr = 11.;
        }
        if h5_temp > -5.5 {
            h5_temp = -5.5;
        }
        let mut ship = -1. * (mucape * mumr * lr75 * h5_temp * shr06) / 42000000.;
        if mucape < 1300. {
            ship *= mucape / 1300.;
        }
        if lr75 < 5.8 {
            ship *= lr75 / 5.8;
        }
        if frz_lvl < 2400. {
            ship *= frz_lvl / 2400.;
        }
        ship
    }

    /// Microburst Composite Index (integer score, 0 floor).
    pub(crate) fn mburst_raw(&self, sb: &RawParcel, lr03: f64, dcape: f64, pwat: f64) -> f64 {
        let vt = self.v_totals_raw();
        let tei_val = self.thetae_diff_raw();
        let sfc_thetae = thermo::thetae(sb.pres, sb.tmpc, sb.dwpc);
        let te = if thermo::ctok(sfc_thetae) >= 355. { 1. } else { 0. };
        let sbcape_term = if sb.bplus.is_nan() {
            f64::NAN
        } else if sb.bplus >= 4300. {
            4.
        } else if sb.bplus >= 3700. {
            2.
        } else if sb.bplus >= 3300. {
            1.
        } else if sb.bplus >= 2000. {
            0.
        } else {
            -5.
        };
        let sbli_term = if sb.li5.is_nan() {
            f64::NAN
        } else if sb.li5 <= -10.0 {
            3.
        } else if sb.li5 <= -9.0 {
            2.
        } else if sb.li5 <= -7.5 {
            1.
        } else {
            0.
        };
        let pwat_term = if pwat.is_nan() {
            f64::NAN
        } else if pwat < 1.5 {
            -3.
        } else {
            0.
        };
        let dcape_term = if dcape.is_nan() {
            f64::NAN
        } else if pwat > 1.70 && dcape > 900. {
            1.
        } else {
            0.
        };
        let lr03_term = if lr03.is_nan() {
            f64::NAN
        } else if lr03 <= 8.4 {
            0.
        } else {
            1.
        };
        let vt_term = if vt.is_nan() {
            f64::NAN
        } else if vt < 27. {
            0.
        } else if vt < 28. {
            1.
        } else if vt < 29. {
            2.
        } else {
            3.
        };
        let ted = if tei_val.is_nan() {
            f64::NAN
        } else if tei_val >= 35. {
            1.
        } else {
            0.
        };
        let mut m = te + sbcape_term + sbli_term + pwat_term + dcape_term + lr03_term + vt_term + ted;
        if m < 0. {
            m = 0.;
        }
        m
    }

    /// Modified SHERB-free MCS Maintenance Probability.
    pub(crate) fn mmp_raw(&self, mucape: f64) -> f64 {
        if mucape < 100. {
            return 0.;
        }
        let agl: Vec<f64> = self.hght.iter().map(|&h| self.agl(h)).collect();
        // missing heights are selected too (see `le_or_missing`); they only
        // shift the b/t index pairing below, exactly as in SHARPpy
        let lowest: Vec<usize> = (0..self.len()).filter(|&i| le_or_missing(agl[i], 1000.)).collect();
        let highest: Vec<usize> = (0..self.len()).filter(|&i| agl[i] >= 6000. && agl[i] < 10000.).collect();
        if lowest.is_empty() || highest.is_empty() {
            return f64::NAN;
        }
        let pbots = self.pres_many(&lowest.iter().map(|&i| self.hght[i]).collect::<Vec<_>>());
        let ptops = self.pres_many(&highest.iter().map(|&i| self.hght[i]).collect::<Vec<_>>());
        // SHARPpy fills an *uninitialised* array and only computes cells with
        // b >= t; we take the max over the computed cells only.
        let mut max_shear = f64::NAN;
        let upper_winds: Vec<_> = ptops.iter().map(|&p| self.components(p)).collect();
        for (b, &pb) in pbots.iter().enumerate() {
            let (ub, vb) = self.components(pb);
            for &(ut, vt) in upper_winds.iter().take(b + 1) {
                let m = mag(ut - ub, vt - vb);
                if !m.is_nan() && (max_shear.is_nan() || m > max_shear) {
                    max_shear = m;
                }
            }
        }
        let max_bulk_shear = kts_to_ms(max_shear);
        let lr38 = self.lapse_rate_raw(3000., 8000., false);
        let plower = self.p_agl(3000.);
        let pupper = self.p_agl(12000.);
        let (mu, mv) = self.mean_wind_raw(plower, pupper, 0., 0.);
        let mean_wind_3t12 = kts_to_ms(mag(mu, mv));
        let a_0 = 13.0;
        let a_1 = -4.59 * pow(10., -2.);
        let a_2 = -1.16;
        let a_3 = -6.17 * pow(10., -4.);
        let a_4 = -0.17;
        1. / (1. + (a_0 + (a_1 * max_bulk_shear) + (a_2 * lr38) + (a_3 * mucape) + (a_4 * mean_wind_3t12)).exp())
    }

    /// Wind Damage Parameter.
    pub(crate) fn wndg_raw(&self, ml: &RawParcel) -> f64 {
        let mlcape = ml.bplus;
        let mut lr03 = self.lapse_rate_raw(0., 3000., false);
        let bot = self.p_agl(1000.);
        let top = self.p_agl(3500.);
        let (mu, mv) = self.mean_wind_raw(bot, top, 0., 0.);
        let mean_wind = kts_to_ms(mag(mu, mv));
        let mut mlcin = ml.bminus;
        if lr03 < 7. {
            lr03 = 0.;
        }
        if mlcin < -50. {
            mlcin = -50.;
        }
        (mlcape / 2000.) * (lr03 / 9.) * (mean_wind / 15.) * ((50. + mlcin) / 40.)
    }

    /// Large Hail Parameter (Johnson & Sugden 2014).
    pub(crate) fn lhp_raw(
        &self,
        mu: &RawParcel,
        sfc_6km_shear: (f64, f64),
        lr75: f64,
        storm_u: f64,
        storm_v: f64,
    ) -> f64 {
        let mag06_shr = kts_to_ms(mag(sfc_6km_shear.0, sfc_6km_shear.1));
        if !(mu.bplus >= 400. && mag06_shr >= 14.) {
            return 0.;
        }
        let (hp_bot, hp_top) = self.temp_layer(-10., -30.);
        let zbot = self.hght(hp_bot);
        let ztop = self.hght(hp_top);
        let thk_hgz = ztop - zbot;
        let mut term_a = ((mu.bplus - 2000.) / 1000.) + ((3200. - thk_hgz) / 500.) + ((lr75 - 6.5) / 2.);
        if term_a < 0. {
            term_a = 0.;
        }
        let p_1km = self.p_agl(1000.);
        let p_3km = self.p_agl(3000.);
        let p_6km = self.p_agl(6000.);
        let (su, sv) = self.wind_shear_raw(self.psfc(), mu.elpres);
        let shear_el = kts_to_ms(mag(su, sv));
        let (eu, ev) = self.components(mu.elpres);
        let grw_el_dir = comp2vec(eu, ev).0;
        let (mu36, mv36) = self.mean_wind_raw(p_3km, p_6km, 0., 0.);
        let grw_36_dir = comp2vec(mu36, mv36).0;
        let mut grw_alpha_el = grw_el_dir - grw_36_dir;
        if grw_alpha_el > 180. {
            grw_alpha_el = -10.;
        }
        let (a, b) = self.mean_wind_raw(self.psfc(), p_1km, storm_u, storm_v);
        let srw_01_dir = comp2vec(a, b).0;
        let (a, b) = self.mean_wind_raw(p_3km, p_6km, storm_u, storm_v);
        let srw_36_dir = comp2vec(a, b).0;
        let srw_alpha_mid = srw_36_dir - srw_01_dir;
        let mut term_b = ((shear_el - 25.) / 5.) + ((grw_alpha_el + 5.) / 20.) + ((srw_alpha_mid - 80.) / 10.);
        if term_b < 0. {
            term_b = 0.;
        }
        term_a * term_b + 5.
    }

    /// SWEAT index.
    pub(crate) fn sweat_raw(&self) -> f64 {
        let td850 = self.dwpt(850.);
        let (u8, v8) = self.components(850.);
        let (u5, v5) = self.components(500.);
        let (d8, s8) = comp2vec(u8, v8);
        let (d5, s5) = comp2vec(u5, v5);
        let tt = self.t_totals_raw();
        let term1 = if td850 > 0. { 12. * td850 } else { 0. };
        let term2 = if tt < 49. { 0. } else { 20. * (tt - 49.) };
        let term3 = 2. * s8;
        let term4 = s5;
        let term5 = if 130. <= d8 && 250. >= d8 && 210. <= d5 && 310. >= d5 && d5 - d8 > 0. && s8 >= 15. && s5 >= 15. {
            125. * ((d5 - d8).to_radians().sin() + 0.2)
        } else {
            0.
        };
        term1 + term2 + term3 + term4 + term5
    }

    /// Precipitation efficiency proxy: PW * mean 1000-700 hPa RH.
    pub(crate) fn precip_eff_raw(&self, pw: f64) -> f64 {
        pw * (self.mean_relh_raw(Some(1000.), Some(700.)) / 100.)
    }

    /// Mean mixing ratio over the lowest 100 hPa (thermo panel).
    pub(crate) fn mean_mixr_sfc100(&self) -> f64 {
        self.mean_mixratio_raw(None, None, Scheme::Sampled)
    }
}
