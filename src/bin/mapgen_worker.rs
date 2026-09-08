#[cfg(target_arch = "wasm32")]
fn main() {
    dom6_simple_map_editor::worker::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
