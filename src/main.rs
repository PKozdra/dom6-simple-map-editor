#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn icon() -> Option<egui::IconData> {
    let img =
        dom6_simple_map_editor::textures::decode_png(include_bytes!("../assets/icon.png")).ok()?;
    Some(egui::IconData {
        rgba: img.rgba,
        width: img.w as u32,
        height: img.h as u32,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use std::path::PathBuf;
    use std::sync::Arc;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let random = args.iter().any(|a| a == "--random");
    let rest: Vec<&String> = args.iter().filter(|a| *a != "--random").collect();
    let initial = rest.first().map(|a| PathBuf::from(a.as_str()));
    let preselect = rest.get(1).and_then(|a| a.parse::<u32>().ok());
    let window = std::env::var("D6SME_WINDOW").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some([w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?])
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Dominions 6 Simple Map Editor")
        .with_inner_size(window.unwrap_or([1400.0, 900.0]))
        .with_min_inner_size(if window.is_some() {
            [320.0, 320.0]
        } else {
            [720.0, 480.0]
        })
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

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    let window = web_sys::window().expect("window");
    let random = window
        .location()
        .search()
        .map(|s| s.contains("random"))
        .unwrap_or(false);
    let document = window.document().expect("document");
    let canvas = document
        .get_element_by_id("editor")
        .expect("canvas")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("canvas element");
    wasm_bindgen_futures::spawn_local(async move {
        let options = eframe::WebOptions {
            should_prevent_default: Box::new(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::F1
                            | egui::Key::F4
                            | egui::Key::Home
                            | egui::Key::PageUp
                            | egui::Key::PageDown,
                        ..
                    }
                )
            }),
            ..Default::default()
        };
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    Ok(Box::new(dom6_simple_map_editor::app::App::new(
                        cc, None, None, random,
                    )))
                }),
            )
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            loading.remove();
        }
        if let Err(e) = result {
            web_sys::console::error_1(&e);
        }
    });
}
