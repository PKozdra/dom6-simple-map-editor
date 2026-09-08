use std::time::Instant;

use dom6_mapgen::generate::generate;
use dom6_mapgen::{Control, Options, Sink, Stage};

struct Timing {
    last: Instant,
    start: Instant,
}

impl Sink for Timing {
    fn stage(&mut self, stage: Stage, call: u32, hash: u64) -> Control {
        let now = Instant::now();
        println!(
            "{:>10} {:>2} {:016x} {:>8.1} ms",
            stage.name(),
            call,
            hash,
            (now - self.last).as_secs_f64() * 1000.0
        );
        self.last = now;
        let _ = self.start;
        Control::Continue
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).map(String::as_str).unwrap_or("gen_out");
    let name = args.get(2).map(String::as_str).unwrap_or("gen");
    let seed: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut opts = Options::default();
    if let (Some(w), Some(h)) = (args.get(4), args.get(5)) {
        opts.width = w.parse().unwrap();
        opts.height = h.parse().unwrap();
    }
    if let Some(p) = args.get(6) {
        opts.provinces = p.parse().unwrap();
    }
    if args.get(7).map(String::as_str) == Some("caves") {
        opts.caves_plane = true;
    }
    std::fs::create_dir_all(out).unwrap();
    let start = Instant::now();
    let mut sink = Timing { last: start, start };
    let g = generate(&opts, seed, name, &mut sink).unwrap();
    println!("total {:.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    let p = &g.planes[0];
    std::fs::write(format!("{out}/{name}.d6m"), &p.d6m).unwrap();
    std::fs::write(format!("{out}/{name}.map"), &p.map_text).unwrap();
    println!(
        "{} x {} provinces {}",
        p.width,
        p.height,
        p.provinces.len() - 1
    );
}
