use dom6_simple_map_editor::imagemap::{
    guess_owners, owners_from_runs, whiten, Look, Looks, Rgb, PASSES,
};
use dom6_simple_map_editor::mapfile::MapFile;
use dom6_simple_map_editor::render::{Plane, Rect};
use dom6_simple_map_editor::terrain::{COLDER, DEEP_SEA, FARM, FOREST, SEA, UNKNOWN};
use std::path::Path;

fn flat(w: usize, h: usize, c: [u8; 3]) -> Rgb {
    Rgb {
        w,
        h,
        data: c.iter().copied().cycle().take(w * h * 3).collect(),
    }
}

fn pass_of(suffix: &str) -> usize {
    PASSES
        .iter()
        .position(|p| p.summer == Some(suffix) || p.winter == suffix)
        .unwrap()
}

fn looks_with(original: Vec<u64>) -> Looks {
    let mut l = Looks::from_base(flat(4, 4, [10, 20, 30]));
    l.original = original;
    l
}

#[test]
fn unchanged_summer_province_keeps_the_main_picture() {
    let mut l = looks_with(vec![0, FOREST]);
    l.set_summer(pass_of("_forest"), flat(2, 2, [1, 2, 3]));
    assert_eq!(l.look_of(1, FOREST, false, false), Look::Base);
}

#[test]
fn changed_terrain_takes_its_terrain_picture() {
    let mut l = looks_with(vec![0, 0]);
    l.set_summer(pass_of("_forest"), flat(2, 2, [1, 2, 3]));
    assert_eq!(
        l.look_of(1, FOREST, false, false),
        Look::Summer(pass_of("_forest"), false)
    );
}

#[test]
fn all_looks_shows_the_terrain_picture_without_a_change() {
    let mut l = looks_with(vec![0, FARM]);
    l.set_summer(pass_of("_farm"), flat(2, 2, [1, 2, 3]));
    assert_eq!(
        l.look_of(1, FARM, false, true),
        Look::Summer(pass_of("_farm"), false)
    );
}

#[test]
fn winter_prefers_the_terrain_winter_picture_then_the_general_one() {
    let mut l = looks_with(vec![0, FOREST]);
    l.set_winter(pass_of("_winter"), flat(2, 2, [9, 9, 9]));
    assert_eq!(
        l.look_of(1, FOREST, true, false),
        Look::Winter(pass_of("_winter"))
    );
    l.set_winter(pass_of("_forestw"), flat(2, 2, [8, 8, 8]));
    assert_eq!(
        l.look_of(1, FOREST, true, false),
        Look::Winter(pass_of("_forestw"))
    );
}

#[test]
fn winter_without_winter_art_whitens() {
    let l = looks_with(vec![0, 0]);
    assert_eq!(l.look_of(1, 0, true, false), Look::BaseWhitened);
    let mut l = looks_with(vec![0, FOREST]);
    l.set_summer(pass_of("_forest"), flat(2, 2, [1, 2, 3]));
    assert_eq!(
        l.look_of(1, FOREST, true, false),
        Look::Summer(pass_of("_forest"), true)
    );
}

#[test]
fn deep_sea_and_unknown_stay_as_drawn() {
    let mut l = looks_with(vec![0, SEA | DEEP_SEA, UNKNOWN]);
    l.set_winter(pass_of("_winter"), flat(2, 2, [9, 9, 9]));
    assert_eq!(l.look_of(1, SEA | DEEP_SEA, true, false), Look::Base);
    assert_eq!(l.look_of(2, UNKNOWN | COLDER, true, false), Look::Base);
}

#[test]
fn whiten_matches_the_engine_formula() {
    assert_eq!(whiten(0), 100);
    assert_eq!(whiten(255), 255);
    assert_eq!(whiten(100), ((100 * 0x9b + 0x639c) / 0xff) as u8);
}

#[test]
fn paint_picks_pictures_per_province_and_never_emits_pure_white() {
    let mut base = flat(4, 2, [10, 20, 30]);
    base.data[0..3].copy_from_slice(&[255, 255, 255]);
    let mut l = Looks::from_base(base);
    l.original = vec![0, 0, 0];
    l.set_summer(pass_of("_forest"), flat(1, 1, [1, 2, 3]));
    let owners = vec![1i16, 1, 2, 2, 1, 1, 2, 0];
    let flags = vec![0u64, 0, FOREST];
    let heights = vec![0i16; 8];
    let plane = Plane {
        w: 4,
        h: 2,
        heights: &heights,
        owners: &owners,
        flags: &flags,
        scale: 50.0,
        hwrap: false,
        vwrap: false,
        capitals: &[],
        rivers: &[],
        mountain_lines: &[],
        bridges: &[],
        cave_plane: false,
        image: None,
    };
    let mut out = vec![0u8; 8 * 4];
    l.paint(&plane, Rect::full(4, 2), false, false, &mut out);
    assert_eq!(&out[0..4], &[254, 254, 254, 255]);
    assert_eq!(&out[4..8], &[10, 20, 30, 255]);
    assert_eq!(&out[8..12], &[1, 2, 3, 255]);
    assert_eq!(&out[28..32], &[10, 20, 30, 255]);
}

#[test]
fn white_pixels_number_provinces_row_by_row_from_the_bottom() {
    let mut img = flat(3, 2, [0, 0, 0]);
    img.data[(2 * 3)..(2 * 3 + 3)].copy_from_slice(&[255; 3]);
    img.data[(4 * 3)..(4 * 3 + 3)].copy_from_slice(&[255; 3]);
    assert_eq!(img.white_pixels(), vec![(2, 0), (1, 1)]);
}

#[test]
fn pb_runs_become_the_owner_raster() {
    let map = MapFile::parse(
        "#dom2title t\n#imagefile t.tga\n#pb 0 0 2 1\n#pb 2 0 5 2\n#pb 1 1 1 2\n",
        Path::new("t.map"),
    );
    let owners = owners_from_runs(4, 2, &map.pb_runs());
    assert_eq!(owners, vec![1, 1, 2, 2, 0, 2, 0, 0]);
}

#[test]
fn guessed_areas_cover_the_plane() {
    let owners = guess_owners(6, 1, &[(0, 0), (5, 0)], false, false);
    assert_eq!(owners, vec![1, 1, 1, 2, 2, 2]);
}
