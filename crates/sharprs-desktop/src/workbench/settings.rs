use super::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Preset {
    version: u8,
    render: RenderOptions,
    parcel: ParcelKind,
    vrot_kt: f64,
}

impl Preset {
    fn read(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let preset: Self =
            serde_json::from_reader(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
        let r = &preset.render;
        if preset.version != 1 {
            return Err("Unsupported chart preset version".into());
        }
        if !(200..=4096).contains(&r.width)
            || !(150..=4096).contains(&r.height)
            || r.width * r.height > 10_000_000
            || !(36.0..=300.).contains(&r.dpi)
            || !(0.25..=4.).contains(&r.text_gamma)
            || !(0.0..=400.).contains(&preset.vrot_kt)
        {
            return Err(
                "Invalid chart dimensions, text settings, or rotational velocity in preset".into(),
            );
        }
        Ok(preset)
    }
}

fn section(ui: &mut Ui, name: &str, open: bool, draw: impl FnOnce(&mut Ui)) {
    ui.separator();
    egui::CollapsingHeader::new(egui::RichText::new(name).strong())
        .default_open(open)
        .show(ui, draw);
}

fn color(ui: &mut Ui, label: &str, value: &mut render::Color) -> bool {
    let mut rgb = [value.r, value.g, value.b];
    let response = ui.color_edit_button_srgb(&mut rgb);
    ui.label(label);
    ui.end_row();
    if response.changed() {
        *value = render::Color::rgb(rgb[0], rgb[1], rgb[2]);
    }
    response.changed()
}

impl Workbench {
    fn preset(&self) -> Preset {
        Preset {
            version: 1,
            render: self.options.clone(),
            parcel: self.view.skewt_parcel,
            vrot_kt: self.view.vrot,
        }
    }

    fn apply_preset(&mut self, preset: Preset) {
        self.options = preset.render;
        self.view.skewt_parcel = if preset.parcel == ParcelKind::User
            && self
                .analyzed
                .as_ref()
                .and_then(|a| a.analysis.parcels.user.as_ref())
                .is_none()
        {
            ParcelKind::MostUnstable
        } else {
            preset.parcel
        };
        self.view.vrot = preset.vrot_kt;
        self.chart = None;
    }

    fn presets(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Load preset…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Chart preset", &["json"])
                    .pick_file()
                {
                    match Preset::read(&path) {
                        Ok(p) => {
                            self.apply_preset(p);
                            self.error = None;
                            self.notice = format!("Loaded {}", path.display());
                        }
                        Err(e) => self.error = Some(format!("Could not load preset · {e}")),
                    }
                }
            }
            if ui.button("Save preset…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Chart preset", &["json"])
                    .set_file_name("SHARPrs-style.json")
                    .save_file()
                {
                    match serde_json::to_vec_pretty(&self.preset())
                        .map_err(|e| e.to_string())
                        .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
                    {
                        Ok(()) => {
                            self.error = None;
                            self.notice = format!("Saved {}", path.display());
                        }
                        Err(e) => self.error = Some(format!("Could not save preset · {e}")),
                    }
                }
            }
        });
    }

    pub(super) fn settings_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("chart-inspector")
            .default_width(246.)
            .width_range(226.0..=320.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Chart settings");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Close").clicked() {
                            self.customize = false;
                        }
                    });
                });
                self.presets(ui);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Keep the inset choices directly in the inspector, outside transient popup menus.
                        ui.separator();
                        ui.strong("Bottom insets");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.right_slot, false, "Left inset");
                            ui.selectable_value(&mut self.right_slot, true, "Right inset");
                        });
                        let slot = if self.right_slot {
                            &mut self.options.right_inset
                        } else {
                            &mut self.options.left_inset
                        };
                        let mut dirty = false;
                        for &(value, label) in INSETS {
                            dirty |= ui.radio_value(slot, value, label).changed();
                        }
                        if [self.options.left_inset, self.options.right_inset]
                            .contains(&Inset::Vrot)
                        {
                            ui.horizontal(|ui| {
                                ui.label("Rotational velocity");
                                dirty |= ui
                                    .add(
                                        egui::DragValue::new(&mut self.view.vrot)
                                            .range(0.0..=400.)
                                            .suffix(" kt"),
                                    )
                                    .changed();
                            });
                        }
                        section(ui, "Appearance", false, |ui| {
                            egui::Grid::new("appearance").show(ui, |ui| {
                                ui.label("Layout");
                                if choose(
                                    ui,
                                    "layout",
                                    &mut self.options.flavor,
                                    &[
                                        (render::Flavor::Pivotal, "Pivotal"),
                                        (render::Flavor::Classic, "Classic"),
                                    ],
                                ) {
                                    let defaults = match self.options.flavor {
                                        render::Flavor::Pivotal => RenderOptions::default(),
                                        render::Flavor::Classic => RenderOptions::classic(),
                                    };
                                    self.options.font_family = defaults.font_family;
                                    self.options.text_backend = defaults.text_backend;
                                    self.options.hinting = defaults.hinting;
                                    self.options.dpi = defaults.dpi;
                                    self.options.text_gamma = defaults.text_gamma;
                                    dirty = true;
                                }
                                ui.end_row();
                                ui.label("Font");
                                dirty |= choose(
                                    ui,
                                    "font",
                                    &mut self.options.font_family,
                                    &[
                                        (render::FontFamily::Roboto, "Roboto"),
                                        (render::FontFamily::Arimo, "Arimo"),
                                    ],
                                );
                                ui.end_row();
                                ui.label("Palette");
                                if choose(
                                    ui,
                                    "palette",
                                    &mut self.options.scheme,
                                    &[
                                        (ColorScheme::Standard, "Standard"),
                                        (ColorScheme::Inverted, "Inverted"),
                                        (ColorScheme::Protanopia, "Protanopia"),
                                    ],
                                ) {
                                    self.options.theme_override = None;
                                    dirty = true;
                                }
                                ui.end_row();
                            });
                            let mut theme = self.options.theme();
                            let mut changed = false;
                            egui::Grid::new("chart-colors").show(ui, |ui| {
                                for (label, value) in [
                                    ("Temperature", &mut theme.temp),
                                    ("Dewpoint", &mut theme.dewp),
                                    ("Wet bulb", &mut theme.wetb),
                                    ("Background", &mut theme.bg),
                                    ("Text and parcel", &mut theme.fg),
                                    ("Panel borders", &mut theme.text_border),
                                    ("Isotherms", &mut theme.skew_itherm),
                                    ("Dry adiabats", &mut theme.skew_adiab),
                                ] {
                                    changed |= color(ui, label, value);
                                }
                                for (label, value) in [
                                    "0–3 km winds",
                                    "3–6 km winds",
                                    "6–9 km winds",
                                    "9–12 km winds",
                                ]
                                .into_iter()
                                .zip(&mut theme.hgt_bands)
                                {
                                    changed |= color(ui, label, value);
                                }
                            });
                            if changed {
                                self.options.theme_override = Some(theme);
                                dirty = true;
                            }
                            if ui
                                .add_enabled(
                                    self.options.theme_override.is_some(),
                                    egui::Button::new("Reset palette colors"),
                                )
                                .clicked()
                            {
                                self.options.theme_override = None;
                                dirty = true;
                            }
                        });
                        section(ui, "Traces and overlays", false, |ui| {
                            for (label, v) in [
                                ("Wet bulb", &mut self.options.show_wet_bulb),
                                ("Virtual temperature", &mut self.options.show_virtual_temp),
                                ("Lifted parcel", &mut self.options.show_parcel_trace),
                                ("Downdraft parcel", &mut self.options.show_downdraft),
                                ("Wind barbs", &mut self.options.show_wind_barbs),
                                ("Dendritic growth zone", &mut self.options.show_dgz),
                                ("Boundary layer top", &mut self.options.show_pbl),
                            ] {
                                dirty |= ui.checkbox(v, label).changed();
                            }
                            ui.horizontal(|ui| {
                                ui.label("Omega");
                                dirty |= choose(
                                    ui,
                                    "omega",
                                    &mut self.options.show_omega,
                                    &[
                                        (None, "Automatic"),
                                        (Some(true), "Show"),
                                        (Some(false), "Hide"),
                                    ],
                                );
                            });
                        });
                        section(ui, "Parcel and storm motion", false, |ui| {
                            egui::Grid::new("analysis-options").show(ui, |ui| {
                                ui.label("Parcel");
                                dirty |= self.parcel_control(ui);
                                ui.end_row();
                                ui.label("Mover");
                                dirty |= choose(
                                    ui,
                                    "mover",
                                    &mut self.options.deviant,
                                    &[
                                        (None, "Automatic"),
                                        (Some(Deviant::Right), "Right"),
                                        (Some(Deviant::Left), "Left"),
                                    ],
                                );
                                ui.end_row();
                            });
                            if let Some(a) = &self.analyzed {
                                if ui.button("Edit parcel & storm vectors…").clicked() {
                                    self.edits = Some(Edits::from_analysis(a));
                                }
                            }
                        });
                        section(ui, "Display units", false, |ui| {
                            egui::Grid::new("units").show(ui, |ui| {
                                ui.label("Surface temp.");
                                dirty |= choose(
                                    ui,
                                    "temp",
                                    &mut self.options.temp_units,
                                    &[
                                        (render::TempUnits::Fahrenheit, "°F"),
                                        (render::TempUnits::Celsius, "°C"),
                                    ],
                                );
                                ui.end_row();
                                ui.label("Wind");
                                dirty |= choose(
                                    ui,
                                    "wind",
                                    &mut self.options.wind_units,
                                    &[
                                        (render::WindUnits::Knots, "kt"),
                                        (render::WindUnits::MetersPerSecond, "m/s"),
                                    ],
                                );
                                ui.end_row();
                                ui.label("Precip. water");
                                dirty |= choose(
                                    ui,
                                    "pw",
                                    &mut self.options.pw_units,
                                    &[
                                        (render::PwUnits::Inches, "in"),
                                        (render::PwUnits::Centimeters, "cm"),
                                    ],
                                );
                                ui.end_row();
                            });
                        });
                        section(ui, "Headers and output", false, |ui| {
                            ui.label("Chart title");
                            let mut title = self.options.header_left.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut title)
                                        .hint_text("Automatic sounding title")
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                self.options.header_left = (!title.is_empty()).then_some(title);
                                dirty = true;
                            }
                            ui.label("Right header");
                            let mut header = self.options.header_right.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut header).changed() {
                                self.options.header_right = (!header.is_empty()).then_some(header);
                                dirty = true;
                            }
                            let mut size = (self.options.width, self.options.height);
                            if choose(
                                ui,
                                "size",
                                &mut size,
                                &[
                                    ((1180, 800), "1180 × 800"),
                                    ((1770, 1200), "1770 × 1200"),
                                    ((2360, 1600), "2360 × 1600"),
                                ],
                            ) {
                                (self.options.width, self.options.height) = size;
                                dirty = true;
                            }
                            ui.checkbox(&mut self.actual_size, "View at full size");
                        });
                        if dirty {
                            self.chart = None;
                            ctx.request_repaint();
                        }
                    });
            });
    }
}
