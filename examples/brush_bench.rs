use dom6_simple_map_editor::project::Project;
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::textures::TexSet;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: brush_bench <map.d6m> [radius] [step] [frames] [px per frame]");
        std::process::exit(2);
    }
    let radius: i32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(60);
    let step: f32 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(500.0);
    let frames: usize = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(60);
    let per_frame: f32 = args.get(5).and_then(|a| a.parse().ok()).unwrap_or(40.0);
    let tex = TexSet::embedded();
    let opts = Options::default();
    let t0 = Instant::now();
    let mut project = Project::open(Path::new(&args[1]), &tex, &opts).unwrap();
    let doc = &mut project.planes[0];
    let w = doc.d6m.width;
    let h = doc.d6m.height;
    println!(
        "open {}x{} {} provinces in {:.0} ms",
        w,
        h,
        doc.province_count(),
        t0.elapsed().as_secs_f32() * 1000.0
    );
    let path_mode = args.get(6).map(|a| a == "path").unwrap_or(true);
    let spacing = (radius as f32 / 3.0).max(1.0);
    let per = step * spacing / (2.0 * radius as f32).max(1.0);
    let (x0, y0) = (w as f32 * 0.1, h as f32 * 0.1);
    let (x1, y1) = (w as f32 * 0.9, h as f32 * 0.9);
    let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let (ux, uy) = ((x1 - x0) / len, (y1 - y0) / len);
    doc.paint_begin("Height brush");
    let mut dist = 0.0f32;
    let mut last = 0.0f32;
    let mut times = Vec::new();
    let mut stamps_total = 0usize;
    let mut prev = (x0 as i32, y0 as i32);
    for f in 0..frames {
        dist = (dist + per_frame).min(len);
        if path_mode {
            let now = ((x0 + ux * dist) as i32, (y0 + uy * dist) as i32);
            let t = Instant::now();
            if f == 0 {
                doc.paint_height_stamps(&[(prev.0, prev.1, step)], radius, true, &tex, &opts);
            }
            doc.paint_height_path(prev, now, radius, step, true, &tex, &opts);
            times.push(t.elapsed().as_secs_f32() * 1000.0);
            stamps_total += 1;
            prev = now;
            continue;
        }
        let mut stamps = Vec::new();
        if f == 0 {
            stamps.push((x0 as i32, y0 as i32, step));
            last = 0.0;
        }
        while last + spacing <= dist {
            last += spacing;
            stamps.push(((x0 + ux * last) as i32, (y0 + uy * last) as i32, per));
        }
        if stamps.is_empty() {
            continue;
        }
        stamps_total += stamps.len();
        let t = Instant::now();
        doc.paint_height_stamps(&stamps, radius, true, &tex, &opts);
        times.push(t.elapsed().as_secs_f32() * 1000.0);
    }
    let t = Instant::now();
    let (_, became) = doc.paint_end_follow(true, &tex, &opts);
    let end_ms = t.elapsed().as_secs_f32() * 1000.0;
    let total: f32 = times.iter().sum();
    let max = times.iter().cloned().fold(0.0, f32::max);
    println!(
        "frames {} stamps {} per-frame avg {:.1} ms max {:.1} ms total {:.0} ms",
        times.len(),
        stamps_total,
        total / times.len().max(1) as f32,
        max,
        total
    );
    println!(
        "paint_end {:.0} ms, {} provinces changed terrain",
        end_ms,
        became.len()
    );
    if let Some(out) = args.get(7) {
        let rgba = &doc.rendered.rgba;
        let cw = (w / 2) as usize;
        let ch = (h / 2) as usize;
        let mut crop = vec![0u8; cw * ch * 4];
        for y in 0..ch {
            let src = ((ch / 2 + y) * w as usize + cw / 2) * 4;
            crop[y * cw * 4..(y + 1) * cw * 4].copy_from_slice(&rgba[src..src + cw * 4]);
        }
        let png = dom6_simple_map_editor::blueprint_editor::encode_png(cw, ch, &crop).unwrap();
        std::fs::write(out, png).unwrap();
    }
}
