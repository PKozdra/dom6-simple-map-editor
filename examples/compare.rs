use dom6_simple_map_editor::d6m::D6m;
use dom6_simple_map_editor::mapfile::MapFile;
use dom6_simple_map_editor::render::{Options, Plane, Rendered};
use dom6_simple_map_editor::terrain::{BORDER_CARVED, UNKNOWN};
use dom6_simple_map_editor::textures::{Image, TexSet};
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

fn score(a: &[u8], b: &[u8]) -> (usize, usize, usize) {
    let mut exact = 0;
    let mut near = 0;
    let mut total = 0;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        total += 1;
        let d = (0..3)
            .map(|i| (pa[i] as i32 - pb[i] as i32).abs())
            .max()
            .unwrap();
        if d == 0 && pa[3] == pb[3] {
            exact += 1;
        }
        if d <= 2 {
            near += 1;
        }
    }
    (exact, near, total)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let d6m_path = Path::new(&args[1]);
    let map_path = Path::new(&args[2]);
    let ref_path = Path::new(&args[3]);
    let out_dir = Path::new(args.get(4).map(String::as_str).unwrap_or("."));
    let stages: Vec<String> = args
        .get(5)
        .map(|s| s.split(',').map(String::from).collect())
        .unwrap_or_default();
    let t0 = Instant::now();
    let d6m = D6m::load(d6m_path).unwrap();
    let map = MapFile::load(map_path).unwrap();
    let tex = TexSet::embedded();
    println!("load {:?}", t0.elapsed());
    let n = d6m.provinces.len();
    let mut flags = vec![0u64; n + 1];
    for (i, p) in d6m.provinces.iter().enumerate() {
        flags[i + 1] = *map.terrain.get(&(i as u32 + 1)).unwrap_or(&p.terrain) as u64;
    }
    let capitals: Vec<(i16, i16)> = d6m.provinces.iter().map(|p| (p.x, p.y)).collect();
    let mut rivers = Vec::new();
    for &(a, b) in &map.neighbours {
        if map.spec_between(a, b) as u64 & BORDER_CARVED != 0
            && a as usize <= n
            && b as usize <= n
            && flags[a as usize] & UNKNOWN == 0
            && flags[b as usize] & UNKNOWN == 0
        {
            rivers.push((a, b));
        }
    }
    let heights = d6m.heights_f32();
    let plane = Plane {
        w: d6m.width,
        h: d6m.height,
        heights: &heights,
        owners: &d6m.owners,
        flags: &flags,
        scale: d6m.map_scale(),
        hwrap: map.hwrap,
        vwrap: map.vwrap,
        capitals: &capitals,
        rivers: &rivers,
        mountain_lines: &[],
        bridges: &[],
        cave_plane: false,
    };
    let refimg: Image =
        dom6_simple_map_editor::tga::decode(&std::fs::read(ref_path).unwrap()).unwrap();
    println!(
        "ref {}x{} map {}x{} scale {} rivers {}",
        refimg.w,
        refimg.h,
        d6m.width,
        d6m.height,
        plane.scale,
        rivers.len()
    );
    let mut configs: Vec<(&str, Options)> = vec![
        (
            "plain",
            Options {
                rivers: false,
                borders: false,
                capitals: false,
                edge_fade: false,
                border_percent: 100,
                decor: false,
            },
        ),
        (
            "rivers",
            Options {
                rivers: true,
                borders: false,
                capitals: false,
                edge_fade: false,
                border_percent: 100,
                decor: false,
            },
        ),
        (
            "borders",
            Options {
                rivers: true,
                borders: true,
                capitals: false,
                edge_fade: false,
                border_percent: 100,
                decor: false,
            },
        ),
        ("full", Options::default()),
    ];
    if !stages.is_empty() {
        configs.retain(|(n, _)| stages.iter().any(|s| s == n));
    }
    for (name, opts) in configs {
        let t = Instant::now();
        let r = Rendered::new(&plane, &tex, &opts);
        let dt = t.elapsed();
        let (exact, near, total) = score(&r.rgba, &refimg.rgba);
        let mut flipped = refimg.clone();
        flipped.flip_rows();
        let (fexact, _, _) = score(&r.rgba, &flipped.rgba);
        println!(
            "{name:8} render {dt:?}  exact {:.2}%  within2 {:.2}%  (flipped-ref exact {:.2}%)",
            exact as f64 * 100.0 / total as f64,
            near as f64 * 100.0 / total as f64,
            fexact as f64 * 100.0 / total as f64
        );
        let top = dom6_simple_map_editor::render::flip_to_top_down(r.w, r.h, &r.rgba);
        write_png(
            &out_dir.join(format!("mine_{name}.png")),
            r.w as u32,
            r.h as u32,
            &top,
        );
        let mut diff = vec![0u8; r.rgba.len()];
        for (i, (pa, pb)) in r
            .rgba
            .chunks_exact(4)
            .zip(refimg.rgba.chunks_exact(4))
            .enumerate()
        {
            let d = (0..3)
                .map(|c| (pa[c] as i32 - pb[c] as i32).abs())
                .max()
                .unwrap();
            let v = if d == 0 {
                0
            } else {
                (d * 4 + 60).min(255) as u8
            };
            diff[i * 4] = v;
            diff[i * 4 + 1] = if d == 0 { 0 } else { 40 };
            diff[i * 4 + 2] = 0;
            diff[i * 4 + 3] = 255;
        }
        let dtop = dom6_simple_map_editor::render::flip_to_top_down(r.w, r.h, &diff);
        write_png(
            &out_dir.join(format!("diff_{name}.png")),
            r.w as u32,
            r.h as u32,
            &dtop,
        );
    }
    let rtop = dom6_simple_map_editor::render::flip_to_top_down(
        refimg.w as i32,
        refimg.h as i32,
        &refimg.rgba,
    );
    write_png(
        &out_dir.join("reference.png"),
        refimg.w as u32,
        refimg.h as u32,
        &rtop,
    );
}
