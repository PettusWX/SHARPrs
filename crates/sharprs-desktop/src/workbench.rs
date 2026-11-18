mod settings;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui::{self, Ui};
use sharprs::core::{AnalysisOptions, ParcelKind};
use sharprs::io::{self, Sounding, SoundingSeries};
use sharprs::render::{self, ColorScheme, Deviant, Inset, RenderOptions};
use sharprs::{Analyzed, ViewOptions};

const DEMO: &str = include_str!("../../../examples/data/sharppy/14061619.OAX");
const INSETS: &[(Inset, &str)] = &[
    (Inset::Sars, "SARS analogs"),
    (Inset::StpStats, "STP statistics"),
    (Inset::CondStp, "Conditional STP"),
    (Inset::Winter, "Winter"),
    (Inset::Fire, "Fire weather"),
    (Inset::Ship, "Significant hail"),
    (Inset::Vrot, "Rotational velocity"),
];
type LoadResult = Result<(SoundingSeries, String), String>;

pub struct Workbench {
    series: SoundingSeries,
    source: String,
    index: usize,
    analyzed: Option<Analyzed>,
    view: ViewOptions,
    options: RenderOptions,
    chart: Option<Chart>,
    loading: Option<Receiver<LoadResult>>,
    error: Option<String>,
    notice: String,
    customize: bool,
    right_slot: bool,
    actual_size: bool,
    edits: Option<Edits>,
    fetch_dialog: bool,
    station: String,
    valid_time: String,
}

struct Chart {
    canvas: render::RasterCanvas,
    texture: egui::TextureHandle,
}

#[derive(Default)]
struct Edits {
    custom_parcel: bool,
    parcel: [f64; 3],
    custom_motion: bool,
    motion: [f64; 4],
    error: Option<String>,
}

fn choose<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    choices: &[(T, &str)],
) -> bool {
    let old = *value;
    let name = choices
        .iter()
        .find(|(v, _)| v == value)
        .map_or("Select", |v| v.1);
    egui::ComboBox::from_id_salt(id)
        .selected_text(name)
        .show_ui(ui, |ui| {
            for &(v, label) in choices {
                ui.selectable_value(value, v, label);
            }
        });
    old != *value
}

impl Workbench {
    pub fn new(ctx: &egui::Context, path: Option<PathBuf>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Roboto".into(),
            egui::FontData::from_static(include_bytes!(
                "../../sharprs-render/fonts/roboto/Roboto-Regular.ttf"
            ))
            .into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Roboto".into());
        ctx.set_fonts(fonts);
        let mut app = Self {
            series: SoundingSeries::new(Vec::new()),
            source: String::new(),
            index: 0,
            analyzed: None,
            view: ViewOptions::default(),
            options: RenderOptions::default(),
            chart: None,
            loading: None,
            error: None,
            notice: String::new(),
            customize: false,
            right_slot: true,
            actual_size: false,
            edits: None,
            fetch_dialog: false,
            station: "KOAX".into(),
            valid_time: sharprs::observations::current_cycle().to_iso8601(),
        };
        if let Some(path) = path {
            app.open(ctx, path);
        } else {
            let demo = io::decode(DEMO, &io::DecodeOptions::with_format(io::Format::Spc))
                .map(|series| (series, "Bundled example · 14061619.OAX".into()))
                .map_err(|e| e.to_string());
            app.loaded(demo);
        }
        app
    }

    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.start_load(ctx, move || {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            io::read_file(&path)
                .map(|series| (series, name))
                .map_err(|e| format!("{} · {e}", path.display()))
        });
    }

    fn start_load(
        &mut self,
        ctx: &egui::Context,
        load: impl FnOnce() -> LoadResult + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.error = None;
        self.notice.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(load());
            ctx.request_repaint();
        });
    }

    fn loaded(&mut self, result: LoadResult) {
        match result {
            Ok((series, name)) if !series.is_empty() => {
                self.series = series;
                self.source = name;
                self.select(0);
            }
            Ok(_) => self.error = Some("The file contained no soundings.".into()),
            Err(e) => self.error = Some(e),
        }
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        self.edits = None;
        self.notice.clear();
        self.chart = None;
        if self.view.skewt_parcel == ParcelKind::User {
            self.view.skewt_parcel = ParcelKind::MostUnstable;
        }
        match Analyzed::new(&self.series.soundings[index]) {
            Ok(a) => {
                self.analyzed = Some(a);
                self.error = None;
            }
            Err(e) => {
                self.analyzed = None;
                self.error = Some(e.to_string());
            }
        }
    }

    fn ensure_chart(&mut self, ctx: &egui::Context) {
        if self.chart.is_some() {
            return;
        }
        if let Some(a) = &self.analyzed {
            let canvas = render::render_canvas(&a.view_with(&self.view), &self.options);
            let pixmap = canvas.pixmap();
            let image = egui::ColorImage::from_rgba_premultiplied(
                [pixmap.width() as usize, pixmap.height() as usize],
                pixmap.data(),
            );
            let texture = ctx.load_texture("Sounding", image, egui::TextureOptions::LINEAR);
            self.chart = Some(Chart { canvas, texture });
        }
    }

    fn save(&mut self, ctx: &egui::Context) {
        self.ensure_chart(ctx);
        let Some(chart) = &self.chart else { return };
        let mut name = std::path::Path::new(&self.source)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if let Some(hour) = self.series.soundings[self.index].meta.forecast_hour {
            name.push_str(&format!("-f{hour:03}"));
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{name}.png"))
            .add_filter("PNG", &["png"])
            .save_file()
        {
            match chart
                .canvas
                .encode_png()
                .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
            {
                Ok(()) => self.notice = format!("Saved {}", path.display()),
                Err(e) => self.error = Some(format!("Could not save {} · {e}", path.display())),
            }
        }
    }

    fn file_picker(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open sounding")
            .pick_file()
        {
            self.open(ctx, path);
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("commands").show(ctx, |ui| {
            ui.add_space(3.);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open…").clicked() {
                    self.file_picker(ctx);
                }
                if ui.button("Fetch observation…").clicked() {
                    self.fetch_dialog = true;
                }
                if self.series.len() > 1 {
                    let mut next = self.index;
                    ui.separator();
                    if ui
                        .add_enabled(next > 0, egui::Button::new("‹"))
                        .on_hover_text("Previous profile")
                        .clicked()
                    {
                        next -= 1;
                    }
                    let label = |i: usize| {
                        self.series.soundings[i]
                            .meta
                            .forecast_hour
                            .map_or_else(|| format!("Profile {}", i + 1), |h| format!("F{h:03}"))
                    };
                    egui::ComboBox::from_id_salt("forecast-hour")
                        .width(75.)
                        .selected_text(label(next))
                        .show_ui(ui, |ui| {
                            for i in 0..self.series.len() {
                                ui.selectable_value(&mut next, i, label(i));
                            }
                        });
                    if ui
                        .add_enabled(next + 1 < self.series.len(), egui::Button::new("›"))
                        .on_hover_text("Next profile")
                        .clicked()
                    {
                        next += 1;
                    }
                    if next != self.index {
                        self.select(next);
                    }
                }
                ui.separator();
                ui.toggle_value(&mut self.customize, "Customize");
                if ui
                    .add_enabled(self.analyzed.is_some(), egui::Button::new("Export PNG…"))
                    .clicked()
                {
                    self.save(ctx);
                }
            });
            ui.add_space(3.);
        });
    }

    fn parcel_control(&mut self, ui: &mut Ui) -> bool {
        let mut parcels = vec![
            (ParcelKind::Surface, "Surface"),
            (ParcelKind::MixedLayer, "Mixed layer"),
            (ParcelKind::Forecast, "Forecast"),
            (ParcelKind::MostUnstable, "Most unstable"),
            (ParcelKind::Effective, "Effective"),
        ];
        if self
            .analyzed
            .as_ref()
            .is_some_and(|a| a.analysis.parcels.user.is_some())
        {
            parcels.push((ParcelKind::User, "User"));
        }
        choose(ui, "parcel-trace", &mut self.view.skewt_parcel, &parcels)
    }
}

impl Edits {
    fn from_analysis(a: &Analyzed) -> Self {
        let level = a.profile.level(a.profile.sfc_index());
        let parcel = &a.analysis.parcels.user;
        let mut settings = Self {
            custom_parcel: parcel.is_some(),
            parcel: [
                parcel
                    .as_ref()
                    .and_then(|p| p.pres)
                    .or(level.pres)
                    .unwrap_or(1000.0),
                parcel
                    .as_ref()
                    .and_then(|p| p.tmpc)
                    .or(level.tmpc)
                    .unwrap_or(20.0),
                parcel
                    .as_ref()
                    .and_then(|p| p.dwpc)
                    .or(level.dwpc)
                    .unwrap_or(10.0),
            ],
            ..Default::default()
        };
        if let Some(m) = a.analysis.storm_motion {
            settings.custom_motion = Some(m) != a.analysis.kinematics.bunkers;
            settings.motion = [
                m.right.direction(),
                m.right.speed(),
                m.left.direction(),
                m.left.speed(),
            ];
        }
        settings
    }

    fn options(&self, sounding: &Sounding) -> Result<AnalysisOptions, String> {
        let csv = |values: &[f64]| {
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let parcel = self.custom_parcel.then(|| csv(&self.parcel));
        let motion = self.custom_motion.then(|| csv(&self.motion));
        sharprs::analysis_options(sounding, parcel.as_deref(), motion.as_deref())
    }

    fn fields(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.custom_parcel, "User parcel");
        ui.add_enabled_ui(self.custom_parcel, |ui| {
            egui::Grid::new("parcel-values").show(ui, |ui| {
                for (v, label) in self.parcel.iter_mut().zip([
                    "Pressure (hPa)",
                    "Temperature (°C)",
                    "Dewpoint (°C)",
                ]) {
                    ui.label(label);
                    ui.add(egui::DragValue::new(v).speed(0.1));
                    ui.end_row();
                }
            });
        });
        ui.separator();
        ui.checkbox(&mut self.custom_motion, "Custom storm motion");
        ui.add_enabled_ui(self.custom_motion, |ui| {
            egui::Grid::new("motion-values").show(ui, |ui| {
                ui.label("");
                ui.label("From (°)");
                ui.label("Speed (kt)");
                ui.end_row();
                for (pair, label) in self.motion.chunks_mut(2).zip(["Right mover", "Left mover"]) {
                    ui.label(label);
                    ui.add(egui::DragValue::new(&mut pair[0]).range(0.0..=360.0));
                    ui.add(egui::DragValue::new(&mut pair[1]).range(0.0..=f64::MAX));
                    ui.end_row();
                }
            });
        });
    }
}

impl Workbench {
    fn inspect(&mut self, response: egui::Response) {
        use render::layout::{BASE_H, BASE_W};
        use sharprs::core::{Field, display::float_str};
        let Some(pos) = response.hover_pos() else {
            return;
        };
        let Some(a) = &self.analyzed else { return };
        let r = response.rect;
        let base = match self.options.flavor {
            render::Flavor::Pivotal => render::WindowLayout::base(),
            render::Flavor::Classic => render::WindowLayout::classic_base(),
        };
        let (w, h) = (self.options.width as f32, self.options.height as f32);
        let scale = (w / BASE_W).min(h / BASE_H);
        let layout = render::WindowLayout::scaled_f(base, w / scale, h / scale);
        let point = egui::pos2(
            (pos.x - r.left()) / r.width() * w / scale,
            (pos.y - r.top()) / r.height() * h / scale,
        );
        if response.clicked() {
            for (right, slot) in [(false, layout.left_inset), (true, layout.right_inset)] {
                if (slot.x..slot.right()).contains(&point.x)
                    && (slot.y..slot.bottom()).contains(&point.y)
                {
                    self.right_slot = right;
                    self.customize = true;
                    return;
                }
            }
        }
        let panel = layout.skewt;
        let geom = render::panels::skewt::SkewGeom::new(
            panel.w.into(),
            panel.h.into(),
            self.options.flavor,
        );
        let x = point.x - panel.x;
        let y = point.y - panel.y;
        let Some(p) = geom.pressure_at(x.into(), y.into()) else {
            return;
        };
        if p > a.profile.sfc_pres() || p < a.profile.level(a.profile.top_index()).pres.unwrap_or(p)
        {
            return;
        }
        let t = a.profile.interp(Field::Temperature, p);
        let td = a.profile.interp(Field::Dewpoint, p);
        let h = a.profile.hght_at_pres(p);
        if response.clicked() {
            if let (Some(t), Some(td)) = (t, td) {
                self.edits = Some(Edits {
                    custom_parcel: true,
                    parcel: [p, t, td],
                    ..Edits::from_analysis(a)
                });
            }
        }
        let temp = |v| match self.options.temp_units {
            render::TempUnits::Celsius => format!("{} °C", float_str(v, 1)),
            render::TempUnits::Fahrenheit => {
                format!("{} °F", float_str(v.map(sharprs::core::thermo::ctof), 1))
            }
        };
        response.on_hover_ui_at_pointer(|ui| {
            ui.strong(format!(
                "{p:.1} hPa   ·   {} m MSL",
                sharprs::core::display::int_str(h)
            ));
            ui.label(format!("T {}     Td {}", temp(t), temp(td)));
            if t.is_some() && td.is_some() {
                ui.weak("Click to lift a parcel here");
            }
        });
    }

    fn observation_window(&mut self, ctx: &egui::Context) {
        if !self.fetch_dialog {
            return;
        }
        let mut open = true;
        let mut fetch = false;
        egui::Window::new("Fetch observation")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Iowa Environmental Mesonet sounding archive");
                egui::Grid::new("observation-request").show(ui, |ui| {
                    ui.label("Station");
                    ui.text_edit_singleline(&mut self.station);
                    ui.end_row();
                    ui.label("Valid time (UTC)");
                    ui.text_edit_singleline(&mut self.valid_time);
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("Current 00Z/12Z cycle").clicked() {
                        self.valid_time = sharprs::observations::current_cycle().to_iso8601();
                    }
                    fetch = ui.button("Fetch").clicked();
                });
            });
        if fetch {
            if let Some(time) = io::UtcTime::parse_iso(&self.valid_time) {
                let station = self.station.trim().to_uppercase();
                self.start_load(ctx, move || {
                    sharprs::observations::fetch(&station, time)
                        .map(|(series, _)| (series, format!("IEM · {station}")))
                });
                open = false;
            } else {
                self.error = Some("Enter a UTC time such as 2024-07-24T00:00Z".into());
            }
        }
        self.fetch_dialog = open;
    }

    fn edit_window(&mut self, ctx: &egui::Context) {
        let Some(edits) = &mut self.edits else { return };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Edit analysis")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Overrides apply to the selected profile.");
                edits.fields(ui);
                if let Some(e) = &edits.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                apply = ui.button("Apply to profile").clicked();
            });
        if apply {
            let a = self
                .analyzed
                .as_mut()
                .expect("edits require a selected analysis");
            match edits.options(&a.sounding) {
                Ok(opts) => {
                    a.analysis = sharprs::core::ConvectiveAnalysis::new(&a.profile, &opts);
                    if opts.user_parcel.is_some() {
                        self.view.skewt_parcel = ParcelKind::User;
                    } else if self.view.skewt_parcel == ParcelKind::User {
                        self.view.skewt_parcel = ParcelKind::MostUnstable;
                    }
                    self.chart = None;
                    self.notice = "Updated analysis".into();
                    open = false;
                }
                Err(e) => edits.error = Some(e),
            }
        }
        if !open {
            self.edits = None;
        }
    }
}

impl eframe::App for Workbench {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(file) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
        {
            self.open(ctx, file);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.file_picker(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(ctx);
        }
        if let Some(rx) = &self.loading {
            match rx.try_recv() {
                Ok(result) => {
                    self.loading = None;
                    self.loaded(result);
                }
                Err(TryRecvError::Disconnected) => {
                    self.loading = None;
                    self.error = Some("Opening the file was interrupted".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        self.toolbar(ctx);
        self.observation_window(ctx);
        self.edit_window(ctx);
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.loading.is_some() {
                    ui.spinner();
                    ui.label("Opening…");
                }
                ui.strong(&self.source);
                if let Some(s) = self.series.get(self.index) {
                    ui.label(format!("{} levels", s.len()));
                }
                if !self.notice.is_empty() {
                    ui.separator();
                    ui.label(&self.notice);
                }
            });
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        });
        if self.customize {
            self.settings_panel(ctx);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ensure_chart(ctx);
            if let Some(chart) = &self.chart {
                let image = egui::Image::new(&chart.texture).sense(egui::Sense::click());
                let response = if self.actual_size {
                    egui::ScrollArea::both()
                        .show(ui, |ui| {
                            ui.add(image.fit_to_exact_size(chart.texture.size_vec2()))
                        })
                        .inner
                } else {
                    let natural = chart.texture.size_vec2();
                    let available = ui.available_size();
                    let factor = (available.x / natural.x).min(available.y / natural.y);
                    let rect =
                        egui::Rect::from_center_size(ui.max_rect().center(), natural * factor);
                    ui.put(rect, image.fit_to_exact_size(rect.size()))
                };
                self.inspect(response);
            } else if self.loading.is_none() {
                ui.label("Open a sounding to view its chart.");
            }
        });
    }
}
