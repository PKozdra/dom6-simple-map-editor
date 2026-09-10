use dom6_simple_map_editor::project::{union, PlaneDoc, Project};
use dom6_simple_map_editor::render::{self, Options, Rect};
use dom6_simple_map_editor::textures::TexSet;
use std::path::Path;
use std::time::Instant;

fn ms(t: Instant) -> f32 {
    t.elapsed().as_secs_f32() * 1000.0
}

struct Series {
    name: &'static str,
    samples: Vec<f32>,
}

impl Series {
    fn new(name: &'static str) -> Series {
        Series {
            name,
            samples: Vec::new(),
        }
    }
    fn push(&mut self, v: f32) {
        self.samples.push(v);
    }
    fn report(&self) {
        if self.samples.is_empty() {
            println!("{:<22} no samples", self.name);
            return;
        }
        let total: f32 = self.samples.iter().sum();
        let max = self.samples.iter().cloned().fold(0.0, f32::max);
        println!(
            "{:<22} n {:>4}  avg {:>8.2} ms  max {:>8.2} ms  total {:>8.0} ms",
            self.name,
            self.samples.len(),
            total / self.samples.len() as f32,
            max,
            total
        );
    }
}

fn largest_province(doc: &PlaneDoc) -> u32 {
    let mut best = (1u32, 0u32);
    for (p, &c) in doc.pixel_counts.iter().enumerate().skip(1) {
        if c > best.1 {
            best = (p as u32, c);
        }
    }
    best.0
}

fn selection_pixels(doc: &PlaneDoc, prov: u32, out: &mut [u8]) -> Option<Rect> {
    let r = doc.bbox(prov)?;
    render::selection_rows(&doc.plane(), prov, r, out);
    Some(r)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: paint_bench <map.d6m> [radius] [px per frame] [frames]");
        std::process::exit(2);
    }
    let radius: i32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(10);
    let per_frame: f32 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(60.0);
    let frames: usize = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(40);
    let batched = args.get(5).map(|a| a != "stamps").unwrap_or(true);
    let tex = TexSet::embedded();
    let opts = Options::default();
    let t = Instant::now();
    let mut project = Project::open(Path::new(&args[1]), &tex, &opts).unwrap();
    let doc = &mut project.planes[0];
    let w = doc.d6m.width;
    let h = doc.d6m.height;
    println!(
        "open {}x{} {} provinces {} sprites in {:.0} ms",
        w,
        h,
        doc.province_count(),
        doc.rendered.sprite_count(),
        ms(t)
    );

    let t = Instant::now();
    let b = render::province_bboxes(&doc.plane());
    println!(
        "province_bboxes full scan {:.2} ms ({} entries)",
        ms(t),
        b.len()
    );

    let prov = largest_province(doc);
    let bb = doc.bbox(prov).unwrap();
    println!(
        "target province {} bbox {}x{} ({} px)",
        prov,
        bb.x1 - bb.x0 + 1,
        bb.y1 - bb.y0 + 1,
        doc.pixel_counts[prov as usize]
    );
    let mut overlay = vec![0u8; (w * h * 4) as usize];
    let t = Instant::now();
    selection_pixels(doc, prov, &mut overlay);
    println!("selection overlay full bbox {:.2} ms", ms(t));

    let t = Instant::now();
    let thumb = render::thumbnail(&doc.rendered, opts.decor, 256, None, None);
    println!("thumbnail full {:.2} ms ({}x{})", ms(t), thumb.w, thumb.h);

    let cap = doc.capitals[prov as usize - 1];
    let (sx, sy) = (cap.0 as i32, cap.1 as i32);
    let spacing = (radius as f32 / 2.0).max(1.0);
    let mut paint = Series::new("paint stamps/frame");
    let mut stamps_per_frame = Series::new("stamps per frame");
    let mut sel = Series::new("selection/frame");
    let mut decor = Series::new("decor tick");
    let mut frame = Series::new("frame total");
    doc.paint_begin("Paint area");
    let mut last = (sx, sy);
    let mut carried = 0.0f32;
    let mut pending: Option<Rect> = None;
    for f in 0..frames {
        let target = (
            sx + ((f + 1) as f32 * per_frame) as i32,
            sy + (f as i32 % 7) - 3,
        );
        let tf = Instant::now();
        let mut points = Vec::new();
        if f == 0 {
            points.push(last);
        }
        let dx = (target.0 - last.0) as f32;
        let dy = (target.1 - last.1) as f32;
        let dist = (dx * dx + dy * dy).sqrt();
        let mut d = spacing - carried;
        while d <= dist {
            let k = d / dist;
            points.push((
                (last.0 as f32 + dx * k).round() as i32,
                (last.1 as f32 + dy * k).round() as i32,
            ));
            d += spacing;
        }
        carried = dist - (d - spacing);
        last = target;
        stamps_per_frame.push(points.len() as f32);
        let t = Instant::now();
        let mut acc: Option<Rect> = None;
        if batched {
            acc = doc.paint_many(Some(prov), &points, radius, &tex, &opts);
        } else {
            for &(x, y) in &points {
                let got = doc.paint(prov, x, y, radius, &tex, &opts);
                acc = union(acc, got);
            }
        }
        paint.push(ms(t));
        let t = Instant::now();
        if batched {
            if let Some(r) = acc {
                render::selection_rows(&doc.plane(), prov, r.expand(2, w, h), &mut overlay);
            }
        } else {
            for _ in 0..points.len() {
                selection_pixels(doc, prov, &mut overlay);
            }
        }
        sel.push(ms(t));
        pending = union(pending, acc);
        if f % 4 == 3 {
            if let Some(r) = pending.take() {
                let t = Instant::now();
                doc.refresh_decor(r, &tex, &opts);
                decor.push(ms(t));
            }
        }
        frame.push(ms(tf));
    }
    let t = Instant::now();
    doc.paint_end_follow(false, &tex, &opts);
    let end_ms = ms(t);
    stamps_per_frame.report();
    paint.report();
    sel.report();
    decor.report();
    frame.report();
    println!("paint_end {end_ms:.1} ms");

    let t = Instant::now();
    doc.rerender(&tex, &opts);
    println!("full rerender (scatter) {:.0} ms", ms(t));
    let t = Instant::now();
    doc.rerender_quick(&tex, &opts);
    println!("full rerender quick {:.0} ms", ms(t));
    let t = Instant::now();
    doc.rerender_ground(&tex, &opts);
    println!("full rerender ground only {:.0} ms", ms(t));
    let t = Instant::now();
    doc.refresh_decor(Rect::full(w, h), &tex, &opts);
    println!("refresh_decor full {:.0} ms", ms(t));
    let t = Instant::now();
    doc.set_capitals(false);
    println!("capitals off {:.2} ms", ms(t));
    let t = Instant::now();
    doc.set_capitals(true);
    println!("capitals on {:.2} ms", ms(t));
    let t = Instant::now();
    let thumb = render::thumbnail(&doc.rendered, opts.decor, 256, Some(thumb), Some(bb));
    println!("thumbnail incremental (bbox) {:.2} ms", ms(t));
    let _ = thumb;
    let t = Instant::now();
    let _ = doc.undo_last(&tex, &opts);
    println!("undo stroke {:.0} ms", ms(t));
}
