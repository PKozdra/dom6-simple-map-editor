#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

fn icon() -> Option<egui::IconData> {
    let img =
        dom6_simple_map_editor::textures::decode_png(include_bytes!("../assets/icon.png")).ok()?;
    Some(egui::IconData {
        rgba: img.rgba,
        width: img.w as u32,
        height: img.h as u32,
    })
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let random = args.iter().any(|a| a == "--random");
    let rest: Vec<&String> = args.iter().filter(|a| *a != "--random").collect();
    let initial = rest.first().map(|a| PathBuf::from(a.as_str()));
    let preselect = rest.get(1).and_then(|a| a.parse::<u32>().ok());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Dominions 6 Simple Map Editor")
        .with_inner_size([1400.0, 900.0])
        .with_min_inner_size([720.0, 480.0])
        .with_drag_and_drop(true);
    if let Some(ic) = icon() {
        viewport = viewport.with_icon(Arc::new(ic));
    }
    let options = eframe::NativeOptions {
        viewport,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Dominions 6 Simple Map Editor",
        options,
        Box::new(move |cc| {
            Ok(Box::new(dom6_simple_map_editor::app::App::new(
                cc, initial, preselect, random,
            )))
        }),
    )
}
