use dom6_simple_map_editor::render::{
    cold_scale, land_texture, province_winter, Options, Plane, Rendered,
};
use dom6_simple_map_editor::terrain::*;
use dom6_simple_map_editor::textures::{Image, Tex, TexSet};

fn flat_textures() -> TexSet {
    let imgs = dom6_simple_map_editor::textures::ALL
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let v = (i * 7 + 20) as u8;
            Image {
                w: 4,
                h: 4,
                rgba: [v, v, 255 - v, 255].repeat(16),
            }
        })
        .collect();
    TexSet::from_images(imgs)
}

fn stored(units: &[f32]) -> Vec<i16> {
    units
        .iter()
        .map(|&u| dom6_simple_map_editor::d6m::stored_from_units(u))
        .collect()
}

fn plane<'a>(
    w: i32,
    h: i32,
    heights: &'a [i16],
    owners: &'a [i16],
    flags: &'a [u64],
    rivers: &'a [(u32, u32)],
) -> Plane<'a> {
    Plane {
        w,
        h,
        heights,
        owners,
        flags,
        scale: 40.0,
        hwrap: false,
        vwrap: false,
        capitals: &[],
        rivers,
        mountain_lines: &[],
        bridges: &[],
        cave_plane: false,
    }
}

#[test]
fn land_texture_table() {
    assert_eq!(land_texture(0, false), Tex::Plain);
    assert_eq!(land_texture(FOREST, false), Tex::Forest);
    assert_eq!(land_texture(FARM, false), Tex::Farm);
    assert_eq!(land_texture(SWAMP, false), Tex::Swamp);
    assert_eq!(land_texture(WASTE, false), Tex::Waste);
    assert_eq!(land_texture(HIGHLAND, false), Tex::Highland);
    assert_eq!(land_texture(CAVE, false), Tex::Cavefloor);
    assert_eq!(land_texture(CAVE | FOREST, false), Tex::Caveforest);
    assert_eq!(land_texture(CAVE | SWAMP, false), Tex::Dripcave);
    assert_eq!(land_texture(CAVE | HIGHLAND, false), Tex::Waste);
    assert_eq!(land_texture(CAVE_WALL, false), Tex::Cave);
    assert_eq!(land_texture(FOREST | FARM, false), Tex::Forest);
    assert_eq!(land_texture(SEA | FOREST, false), Tex::Forest);
    assert_eq!(land_texture(0, true), Tex::Winter);
    assert_eq!(land_texture(FOREST, true), Tex::Winterwood);
    assert_eq!(land_texture(FARM, true), Tex::Winterfarm);
    assert_eq!(land_texture(CAVE | SWAMP, true), Tex::Frozendrip);
    assert!(province_winter(COLDER, false));
    assert!(!province_winter(COLDER | WARMER | CAVE_WALL, false));
    assert!(!province_winter(WARMER, false));
}

#[test]
fn water_bands_follow_depth() {
    let tex = flat_textures();
    let w = 8;
    let h = 1;
    let heights = stored(&[5.0, -1.0, -9.9, -10.0, -30.0, -33.0, -36.0, -100.0]);
    let owners: Vec<i16> = vec![1; 8];
    let flags = vec![0u64, SEA];
    let p = plane(w, h, &heights, &owners, &flags, &[]);
    let opts = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let r = Rendered::new(&p, &tex, &opts);
    let px = |x: usize| [r.rgba[x * 4], r.rgba[x * 4 + 1], r.rgba[x * 4 + 2]];
    let want = |t: Tex| {
        let s = tex.sample(t, 0, 0);
        [s[0], s[1], s[2]]
    };
    assert_eq!(px(0), want(Tex::Plain));
    assert_eq!(px(1), want(Tex::Shallowsea));
    assert_eq!(px(2), want(Tex::Shallowsea));
    assert_eq!(px(3), want(Tex::Water));
    assert_eq!(px(4), want(Tex::Water));
    let deep = want(Tex::Deepsea);
    let water = want(Tex::Water);
    let mid = px(5);
    assert!(mid != deep && mid != water);
    assert_eq!(px(6), deep);
    assert_eq!(px(7), deep);
}

#[test]
fn unowned_pixels_stay_transparent_and_gorge_darkens() {
    let tex = flat_textures();
    let heights = stored(&[-20.0, -20.0, -20.0]);
    let owners = vec![0i16, 1, 2];
    let flags = vec![0u64, SEA, SEA | HIGHLAND];
    let p = plane(3, 1, &heights, &owners, &flags, &[]);
    let opts = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let r = Rendered::new(&p, &tex, &opts);
    assert_eq!(&r.rgba[0..4], &[0, 0, 0, 0]);
    let plain = tex.sample(Tex::Water, 1, 0);
    assert_eq!(&r.rgba[4..7], &plain[..3]);
    let g = &r.rgba[8..11];
    assert_eq!(g[0], (plain[0] as f64 * 0.9) as u8);
}

#[test]
fn rivers_carve_only_land_between_the_pair() {
    let tex = flat_textures();
    let w = 6;
    let h = 6;
    let mut owners = vec![0i16; 36];
    for y in 0..6 {
        for x in 0..6 {
            owners[y * 6 + x] = if x < 3 { 1 } else { 2 };
        }
    }
    let heights = stored(&[20.0; 36]);
    let flags = vec![0u64, 0, 0];
    let rivers = vec![(1u32, 2u32)];
    let p = plane(w, h, &heights, &owners, &flags, &rivers);
    let opts = Options {
        rivers: true,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let r = Rendered::new(&p, &tex, &opts);
    assert_eq!(r.carved[2], dom6_simple_map_editor::d6m::RIVER_SENTINEL);
    assert_eq!(r.carved[3], dom6_simple_map_editor::d6m::RIVER_SENTINEL);
    assert_eq!(r.carved[0], dom6_simple_map_editor::d6m::RIVER_SENTINEL);
    assert_eq!(r.carved[5], 20.0);
    assert_eq!(r.carved[3 * 6 + 5], 20.0);
    let shallow = tex.sample(Tex::Shallowsea, 2, 0);
    assert_eq!(&r.rgba[8..11], &shallow[..3]);
}

#[test]
fn borders_brighten_the_seam() {
    let tex = flat_textures();
    let w = 12;
    let h = 4;
    let mut owners = vec![0i16; 48];
    for y in 0..4 {
        for x in 0..12 {
            owners[y * 12 + x] = if x < 6 { 1 } else { 2 };
        }
    }
    let heights = stored(&[20.0; 48]);
    let flags = vec![0u64, 0, 0];
    let p = plane(w, h, &heights, &owners, &flags, &[]);
    let base = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let with = Options {
        borders: true,
        ..base
    };
    let a = Rendered::new(&p, &tex, &base);
    let b = Rendered::new(&p, &tex, &with);
    let i = (2 * 12 + 5) * 4;
    assert!(b.rgba[i] > a.rgba[i]);
    let far = (2 * 12) * 4;
    assert_eq!(b.rgba[far], a.rgba[far]);
    assert_eq!(b.mask[2 * 12 + 5], 2);
}

#[test]
fn partial_rerender_matches_full() {
    let tex = flat_textures();
    let w = 40;
    let h = 30;
    let mut owners = vec![0i16; (w * h) as usize];
    let mut heights = vec![15.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let id = 1 + (x / 10) as i16 + 4 * (y / 10) as i16;
            owners[(y * w + x) as usize] = id;
            if id == 6 {
                heights[(y * w + x) as usize] = -25.0;
            }
        }
    }
    let flags = vec![0u64; 13];
    let opts = Options::default();
    let hs = stored(&heights);
    let p = plane(w, h, &hs, &owners, &flags, &[]);
    let mut r = Rendered::new(&p, &tex, &opts);
    let mut heights2 = heights.clone();
    for i in 0..heights2.len() {
        if owners[i] == 7 {
            heights2[i] = -40.0;
        }
    }
    let hs2 = stored(&heights2);
    let p2 = plane(w, h, &hs2, &owners, &flags, &[]);
    r.render(
        &p2,
        &tex,
        &opts,
        dom6_simple_map_editor::render::Rect {
            x0: 20,
            y0: 10,
            x1: 29,
            y1: 19,
        },
    );
    let full = Rendered::new(&p2, &tex, &opts);
    assert_eq!(r.rgba, full.rgba);
}

#[test]
fn winter_season_cold_scale() {
    assert!(province_winter(0, true));
    assert!(province_winter(FOREST, true));
    assert!(province_winter(COLDER, true));
    assert!(!province_winter(WARMER, true));
    assert!(!province_winter(CAVE, true));
    assert!(province_winter(CAVE | COLDER, true));
    assert!(!province_winter(CAVE_WALL, true));
    assert!(!province_winter(OUTER_PLANE, true));
    assert!(!province_winter(VOID_LAND, true));
    assert!(province_winter(SEA, true));
    assert!(!province_winter(SEA | DEEP_SEA, true));
    assert!(!province_winter(SEA | DEEP_SEA | COLDER, true));
    assert_eq!(cold_scale(COLDER, true), 2);
    assert_eq!(cold_scale(WARMER, true), 0);
    assert_eq!(cold_scale(SEA | COLDER, true), 1);
    assert_eq!(cold_scale(0, false), 0);
}

#[test]
fn winter_repaints_land_and_shallow_water() {
    let tex = flat_textures();
    let w = 6;
    let h = 1;
    let heights: Vec<f32> = vec![5.0, 5.0, 5.0, -1.0, -50.0, -50.0];
    let owners: Vec<i16> = vec![1, 2, 3, 4, 4, 5];
    let flags = vec![0u64, 0, FARM, WARMER, FRESH_WATER, SEA | DEEP_SEA];
    let hs = stored(&heights);
    let p = plane(w, h, &hs, &owners, &flags, &[]);
    let opts = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let summer = Rendered::new(&p, &tex, &opts);
    let winter = Rendered::new(
        &p,
        &tex,
        &Options {
            winter: true,
            ..opts
        },
    );
    let px = |r: &Rendered, x: usize| [r.rgba[x * 4], r.rgba[x * 4 + 1], r.rgba[x * 4 + 2]];
    let want = |t: Tex| {
        let s = tex.sample(t, 0, 0);
        [s[0], s[1], s[2]]
    };
    assert_eq!(px(&summer, 0), want(Tex::Plain));
    assert_eq!(px(&winter, 0), want(Tex::Winter));
    assert_eq!(px(&summer, 1), want(Tex::Farm));
    assert_eq!(px(&winter, 1), want(Tex::Winterfarm));
    assert_eq!(px(&summer, 2), want(Tex::Plain));
    assert_eq!(px(&winter, 2), want(Tex::Plain));
    assert_eq!(px(&summer, 3), want(Tex::Shallowsea));
    assert_eq!(px(&winter, 3), want(Tex::Frozen));
    assert_eq!(px(&summer, 4), px(&winter, 4));
    assert_eq!(px(&summer, 5), px(&winter, 5));
}

#[test]
fn dirt_darkens_land_and_leaves_unowned_alone() {
    let tex = flat_textures();
    let w = 96;
    let h = 96;
    let heights: Vec<f32> = (0..w * h)
        .map(|i| if i % w < 8 { -50.0 } else { 20.0 })
        .collect();
    let owners: Vec<i16> = (0..w * h).map(|i| if i % w < 4 { 0 } else { 1 }).collect();
    let flags = vec![0u64, 0];
    let hs = stored(&heights);
    let p = plane(w, h, &hs, &owners, &flags, &[]);
    let base = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: false,
        winter: false,
        grey_no_start: false,
    };
    let clean = Rendered::new(&p, &tex, &base);
    let dirty = Rendered::new(&p, &tex, &Options { dirt: true, ..base });
    let mut changed = 0;
    for (i, own) in owners.iter().enumerate() {
        if *own == 0 {
            assert_eq!(dirty.rgba[i * 4..i * 4 + 4], clean.rgba[i * 4..i * 4 + 4]);
        } else if dirty.rgba[i * 4..i * 4 + 3] != clean.rgba[i * 4..i * 4 + 3] {
            changed += 1;
        }
        assert!(dirty.rgba[i * 4] <= clean.rgba[i * 4] || dirty.rgba[i * 4 + 3] == 255);
    }
    assert!(
        changed > (w * h) as usize / 4,
        "dirt touched {changed} pixels"
    );
    let again = Rendered::new(&p, &tex, &Options { dirt: true, ..base });
    assert_eq!(dirty.rgba, again.rgba);
}

#[test]
fn dirt_partial_rerender_matches_full() {
    let tex = flat_textures();
    let w = 80;
    let h = 80;
    let heights: Vec<f32> = vec![20.0; (w * h) as usize];
    let owners: Vec<i16> = vec![1; (w * h) as usize];
    let flags = vec![0u64, 0];
    let hs = stored(&heights);
    let p = plane(w, h, &hs, &owners, &flags, &[]);
    let opts = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
        dirt: true,
        winter: false,
        grey_no_start: false,
    };
    let full = Rendered::new(&p, &tex, &opts);
    let mut partial = Rendered::new(&p, &tex, &opts);
    partial.render(
        &p,
        &tex,
        &opts,
        dom6_simple_map_editor::render::Rect {
            x0: 20,
            y0: 20,
            x1: 50,
            y1: 50,
        },
    );
    assert_eq!(full.rgba, partial.rgba);
}
