use dom6_mapgen::generate::generate_with_terrain;
use dom6_mapgen::layouts::{
    cave_layout_blue_acc, cave_layout_blueprint_variant, cave_layout_variant_for_seed,
    layout_blueprint_variant, layout_variant_for_seed, CaveLayout, LAYOUT_SIZE,
};
use dom6_mapgen::{Control, Layout, Options as GenOptions, Sink, Stage};
use dom6_simple_map_editor::project::Project;
use dom6_simple_map_editor::render::{flip_to_top_down, Options};
use dom6_simple_map_editor::textures::TexSet;
use std::path::{Path, PathBuf};

const SEED: u32 = 1;
const WIDTH: i32 = 1536;
const HEIGHT: i32 = 1024;
const THUMB_WIDTH: usize = 192;

const NAMES: [&str; 8] = [
    "standard",
    "circle_world",
    "small_lakes",
    "one_sea",
    "two_seas",
    "twirling_sea",
    "no_mans_land",
    "forbidden_center",
];

const CAVE_NAMES: [&str; 5] = [
    "cave_random",
    "cave_small_caves",
    "cave_one_cave",
    "cave_two_caves",
    "cave_circle_cave",
];

struct Quiet;

impl Sink for Quiet {
    fn stage(&mut self, _stage: Stage, _call: u32, _hash: u64) -> Control {
        Control::Continue
    }
    fn progress(&mut self, _stage: Stage, _done: u32, _total: u32) -> Control {
        Control::Continue
    }
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(rgba).unwrap();
}

fn downscale(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![255u8; dw * dh * 4];
    for y in 0..dh {
        let y0 = y * sh / dh;
        let y1 = (((y + 1) * sh) / dh).max(y0 + 1);
        for x in 0..dw {
            let x0 = x * sw / dw;
            let x1 = (((x + 1) * sw) / dw).max(x0 + 1);
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = (sy * sw + sx) * 4;
                    for c in 0..3 {
                        acc[c] += u32::from(src[p + c]);
                    }
                    n += 1;
                }
            }
            let d = (y * dw + x) * 4;
            for c in 0..3 {
                out[d + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    out
}

fn owner_bytes(owners: &[i16]) -> Vec<u8> {
    let mut out = vec![0u8; owners.len() * 4];
    for (i, &o) in owners.iter().enumerate() {
        out[i * 4] = u8::from(o > 0);
    }
    out
}

fn owned_bounds(flags: &[u8], w: usize, h: usize) -> (usize, usize, usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if flags[(y * w + x) * 4] != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || y0 > y1 {
        (0, 0, w - 1, h - 1)
    } else {
        (x0, y0, x1, y1)
    }
}

fn thumb(out_dir: &Path, name: &str, gen_opts: &GenOptions, plane_index: usize, tex: &TexSet) {
    let opts = Options {
        grey_no_start: true,
        ..Options::default()
    };
    let g = generate_with_terrain(gen_opts, SEED, name, &mut Quiet).unwrap();
    let planes: Vec<(&[u8], &str)> = g
        .planes
        .iter()
        .map(|p| (p.d6m.as_slice(), p.map_text.as_str()))
        .collect();
    let gates: Vec<(u16, u16)> = g.gates.iter().map(|g| (g.surface, g.cave)).collect();
    let mut project =
        Project::from_generated(out_dir.to_path_buf(), name, &planes, &gates, tex, &opts).unwrap();
    let doc = &mut project.planes[plane_index];
    doc.rerender(tex, &opts);
    let r = &doc.rendered;
    let mut composed = r.rgba.clone();
    for (dst, src) in composed.chunks_exact_mut(4).zip(r.decor.chunks_exact(4)) {
        let a = u32::from(src[3]);
        if a == 0 {
            continue;
        }
        for c in 0..3 {
            dst[c] =
                (u32::from(src[c]) + (u32::from(dst[c]) * (255 - a) + 127) / 255).min(255) as u8;
        }
        dst[3] = 255;
    }
    let top = flip_to_top_down(r.w, r.h, &composed);
    let owners = flip_to_top_down(r.w, r.h, &owner_bytes(&doc.d6m.owners));
    let (x0, y0, x1, y1) = owned_bounds(&owners, r.w as usize, r.h as usize);
    let sw = x1 - x0 + 1;
    let sh = y1 - y0 + 1;
    let mut cropped = Vec::with_capacity(sw * sh * 4);
    for y in y0..=y1 {
        let row = (y * r.w as usize + x0) * 4;
        cropped.extend_from_slice(&top[row..row + sw * 4]);
    }
    let top = cropped;
    let dw = THUMB_WIDTH;
    let dh = (sh * dw + sw / 2) / sw;
    let small = downscale(&top, sw, sh, dw, dh);
    let path = out_dir.join(format!("{name}.png"));
    write_png(&path, dw as u32, dh as u32, &small);
    let bytes = std::fs::metadata(&path).unwrap().len();
    println!("{name}: {sw}x{sh} -> {dw}x{dh}, {bytes} bytes");
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("assets/layouts").to_path_buf());
    std::fs::create_dir_all(&out_dir).unwrap();
    let tex = TexSet::embedded();
    for (i, kind) in Layout::ALL.iter().enumerate() {
        let blueprint = layout_blueprint_variant(
            *kind,
            layout_variant_for_seed(*kind, SEED),
            LAYOUT_SIZE,
            LAYOUT_SIZE,
        );
        let gen_opts = GenOptions {
            width: WIDTH,
            height: HEIGHT,
            blueprint,
            ..GenOptions::default()
        };
        thumb(&out_dir, NAMES[i], &gen_opts, 0, &tex);
    }
    for (i, kind) in CaveLayout::ALL.iter().enumerate() {
        let cave_blueprint = cave_layout_blueprint_variant(
            *kind,
            cave_layout_variant_for_seed(*kind, SEED),
            LAYOUT_SIZE,
            LAYOUT_SIZE,
        );
        let gen_opts = GenOptions {
            width: WIDTH,
            height: HEIGHT,
            caves_plane: true,
            cave_blueprint,
            blue_acc: cave_layout_blue_acc(*kind),
            ..GenOptions::default()
        };
        thumb(&out_dir, CAVE_NAMES[i], &gen_opts, 1, &tex);
    }
}
