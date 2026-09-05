use dom6_simple_map_editor::d6m::{D6m, Province, STORED_LIMIT};
use dom6_simple_map_editor::project::{FlagOp, HeightOp, Project};
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::terrain::{BORDER_BRIDGE, BORDER_RIVER, DEEP_SEA, SEA};
use dom6_simple_map_editor::textures::{Image, TexSet};
use std::path::{Path, PathBuf};

fn tex() -> TexSet {
    let imgs = dom6_simple_map_editor::textures::ALL
        .iter()
        .enumerate()
        .map(|(i, _)| Image {
            w: 2,
            h: 2,
            rgba: [i as u8 * 3, 10, 20, 255].repeat(4),
        })
        .collect();
    TexSet::from_images(imgs)
}

fn make_map(dir: &Path, name: &str, plane: u32, with_pb: bool) -> (PathBuf, PathBuf) {
    let w = 20;
    let h = 12;
    let mut heights = Vec::new();
    let mut owners = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let id = if x < 10 { 1 } else { 2 };
            owners.push(id as i16);
            heights.push(if id == 1 {
                320 + (y as i16 * 8)
            } else {
                -400 - x as i16
            });
        }
    }
    let d = D6m {
        version: 3,
        width: w,
        height: h,
        passthrough: 0,
        scale_frac: 0,
        scale_int: 30,
        provinces: vec![
            Province {
                x: 4,
                y: 5,
                terrain: 0,
            },
            Province {
                x: 15,
                y: 5,
                terrain: 4,
            },
        ],
        heights,
        owners,
        trailing: Vec::new(),
    };
    let suffix = if plane > 1 {
        format!("_plane{plane}")
    } else {
        String::new()
    };
    let d6m_path = dir.join(format!("{name}{suffix}.d6m"));
    let map_path = dir.join(format!("{name}{suffix}.map"));
    std::fs::write(&d6m_path, d.to_bytes()).unwrap();
    let mut text = format!(
        "#dom2title {name}\n#imagefile {name}{suffix}.d6m\n#mapsize {w} {h}\n\n#landname 1 \"Green Hill\"\n#landname 2 \"Blue Deep\"\n#terrain 1 0\n#terrain 2 4\n#neighbour 1 2\n"
    );
    if with_pb {
        text.push_str("\n-- borders\n#pb 0 0 10 1\n#pb 10 0 10 2\n");
    }
    std::fs::write(&map_path, text).unwrap();
    (d6m_path, map_path)
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d6sme_test_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn opens_all_planes_from_one_file() {
    let dir = temp_dir("planes");
    let (p1, _) = make_map(&dir, "island", 1, false);
    make_map(&dir, "island", 2, false);
    let t = tex();
    let opts = Options::default();
    let proj = Project::open(&p1, &t, &opts).unwrap();
    assert_eq!(proj.planes.len(), 2);
    assert_eq!(proj.planes[1].index, 2);
    assert_eq!(proj.planes[0].name(1), "Green Hill");
    let via_map = Project::open(&dir.join("island_plane2.map"), &t, &opts).unwrap();
    assert_eq!(via_map.base, "island");
    assert_eq!(via_map.planes.len(), 2);
}

#[test]
fn sea_preset_sinks_province_updates_map_and_keeps_backups() {
    let dir = temp_dir("edit");
    let (p1, map_path) = make_map(&dir, "lake", 1, false);
    let original = std::fs::read(&p1).unwrap();
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let before = doc.stats(1);
    assert!(before.min > 0.0);
    assert!(doc.apply(
        1,
        HeightOp::Below(-20.0),
        FlagOp::DeepSea,
        "Deep sea",
        &t,
        &opts
    ));
    let after = doc.stats(1);
    assert!((after.max + 20.0).abs() < 0.07);
    assert!((after.max - after.min - (before.max - before.min)).abs() < 0.07);
    assert_eq!(doc.flags[1] & (SEA | DEEP_SEA), SEA | DEEP_SEA);
    assert!(doc.dirty);
    let written = doc.save().unwrap();
    assert_eq!(written.len(), 2);
    assert!(!doc.dirty);
    let bak = PathBuf::from(format!("{}.bak", p1.display()));
    assert_eq!(std::fs::read(&bak).unwrap(), original);
    let reloaded = D6m::load(&p1).unwrap();
    for (i, &o) in reloaded.owners.iter().enumerate() {
        if o == 1 {
            assert!(reloaded.heights[i] < 0);
        } else {
            assert_eq!(reloaded.heights[i], -400 - (i % 20) as i16);
        }
    }
    assert_eq!(reloaded.provinces[0].terrain as u64, SEA | DEEP_SEA);
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains(&format!("#terrain 1 {}", SEA | DEEP_SEA)));
    assert!(text.contains("#landname 1 \"Green Hill\""));
    assert!(text.contains("#terrain 2 4"));
    let mut proj2 = Project::open(&p1, &t, &opts).unwrap();
    let doc2 = &mut proj2.planes[0];
    assert!(doc2.apply(1, HeightOp::Above(30.0), FlagOp::Land, "Land", &t, &opts));
    assert_eq!(doc2.flags[1] & SEA, 0);
    assert!(doc2.undo_last(&t, &opts).is_some());
    assert_eq!(doc2.flags[1] & SEA, SEA);
    assert!((doc2.stats(1).max + 20.0).abs() < 0.07);
    assert!(doc2.redo_last(&t, &opts).is_some());
    assert!(doc2.stats(1).min >= 30.0 - 0.07);
    doc2.save().unwrap();
    assert!(!PathBuf::from(format!("{}.bak.1", p1.display())).exists());
    assert_eq!(std::fs::read(&bak).unwrap(), original);
}

#[test]
fn flat_and_offset_ops() {
    let dir = temp_dir("ops");
    let (p1, _) = make_map(&dir, "flat", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert!(doc.apply(2, HeightOp::Flat(-5.0), FlagOp::Keep, "Shallows", &t, &opts));
    let s = doc.stats(2);
    assert_eq!(s.min, -5.0);
    assert_eq!(s.max, -5.0);
    assert!(doc.apply(2, HeightOp::Offset(12.5), FlagOp::Keep, "Raise", &t, &opts));
    assert_eq!(doc.stats(2).max, 7.5);
    assert!(!doc.apply(2, HeightOp::Offset(0.0), FlagOp::Keep, "Nothing", &t, &opts));
}

#[test]
fn names_gates_links_and_borders_round_trip_through_the_map_file() {
    let dir = temp_dir("mapedits");
    let (p1, map_path) = make_map(&dir, "links", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert!(doc.set_name(1, "Old Mill", &t, &opts));
    assert!(doc.set_name(2, "", &t, &opts));
    assert!(doc.set_gate(1, 3, &t, &opts));
    assert!(doc.flags[1] & dom6_simple_map_editor::terrain::GATEWAY != 0);
    assert!(doc.set_spec(1, 2, BORDER_RIVER as i64, &t, &opts));
    assert_eq!(doc.rivers, vec![(1, 2)]);
    assert!(doc.set_link(1, 2, false, &t, &opts));
    assert!(doc.neighbours(1).is_empty());
    assert!(doc.rivers.is_empty());
    assert!(doc.set_link(2, 1, true, &t, &opts));
    assert_eq!(doc.neighbours(2), vec![1]);
    doc.save().unwrap();
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains("#landname 1 \"Old Mill\""));
    assert!(!text.contains("#landname 2"));
    assert!(text.contains("#gate 1 3"));
    assert!(text.contains("#neighbour 1 2"));
    assert!(!text.contains("#neighbourspec"));
    for _ in 0..6 {
        assert!(doc.undo_last(&t, &opts).is_some());
    }
    assert_eq!(doc.name(1), "Green Hill");
    assert_eq!(doc.name(2), "Blue Deep");
    assert_eq!(doc.gate(1), 0);
    assert_eq!(doc.neighbours(1), vec![2]);
    assert_eq!(doc.spec(1, 2), 0);
}

#[test]
fn removing_a_river_lifts_its_trench_and_repair_fixes_scars() {
    let dir = temp_dir("trench");
    let (p1, _) = make_map(&dir, "river", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert!(doc.apply(2, HeightOp::Flat(40.0), FlagOp::Land, "Land", &t, &opts));
    assert!(doc.set_spec(1, 2, BORDER_RIVER as i64, &t, &opts));
    let w = doc.width() as usize;
    let mut scarred = Vec::new();
    for y in 0..doc.height() as usize {
        for x in 8..12usize {
            scarred.push(y * w + x);
        }
    }
    for &i in &scarred {
        doc.d6m.heights[i] = -STORED_LIMIT;
        doc.heights[i] = -2000.0;
    }
    assert_eq!(doc.scar_count(), scarred.len());
    assert!(doc.set_spec(1, 2, 0, &t, &opts));
    assert_eq!(doc.scar_count(), 0);
    for &i in &scarred {
        assert!(doc.d6m.heights[i] > 0, "pixel {i} still sunk");
    }
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.scar_count(), scarred.len());
    assert_eq!(doc.repair_scars(&t, &opts), scarred.len());
    assert_eq!(doc.scar_count(), 0);
}

#[test]
fn painting_ownership_rewrites_pb_runs() {
    let dir = temp_dir("paint");
    let (p1, map_path) = make_map(&dir, "paint", 1, true);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert_eq!(doc.pixel_counts[1], 120);
    doc.paint_begin("Paint area");
    assert!(doc.paint(1, 12, 5, 1, &t, &opts).is_some());
    doc.paint_end();
    assert_eq!(doc.owner_at(12, 5), 1);
    assert_eq!(doc.owner_at(13, 5), 1);
    assert_eq!(doc.owner_at(14, 5), 2);
    assert_eq!(doc.owner_at(10, 5), 2);
    assert_eq!(doc.pixel_counts[1], 125);
    assert_eq!(doc.undo.len(), 1);
    doc.save().unwrap();
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains(
        "#pb 0 5 10 1
#pb 10 5 1 2
#pb 11 5 3 1
#pb 14 5 6 2
"
    ));
    assert!(text.contains("#pb 0 0 10 1\n#pb 10 0 10 2\n"));
    assert!(!text.contains("#pb 0 0 10 1\n#pb 10 0 10 2\n#pb 0 0"));
    let reloaded = D6m::load(&p1).unwrap();
    assert_eq!(reloaded.owners[5 * 20 + 12], 1);
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.owner_at(12, 5), 2);
}

#[test]
fn height_brush_random_terrain_and_planes() {
    let dir = temp_dir("extra");
    let (p1, _) = make_map(&dir, "extra", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let before = doc.stats(1);
    doc.paint_begin("Height brush");
    assert!(doc.paint_height(4, 5, 2, 50.0, &t, &opts).is_some());
    assert!(doc.paint_height(4, 5, 2, 50.0, &t, &opts).is_some());
    doc.paint_end();
    assert_eq!(doc.undo.len(), 1);
    let after = doc.stats(1);
    assert!(after.max > before.max + 90.0);
    assert!(doc.undo_last(&t, &opts).is_some());
    let back = doc.stats(1);
    assert!((back.max - before.max).abs() < 0.07);
    assert!((back.min - before.min).abs() < 0.07);
    doc.flags[1] |= (1 << 13) | dom6_simple_map_editor::terrain::MANY_SITES;
    let changed = doc.randomize_terrain(&t, &opts);
    assert!(changed >= 1);
    assert_eq!(doc.flags[1] & (1 << 13), 0);
    assert_ne!(
        doc.flags[1] & dom6_simple_map_editor::terrain::MANY_SITES,
        0
    );
    assert_eq!(doc.flags[2] & SEA, SEA);
    doc.paint_begin("Paint area");
    assert!(doc.paint(1, 12, 5, 1, &t, &opts).is_some());
    doc.paint_end();
    assert_eq!(doc.owner_at(12, 5), 1);
    doc.paint_begin("Remove area");
    assert!(doc.paint_restore(1, 12, 5, 1, &t, &opts).is_some());
    doc.paint_end();
    assert_eq!(doc.owner_at(12, 5), 2);
    assert_eq!(doc.owner_at(9, 5), 1);
    doc.paint_begin("Remove area");
    assert!(doc.paint_restore(1, 9, 5, 0, &t, &opts).is_some());
    doc.paint_end();
    assert_eq!(doc.owner_at(9, 5), 0);
    let src = temp_dir("extra_src");
    let (s1, _) = make_map(&src, "cave", 1, false);
    std::fs::remove_file(src.join("cave.map")).unwrap();
    let n = proj.add_plane(&s1, &t, &opts).unwrap();
    assert_eq!(n, 2);
    assert_eq!(proj.planes.len(), 2);
    let text = std::fs::read_to_string(dir.join("extra_plane2.map")).unwrap();
    assert!(text.contains("#imagefile extra_plane2.d6m"));
    assert!(text.contains("#neighbour 1 2"));
    assert!(text.contains("#terrain 2 4"));
    assert_eq!(proj.planes[1].neighbours(1), vec![2]);
    let reopened = Project::open(&p1, &t, &opts).unwrap();
    assert_eq!(reopened.planes.len(), 2);
    let moved = proj.remove_last_plane().unwrap();
    assert_eq!(moved.len(), 2);
    assert!(!dir.join("extra_plane2.d6m").exists());
    assert!(dir.join("extra_plane2.d6m.removed").exists());
    assert!(proj.remove_last_plane().is_err());
}

#[test]
fn no_start_setter_counts_crossings_fractionally() {
    let dir = temp_dir("nostart");
    let (p1, map_path) = make_map(&dir, "nostart", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert_eq!(doc.connection_score(1, 0.5), 0.0);
    assert_eq!(doc.connection_score(2, 0.5), 0.0);
    doc.flags[2] &= !SEA;
    assert_eq!(doc.connection_score(1, 0.5), 1.0);
    assert!(doc.set_spec(1, 2, (BORDER_RIVER | BORDER_BRIDGE) as i64, &t, &opts));
    assert_eq!(doc.connection_score(1, 0.5), 1.0);
    assert!(doc.set_spec(1, 2, BORDER_RIVER as i64, &t, &opts));
    assert_eq!(doc.connection_score(1, 0.5), 0.5);
    assert_eq!(doc.set_no_starts(0.5, 0.5, &t, &opts), 0);
    doc.flags[2] |= dom6_simple_map_editor::terrain::GOOD_START;
    assert_eq!(doc.set_no_starts(1.0, 0.5, &t, &opts), 2);
    assert_ne!(doc.flags[1] & dom6_simple_map_editor::terrain::NO_START, 0);
    assert_eq!(
        doc.flags[2] & dom6_simple_map_editor::terrain::GOOD_START,
        0
    );
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.flags[1] & dom6_simple_map_editor::terrain::NO_START, 0);
    assert!(doc.redo_last(&t, &opts).is_some());
    doc.save().unwrap();
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains(&format!(
        "#terrain 1 {}",
        dom6_simple_map_editor::terrain::NO_START
    )));
}

#[test]
fn export_writes_an_image_map_beside_the_recipe() {
    let dir = temp_dir("image");
    let (p1, _) = make_map(&dir, "bake", 1, false);
    make_map(&dir, "bake", 2, false);
    let t = tex();
    let opts = Options::default();
    let proj = Project::open(&p1, &t, &opts).unwrap();
    let files = proj.export_image_map().unwrap();
    assert_eq!(files.len(), 4);
    let text = std::fs::read_to_string(dir.join("bake_image.map")).unwrap();
    assert!(text.contains("#imagefile bake_image.tga"));
    assert!(text.contains("#dom2title bake (image)"));
    assert!(text.contains("#pb 0 0 10 1"));
    assert!(text.contains("#terrain 2 4"));
    let text2 = std::fs::read_to_string(dir.join("bake_image_plane2.map")).unwrap();
    assert!(text2.contains("#imagefile bake_image_plane2.tga"));
    let tga = std::fs::read(dir.join("bake_image.tga")).unwrap();
    let img = dom6_simple_map_editor::tga::decode(&tga).unwrap();
    assert_eq!((img.w, img.h), (20, 12));
    let white = img
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] == 255 && p[1] == 255 && p[2] == 255)
        .count();
    assert_eq!(white, 2);
    assert!(img.rgba.chunks_exact(4).all(|p| p[3] == 255));
}
