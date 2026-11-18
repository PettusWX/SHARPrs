//! Color schemes and display units, modelled on SHARPpy's preferences.

use crate::canvas::Color;

/// SHARPpy's three built-in color styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ColorScheme {
    /// Black background (SHARPpy default, Pivotal Weather).
    #[default]
    Standard,
    /// White background.
    Inverted,
    /// Protanopia-friendly variant of the standard scheme.
    Protanopia,
}

/// Surface temperature display units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TempUnits {
    #[default]
    Fahrenheit,
    Celsius,
}

/// Wind speed display units (all view-model winds are in knots).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindUnits {
    #[default]
    Knots,
    MetersPerSecond,
}

/// Precipitable water display units (view-model PW is in inches).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PwUnits {
    #[default]
    Inches,
    Centimeters,
}

/// Every color SHARPpy's preference dialog exposes, plus fixed colors that
/// SHARPpy hard-codes in the widgets. Field names follow the SHARPpy
/// preference keys.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,

    pub temp: Color,
    pub dewp: Color,
    pub wetb: Color,
    pub skew_dgz: Color,
    pub skew_itherm: Color,
    pub skew_itherm_hgz: Color,
    pub skew_adiab: Color,
    pub skew_mixr: Color,
    pub skew_hgt: Color,
    pub skew_lcl_mkr: Color,
    pub skew_lfc_mkr: Color,
    pub skew_el_mkr: Color,

    pub hodo_itach: Color,
    pub hodo_crit: Color,
    pub eff_inflow: Color,

    /// Height-band colors: 0-3, 3-6, 6-9, 9-12 km.
    pub hgt_bands: [Color; 4],

    /// PW anomaly colors: -3, -2, -1 sigma, (fg), +1, +2, +3 sigma.
    pub pwat_m: [Color; 3],
    pub pwat_p: [Color; 3],

    /// Alert levels 1..6.
    pub alert: [Color; 6],
    pub alert_lscp: Color,

    pub stp_box: Color,
    pub stp_line: Color,

    pub pcl_sel: Color,
    pub pcl_cin_hi: Color,
    pub pcl_cin_md: Color,
    pub pcl_cin_lo: Color,

    pub spd_itach: Color,

    pub srw_clsc: Color,
    pub srw_trace: Color,
    pub srw_0_2: Color,
    pub srw_4_6: Color,
    pub srw_9_11: Color,

    pub winter_dgz: Color,

    pub watch_pdstor: Color,
    pub watch_tor: Color,
    pub watch_svr: Color,
    pub watch_mrglsvr: Color,
    pub watch_flood: Color,
    pub watch_blizzard: Color,
    pub watch_fire: Color,
    pub watch_heat: Color,
    pub watch_none: Color,

    /// Border color of the text area / inset frames (fixed in SHARPpy).
    pub text_border: Color,
}

const fn c(s: &str) -> Color {
    Color::hex(s)
}

impl Theme {
    pub fn for_scheme(s: ColorScheme) -> Theme {
        match s {
            ColorScheme::Standard => Theme::standard(),
            ColorScheme::Inverted => Theme::inverted(),
            ColorScheme::Protanopia => Theme::protanopia(),
        }
    }

    pub fn standard() -> Theme {
        Theme {
            bg: c("#000000"),
            fg: c("#ffffff"),
            temp: c("#ff0000"),
            dewp: c("#00ff00"),
            wetb: c("#00ffff"),
            skew_dgz: c("#f5d800"),
            skew_itherm: c("#555555"),
            skew_itherm_hgz: c("#0000ff"),
            skew_adiab: c("#333333"),
            skew_mixr: c("#006600"),
            skew_hgt: c("#ff0000"),
            skew_lcl_mkr: c("#00ff00"),
            skew_lfc_mkr: c("#ffff00"),
            skew_el_mkr: c("#ff00ff"),
            hodo_itach: c("#555555"),
            hodo_crit: c("#00ffff"),
            eff_inflow: c("#04dbd8"),
            hgt_bands: [c("#ff0000"), c("#00ff00"), c("#ffff00"), c("#00ffff")],
            pwat_m: [c("#ff7f00"), c("#ee9a00"), c("#ffdab9")],
            pwat_p: [c("#98fb98"), c("#66cd00"), c("#00ff00")],
            alert: [c("#775000"), c("#996600"), c("#ffffff"), c("#ffff00"), c("#ff0000"), c("#e700df")],
            alert_lscp: c("#00ffff"),
            stp_box: c("#00ff00"),
            stp_line: c("#0080ff"),
            pcl_sel: c("#00ffff"),
            pcl_cin_hi: c("#993333"),
            pcl_cin_md: c("#996600"),
            pcl_cin_lo: c("#00ff00"),
            spd_itach: c("#9d5736"),
            srw_clsc: c("#b1019a"),
            srw_trace: c("#ff0000"),
            srw_0_2: c("#8b0000"),
            srw_4_6: c("#6495ed"),
            srw_9_11: c("#9400d3"),
            winter_dgz: c("#ffff00"),
            watch_pdstor: c("#ff00ff"),
            watch_tor: c("#ff0000"),
            watch_svr: c("#ffff00"),
            watch_mrglsvr: c("#0099cc"),
            watch_flood: c("#5ffb17"),
            watch_blizzard: c("#3366ff"),
            watch_fire: c("#ff9900"),
            watch_heat: c("#c85a17"),
            watch_none: c("#ffcc33"),
            text_border: c("#3399cc"),
        }
    }

    pub fn inverted() -> Theme {
        Theme {
            bg: c("#ffffff"),
            fg: c("#000000"),
            temp: c("#dd0000"),
            dewp: c("#00dd00"),
            wetb: c("#00cccc"),
            skew_dgz: c("#f5d800"),
            skew_itherm: c("#bbbbbb"),
            skew_itherm_hgz: c("#6666ff"),
            skew_adiab: c("#dddddd"),
            skew_mixr: c("#adebad"),
            skew_hgt: c("#dd0000"),
            skew_lcl_mkr: c("#00bb00"),
            skew_lfc_mkr: c("#bbbb00"),
            skew_el_mkr: c("#bb00bb"),
            hodo_itach: c("#bbbbbb"),
            hodo_crit: c("#00cccc"),
            eff_inflow: c("#00cccc"),
            hgt_bands: [c("#dd0000"), c("#00dd00"), c("#dddd00"), c("#00dddd")],
            pwat_m: [c("#ff9900"), c("#cc7a00"), c("#804d00")],
            pwat_p: [c("#006600"), c("#00b300"), c("#00ff00")],
            alert: [c("#dfbb9f"), c("#cc9666"), c("#666666"), c("#999900"), c("#cc0000"), c("#e700df")],
            alert_lscp: c("#00cccc"),
            stp_box: c("#00aa00"),
            stp_line: c("#66b3ff"),
            pcl_sel: c("#00cccc"),
            pcl_cin_hi: c("#dfbb9f"),
            pcl_cin_md: c("#cc9666"),
            pcl_cin_lo: c("#00cc00"),
            spd_itach: c("#ebd2c6"),
            srw_clsc: c("#fe81ed"),
            srw_trace: c("#dd0000"),
            srw_0_2: c("#8b0000"),
            srw_4_6: c("#6495ed"),
            srw_9_11: c("#9400d3"),
            winter_dgz: c("#999900"),
            watch_pdstor: c("#dd00dd"),
            watch_tor: c("#dd0000"),
            watch_svr: c("#aaaa00"),
            watch_mrglsvr: c("#0099cc"),
            watch_flood: c("#5ffb17"),
            watch_blizzard: c("#3366ff"),
            watch_fire: c("#ff9900"),
            watch_heat: c("#c85a17"),
            watch_none: c("#cc9900"),
            text_border: c("#3399cc"),
        }
    }

    pub fn protanopia() -> Theme {
        Theme {
            dewp: c("#00ffff"),
            wetb: c("#6666ff"),
            eff_inflow: c("#00ffff"),
            hgt_bands: [c("#ff0000"), c("#0000ff"), c("#ff9900"), c("#00ffff")],
            alert: [c("#775000"), c("#996600"), c("#00ff99"), c("#ffffff"), c("#00ffff"), c("#ff00ff")],
            alert_lscp: c("#ffff00"),
            ..Theme::standard()
        }
    }

    /// Hodograph / speed-column color for a height AGL in meters.
    pub fn height_band(&self, z_agl: f64) -> Color {
        if z_agl < 3000.0 {
            self.hgt_bands[0]
        } else if z_agl < 6000.0 {
            self.hgt_bands[1]
        } else if z_agl < 9000.0 {
            self.hgt_bands[2]
        } else {
            self.hgt_bands[3]
        }
    }

    /// Color for SHARPpy's "Possible Hazard Type" strings.
    pub fn watch_color(&self, watch: &str) -> Color {
        match watch {
            "PDS TOR" => self.watch_pdstor,
            "TOR" | "MRGL TOR" => self.watch_tor,
            "SVR" => self.watch_svr,
            "MRGL SVR" => self.watch_mrglsvr,
            "FLASH FLOOD" => self.watch_flood,
            "BLIZZARD" | "WIND CHILL" | "FREEZE" => self.watch_blizzard,
            "FIRE WEATHER" => self.watch_fire,
            "EXCESSIVE HEAT" => self.watch_heat,
            _ => self.watch_none,
        }
    }
}
