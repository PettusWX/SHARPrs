#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod workbench;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SHARPrs")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../../packaging/icon.png"))
                    .expect("bundled application icon"),
            )
            .with_inner_size([1200.0, 900.0])
            .with_min_inner_size([640.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "SHARPrs",
        options,
        Box::new(move |cc| Ok(Box::new(workbench::Workbench::new(&cc.egui_ctx, path)))),
    )
}
