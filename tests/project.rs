use dom6_simple_map_editor::d6m::{D6m, Province, STORED_LIMIT};
use dom6_simple_map_editor::project::{wrap_delta, wrap_point, FlagOp, HeightOp, Project};
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::terrain::{BORDER_BRIDGE, BORDER_RIVER, DEEP_SEA, NO_START, SEA};
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
    assert!(!PathBuf::from(format!("{}.bak", p1.display())).exists());
    assert_ne!(std::fs::read(&p1).unwrap(), original);
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
    assert!(!PathBuf::from(format!("{}.bak", p1.display())).exists());
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
    doc.paint_end(&t, &opts);
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
    doc.paint_end(&t, &opts);
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
    doc.paint_end(&t, &opts);
    assert_eq!(doc.owner_at(12, 5), 1);
    doc.paint_begin("Remove area");
    assert!(doc.paint_restore(12, 5, 1, &t, &opts).is_some());
    doc.paint_end(&t, &opts);
    assert_eq!(doc.owner_at(12, 5), 2);
    assert_eq!(doc.owner_at(9, 5), 1);
    doc.paint_begin("Remove area");
    assert!(doc.paint_restore(9, 5, 0, &t, &opts).is_none());
    doc.paint_end(&t, &opts);
    assert_eq!(doc.owner_at(9, 5), 1);
    let steps = doc.undo.len();
    assert!(steps >= 2);
    assert_eq!(doc.undo_all(&t, &opts), steps);
    assert!(doc.undo.is_empty());
    assert_eq!(doc.redo.len(), steps);
    assert_eq!(doc.owner_at(12, 5), 2);
    assert_eq!(doc.flags[1] & (1 << 13), 1 << 13);
    while doc.redo_last(&t, &opts).is_some() {}
    assert_eq!(doc.flags[1] & (1 << 13), 0);
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
    assert!(doc.set_spec(1, 2, BORDER_BRIDGE as i64, &t, &opts));
    assert_eq!(doc.connection_score(1, 0.5), 1.0);
    assert!(doc.set_spec(1, 2, (BORDER_RIVER | BORDER_BRIDGE) as i64, &t, &opts));
    assert_eq!(doc.connection_score(1, 0.5), 0.5);
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
fn provinces_can_be_added_and_capitals_moved() {
    let dir = temp_dir("addprov");
    let (p1, map_path) = make_map(&dir, "addprov", 1, true);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert_eq!(doc.province_count(), 2);
    assert!(doc.capital_inside(1));
    let p = doc.add_province(10, 6, 2, &t, &opts).unwrap();
    assert_eq!(p, 3);
    assert_eq!(doc.province_count(), 3);
    assert_eq!(doc.owner_at(10, 6), 3);
    assert!(doc.capital_inside(3));
    assert_eq!(doc.capital(3), Some((10, 6)));
    assert!(doc.pixel_counts[3] > 1);
    assert!(doc.set_flags(
        3,
        dom6_simple_map_editor::terrain::FOREST,
        "Terrain",
        &t,
        &opts
    ));
    assert!(doc.set_name(3, "Newland", &t, &opts));
    assert!(doc.set_link(3, 1, true, &t, &opts));
    assert!(!doc.set_capital(3, 0, 0, &t, &opts));
    assert!(doc.set_capital(3, 11, 6, &t, &opts));
    assert_eq!(doc.capital(3), Some((11, 6)));
    assert!(doc.centre_capital(3, &t, &opts));
    assert_eq!(doc.capital(3), Some((10, 6)));
    for _ in 0..6 {
        assert!(doc.undo_last(&t, &opts).is_some());
    }
    assert_eq!(doc.province_count(), 2);
    assert_eq!(doc.owner_at(10, 6), 2);
    assert!(doc.neighbours(1).iter().all(|&n| n != 3));
    assert_eq!(doc.flags.len(), 3);
    for _ in 0..6 {
        assert!(doc.redo_last(&t, &opts).is_some());
    }
    assert_eq!(doc.province_count(), 3);
    assert_eq!(doc.name(3), "Newland");
    assert!(doc.linked(1, 3));
    let files = doc.save().unwrap();
    assert_eq!(files.len(), 2);
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains(&format!(
        "#terrain 3 {}",
        dom6_simple_map_editor::terrain::FOREST
    )));
    assert!(text.contains("#landname 3 \"Newland\""));
    assert!(text.contains("#neighbour 1 3"));
    let again = Project::open(&p1, &t, &opts).unwrap();
    let d = &again.planes[0];
    assert_eq!(d.province_count(), 3);
    assert_eq!(d.capital(3), Some((10, 6)));
    assert_eq!(d.owner_at(10, 6), 3);
}

#[test]
fn saving_warns_about_an_empty_province_but_still_writes() {
    let dir = temp_dir("emptyprov");
    let (p1, _) = make_map(&dir, "emptyprov", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let p = doc.add_province(5, 5, 1, &t, &opts).unwrap();
    doc.paint_begin("Paint area");
    assert!(doc.paint(1, 5, 5, 3, &t, &opts).is_some());
    doc.paint_end(&t, &opts);
    assert_eq!(doc.pixel_counts[p as usize], 0);
    assert_eq!(
        doc.area_warning().as_deref(),
        Some(format!("province {p} has no area").as_str())
    );
    assert!(doc.save().is_ok());
    assert!(doc.undo_last(&t, &opts).is_some());
    assert!(doc.undo_last(&t, &opts).is_some());
    assert!(doc.area_warning().is_none());
    assert!(doc.save().is_ok());
}

#[test]
fn removing_a_province_fills_and_renumbers() {
    let dir = temp_dir("rmprov");
    let (p1, map_path) = make_map(&dir, "rmprov", 1, true);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let p = doc.add_province(10, 6, 2, &t, &opts).unwrap();
    assert_eq!(p, 3);
    assert!(doc.set_name(3, "Gone", &t, &opts));
    assert!(doc.set_link(3, 1, true, &t, &opts));
    assert!(doc.set_spec(3, 1, BORDER_RIVER as i64, &t, &opts));
    let before = doc.d6m.owners.clone();
    assert!(doc.remove_province(3, &t, &opts));
    assert_eq!(doc.province_count(), 2);
    assert!(doc.d6m.owners.iter().all(|&o| o == 1 || o == 2));
    assert_eq!(doc.owner_at(8, 6), 1);
    assert_eq!(doc.owner_at(12, 6), 2);
    assert!(doc.neighbours(1).iter().all(|&n| n != 3));
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.province_count(), 3);
    assert_eq!(doc.d6m.owners, before);
    assert_eq!(doc.name(3), "Gone");
    assert!(doc.linked(1, 3));
    assert_eq!(doc.spec(1, 3), BORDER_RIVER as i64);
    assert!(doc.redo_last(&t, &opts).is_some());
    assert!(doc.set_name(2, "Second", &t, &opts));
    assert!(doc.remove_province(1, &t, &opts));
    assert_eq!(doc.province_count(), 1);
    assert!(doc.d6m.owners.iter().all(|&o| o == 1));
    assert_eq!(doc.name(1), "Second");
    assert_eq!(doc.flags[1] & SEA, SEA);
    assert!(doc.save().is_ok());
    let text = std::fs::read_to_string(&map_path).unwrap();
    assert!(text.contains("#terrain 1 4"));
    assert!(text.contains("#landname 1 \"Second\""));
    assert!(!text.contains("#terrain 2 "));
    assert!(!text.contains("#neighbour"));
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.province_count(), 2);
    assert_eq!(doc.name(2), "Second");
    assert_eq!(doc.owner_at(2, 2), 1);
    assert!(!doc.remove_province(9, &t, &opts));
}

#[test]
fn generated_bytes_open_as_an_unsaved_project_and_save_where_retargeted() {
    let mut opts = dom6_mapgen::Options {
        width: 512,
        height: 512,
        provinces: 12,
        ..dom6_mapgen::Options::default()
    };
    opts.cave_world = false;
    let g = dom6_mapgen::generate::generate_with_terrain(
        &opts,
        3,
        "random_3",
        &mut dom6_mapgen::stage::NoSink,
    )
    .unwrap();
    let plane = &g.planes[0];
    let t = tex();
    let ropts = Options::default();
    let mut proj = Project::from_generated(
        PathBuf::from("."),
        "random_3",
        &[(plane.d6m.as_slice(), plane.map_text.as_str())],
        &[],
        &t,
        &ropts,
    )
    .unwrap();
    assert!(proj.unsaved);
    assert_eq!(proj.base, "random_3");
    assert_eq!(proj.planes.len(), 1);
    assert_eq!(proj.planes[0].width(), plane.width);
    assert_eq!(proj.planes[0].height(), plane.height);
    assert_eq!(proj.planes[0].province_count(), plane.provinces.len() - 1);
    assert!(proj.any_dirty());

    let dir = temp_dir("generated");
    proj.retarget(dir.clone(), "isle");
    assert!(!proj.unsaved);
    let written = proj.planes[0].save().unwrap();
    assert_eq!(written.len(), 2);
    assert!(dir.join("isle.d6m").exists());
    assert!(dir.join("isle.map").exists());
    let text = std::fs::read_to_string(dir.join("isle.map")).unwrap();
    assert!(text.contains("#imagefile isle.d6m"));
    assert!(text.contains("#dom2title isle"));

    let reopened = Project::open(&dir.join("isle.d6m"), &t, &ropts).unwrap();
    assert!(!reopened.unsaved);
    assert_eq!(
        reopened.planes[0].province_count(),
        proj.planes[0].province_count()
    );
    assert_eq!(
        std::fs::read(dir.join("isle.d6m")).unwrap().len(),
        plane.d6m.len()
    );
}

#[test]
fn a_new_game_map_opens_with_its_caves_plane_and_gateways_and_survives_a_save() {
    let opts = dom6_mapgen::Options {
        width: 512,
        height: 512,
        caves_plane: true,
        ..dom6_mapgen::Options::default()
    };
    let g = dom6_mapgen::generate::generate_new_game(
        &opts,
        11,
        2,
        10,
        "random_11",
        &mut dom6_mapgen::stage::NoSink,
    )
    .unwrap();
    assert_eq!(g.planes.len(), 2);
    assert!(!g.gates.is_empty());
    let planes: Vec<(&[u8], &str)> = g
        .planes
        .iter()
        .map(|p| (p.d6m.as_slice(), p.map_text.as_str()))
        .collect();
    let gates: Vec<(u16, u16)> = g.gates.iter().map(|g| (g.surface, g.cave)).collect();
    let t = tex();
    let ropts = Options::default();
    let dir = temp_dir("newgame");
    let mut proj =
        Project::from_generated(dir.clone(), "random_11", &planes, &gates, &t, &ropts).unwrap();
    assert_eq!(proj.planes.len(), 2);
    assert_eq!(proj.planes[1].index, 2);
    assert_eq!(proj.planes[0].height(), g.planes[0].height);
    assert_eq!(proj.planes[1].height(), g.planes[1].height);
    for (n, (surface, cave)) in gates.iter().enumerate() {
        let n = n as i32 + 1;
        assert_eq!(proj.planes[0].gate(*surface as u32), n);
        assert_eq!(proj.planes[1].gate(*cave as u32), n);
    }

    proj.retarget(dir.clone(), "under");
    for d in &mut proj.planes {
        d.save().unwrap();
    }
    assert!(dir.join("under.d6m").exists());
    assert!(dir.join("under_plane2.d6m").exists());
    assert!(dir.join("under_plane2.map").exists());

    let reopened = Project::open(&dir.join("under.d6m"), &t, &ropts).unwrap();
    assert_eq!(reopened.planes.len(), 2);
    assert_eq!(
        reopened.planes[1].province_count(),
        proj.planes[1].province_count()
    );
    let (s0, c0) = gates[0];
    assert_eq!(reopened.planes[0].gate(s0 as u32), 1);
    assert_eq!(reopened.planes[1].gate(c0 as u32), 1);
}

#[test]
fn a_generated_document_is_dirty_but_not_edited_until_a_real_change() {
    let opts = dom6_mapgen::Options {
        width: 512,
        height: 512,
        provinces: 12,
        ..dom6_mapgen::Options::default()
    };
    let g = dom6_mapgen::generate::generate_with_terrain(
        &opts,
        4,
        "random_4",
        &mut dom6_mapgen::stage::NoSink,
    )
    .unwrap();
    let plane = &g.planes[0];
    let t = tex();
    let ropts = Options::default();
    let dir = temp_dir("edited");
    let mut proj = Project::from_generated(
        dir.clone(),
        "random_4",
        &[(plane.d6m.as_slice(), plane.map_text.as_str())],
        &[],
        &t,
        &ropts,
    )
    .unwrap();
    assert!(proj.any_dirty());
    assert!(!proj.planes.iter().any(|d| d.edited));
    assert!(proj.planes[0].apply(
        1,
        HeightOp::Flat(-5.0),
        FlagOp::Keep,
        "Shallows",
        &t,
        &ropts
    ));
    assert!(proj.planes.iter().any(|d| d.edited));
    proj.planes[0].save().unwrap();
    assert!(!proj.planes.iter().any(|d| d.edited));
}

#[test]
fn gateway_jump_cycles_across_planes() {
    let dir = temp_dir("gatejump");
    let (p1, _) = make_map(&dir, "gates", 1, false);
    make_map(&dir, "gates", 2, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    assert!(proj.planes[0].set_gate(1, 3, &t, &opts));
    assert!(proj.planes[0].set_gate(2, 3, &t, &opts));
    assert!(proj.planes[1].set_gate(1, 3, &t, &opts));
    assert!(proj.planes[1].set_gate(2, 7, &t, &opts));
    assert_eq!(proj.gateway_ring(3), vec![(0, 1), (0, 2), (1, 1)]);
    assert_eq!(proj.next_gateway(0, 1), Some((0, 2)));
    assert_eq!(proj.next_gateway(0, 2), Some((1, 1)));
    assert_eq!(proj.next_gateway(1, 1), Some((0, 1)));
    assert_eq!(proj.next_gateway(1, 2), None);
    assert!(proj.gateway_ring(0).is_empty());
    assert_eq!(proj.next_gateway(0, 1), proj.next_gateway(0, 1));
    assert!(proj.planes[0].set_gate(2, 0, &t, &opts));
    assert_eq!(proj.next_gateway(0, 1), Some((1, 1)));
    assert!(proj.planes[1].set_gate(1, 0, &t, &opts));
    assert_eq!(proj.next_gateway(0, 1), None);
    assert_eq!(proj.next_gateway(5, 1), None);
}

#[test]
fn wrapped_pointer_coordinates_fold_back_onto_the_map() {
    assert_eq!(wrap_point(-1, -1, 100, 50, true, true), (99, 49));
    assert_eq!(wrap_point(250, 130, 100, 50, true, true), (50, 30));
    assert_eq!(wrap_point(-1, -1, 100, 50, false, false), (-1, -1));
    assert_eq!(wrap_point(-1, 60, 100, 50, true, false), (99, 60));
    assert_eq!(wrap_point(120, -3, 100, 50, false, true), (120, 47));
    assert_eq!(wrap_point(7, 9, 0, 0, true, true), (7, 9));
    assert_eq!(wrap_delta(-98.0, 100.0, true), 2.0);
    assert_eq!(wrap_delta(60.0, 100.0, true), -40.0);
    assert_eq!(wrap_delta(-98.0, 100.0, false), -98.0);
}

#[test]
fn renaming_a_saved_map_moves_its_files_and_title() {
    let dir = temp_dir("rename");
    let (p1, _) = make_map(&dir, "oldname", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    proj.rename("newname");
    assert_eq!(proj.base, "newname");
    assert_eq!(proj.planes[0].d6m_path, dir.join("newname.d6m"));
    assert_eq!(proj.planes[0].map_path, Some(dir.join("newname.map")));
    proj.planes[0].save().unwrap();
    let text = std::fs::read_to_string(dir.join("newname.map")).unwrap();
    assert!(text.contains("#dom2title newname"));
    assert!(text.contains("#imagefile newname.d6m"));
    proj.rename("");
    assert_eq!(proj.base, "newname");
}

#[test]
fn removing_a_coastal_province_keeps_sea_pixels_in_the_sea_and_baseline_in_step() {
    let dir = temp_dir("rmcoast");
    let w = 20;
    let h = 12;
    let mut heights = Vec::new();
    let mut owners = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let id: i16 = if x < 7 {
                1
            } else if x < 13 {
                2
            } else {
                3
            };
            owners.push(id);
            let wet = id == 3 || (id == 2 && y >= 6);
            heights.push(if wet { -300 } else { 200 });
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
                x: 3,
                y: 5,
                terrain: 0,
            },
            Province {
                x: 10,
                y: 3,
                terrain: 0,
            },
            Province {
                x: 16,
                y: 5,
                terrain: 4,
            },
        ],
        heights,
        owners,
        trailing: Vec::new(),
    };
    let d6m_path = dir.join("rmcoast.d6m");
    std::fs::write(&d6m_path, d.to_bytes()).unwrap();
    std::fs::write(
        dir.join("rmcoast.map"),
        "#dom2title rmcoast\n#imagefile rmcoast.d6m\n#mapsize 20 12\n#terrain 1 0\n#terrain 2 0\n#terrain 3 4\n#neighbour 1 2\n#neighbour 2 3\n",
    )
    .unwrap();
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&d6m_path, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert!(doc.remove_province(2, &t, &opts));
    for y in 0..h {
        for x in 7..13 {
            let want = if y >= 6 { 2 } else { 1 };
            assert_eq!(doc.owner_at(x, y), want, "pixel {x},{y}");
            assert_eq!(
                doc.baseline[(y * w + x) as usize],
                want as i16,
                "baseline {x},{y}"
            );
        }
    }
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.owner_at(10, 8), 2);
    assert_eq!(doc.baseline[(8 * w + 10) as usize], 2);
}

#[test]
fn a_height_stroke_can_carry_the_sea_marks_with_it() {
    let dir = temp_dir("follow");
    let (p1, _) = make_map(&dir, "follow", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert_eq!(doc.flags[1] & SEA, 0);
    doc.paint_begin("Height brush");
    doc.paint_height_stamps(&[(5, 6, -900.0)], 40, true, &t, &opts);
    let (_, became) = doc.paint_end_follow(true, &t, &opts);
    assert!(became
        .iter()
        .any(|&(p, f)| p == 1 && f & (SEA | DEEP_SEA) == SEA | DEEP_SEA));
    assert!(became
        .iter()
        .any(|&(p, f)| p == 2 && f & DEEP_SEA == DEEP_SEA));
    assert_eq!(doc.flags[1] & SEA, SEA);
    assert_eq!(doc.flags[1] & DEEP_SEA, DEEP_SEA);
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.flags[1] & SEA, 0);
    assert_eq!(doc.flags[2] & DEEP_SEA, 0);
    assert!(doc.redo_last(&t, &opts).is_some());
    assert_eq!(doc.flags[1] & DEEP_SEA, DEEP_SEA);
    doc.paint_begin("Height brush");
    doc.paint_height_stamps(&[(15, 6, 900.0)], 40, true, &t, &opts);
    let (_, became) = doc.paint_end_follow(true, &t, &opts);
    assert!(became.iter().any(|&(p, f)| p == 2 && f & SEA == 0));
    assert_eq!(doc.flags[2] & SEA, 0);
}

#[test]
fn no_start_marks_can_be_cleared_for_the_whole_plane() {
    let dir = temp_dir("clearns");
    let (p1, _) = make_map(&dir, "clearns", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let f = doc.flags[1];
    assert!(doc.set_flags(1, f | NO_START, "ns", &t, &opts));
    assert_eq!(doc.clear_no_starts(&t, &opts), 1);
    assert_eq!(doc.flags[1] & NO_START, 0);
    assert_eq!(doc.clear_no_starts(&t, &opts), 0);
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.flags[1] & NO_START, NO_START);
}

#[test]
fn nation_starts_land_on_fitting_provinces_and_reach_the_map_file() {
    use dom6_simple_map_editor::starts::{place, Want};
    use dom6_simple_map_editor::terrain::{CAVE, GOOD_START};
    let opts = dom6_mapgen::Options {
        width: 512,
        height: 512,
        caves_plane: true,
        ..dom6_mapgen::Options::default()
    };
    let g = dom6_mapgen::generate::generate_new_game(
        &opts,
        11,
        4,
        10,
        "random_11",
        &mut dom6_mapgen::stage::NoSink,
    )
    .unwrap();
    let planes: Vec<(&[u8], &str)> = g
        .planes
        .iter()
        .map(|p| (p.d6m.as_slice(), p.map_text.as_str()))
        .collect();
    let gates: Vec<(u16, u16)> = g.gates.iter().map(|g| (g.surface, g.cave)).collect();
    let t = tex();
    let ropts = Options::default();
    let dir = temp_dir("starts");
    let mut proj =
        Project::from_generated(dir.clone(), "random_11", &planes, &gates, &t, &ropts).unwrap();
    let wants = vec![
        Want {
            nation: 43,
            uw: true,
            coast: false,
            cave: 0,
            likesterr: DEEP_SEA,
        },
        Want {
            nation: 15,
            uw: false,
            coast: false,
            cave: 2,
            likesterr: 0,
        },
        Want {
            nation: 5,
            uw: false,
            coast: false,
            cave: 0,
            likesterr: 0,
        },
        Want {
            nation: 29,
            uw: false,
            coast: true,
            cave: 0,
            likesterr: 0,
        },
    ];
    let (graph, links) = proj.graph();
    assert_eq!(graph.len(), 2);
    assert!(!links.is_empty());
    let placed = place(&graph, &links, &wants, 11);
    assert_eq!(placed.len(), 4);
    let n = proj.apply_starts(&placed, false, &t, &ropts);
    assert_eq!(n, 4);
    for p in &placed {
        let d = &proj.planes[p.plane];
        let f = d.flags[p.prov as usize];
        assert!(f & GOOD_START != 0);
        assert!(f & NO_START == 0);
        match p.nation {
            43 => assert!(f & SEA != 0),
            15 => assert!(p.plane == 1 || f & CAVE != 0),
            _ => assert!(f & SEA == 0),
        }
    }
    assert_eq!(proj.specstarts().len(), 4);
    assert_eq!(proj.start_nation(placed[0].plane, placed[0].prov), Some(43));
    let text = proj.planes[0].map.as_ref().unwrap().to_text();
    assert_eq!(text.matches("#specstart ").count(), 4);
    let cave = placed.iter().find(|p| p.nation == 15).unwrap();
    let global = proj.plane_offset(cave.plane) + cave.prov;
    assert!(text.contains(&format!("#specstart 15 {global}")));
    assert!(global > proj.planes[0].province_count() as u32);
    proj.apply_starts(&placed[..2], true, &t, &ropts);
    let text = proj.planes[0].map.as_ref().unwrap().to_text();
    assert_eq!(text.matches("#specstart ").count(), 0);
    assert_eq!(text.matches("#start ").count(), 2);
    assert_eq!(proj.generic_starts().len(), 2);
    let dropped = placed[3];
    assert!(proj.planes[dropped.plane].flags[dropped.prov as usize] & GOOD_START == 0);
    for d in &mut proj.planes {
        assert!(d.save().is_ok());
    }
    let reopened = Project::open(&dir.join("random_11.d6m"), &t, &ropts).unwrap();
    assert_eq!(reopened.generic_starts().len(), 2);
}

#[test]
fn generator_settings_survive_save_and_reopen() {
    use dom6_simple_map_editor::gen_settings;
    use dom6_simple_map_editor::generator_panel::Form;
    let dir = temp_dir("gensettings");
    let (p1, map_path) = make_map(&dir, "gensettings", 1, true);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    assert!(proj.generator_settings().is_empty());
    let form = Form {
        seed: 77,
        ..Form::default()
    };
    proj.set_generator_settings(&gen_settings::encode(&form));
    proj.planes[0].save().unwrap();
    let text = std::fs::read_to_string(&map_path).unwrap();
    let title = text.find("#dom2title").unwrap();
    let block = text.find("-- gen.seed 77").unwrap();
    assert!(block > title);
    assert!(block < text.find("#terrain").unwrap());
    let reopened = Project::open(&p1, &t, &opts).unwrap();
    let restored = gen_settings::decode(&reopened.generator_settings()).unwrap();
    assert_eq!(restored.form.seed, 77);
    assert!(restored.form.manual_seed);
}

#[test]
fn isolated_provinces_get_linked_to_the_province_they_touch() {
    let dir = temp_dir("isolated");
    let (p1, map_path) = make_map(&dir, "isolated", 1, false);
    let text = std::fs::read_to_string(&map_path).unwrap();
    std::fs::write(
        &map_path,
        text.replace(
            "#neighbour 1 2
",
            "",
        ),
    )
    .unwrap();
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    assert!(doc.neighbours(1).is_empty());
    assert_eq!(doc.raster_neighbours(1), vec![(2, 12)]);
    let linked = doc.link_isolated(&t, &opts);
    assert_eq!(linked, vec![(1, 2)]);
    assert_eq!(doc.neighbours(1), vec![2]);
    assert!(doc.link_isolated(&t, &opts).is_empty());
}

#[test]
fn bounding_boxes_stay_exact_through_strokes_undo_and_redo() {
    use dom6_simple_map_editor::render::province_bboxes;
    let dir = temp_dir("bboxes");
    let (p1, _) = make_map(&dir, "bboxes", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let check = |doc: &dom6_simple_map_editor::project::PlaneDoc| {
        assert_eq!(doc.rendered.bboxes, province_bboxes(&doc.plane()));
    };
    check(doc);
    doc.paint_begin("Paint area");
    assert!(doc
        .paint_many(Some(1), &[(12, 5), (14, 6), (15, 7)], 2, &t, &opts)
        .is_some());
    check(doc);
    assert!(doc.paint_many(None, &[(14, 6)], 1, &t, &opts).is_some());
    check(doc);
    assert!(doc.paint_many(Some(0), &[(3, 3)], 2, &t, &opts).is_some());
    check(doc);
    doc.paint_end(&t, &opts);
    check(doc);
    assert!(doc.undo_last(&t, &opts).is_some());
    check(doc);
    assert!(doc.redo_last(&t, &opts).is_some());
    check(doc);
    doc.paint_begin("Paint area");
    assert!(doc.paint(1, 15, 5, 30, &t, &opts).is_some());
    doc.paint_end(&t, &opts);
    check(doc);
    assert!(doc.bbox(2).is_none());
    assert_eq!(doc.pixel_counts[2], 0);
    assert!(doc.undo_last(&t, &opts).is_some());
    check(doc);
    assert!(doc.bbox(2).is_some());
}

#[test]
fn capital_marks_toggle_without_a_re_render() {
    let dir = temp_dir("capitals");
    let (p1, _) = make_map(&dir, "capitals", 1, false);
    let t = tex();
    let with = Options::default();
    let without = Options {
        capitals: false,
        ..with
    };
    let mut proj = Project::open(&p1, &t, &with).unwrap();
    let doc = &mut proj.planes[0];
    let marked = doc.rendered.rgba.clone();
    let cap = ((5 * 20 + 4) * 4) as usize;
    assert_eq!(&marked[cap..cap + 4], &[255, 255, 255, 255]);
    doc.set_capitals(false);
    let plain = doc.rendered.rgba.clone();
    doc.rerender(&t, &without);
    assert_eq!(plain, doc.rendered.rgba);
    assert_ne!(&plain[cap..cap + 4], &[255, 255, 255, 255]);
    doc.set_capitals(true);
    assert_eq!(marked, doc.rendered.rgba);
    doc.paint_begin("Paint area");
    assert!(doc.paint(2, 4, 5, 1, &t, &without).is_some());
    doc.paint_end(&t, &without);
    doc.set_capitals(false);
    let after = doc.rendered.rgba.clone();
    doc.rerender(&t, &without);
    assert_eq!(after, doc.rendered.rgba);
}

#[test]
fn incremental_thumbnail_matches_a_full_one() {
    use dom6_simple_map_editor::render::{thumbnail, Rect};
    let dir = temp_dir("thumb");
    let (p1, _) = make_map(&dir, "thumb", 1, false);
    let t = tex();
    let opts = Options::default();
    let mut proj = Project::open(&p1, &t, &opts).unwrap();
    let doc = &mut proj.planes[0];
    let first = thumbnail(&doc.rendered, true, 5, None, None);
    assert_eq!((first.w, first.h, first.k), (5, 3, 4));
    doc.paint_begin("Paint area");
    let rect = doc.paint(1, 12, 5, 2, &t, &opts).unwrap();
    doc.paint_end(&t, &opts);
    let touched = doc.rendered.touched;
    let full = thumbnail(&doc.rendered, true, 5, None, None);
    let dirty = Rect {
        x0: rect.x0.min(touched.x0),
        y0: rect.y0.min(touched.y0),
        x1: rect.x1.max(touched.x1),
        y1: rect.y1.max(touched.y1),
    };
    let inc = thumbnail(&doc.rendered, true, 5, Some(first), Some(dirty));
    assert_eq!(full.rgba, inc.rgba);
}
