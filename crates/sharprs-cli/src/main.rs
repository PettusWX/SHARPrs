//! `sharprs` — SHARPpy-style sounding analysis from the command line.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use sharprs::render::{self, ColorScheme, Deviant, FontFamily, Inset, PwUnits, RenderOptions, TempUnits, WindUnits};
use sharprs::{Analyzed, Select, TitleStyle, ViewOptions};

#[derive(Parser)]
#[command(name = "sharprs", version, about = "SHARPpy-compatible sounding analysis and SPC-window plots, in pure Rust")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download an observed sounding from the Iowa Environmental Mesonet as IEM JSON.
    Fetch {
        /// Sounding station, e.g. KOAX.
        station: String,
        /// UTC valid time, e.g. 2024-07-24T00:00Z. Defaults to the current 00Z/12Z cycle.
        #[arg(long)]
        time: Option<String>,
        /// Write the original JSON to a file (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render the full SPC window to a PNG.
    Plot {
        /// Sounding file (SPC text, BUFKIT, UWyo, IEM JSON, CSV).
        file: PathBuf,
        /// Output PNG (default: input name with .png).
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        pick: Pick,
        #[command(flatten)]
        overrides: Overrides,
        #[command(flatten)]
        style: Style,
    },
    /// Print the computed parameters.
    Params {
        file: PathBuf,
        #[command(flatten)]
        pick: Pick,
        #[command(flatten)]
        overrides: Overrides,
        /// Emit the full analysis as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List the soundings in a file (BUFKIT files hold every forecast hour).
    Info { file: PathBuf },
    /// Convert a sounding to SPC text (SHARPpy-readable) or CSV.
    Convert {
        file: PathBuf,
        #[command(flatten)]
        pick: Pick,
        #[arg(long, value_enum, default_value = "spc")]
        to: ConvertTo,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render every sounding listed in a manifest.json into a folder.
    Gallery {
        /// Path to manifest.json (see examples/data).
        manifest: PathBuf,
        /// Output folder.
        #[arg(short, long, default_value = "examples/gallery")]
        output: PathBuf,
        /// Also render inverted and classic-style variants of each sounding.
        #[arg(long)]
        variants: bool,
        #[command(flatten)]
        style: Style,
    },
}

#[derive(Args, Clone)]
struct Pick {
    /// Forecast hour to use from a multi-time file.
    #[arg(long, short = 'f')]
    fhr: Option<u32>,
    /// Index of the sounding in the file (0-based).
    #[arg(long, conflicts_with = "fhr")]
    index: Option<usize>,
}

impl Pick {
    fn select(&self) -> Select {
        match (self.fhr, self.index) {
            (Some(h), _) => Select::ForecastHour(h),
            (_, Some(i)) => Select::Index(i),
            _ => Select::First,
        }
    }
}

#[derive(Args, Clone)]
struct Style {
    /// Color scheme.
    #[arg(long, value_enum, default_value = "standard")]
    scheme: Scheme,
    /// Look: Pivotal Weather (Roboto, 96 dpi) or classic SHARPpy desktop.
    #[arg(long, value_enum, default_value = "pivotal")]
    flavor: FlavorArg,
    #[arg(long, value_enum)]
    font: Option<FontArg>,
    #[arg(long, value_enum, default_value = "f")]
    temp_units: TempArg,
    #[arg(long, value_enum, default_value = "kt")]
    wind_units: WindArg,
    #[arg(long, value_enum, default_value = "in")]
    pw_units: PwArg,
    /// Output size, e.g. 1180x800 (the reference) or 2360x1600.
    #[arg(long, default_value = "1180x800")]
    size: String,
    #[arg(long, value_enum, default_value = "sars")]
    left_inset: InsetArg,
    #[arg(long, value_enum, default_value = "stp-stats")]
    right_inset: InsetArg,
    /// Storm mover shown. Defaults to left south of the equator and right elsewhere.
    #[arg(long, value_enum)]
    deviant: Option<DeviantArg>,
    /// Highlight the dendritic growth zone on the skew-T.
    #[arg(long)]
    dgz: bool,
    /// Mark the PBL top on the skew-T.
    #[arg(long)]
    pbl: bool,
    /// Logical DPI for text (Qt semantics). Default: 96 for pivotal, 72 for classic.
    #[arg(long)]
    dpi: Option<f32>,
    /// Title (top left). Default is built from the file metadata.
    #[arg(long)]
    title: Option<String>,
    #[arg(long, value_enum, default_value = "pivotal")]
    title_style: TitleArg,
    /// Text at the top right.
    #[arg(long)]
    header_right: Option<String>,
    /// Parcel lifted on the skew-T.
    #[arg(long, value_enum, default_value = "mu")]
    parcel: ParcelArg,
}

#[derive(ValueEnum, Clone, Copy)]
enum Scheme {
    Standard,
    Inverted,
    Protanopia,
}
#[derive(ValueEnum, Clone, Copy)]
enum FlavorArg {
    Pivotal,
    Classic,
}
#[derive(ValueEnum, Clone, Copy)]
enum FontArg {
    Roboto,
    Arimo,
}
#[derive(ValueEnum, Clone, Copy)]
enum TempArg {
    F,
    C,
}
#[derive(ValueEnum, Clone, Copy)]
enum WindArg {
    Kt,
    Ms,
}
#[derive(ValueEnum, Clone, Copy)]
enum PwArg {
    In,
    Cm,
}
#[derive(ValueEnum, Clone, Copy)]
enum InsetArg {
    Sars,
    StpStats,
    CondStp,
    Winter,
    Fire,
    Ship,
    Vrot,
}
#[derive(ValueEnum, Clone, Copy)]
enum DeviantArg {
    Right,
    Left,
}
#[derive(ValueEnum, Clone, Copy)]
enum TitleArg {
    Pivotal,
    Sharppy,
}
#[derive(ValueEnum, Clone, Copy)]
enum ParcelArg {
    Sfc,
    Ml,
    Fcst,
    Mu,
    Eff,
}

#[derive(Args)]
struct Overrides {
    /// Lift a parcel at pressure (hPa), temperature (°C), and dewpoint (°C).
    #[arg(long, value_name = "P,T,TD")]
    user_parcel: Option<String>,
    /// Right and left storm motions as direction FROM (degrees) and speed (knots).
    #[arg(long, value_name = "RM_DIR,RM_KT,LM_DIR,LM_KT")]
    storm_motion: Option<String>,
}

impl Overrides {
    fn analyze(&self, sounding: &sharprs::io::Sounding) -> Result<Analyzed, String> {
        let options = sharprs::analysis_options(sounding, self.user_parcel.as_deref(), self.storm_motion.as_deref())?;
        Analyzed::with_options(sounding, &options).map_err(|e| e.to_string())
    }
}
#[derive(ValueEnum, Clone, Copy)]
enum ConvertTo {
    Spc,
    Csv,
}

fn inset(i: InsetArg) -> Inset {
    match i {
        InsetArg::Sars => Inset::Sars,
        InsetArg::StpStats => Inset::StpStats,
        InsetArg::CondStp => Inset::CondStp,
        InsetArg::Winter => Inset::Winter,
        InsetArg::Fire => Inset::Fire,
        InsetArg::Ship => Inset::Ship,
        InsetArg::Vrot => Inset::Vrot,
    }
}

impl Style {
    fn render_options(&self) -> Result<RenderOptions, String> {
        let mut o = match self.flavor {
            FlavorArg::Pivotal => RenderOptions::default(),
            FlavorArg::Classic => RenderOptions::classic(),
        };
        o.scheme = match self.scheme {
            Scheme::Standard => ColorScheme::Standard,
            Scheme::Inverted => ColorScheme::Inverted,
            Scheme::Protanopia => ColorScheme::Protanopia,
        };
        if let Some(f) = self.font {
            o.font_family = match f {
                FontArg::Roboto => FontFamily::Roboto,
                FontArg::Arimo => FontFamily::Arimo,
            };
        }
        o.temp_units = match self.temp_units {
            TempArg::F => TempUnits::Fahrenheit,
            TempArg::C => TempUnits::Celsius,
        };
        o.wind_units = match self.wind_units {
            WindArg::Kt => WindUnits::Knots,
            WindArg::Ms => WindUnits::MetersPerSecond,
        };
        o.pw_units = match self.pw_units {
            PwArg::In => PwUnits::Inches,
            PwArg::Cm => PwUnits::Centimeters,
        };
        let (w, h) = self
            .size
            .split_once(['x', 'X'])
            .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
            .ok_or_else(|| format!("invalid --size '{}', expected WIDTHxHEIGHT", self.size))?;
        if w < 200 || h < 150 || w > 16000 || h > 16000 {
            return Err(format!("--size {w}x{h} out of range"));
        }
        o.width = w;
        o.height = h;
        o.left_inset = inset(self.left_inset);
        o.right_inset = inset(self.right_inset);
        o.deviant = self.deviant.map(|mover| match mover {
            DeviantArg::Right => Deviant::Right,
            DeviantArg::Left => Deviant::Left,
        });
        o.show_dgz = self.dgz;
        o.show_pbl = self.pbl;
        if let Some(d) = self.dpi {
            if !(36.0..=600.0).contains(&d) {
                return Err(format!("--dpi {d} out of range"));
            }
            o.dpi = d;
        }
        if let Some(r) = &self.header_right {
            o.header_right = Some(r.clone());
        }
        Ok(o)
    }

    fn view_options(&self) -> ViewOptions {
        use sharprs::core::ParcelKind as K;
        ViewOptions {
            title: self.title.clone(),
            title_style: match self.title_style {
                TitleArg::Pivotal => TitleStyle::Pivotal,
                TitleArg::Sharppy => TitleStyle::Sharppy,
            },
            skewt_parcel: match self.parcel {
                ParcelArg::Sfc => K::Surface,
                ParcelArg::Ml => K::MixedLayer,
                ParcelArg::Fcst => K::Forecast,
                ParcelArg::Mu => K::MostUnstable,
                ParcelArg::Eff => K::Effective,
            },
            ..ViewOptions::default()
        }
    }
}

fn plot(analyzed: &Analyzed, style: &Style, out: &Path) -> Result<(), String> {
    let ropts = style.render_options()?;
    let mut vopts = style.view_options();
    if analyzed.analysis.parcels.user.is_some() {
        vopts.skewt_parcel = sharprs::core::ParcelKind::User;
    }
    let view = analyzed.view_with(&vopts);
    let png = render::render_png(&view, &ropts)?;
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(out, png).map_err(|e| format!("{}: {e}", out.display()))
}

fn fmt_opt(v: Option<f64>, prec: usize) -> String {
    v.map(|x| format!("{x:.prec$}")).unwrap_or_else(|| "--".into())
}

fn print_params(a: &Analyzed) {
    let an = &a.analysis;
    println!("{}", sharprs::sounding_title(&a.sounding, TitleStyle::Pivotal));
    println!();
    println!("{:<6}{:>8}{:>8}{:>8}{:>8}{:>8}{:>8}", "PCL", "CAPE", "CINH", "LCL", "LI", "LFC", "EL");
    let parcels = [("SFC", &an.parcels.sfc), ("ML", &an.parcels.ml), ("FCST", &an.parcels.fcst), ("MU", &an.parcels.mu)];
    for (name, p) in parcels.into_iter().chain(an.parcels.user.as_ref().map(|p| ("USER", p))) {
        println!(
            "{:<6}{:>8}{:>8}{:>8}{:>8}{:>8}{:>8}",
            name,
            fmt_opt(p.cape, 0),
            sharprs::core::display::int_str(p.cinh),
            fmt_opt(p.lcl_hght, 0),
            fmt_opt(p.li5, 0),
            fmt_opt(p.lfc_hght, 0),
            fmt_opt(p.el_hght, 0)
        );
    }
    let t = &an.thermo;
    let d = an.severe.deviant.selected(an.southern_hemisphere);
    let sr = an.kinematics.storm_relative.selected(an.southern_hemisphere);
    println!();
    println!("PW {} in   K {}   TT {}   DCAPE {}", fmt_opt(t.pwat, 2), fmt_opt(t.k_index, 0), fmt_opt(t.totals_totals, 0), fmt_opt(an.downdraft.dcape, 0));
    println!(
        "Lapse rates (C/km): sfc-3km {}  3-6km {}  850-500 {}  700-500 {}",
        fmt_opt(t.lapse_rate_0_3km, 1),
        fmt_opt(t.lapse_rate_3_6km, 1),
        fmt_opt(t.lapse_rate_850_500, 1),
        fmt_opt(t.lapse_rate_700_500, 1)
    );
    let h = |x: &Option<sharprs::core::Helicity>| x.as_ref().map(|h| h.total);
    let spd = |x: &Option<sharprs::core::Vector>| x.as_ref().map(|v| v.speed());
    println!("SRH 0-1km {}  0-3km {}  eff {} m2/s2", fmt_opt(h(&sr.srh_1km), 0), fmt_opt(h(&sr.srh_3km), 0), fmt_opt(h(&sr.esrh), 0));
    println!(
        "Shear 0-1km {}  0-6km {}  EBWD {} kt",
        fmt_opt(spd(&an.kinematics.shear_1km), 0),
        fmt_opt(spd(&an.kinematics.shear_6km), 0),
        fmt_opt(spd(&an.kinematics.ebwd), 0)
    );
    if let Some(b) = &an.kinematics.bunkers {
        println!(
            "Bunkers RM {:.0}/{:.0} kt   LM {:.0}/{:.0} kt",
            b.right.direction(),
            b.right.speed(),
            b.left.direction(),
            b.left.speed()
        );
    }
    if let Some(storm) = an.storm_motion.filter(|s| Some(*s) != an.kinematics.bunkers) {
        println!("Selected RM {:.0}/{:.0} kt   LM {:.0}/{:.0} kt", storm.right.direction(),
            storm.right.speed(), storm.left.direction(), storm.left.speed());
    }
    println!(
        "STP(cin) {}  STP(fix) {}  SCP {}  SHIP {}",
        fmt_opt(d.stp_cin, 1),
        fmt_opt(d.stp_fixed, 1),
        fmt_opt(d.scp, 1),
        fmt_opt(an.severe.ship, 1)
    );
    let haz: Vec<&str> = an.hazard_types_selected().iter().map(|h| h.label()).collect();
    println!("Possible hazard: {}", haz.first().copied().unwrap_or("--"));
}

fn gallery(manifest: &Path, out: &Path, variants: bool, style: &Style) -> Result<(), String> {
    let m = sharprs::io::Manifest::load(manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let base = style.render_options()?;
    let mut styles = vec![("", "", base.clone())];
    if variants {
        std::fs::create_dir_all(out.join("variants")).map_err(|err| err.to_string())?;
        styles.push(("variants", "_inverted", RenderOptions { scheme: ColorScheme::Inverted, ..base.clone() }));
        styles.push(("variants", "_classic", RenderOptions { scheme: base.scheme, ..RenderOptions::classic() }));
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16).min(m.soundings.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    let vopts = style.view_options();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(e) = m.soundings.get(i) else { break };
                    let view = m.load_sounding(e).map_err(|err| err.to_string())
                        .and_then(|s| Analyzed::new(&s).map_err(|err| err.to_string()))
                        .map(|a| a.view_with(&vopts));
                    let stem = if e.category == "edge" { format!("edge_{}", e.id) } else { e.id.clone() };
                    for (dir, suffix, opts) in &styles {
                        let path = out.join(dir).join(format!("{stem}{suffix}.png"));
                        let result = view.as_ref().map_err(Clone::clone)
                            .and_then(|v| render::render_png(v, opts))
                            .and_then(|png| {
                                if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                                std::fs::write(&path, png).map_err(|e| e.to_string())
                            });
                        match result {
                            Ok(()) => eprintln!("  wrote {}", path.display()),
                            Err(err) => failures.lock().unwrap().push(format!("{}: {err}", path.display())),
                        }
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    eprintln!("{} images, {} failed", m.soundings.len() * styles.len() - failures.len(), failures.len());
    for f in &failures {
        eprintln!("  FAILED {f}");
    }
    if failures.is_empty() { Ok(()) } else { Err(format!("{} renders failed", failures.len())) }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Cmd::Fetch { station, time, output } => {
            use std::io::Write;
            let time = match time {
                Some(t) => sharprs::io::UtcTime::parse_iso(&t).ok_or_else(|| format!("Invalid UTC time: {t}"))?,
                None => sharprs::observations::current_cycle(),
            };
            let (_, bytes) = sharprs::observations::fetch(&station, time)?;
            match output {
                Some(p) => std::fs::write(&p, bytes).map_err(|e| format!("{}: {e}", p.display())),
                None => std::io::stdout().lock().write_all(&bytes).map_err(|e| e.to_string()),
            }
        }
        Cmd::Plot { file, output, pick, style, overrides } => {
            let snd = sharprs::load(&file, pick.select()).map_err(|e| e.to_string())?;
            let a = overrides.analyze(&snd)?;
            let out = output.unwrap_or_else(|| file.with_extension("png"));
            plot(&a, &style, &out)?;
            eprintln!("wrote {}", out.display());
            Ok(())
        }
        Cmd::Params { file, pick, json, overrides } => {
            let snd = sharprs::load(&file, pick.select()).map_err(|e| e.to_string())?;
            let a = overrides.analyze(&snd)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&a.analysis).map_err(|e| e.to_string())?);
            } else {
                print_params(&a);
            }
            Ok(())
        }
        Cmd::Info { file } => {
            let series = sharprs::io::read_file(&file).map_err(|e| e.to_string())?;
            println!("{}: {} sounding(s)", file.display(), series.len());
            for (i, s) in series.iter().enumerate() {
                println!("  [{i}] {}  ({} levels)", s.meta.title(), s.len());
            }
            Ok(())
        }
        Cmd::Convert { file, pick, to, output } => {
            let snd = sharprs::load(&file, pick.select()).map_err(|e| e.to_string())?;
            let text = match to {
                ConvertTo::Spc => sharprs::io::write_spc(&snd),
                ConvertTo::Csv => sharprs::io::write_csv(&snd),
            };
            match output {
                Some(p) => std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display())),
                None => {
                    print!("{text}");
                    Ok(())
                }
            }
        }
        Cmd::Gallery { manifest, output, variants, style } => gallery(&manifest, &output, variants, &style),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
