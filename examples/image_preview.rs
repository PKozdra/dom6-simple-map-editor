use dom6_simple_map_editor::project::Project;
use dom6_simple_map_editor::render::{flip_to_top_down, Options};
use dom6_simple_map_editor::textures::TexSet;
use std::path::Path;
use std::time::Instant;

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(rgba).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map_path = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let has = |k: &str| args.iter().skip(3).any(|a| a == k);
    let opts = Options {
        winter: has("winter"),
        all_looks: has("looks"),
        borders: !has("noborders"),
        ..Options::default()
    };
    let tex = TexSet::embedded();
    let t = Instant::now();
    let mut project = Project::open(map_path, &tex, &opts).unwrap();
    if has("touch") {
        let d = &mut project.planes[0];
        let f = d.flags[1];
        d.set_flags(1, f ^ 0x80, "Terrain", &tex, &opts);
        let (cx, cy) = d.capital(2).unwrap();
        d.paint_begin("Paint");
        d.paint(1, cx, cy, 6, &tex, &opts);
        d.paint_end(&tex, &opts);
        println!("saved {:?}", d.save().unwrap());
    }
    println!("open {:?} notes {:?}", t.elapsed(), project.notes);
    for d in &project.planes {
        let inside = (1..=d.province_count() as u32)
            .filter(|&p| d.capital_inside(p))
            .count();
        println!(
            "plane {} image {} {}x{} provinces {} capitals inside own area {} empty {}",
            d.index,
            d.is_image(),
            d.width(),
            d.height(),
            d.province_count(),
            inside,
            d.empty_provinces().len()
        );
    }
    let d = &project.planes[0];
    let rgba = d.rendered.composed(&[]);
    let top = flip_to_top_down(d.width(), d.height(), &rgba);
    write_png(out, d.width() as u32, d.height() as u32, &top);
}
