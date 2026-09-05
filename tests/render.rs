use dom6_simple_map_editor::render::{land_texture, province_winter, Options, Plane, Rendered};
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

fn plane<'a>(
    w: i32,
    h: i32,
    heights: &'a [f32],
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
    assert!(province_winter(COLDER));
    assert!(!province_winter(COLDER | WARMER | CAVE_WALL));
    assert!(!province_winter(WARMER));
}

#[test]
fn water_bands_follow_depth() {
    let tex = flat_textures();
    let w = 8;
    let h = 1;
    let heights: Vec<f32> = vec![5.0, -1.0, -9.9, -10.0, -30.0, -33.0, -36.0, -100.0];
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
    let heights = vec![-20.0f32, -20.0, -20.0];
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
    let heights = vec![20.0f32; 36];
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
    let heights = vec![20.0f32; 48];
    let flags = vec![0u64, 0, 0];
    let p = plane(w, h, &heights, &owners, &flags, &[]);
    let base = Options {
        rivers: false,
        borders: false,
        capitals: false,
        edge_fade: false,
        border_percent: 100,
        decor: false,
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
    let p = plane(w, h, &heights, &owners, &flags, &[]);
    let mut r = Rendered::new(&p, &tex, &opts);
    let mut heights2 = heights.clone();
    for i in 0..heights2.len() {
        if owners[i] == 7 {
            heights2[i] = -40.0;
        }
    }
    let p2 = plane(w, h, &heights2, &owners, &flags, &[]);
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
