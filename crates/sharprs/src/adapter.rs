//! Core analysis → render view model.

use sharprs_core::{self as core, ConvectiveAnalysis, Field, Parcel, ParcelKind, Profile, Vector};
use sharprs_render::{self as render, RL};

use crate::title::{TitleStyle, sounding_title};

/// Choices that affect what the view shows (not how it is styled).
#[derive(Clone, Debug)]
pub struct ViewOptions {
    /// Header title; `None` builds one from the metadata with `title_style`.
    pub title: Option<String>,
    pub title_style: TitleStyle,
    /// The four thermo-table parcels, in display order (SHARPpy default:
    /// SFC, ML, FCST, MU).
    pub table_parcels: [ParcelKind; 4],
    /// Parcel lifted on the skew-T (SHARPpy default: MU).
    pub skewt_parcel: ParcelKind,
    /// Rotational velocity for the VROT inset (kt).
    pub vrot: f64,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            title: None,
            title_style: TitleStyle::Pivotal,
            table_parcels: [ParcelKind::Surface, ParcelKind::MixedLayer, ParcelKind::Forecast, ParcelKind::MostUnstable],
            skewt_parcel: ParcelKind::MostUnstable,
            vrot: 0.0,
        }
    }
}

fn nan(v: Option<f64>) -> f64 {
    v.unwrap_or(f64::NAN)
}
fn col(prof: &Profile, f: Field) -> Vec<f64> {
    prof.column(f).into_iter().map(nan).collect()
}
fn uv(v: &Option<Vector>) -> Option<(f64, f64)> {
    v.as_ref().map(|v| (v.u, v.v))
}
fn ds(v: &Option<Vector>) -> Option<(f64, f64)> {
    v.as_ref().map(|v| (v.direction(), v.speed()))
}
fn m_to_ft(m: f64) -> f64 {
    core::units::m_to_ft(m)
}

/// SARS analog ids are `yymmddHH.STN`; SHARPpy's inset shows them as
/// `strftime("%d %b %y %HZ")` followed by the station in parentheses,
/// e.g. `03050807.ADM` -> `08 May 03 07Z (ADM)`. Ids that don't parse are
/// shown verbatim.
fn sars_match_label(id: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let parsed = id.split_once('.').and_then(|(date, stn)| {
        if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let month: usize = date[2..4].parse().ok()?;
        let name = MONTHS.get(month.checked_sub(1)?)?;
        Some(format!("{} {} {} {}Z ({})", &date[4..6], name, &date[0..2], &date[6..8], stn))
    });
    parsed.unwrap_or_else(|| id.to_string())
}

fn parcel_label(kind: ParcelKind) -> &'static str {
    match kind {
        ParcelKind::Surface => "SFC",
        ParcelKind::Forecast => "FCST",
        ParcelKind::MixedLayer => "ML",
        ParcelKind::MostUnstable => "MU",
        ParcelKind::Effective => "EFF",
        ParcelKind::User => "USER",
    }
}

fn parcel_view(p: &Parcel) -> render::ParcelView {
    render::ParcelView {
        name: parcel_label(p.kind).to_string(),
        bplus: p.cape,
        bminus: p.cinh,
        lclhght: p.lcl_hght,
        lfchght: p.lfc_hght,
        elhght: p.el_hght,
        li5: p.li5,
        lclpres: p.lcl_pres,
        lfcpres: p.lfc_pres,
        elpres: p.el_pres,
        b3km: p.cape_3km,
        p0c: p.p0c,
        pm20c: p.pm20c,
        pm30c: p.pm30c,
        hght0c: p.hght0c,
        hghtm20c: p.hghtm20c,
        hghtm30c: p.hghtm30c,
        ptrace: p.trace.iter().map(|t| t.pres).collect(),
        ttrace: p.trace.iter().map(|t| t.vtmp).collect(),
    }
}

/// Build the render view model from a computed analysis.
pub fn build_view(
    sounding: &sharprs_io::Sounding,
    prof: &Profile,
    a: &ConvectiveAnalysis,
    vopts: &ViewOptions,
) -> render::SoundingView {
    let meta = prof.meta();
    let title = vopts.title.clone().unwrap_or_else(|| sounding_title(sounding, vopts.title_style));

    // ---- profile arrays -------------------------------------------------
    let profile = render::ProfileView {
        pres: prof.pressures().into_iter().map(nan).collect(),
        hght: col(prof, Field::Height),
        tmpc: col(prof, Field::Temperature),
        dwpc: col(prof, Field::Dewpoint),
        vtmp: col(prof, Field::VirtualTemperature),
        wetbulb: col(prof, Field::WetBulb),
        thetae: col(prof, Field::ThetaE),
        u: col(prof, Field::U),
        v: col(prof, Field::V),
        omeg: if prof.has_omega() { col(prof, Field::Omega) } else { Vec::new() },
        sfc: prof.sfc_index(),
    };

    // ---- parcels ----------------------------------------------------------
    let mut rows: Vec<&Parcel> = vopts.table_parcels.iter().filter_map(|&k| a.parcels.get(k)).collect();
    let selected_parcel = rows.iter().position(|p| p.kind == vopts.skewt_parcel).unwrap_or_else(|| {
        if let Some(p) = a.parcels.get(vopts.skewt_parcel) {
            if rows.len() == 4 { rows.pop(); }
            rows.push(p);
        }
        rows.len().saturating_sub(1)
    });
    let selected = rows.get(selected_parcel).copied();
    let parcels = rows.into_iter().map(parcel_view).collect();

    let ml = &a.parcels.ml;
    let mu = &a.parcels.mu;
    let k = &a.kinematics;
    let srr = &k.storm_relative.right;
    let srl = &k.storm_relative.left;
    let dv = &a.severe.deviant;
    let rl = |r: Option<f64>, l: Option<f64>| RL::new(r, l);

    // ---- indices ------------------------------------------------------------
    let t = &a.thermo;
    let indices = render::ThermoIndices {
        pwat: t.pwat,
        pwv_flag: a.pw_flag.unwrap_or(0) as i32,
        mean_mixr: t.mean_mixr,
        low_rh: t.low_rh,
        mid_rh: t.mid_rh,
        dcape: a.downdraft.dcape,
        drush_f: a.downdraft.downt_f,
        k_idx: t.k_index,
        totals_totals: t.totals_totals,
        conv_t_f: t.conv_t,
        max_t_f: t.max_t,
        esp: a.severe.esp,
        // SHARPpy's params.esp() returns the integer 0 (printed "0" rather
        // than "0.0") when the 0-3 km lapse rate is under 7 C/km or MLCAPE
        // is under 250 J/kg.
        esp_gated: t.lapse_rate_0_3km.is_some_and(|l| l < 7.0) || ml.cape.is_some_and(|c| c < 250.0),
        mmp: a.severe.mmp,
        wndg: a.severe.wndg,
        tei: t.tei,
        cape3km_ml: ml.cape_3km,
        mburst: a.severe.mburst,
        sig_severe: a.severe.sig_severe,
        lapserate_3km: t.lapse_rate_0_3km,
        lapserate_3_6km: t.lapse_rate_3_6km,
        lapserate_850_500: t.lapse_rate_850_500,
        lapserate_700_500: t.lapse_rate_700_500,
    };
    let severe = render::SevereIndices {
        scp: rl(dv.right.scp, dv.left.scp),
        stp_cin: rl(dv.right.stp_cin, dv.left.stp_cin),
        stp_fixed: rl(dv.right.stp_fixed, dv.left.stp_fixed),
        ship: a.severe.ship,
    };

    // ---- kinematics -----------------------------------------------------------
    let total = |h: &Option<core::Helicity>| h.as_ref().map(|h| h.total);
    let kinematics = render::KinematicsView {
        srh1km: rl(total(&srr.srh_1km), total(&srl.srh_1km)),
        srh3km: rl(total(&srr.srh_3km), total(&srl.srh_3km)),
        esrh: rl(total(&srr.esrh), total(&srl.esrh)),
        ehi1km: rl(dv.right.ehi_sfc_1km, dv.left.ehi_sfc_1km),
        ehi3km: rl(dv.right.ehi_sfc_3km, dv.left.ehi_sfc_3km),
        ehi_eff: rl(dv.right.ehi_eff, dv.left.ehi_eff),
        shear_1km: uv(&k.shear_1km),
        shear_3km: uv(&k.shear_3km),
        shear_6km: uv(&k.shear_6km),
        shear_8km: uv(&k.shear_8km),
        shear_lcl_el: uv(&k.shear_lcl_el),
        shear_eff: uv(&k.shear_eff),
        ebwd: uv(&k.ebwd),
        mean_1km: ds(&k.mean_1km),
        mean_3km: ds(&k.mean_3km),
        mean_6km: ds(&k.mean_6km),
        mean_8km: ds(&k.mean_8km),
        mean_lcl_el: ds(&k.mean_lcl_el),
        mean_eff: ds(&k.mean_eff),
        mean_ebw: ds(&k.mean_ebw),
        srw_1km: RL::new(ds(&srr.srw_1km), ds(&srl.srw_1km)),
        srw_3km: RL::new(ds(&srr.srw_3km), ds(&srl.srw_3km)),
        srw_6km: RL::new(ds(&srr.srw_6km), ds(&srl.srw_6km)),
        srw_8km: RL::new(ds(&srr.srw_8km), ds(&srl.srw_8km)),
        srw_lcl_el: RL::new(ds(&srr.srw_lcl_el), ds(&srl.srw_lcl_el)),
        srw_4_5km: RL::new(ds(&srr.srw_4_5km), ds(&srl.srw_4_5km)),
        srw_eff: RL::new(ds(&srr.srw_eff), ds(&srl.srw_eff)),
        srw_ebw: RL::new(ds(&srr.srw_ebw), ds(&srl.srw_ebw)),
        srw_0_2km: RL::new(uv(&srr.srw_0_2km), uv(&srl.srw_0_2km)),
        srw_4_6km: RL::new(uv(&srr.srw_4_6km), uv(&srl.srw_4_6km)),
        srw_9_11km: RL::new(uv(&srr.srw_9_11km), uv(&srl.srw_9_11km)),
        brn_shear: mu.brn_shear,
        wind1km: ds(&k.wind_1km),
        wind6km: ds(&k.wind_6km),
    };

    let storm = render::StormMotionView {
        bunkers: RL::new(
            k.bunkers.as_ref().map(|b| (b.right.u, b.right.v)),
            k.bunkers.as_ref().map(|b| (b.left.u, b.left.v)),
        ),
        storm_motion: RL::new(
            a.storm_motion.as_ref().map(|b| (b.right.u, b.right.v)),
            a.storm_motion.as_ref().map(|b| (b.left.u, b.left.v)),
        ),
        corfidi_up: k.corfidi.as_ref().map(|c| (c.upshear.u, c.upshear.v)),
        corfidi_dn: k.corfidi.as_ref().map(|c| (c.downshear.u, c.downshear.v)),
        mean_lcl_el: ds(&k.mean_lcl_el),
        critical_angle: rl(srr.critical_angle, srl.critical_angle),
    };

    // ---- small panels -----------------------------------------------------------
    let inf_temp_adv = a
        .temp_advection
        .iter()
        .map(|l| render::AdvectionLayer { adv: l.advection, ptop: l.ptop, pbot: l.pbot })
        .collect();

    let slinky = |motion: Option<Vector>| {
        let p = selected?;
        let s = prof.parcel_trajectory(p, motion?)?;
        Some(render::SlinkyView {
            traj: s.trajectory.into_iter().map(|[x, y, z]| (x, y, z)).collect(),
            tilt: s.tilt,
            has_el: p.cape.is_some_and(|c| c > 1e-3) && p.el_hght.is_some(),
        })
    };

    let hazard = |v: &Vec<core::HazardType>| v.first().map(|h| h.label().to_string());

    // ---- insets -------------------------------------------------------------------
    let sup = |m: &core::sars::SupercellMatches| render::SarsCategory {
        matches: m
            .quality
            .iter()
            .map(|q| render::SarsMatch { label: sars_match_label(&q.id), kind: q.category.label().to_string(), hail_size: None })
            .collect(),
        n_loose: Some(m.loose_count as u32),
        prob: Some(m.tor_prob),
    };
    let hail = |m: &core::sars::HailMatches| render::SarsCategory {
        matches: m
            .quality
            .iter()
            .map(|q| render::SarsMatch { label: sars_match_label(&q.id), kind: format!("{:.2}", q.size), hail_size: Some(q.size) })
            .collect(),
        n_loose: Some(m.loose_count as u32),
        prob: Some(m.sig_prob),
    };
    let sars = render::SarsView {
        supercell: RL::new(sup(&a.sars.supercell.right), sup(&a.sars.supercell.left)),
        hail: RL::new(hail(&a.sars.hail.right), hail(&a.sars.hail.left)),
    };

    let stp_inset = render::StpInsetView {
        mlcape: ml.cape,
        mllcl: ml.lcl_hght,
        esrh: rl(total(&srr.esrh), total(&srl.esrh)),
        ebwd: k.ebwd.as_ref().map(|v| v.speed()),
    };

    let w = &a.winter;
    let ft_msl = |p: f64| prof.hght_at_pres(p).map(m_to_ft);
    let (tpos, tneg, ttop, tbot) = w.temp_energy.map(|e| (e.pos, e.neg, e.top, e.bot)).unwrap_or((0., 0., f64::NAN, f64::NAN));
    let (wpos, wneg, wtop, wbot) =
        w.wetbulb_energy.map(|e| (e.pos, e.neg, e.top, e.bot)).unwrap_or((0., 0., f64::NAN, f64::NAN));
    let sfc_t = prof.level(prof.sfc_index()).tmpc;
    let winter = render::WinterView {
        dgz_pbot: opt_f(w.dgz_pbot),
        dgz_ptop: opt_f(w.dgz_ptop),
        dgz_zbot_ft: ft_msl(w.dgz_pbot),
        dgz_ztop_ft: ft_msl(w.dgz_ptop),
        dgz_meanrh: w.dgz_mean_rh,
        dgz_pw: w.dgz_pw,
        dgz_meanq: w.dgz_mean_mixr,
        dgz_meanomeg: w.dgz_mean_omega,
        oprh: w.oprh,
        init_phase: w
            .initial_phase
            .as_ref()
            .map(|ip| (ip.pres, nan(ft_msl(ip.pres)), ip.tmpc, ip.phase.label().to_string())),
        tpos,
        tneg,
        ttop,
        tbot,
        wpos,
        wneg,
        wtop,
        wbot,
        precip_type: w.precip_type.label().to_string(),
        sfc_tmpf: sfc_t.map(core::thermo::ctof),
    };

    let f = &a.fire;
    let sfc_lvl = prof.level(prof.sfc_index());
    let sfc_wind = match (sfc_lvl.u, sfc_lvl.v) {
        (Some(u), Some(v)) => Some(Vector::new(u, v)),
        _ => None,
    };
    let haines = match f.haines_elevation {
        core::HainesElevation::Low => f.haines_low,
        core::HainesElevation::Mid => f.haines_mid,
        core::HainesElevation::High => f.haines_high,
    };
    let fire = render::FireView {
        sfc_wind: ds(&sfc_wind),
        mean_wind_01km: ds(&f.mean_wind_0_1km),
        mean_wind_pbl: ds(&f.mean_wind_pbl),
        pbl_max_wind: f.pbl_max_wind.as_ref().map(|(v, _)| (v.direction(), v.speed())),
        sfc_rh: f.sfc_rh,
        rh01km: f.rh_0_1km,
        pbl_rh: f.pbl_rh,
        pwat: t.pwat,
        bplus_fire: f.mu_cape,
        pbl_h: f.pbl_depth,
        fosberg: f.fosberg,
        haines_elevation: Some(f.haines_elevation),
        haines: haines.map(|h| h.round() as i32),
    };

    render::SoundingView {
        title,
        latitude: meta.latitude.unwrap_or(35.0),
        observed: sounding.meta.source.is_observed(),
        profile,
        parcels,
        selected_parcel,
        dcape_ptrace: a.downdraft.trace.iter().map(|x| x.0).collect(),
        dcape_ttrace: a.downdraft.trace.iter().map(|x| x.1).collect(),
        max_lapse_rate_2_6: t.max_lapse_rate.map(|m| (m.rate, m.pbot, m.ptop)),
        eff_inflow: a.effective_layer.map(|e| (e.pbot, e.ptop)),
        dgz: (w.dgz_pbot.is_finite() && w.dgz_ptop.is_finite()).then_some((w.dgz_pbot, w.dgz_ptop)),
        wbz_pres: t.wbz_pres,
        frz_pres: t.frz_pres,
        ppbl_top: t.pbl_top,
        indices,
        severe,
        kinematics,
        storm,
        inf_temp_adv,
        slinky: RL::new(slinky(a.storm_motion.map(|s| s.right)), slinky(a.storm_motion.map(|s| s.left))),
        watch_type: RL::new(hazard(&a.hazards.right), hazard(&a.hazards.left)),
        sars,
        stp_inset,
        winter: Some(winter),
        fire: Some(fire),
        vrot: vopts.vrot,
    }
}

fn opt_f(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}
