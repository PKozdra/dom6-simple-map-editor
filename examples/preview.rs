use dom6_simple_map_editor::project::{FlagOp, HeightOp, Project};
use dom6_simple_map_editor::render::{flip_to_top_down, Options};
use dom6_simple_map_editor::terrain::{GOOD_THRONE, MOUNTAIN};
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
    let crop: Option<[i32; 4]> = args.get(3).map(|s| {
        let v: Vec<i32> = s.split(',').map(|t| t.parse().unwrap()).collect();
        [v[0], v[1], v[2], v[3]]
    });
    let t0 = Instant::now();
    let tex = TexSet::embedded();
    let t_tex = t0.elapsed();
    let t_load = Instant::now();
    let d6m_path = if map_path.extension().map(|e| e == "map").unwrap_or(false) {
        map_path.with_extension("d6m")
    } else {
        map_path.to_path_buf()
    };
    let raw = dom6_simple_map_editor::d6m::D6m::load(&d6m_path).unwrap();
    let t_load = t_load.elapsed();
    let plain = Options {
        decor: false,
        ..Options::default()
    };
    let t1 = Instant::now();
    let mut project = Project::open(map_path, &tex, &plain).unwrap();
    let t_open = t1.elapsed();
    let opts = Options::default();
    if args.get(4).map(|a| a == "repair").unwrap_or(false) {
        let n = project.planes[0].repair_scars(&tex, &plain);
        println!("repaired {n} scar pixels");
    }
    let t2 = Instant::now();
    project.planes[0].rerender(&tex, &opts);
    let t_decor = t2.elapsed();
    println!(
        "d6m load {:?} ({} px), ground render {:?}, ground plus sprites {:?}",
        t_load,
        raw.width * raw.height,
        t_open,
        t_decor
    );
    let doc = &mut project.planes[0];
    let prov = (doc.province_count() / 2).max(1) as u32;
    let flags = doc.flags[prov as usize];
    let (cx, cy) = doc.capitals[prov as usize - 1];
    let mut bench =
        |label: &str, f: &mut dyn FnMut(&mut dom6_simple_map_editor::project::PlaneDoc)| {
            let t = Instant::now();
            f(doc);
            let touched = doc.rendered.touched;
            println!(
                "edit {label:<28} {:>9.1?}  touched {}x{} px",
                t.elapsed(),
                touched.x1 - touched.x0 + 1,
                touched.y1 - touched.y0 + 1
            );
        };
    bench("throne flag on", &mut |d| {
        d.set_flags(prov, flags | GOOD_THRONE, "Terrain", &tex, &opts);
    });
    bench("undo", &mut |d| {
        d.undo_last(&tex, &opts);
    });
    bench("mountain flag on", &mut |d| {
        d.set_flags(prov, flags | MOUNTAIN, "Terrain", &tex, &opts);
    });
    bench("sea preset", &mut |d| {
        d.apply(
            prov,
            HeightOp::Below(-20.0),
            FlagOp::DeepSea,
            "Deep sea",
            &tex,
            &opts,
        );
    });
    bench("land preset", &mut |d| {
        d.apply(
            prov,
            HeightOp::Above(30.0),
            FlagOp::Land,
            "Land",
            &tex,
            &opts,
        );
    });
    bench("paint stroke step r=10", &mut |d| {
        d.paint_begin("Paint area");
        d.paint(prov, cx as i32 + 30, cy as i32, 10, &tex, &opts);
        d.paint_end(&tex, &opts);
    });
    bench("height brush step r=10", &mut |d| {
        d.paint_begin("Height brush");
        d.paint_height(cx as i32, cy as i32, 10, 10.0, &tex, &opts);
    });
    bench("height stroke 20 steps r=30", &mut |d| {
        for k in 0..20 {
            d.paint_height(cx as i32 + k * 6, cy as i32, 30, 10.0, &tex, &opts);
        }
    });
    bench("height stroke end", &mut |d| {
        d.paint_end(&tex, &opts);
    });
    bench("random terrain (plane)", &mut |d| {
        d.randomize_terrain(&tex, &opts);
    });
    bench("undo random terrain", &mut |d| {
        d.undo_last(&tex, &opts);
    });
    let no_borders = Options {
        borders: false,
        decor: false,
        ..Options::default()
    };
    bench("full ground, no borders", &mut |d| {
        d.rerender(&tex, &no_borders);
    });
    let no_decor = Options {
        decor: false,
        ..Options::default()
    };
    bench("full ground with borders", &mut |d| {
        d.rerender(&tex, &no_decor);
    });
    bench("full ground plus sprites", &mut |d| {
        d.rerender(&tex, &opts);
    });
    let doc = &project.planes[0];
    let r = &doc.rendered;
    let w = r.w as usize;
    let h = r.h as usize;
    let mut composed = r.rgba.clone();
    for (dst, src) in composed.chunks_exact_mut(4).zip(r.decor.chunks_exact(4)) {
        let a = src[3] as u32;
        if a == 0 {
            continue;
        }
        for c in 0..3 {
            dst[c] = (src[c] as u32 + (dst[c] as u32 * (255 - a) + 127) / 255).min(255) as u8;
        }
        dst[3] = 255;
    }
    let top = flip_to_top_down(r.w, r.h, &composed);
    let (x0, y0, cw, ch) = match crop {
        Some([x, y, cw, ch]) => (x as usize, y as usize, cw as usize, ch as usize),
        None => (0, 0, w, h),
    };
    let mut cut = vec![0u8; cw * ch * 4];
    for y in 0..ch {
        let src = ((y0 + y) * w + x0) * 4;
        cut[y * cw * 4..(y + 1) * cw * 4].copy_from_slice(&top[src..src + cw * 4]);
    }
    write_png(out, cw as u32, ch as u32, &cut);
    println!(
        "textures {:?} open+render {:?} size {}x{} provinces {} sprites {} scars {}",
        t_tex,
        t_open,
        w,
        h,
        doc.province_count(),
        r.sprite_count(),
        doc.scar_count()
    );
}
