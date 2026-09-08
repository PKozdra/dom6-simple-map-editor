use std::time::Instant;

use dom6_mapgen::generate::generate_with_terrain;
use dom6_mapgen::{Control, Sink, Stage};
use dom6_simple_map_editor::generator_panel::Form;
use dom6_simple_map_editor::project::Project;
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::textures::TexSet;

struct Timing {
    last: Instant,
}

impl Sink for Timing {
    fn wants_hash(&self) -> bool {
        std::env::var_os("BENCH_HASH").is_some()
    }

    fn stage(&mut self, stage: Stage, call: u32, _hash: u64) -> Control {
        let now = Instant::now();
        println!(
            "{:>12} {:>2} {:>8.1} ms",
            stage.name(),
            call,
            (now - self.last).as_secs_f64() * 1000.0
        );
        self.last = now;
        Control::Continue
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u32 = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(669246480);
    let mut form = Form {
        seed,
        ..Form::default()
    };
    if let (Some(w), Some(h)) = (args.get(2), args.get(3)) {
        form.auto_size = false;
        form.width = w.parse().unwrap();
        form.height = h.parse().unwrap();
    }
    if let Some(p) = args.get(4) {
        form.opts.provinces = p.parse().unwrap();
    }
    if let Some(i) = args.get(5) {
        form.opts.extra_islands = i.parse().unwrap();
    }
    if std::env::var_os("BENCH_HWRAP").is_some() {
        form.opts.hwrap = true;
    }
    let opts = form.options(form.blueprint(), form.cave_blueprint().unwrap());
    let start = Instant::now();
    let mut sink = Timing { last: start };
    let g = generate_with_terrain(&opts, seed, "bench", &mut sink).unwrap();
    println!(
        "generate total {:.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    if let Some(dir) = std::env::var_os("BENCH_SAVE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, p) in g.planes.iter().enumerate() {
            std::fs::write(dir.join(format!("bench_{i}.d6m")), &p.d6m).unwrap();
            std::fs::write(dir.join(format!("bench_{i}.map")), &p.map_text).unwrap();
        }
        return;
    }
    if std::env::var_os("BENCH_GEN_ONLY").is_some() {
        return;
    }
    let t = Instant::now();
    let tex = TexSet::embedded();
    println!(
        "TexSet::embedded {:.1} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let ropts = Options::default();
    let planes: Vec<(&[u8], &str)> = g
        .planes
        .iter()
        .map(|p| (p.d6m.as_slice(), p.map_text.as_str()))
        .collect();
    let t = Instant::now();
    let gates: Vec<(u16, u16)> = g.gates.iter().map(|g| (g.surface, g.cave)).collect();
    let p = Project::from_generated(std::env::temp_dir(), "bench", &planes, &gates, &tex, &ropts)
        .unwrap();
    println!(
        "from_generated {:.1} ms ({} planes, {} x {})",
        t.elapsed().as_secs_f64() * 1000.0,
        p.planes.len(),
        p.planes[0].width(),
        p.planes[0].height()
    );
}
