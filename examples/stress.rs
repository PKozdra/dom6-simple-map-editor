use dom6_simple_map_editor::project::{FlagOp, HeightOp, Project};
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::terrain::*;
use dom6_simple_map_editor::textures::TexSet;
use std::path::{Path, PathBuf};

fn copy_map(src: &Path, dst_dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dst_dir).unwrap();
    for e in std::fs::read_dir(dst_dir).unwrap().flatten() {
        std::fs::remove_file(e.path()).ok();
    }
    let base = src.file_stem().unwrap().to_string_lossy().into_owned();
    let src_dir = src.parent().unwrap();
    let mut first = None;
    for e in std::fs::read_dir(src_dir).unwrap().flatten() {
        let fname = e.file_name().to_string_lossy().into_owned();
        if let Some(rest) = fname.strip_prefix(&base) {
            if rest.contains(".bak") {
                continue;
            }
            let out = dst_dir.join(format!("{name}{rest}"));
            std::fs::copy(e.path(), &out).unwrap();
            if rest.ends_with(".map") {
                let text = std::fs::read_to_string(&out).unwrap();
                std::fs::write(
                    &out,
                    text.replace(&format!("#imagefile {base}"), &format!("#imagefile {name}")),
                )
                .unwrap();
            }
            if rest == ".d6m" {
                first = Some(out);
            }
        }
    }
    first.expect("no .d6m beside the source")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: stress <source.d6m> <out dir> <name>");
        std::process::exit(2);
    }
    let src = Path::new(&args[1]);
    let out_dir = Path::new(&args[2]);
    let name = &args[3];
    let d6m = copy_map(src, out_dir, name);
    let tex = TexSet::embedded();
    let opts = Options {
        decor: false,
        ..Options::default()
    };
    let mut project = Project::open(&d6m, &tex, &opts).unwrap();
    let mut log = Vec::new();
    {
        let doc = &mut project.planes[0];
        let n = doc.province_count() as u32;
        let pick = |k: u32| -> u32 { 1 + (k * 7) % n };
        let land: Vec<u32> = (1..=n)
            .filter(|&p| doc.flags[p as usize] & SEA == 0)
            .collect();
        let sea: Vec<u32> = (1..=n)
            .filter(|&p| doc.flags[p as usize] & SEA != 0)
            .collect();
        let l0 = land[0];
        let l1 = land[1 % land.len()];
        let s0 = sea.first().copied().unwrap_or(l1);
        log.push(format!(
            "provinces {n}, land {}, sea {}",
            land.len(),
            sea.len()
        ));
        log.push(format!(
            "sea preset on {l0}: {}",
            doc.apply(
                l0,
                HeightOp::Below(-20.0),
                FlagOp::DeepSea,
                "Deep sea",
                &tex,
                &opts
            )
        ));
        log.push(format!(
            "land preset on {s0}: {}",
            doc.apply(s0, HeightOp::Above(30.0), FlagOp::Land, "Land", &tex, &opts)
        ));
        log.push(format!(
            "flatten {l1}: {}",
            doc.apply(
                l1,
                HeightOp::Flat(12.0),
                FlagOp::Keep,
                "Height",
                &tex,
                &opts
            )
        ));
        log.push(format!(
            "offset {}: {}",
            pick(3),
            doc.apply(
                pick(3),
                HeightOp::Offset(-8.0),
                FlagOp::Keep,
                "Lower",
                &tex,
                &opts
            )
        ));
        let f = doc.flags[pick(4) as usize];
        log.push(format!(
            "flags {}: {}",
            pick(4),
            doc.set_flags(
                pick(4),
                f | FOREST | MOUNTAIN | GOOD_THRONE | MANY_SITES,
                "Terrain",
                &tex,
                &opts
            )
        ));
        let f = doc.flags[pick(5) as usize];
        log.push(format!(
            "flags {}: {}",
            pick(5),
            doc.set_flags(
                pick(5),
                (f | NO_START | CAVE_WALL) & !FOREST,
                "Terrain",
                &tex,
                &opts
            )
        ));
        log.push(format!(
            "name: {}",
            doc.set_name(pick(6), "Stress \"Quoted\" Name", &tex, &opts)
        ));
        log.push(format!(
            "name clear: {}",
            doc.set_name(pick(7), "", &tex, &opts)
        ));
        log.push(format!("gate: {}", doc.set_gate(pick(8), 3, &tex, &opts)));
        let a = pick(9);
        let far = (1..=n).find(|&b| b != a && !doc.linked(a, b)).unwrap();
        log.push(format!(
            "link {a}-{far}: {}",
            doc.set_link(a, far, true, &tex, &opts)
        ));
        log.push(format!(
            "spec river {a}-{far}: {}",
            doc.set_spec(a, far, (BORDER_RIVER | BORDER_BRIDGE) as i64, &tex, &opts)
        ));
        let nb = doc.neighbours(pick(10))[0];
        log.push(format!(
            "spec pass {}-{nb}: {}",
            pick(10),
            doc.set_spec(
                pick(10),
                nb,
                (BORDER_MOUNTAIN_PASS | BORDER_IMPASSABLE) as i64,
                &tex,
                &opts
            )
        ));
        let nb2 = doc.neighbours(pick(11))[0];
        log.push(format!(
            "unlink {}-{nb2}: {}",
            pick(11),
            doc.set_link(pick(11), nb2, false, &tex, &opts)
        ));
        let (cx, cy) = doc.capitals[l0 as usize - 1];
        doc.paint_begin("Paint area");
        let mut painted = 0;
        for k in 0..12 {
            if doc
                .paint(l1, cx as i32 + 40 + k * 4, cy as i32, 15, &tex, &opts)
                .is_some()
            {
                painted += 1;
            }
        }
        doc.paint_end(&tex, &opts);
        log.push(format!("paint steps {painted}"));
        doc.paint_begin("Paint area");
        log.push(format!(
            "paint none: {}",
            doc.paint(0, cx as i32 - 30, cy as i32, 8, &tex, &opts)
                .is_some()
        ));
        doc.paint_end(&tex, &opts);
        doc.paint_begin("Remove area");
        log.push(format!(
            "restore: {}",
            doc.paint_restore(cx as i32 + 60, cy as i32, 10, &tex, &opts)
                .is_some()
        ));
        doc.paint_end(&tex, &opts);
        doc.paint_begin("Height brush");
        for k in 0..10 {
            doc.paint_height(cx as i32 + k * 5, cy as i32 + 30, 20, -12.0, &tex, &opts);
        }
        doc.paint_end(&tex, &opts);
        log.push(format!("scars before {}", doc.scar_count()));
        log.push(format!("repair {}", doc.repair_scars(&tex, &opts)));
        log.push(format!(
            "random terrain {}",
            doc.randomize_terrain(&tex, &opts)
        ));
        log.push(format!(
            "no start {}",
            doc.set_no_starts(3.0, 0.5, &tex, &opts)
        ));
        let np = doc.add_province(cx as i32 + 120, cy as i32 + 80, 12, &tex, &opts);
        log.push(format!("new province {np:?}"));
        if let Some(np) = np {
            doc.paint_begin("Paint area");
            for k in 0..8 {
                doc.paint(np, cx as i32 + 120 + k * 6, cy as i32 + 80, 14, &tex, &opts);
            }
            doc.paint_end(&tex, &opts);
            log.push(format!(
                "new province pixels {}",
                doc.pixel_counts[np as usize]
            ));
            log.push(format!(
                "centre capital {}",
                doc.centre_capital(np, &tex, &opts)
            ));
            log.push(format!("capital inside {}", doc.capital_inside(np)));
            log.push(format!(
                "link new {}",
                doc.set_link(np, l0, true, &tex, &opts)
            ));
            log.push(format!(
                "name new {}",
                doc.set_name(np, "Added", &tex, &opts)
            ));
        }
        log.push(format!(
            "remove province 7: {}",
            doc.remove_province(7, &tex, &opts)
        ));
        log.push(format!(
            "provinces now {}, empty {:?}",
            doc.province_count(),
            doc.empty_provinces()
        ));
        log.push(format!("undo {:?}", doc.undo_last(&tex, &opts)));
        log.push(format!("redo {:?}", doc.redo_last(&tex, &opts)));
    }
    let src_clone = out_dir.join("plane_src");
    let src2 = copy_map(src, &src_clone, "extra");
    let idx = project.add_plane(&src2, &tex, &opts).unwrap();
    log.push(format!("added plane {idx}"));
    {
        let doc = &mut project.planes[idx as usize - 1];
        log.push(format!(
            "plane {idx} random terrain {}",
            doc.randomize_terrain(&tex, &opts)
        ));
        log.push(format!(
            "plane {idx} gate {}",
            doc.set_gate(1, 3, &tex, &opts)
        ));
    }
    let idx2 = project.add_plane(&src2, &tex, &opts).unwrap();
    log.push(format!(
        "added plane {idx2}, removed {:?}",
        project.remove_last_plane().unwrap()
    ));
    let mut written = Vec::new();
    for d in &mut project.planes {
        if d.dirty {
            written.extend(d.save().unwrap());
        }
    }
    log.push(format!("saved {} files", written.len()));
    let reopened = Project::open(&d6m, &tex, &opts).unwrap();
    log.push(format!(
        "reopened {} planes, notes {:?}",
        reopened.planes.len(),
        reopened.notes
    ));
    for (a, b) in project.planes.iter().zip(reopened.planes.iter()) {
        assert_eq!(
            a.d6m.owners, b.d6m.owners,
            "owners differ on plane {}",
            a.index
        );
        assert_eq!(
            a.d6m.heights, b.d6m.heights,
            "heights differ on plane {}",
            a.index
        );
        assert_eq!(a.flags, b.flags, "flags differ on plane {}", a.index);
        assert_eq!(a.names, b.names, "names differ on plane {}", a.index);
        assert_eq!(a.gates, b.gates, "gates differ on plane {}", a.index);
        assert_eq!(
            a.capitals, b.capitals,
            "capitals differ on plane {}",
            a.index
        );
        for p in 1..=a.province_count() as u32 {
            assert_eq!(
                a.neighbours(p),
                b.neighbours(p),
                "links differ at {p} on plane {}",
                a.index
            );
            for q in a.neighbours(p) {
                assert_eq!(
                    a.spec(p, q),
                    b.spec(p, q),
                    "spec differs {p}-{q} on plane {}",
                    a.index
                );
            }
        }
    }
    log.push("round trip identical".to_string());
    for l in &log {
        println!("{l}");
    }
}
