#![cfg(not(target_arch = "wasm32"))]

use dom6_simple_map_editor::imagemap::WHITE;
use dom6_simple_map_editor::project::{PictureBrush, PlaneDoc, Project};
use dom6_simple_map_editor::render::Options;
use dom6_simple_map_editor::textures::{Image, TexSet};
use std::path::{Path, PathBuf};

const W: usize = 8;
const H: usize = 6;

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

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("d6sme_picture_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn picture(whites: &[(usize, usize)], bpp: u8) -> Vec<u8> {
    let mut rgb = vec![0u8; W * H * 3];
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) * 3;
            rgb[i] = (10 + x * 4) as u8;
            rgb[i + 1] = (20 + y * 5) as u8;
            rgb[i + 2] = 90;
        }
    }
    for &(x, y) in whites {
        let i = (y * W + x) * 3;
        rgb[i..i + 3].copy_from_slice(&WHITE);
    }
    dom6_simple_map_editor::tga::encode_rgb_bottom_up(W, H, &rgb, bpp)
}

fn map_text() -> String {
    let mut t = String::from("#dom2title pic\n#imagefile pic.tga\n#mapsize 8 6\n");
    t.push_str("#terrain 1 4\n#terrain 2 8192\n#terrain 3 32\n");
    t.push_str("#landname 1 Alpha\n#landname 2 Beta\n#landname 3 Gamma\n");
    t.push_str("#gate 2 7\n");
    t.push_str("#neighbour 1 2\n#neighbour 1 3\n#neighbour 2 3\n");
    t.push_str("#specstart 5 2\n");
    for y in 0..3 {
        t.push_str(&format!("#pb 0 {y} 4 1\n"));
        t.push_str(&format!("#pb 4 {y} 4 2\n"));
    }
    for y in 3..6 {
        t.push_str(&format!("#pb 0 {y} 8 3\n"));
    }
    t
}

fn build(tag: &str, bpp: u8) -> (PathBuf, PathBuf) {
    let dir = temp_dir(tag);
    std::fs::write(dir.join("pic.tga"), picture(&[(2, 1), (5, 1), (2, 4)], bpp)).unwrap();
    std::fs::write(dir.join("pic.map"), map_text()).unwrap();
    let map = dir.join("pic.map");
    (dir, map)
}

fn open(map: &Path, t: &TexSet, opts: &Options) -> Project {
    Project::open(map, t, opts).unwrap()
}

#[derive(PartialEq, Debug)]
struct Snapshot {
    flags: Vec<u64>,
    names: Vec<String>,
    gates: Vec<i32>,
    capitals: Vec<(i16, i16)>,
    owners: Vec<i16>,
    baseline: Vec<i16>,
    counts: Vec<u32>,
    original: Vec<u64>,
    bboxes: Vec<[i32; 4]>,
    picture: Vec<u8>,
    text: String,
}

fn snap(d: &PlaneDoc) -> Snapshot {
    let l = d.image.as_ref().unwrap();
    Snapshot {
        flags: d.flags.clone(),
        names: d.names.clone(),
        gates: d.gates.clone(),
        capitals: d.capitals.clone(),
        owners: d.d6m.owners.clone(),
        baseline: d.baseline.clone(),
        counts: d.pixel_counts.clone(),
        original: l.original.clone(),
        bboxes: d.rendered.bboxes.clone(),
        picture: l.base.data.clone(),
        text: d.map.as_ref().unwrap().to_text(),
    }
}

fn relaxed(s: Snapshot) -> Snapshot {
    let mut lines: Vec<String> = s
        .text
        .lines()
        .map(|l| l.replace('"', "").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    lines.sort();
    Snapshot {
        text: lines.join("\n"),
        ..s
    }
}

fn whites(d: &PlaneDoc) -> Vec<(i16, i16)> {
    d.image.as_ref().unwrap().base.white_pixels()
}

#[test]
fn moving_a_capital_across_rows_renumbers_everything() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("move", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    assert_eq!(doc.province_count(), 3);
    assert_eq!(doc.name(1), "Alpha");
    let before = snap(doc);

    let moved = doc.set_capital(1, 2, 2, &t, &opts);
    assert_eq!(moved, Some(2));
    assert_eq!(doc.name(2), "Alpha");
    assert_eq!(doc.name(1), "Beta");
    assert_eq!(doc.flags[2], 4);
    assert_eq!(doc.flags[1], 8192);
    assert_eq!(doc.gate(1), 7);
    assert_eq!(doc.gate(2), 0);
    assert_eq!(doc.capital(2), Some((2, 2)));
    assert_eq!(doc.owner_at(0, 0), 2);
    assert_eq!(doc.owner_at(6, 0), 1);
    assert_eq!(doc.owner_at(0, 4), 3);
    assert_eq!(doc.pixel_counts[2], 12);
    assert_eq!(doc.bbox(2).unwrap().x1, 3);
    assert!(doc.neighbours(2).contains(&1));
    assert!(doc.neighbours(2).contains(&3));

    let text = doc.map.as_ref().unwrap().to_text();
    assert!(text.contains("#landname 2 Alpha"));
    assert!(text.contains("#landname 1 Beta"));
    assert!(text.contains("#terrain 2 4"));
    assert!(text.contains("#gate 1 7"));
    assert!(text.contains("#specstart 5 1"));

    assert_eq!(whites(doc), vec![(5, 1), (2, 2), (2, 4)]);
    let l = doc.image.as_ref().unwrap();
    assert!(!l.base.is_white(2, 1));
    assert_ne!(l.base.pixel(2, 1), WHITE);

    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(snap(doc), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adding_then_removing_a_province_round_trips() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("add", 32);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    let before = snap(doc);

    let added = doc.add_province(0, 4, 1, &t, &opts);
    assert_eq!(added, Some(3));
    assert_eq!(doc.province_count(), 4);
    assert_eq!(doc.name(4), "Gamma");
    assert_eq!(doc.capital(3), Some((0, 4)));
    assert_eq!(doc.owner_at(0, 4), 3);
    assert_eq!(doc.owner_at(7, 5), 4);
    assert!(doc.pixel_counts[3] > 0);
    assert_eq!(whites(doc).len(), 4);
    let text = doc.map.as_ref().unwrap().to_text();
    assert!(text.contains("#landname 4 Gamma"));
    assert!(text.contains("#specstart 5 2"));

    assert!(doc.remove_province(3, &t, &opts));
    assert_eq!(doc.province_count(), 3);
    assert_eq!(doc.name(3), "Gamma");
    assert_eq!(doc.owner_at(0, 4), 3);
    assert_eq!(whites(doc), vec![(2, 1), (5, 1), (2, 4)]);

    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(doc.province_count(), 4);
    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(relaxed(snap(doc)), relaxed(before));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_capitals_are_pure_white_after_edits() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("white", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    doc.set_capital(1, 1, 0, &t, &opts);
    doc.add_province(7, 5, 1, &t, &opts);
    doc.remove_province(2, &t, &opts);
    let mut caps = doc.capitals.clone();
    caps.sort_unstable_by_key(|&(x, y)| (y, x));
    assert_eq!(whites(doc), caps);
    assert!(doc
        .add_province(caps[0].0 as i32, caps[0].1 as i32, 1, &t, &opts)
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_capital_may_not_land_on_another_white_pixel() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("clash", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    assert!(doc.set_capital(2, 5, 1, &t, &opts).is_none());
    assert!(doc.set_capital(1, 5, 1, &t, &opts).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_24_bit_picture_survives_an_encode_and_decode() {
    for bpp in [24u8, 32] {
        let bytes = picture(&[(2, 1), (5, 1)], bpp);
        let img = dom6_simple_map_editor::tga::decode(&bytes).unwrap();
        assert_eq!((img.w, img.h), (W, H));
        let rgb: Vec<u8> = img
            .rgba
            .chunks_exact(4)
            .flat_map(|p| p[..3].to_vec())
            .collect();
        let again = dom6_simple_map_editor::tga::encode_rgb_bottom_up(W, H, &rgb, bpp);
        assert_eq!(again, bytes);
        assert_eq!(dom6_simple_map_editor::tga::depth(&bytes), bpp);
    }
}

#[test]
fn saving_writes_the_picture_once_and_keeps_the_original() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("save", 24);
    let original = std::fs::read(dir.join("pic.tga")).unwrap();
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    assert_eq!(doc.set_capital(1, 2, 2, &t, &opts), Some(2));
    let written = doc.save().unwrap();
    assert!(written.contains(&dir.join("pic.tga")));
    assert!(written.contains(&map));
    assert_eq!(std::fs::read(dir.join("pic.tga.bak")).unwrap(), original);
    let saved = std::fs::read(dir.join("pic.tga")).unwrap();
    assert_ne!(saved, original);
    assert_eq!(dom6_simple_map_editor::tga::depth(&saved), 24);

    let reopened = Project::open(&map, &t, &opts).unwrap();
    let back = &reopened.planes[0];
    assert_eq!(back.province_count(), 3);
    assert_eq!(back.name(2), "Alpha");
    assert_eq!(back.capital(2), Some((2, 2)));
    assert_eq!(back.owner_at(0, 0), 2);

    std::fs::write(dir.join("pic.tga.bak"), b"marker").unwrap();
    let doc = &mut proj.planes[0];
    assert_eq!(doc.set_capital(3, 3, 4, &t, &opts), Some(3));
    doc.save().unwrap();
    assert_eq!(std::fs::read(dir.join("pic.tga.bak")).unwrap(), b"marker");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn later_plane_counts_keep_the_surface_starts_right() {
    let t = tex();
    let opts = Options::default();
    let dir = temp_dir("planes");
    std::fs::write(dir.join("pic.tga"), picture(&[(2, 1), (5, 1), (2, 4)], 24)).unwrap();
    let mut surface = map_text();
    surface.push_str("#specstart 6 5\n");
    std::fs::write(dir.join("pic.map"), surface).unwrap();
    std::fs::write(dir.join("pic_plane2.tga"), picture(&[(1, 0), (6, 3)], 24)).unwrap();
    std::fs::write(
        dir.join("pic_plane2.map"),
        "#dom2title cave\n#imagefile pic_plane2.tga\n#mapsize 8 6\n#terrain 1 0\n#terrain 2 0\n",
    )
    .unwrap();
    let map = dir.join("pic.map");
    let mut proj = open(&map, &t, &opts);
    assert_eq!(proj.planes.len(), 2);
    assert_eq!(proj.plane_offset(1), 3);
    assert_eq!(proj.specstarts(), vec![(5, 0, 2), (6, 1, 2)]);

    assert_eq!(proj.planes[1].add_province(0, 0, 1, &t, &opts), Some(1));
    proj.fix_starts(1);
    assert_eq!(proj.specstarts(), vec![(5, 0, 2), (6, 1, 3)]);

    assert!(proj.planes[1].remove_province(1, &t, &opts));
    proj.fix_starts(1);
    assert_eq!(proj.specstarts(), vec![(5, 0, 2), (6, 1, 2)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_surface_change_shifts_the_later_plane_globals() {
    let t = tex();
    let opts = Options::default();
    let dir = temp_dir("surface");
    std::fs::write(dir.join("pic.tga"), picture(&[(2, 1), (5, 1), (2, 4)], 24)).unwrap();
    let mut surface = map_text();
    surface.push_str("#specstart 6 5\n");
    std::fs::write(dir.join("pic.map"), surface).unwrap();
    std::fs::write(dir.join("pic_plane2.tga"), picture(&[(1, 0), (6, 3)], 24)).unwrap();
    std::fs::write(
        dir.join("pic_plane2.map"),
        "#dom2title cave\n#imagefile pic_plane2.tga\n#mapsize 8 6\n#terrain 1 0\n#terrain 2 0\n",
    )
    .unwrap();
    let map = dir.join("pic.map");
    let mut proj = open(&map, &t, &opts);
    assert_eq!(proj.planes[0].add_province(7, 0, 1, &t, &opts), Some(1));
    proj.fix_starts(0);
    assert_eq!(proj.plane_offset(1), 4);
    assert_eq!(proj.specstarts(), vec![(5, 0, 3), (6, 1, 2)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picture_plane_still_refuses_the_last_province() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("last", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    assert!(doc.remove_province(1, &t, &opts));
    assert!(doc.remove_province(1, &t, &opts));
    assert!(!doc.remove_province(1, &t, &opts));
    assert_eq!(doc.province_count(), 1);
    assert_eq!(whites(doc).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_undo_restores_the_picture_and_the_map() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("undoall", 32);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    let before = snap(doc);
    doc.set_capital(3, 5, 5, &t, &opts);
    doc.add_province(0, 0, 1, &t, &opts);
    doc.remove_province(2, &t, &opts);
    doc.centre_capital(1, &t, &opts);
    assert_eq!(doc.undo_all(&t, &opts), 4);
    assert_eq!(relaxed(snap(doc)), relaxed(before));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picture_stroke_is_one_undo_step_and_never_paints_white() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("brush", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    let before = snap(doc);

    doc.paint_begin("Picture brush");
    assert!(doc
        .paint_picture(
            &[(2, 1)],
            2,
            PictureBrush::Colour([255, 255, 255]),
            &t,
            &opts
        )
        .is_some());
    assert!(doc
        .paint_picture(&[(4, 2)], 2, PictureBrush::Colour([1, 2, 3]), &t, &opts)
        .is_some());
    assert!(doc.paint_end(&t, &opts).is_some());
    assert_eq!(doc.undo.len(), 1);
    assert!(doc.picture_dirty);
    assert_eq!(whites(doc), vec![(2, 1), (5, 1), (2, 4)]);
    let l = doc.image.as_ref().unwrap();
    assert_eq!(l.base.pixel(1, 1), [254, 254, 254]);
    assert_eq!(l.base.pixel(4, 2), [1, 2, 3]);

    assert!(doc.undo_last(&t, &opts).is_some());
    assert_eq!(snap(doc), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_right_button_puts_back_the_opened_picture() {
    let t = tex();
    let opts = Options::default();
    let (dir, map) = build("restore", 24);
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    let before = snap(doc);
    doc.paint_picture(&[(4, 2)], 2, PictureBrush::Colour([1, 2, 3]), &t, &opts);
    assert_ne!(snap(doc).picture, before.picture);
    doc.paint_picture(&[(4, 2)], 3, PictureBrush::Restore, &t, &opts);
    assert_eq!(snap(doc).picture, before.picture);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_terrain_stamp_copies_the_terrain_picture() {
    let t = tex();
    let opts = Options::default();
    let dir = temp_dir("stamp");
    std::fs::write(dir.join("pic.tga"), picture(&[(2, 1), (5, 1), (2, 4)], 24)).unwrap();
    std::fs::write(dir.join("pic.map"), map_text()).unwrap();
    let mut forest = vec![0u8; W * H * 3];
    for (i, b) in forest.iter_mut().enumerate() {
        *b = (i % 200) as u8;
    }
    std::fs::write(
        dir.join("pic_forest.tga"),
        dom6_simple_map_editor::tga::encode_rgb_bottom_up(W, H, &forest, 24),
    )
    .unwrap();
    let map = dir.join("pic.map");
    let mut proj = open(&map, &t, &opts);
    let doc = &mut proj.planes[0];
    let choices: Vec<usize> = doc
        .image
        .as_ref()
        .unwrap()
        .terrain_choices()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(choices.len(), 1);
    assert!(doc
        .paint_picture(&[(4, 2)], 1, PictureBrush::Terrain(choices[0]), &t, &opts)
        .is_some());
    let i = (2 * W + 4) * 3;
    assert_eq!(
        doc.image.as_ref().unwrap().base.pixel(4, 2),
        [forest[i], forest[i + 1], forest[i + 2]]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
