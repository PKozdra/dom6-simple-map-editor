use dom6_simple_map_editor::decor::{
    draw_sprites, mountain_line_set, mountain_sprites, order, province_sprites, Rng, Sprite,
};
use dom6_simple_map_editor::render::{Options, Plane, Rect, Rendered};
use dom6_simple_map_editor::terrain::*;
use dom6_simple_map_editor::textures::{Frame, Image, TexSet};
use std::collections::HashSet;

fn textures() -> TexSet {
    let imgs = dom6_simple_map_editor::textures::ALL
        .iter()
        .map(|_| Image {
            w: 2,
            h: 2,
            rgba: [40, 90, 40, 255].repeat(4),
        })
        .collect();
    let frames = (0..dom6_simple_map_editor::textures::SPRITE_COUNT)
        .map(|_| {
            Frame::from_image(Image {
                w: 8,
                h: 8,
                rgba: [200, 30, 30, 255].repeat(64),
            })
        })
        .collect();
    TexSet::from_images(imgs).with_frames(frames)
}

struct World {
    w: i32,
    h: i32,
    heights: Vec<f32>,
    owners: Vec<i16>,
    flags: Vec<u64>,
    capitals: Vec<(i16, i16)>,
    lines: Vec<(u32, u32)>,
    bridges: Vec<(u32, u32)>,
}

impl World {
    fn new(left: u64, right: u64, lines: Vec<(u32, u32)>) -> World {
        let w = 160;
        let h = 120;
        let mut heights = Vec::new();
        let mut owners = Vec::new();
        for _y in 0..h {
            for x in 0..w {
                owners.push(if x < 80 { 1 } else { 2 });
                heights.push(100.0);
            }
        }
        World {
            w,
            h,
            heights,
            owners,
            flags: vec![0, left, right],
            capitals: vec![(40, 60), (120, 60)],
            lines,
            bridges: Vec::new(),
        }
    }

    fn plane(&self) -> Plane<'_> {
        Plane {
            w: self.w,
            h: self.h,
            heights: &self.heights,
            owners: &self.owners,
            flags: &self.flags,
            scale: 30.0,
            hwrap: false,
            vwrap: false,
            capitals: &self.capitals,
            rivers: &[],
            mountain_lines: &self.lines,
            bridges: &self.bridges,
            cave_plane: false,
        }
    }
}

#[test]
fn rng_is_deterministic_and_bounded() {
    let mut a = Rng::new(7);
    let mut b = Rng::new(7);
    for _ in 0..1000 {
        let x = a.below(13);
        assert_eq!(x, b.below(13));
        assert!((0..13).contains(&x));
        let u = a.unit();
        assert!((0.0..1.0).contains(&u));
        b.unit();
    }
    assert_eq!(a.below(0), 0);
    assert!((4..=8).contains(&a.dice(4, 2)));
}

#[test]
fn forests_get_trees_plains_get_grass_and_water_stays_bare() {
    let world = World::new(FOREST, SEA, Vec::new());
    let p = world.plane();
    let lines = HashSet::new();
    let mut forest = Vec::new();
    province_sprites(&p, &world.heights, &lines, 1, &mut forest);
    assert!(forest.len() > 100, "forest has {} sprites", forest.len());
    assert!(forest.iter().any(|s| (0x16..0x1c).contains(&s.idx)));
    assert!(forest
        .iter()
        .all(|s| (0x16..0x1c).contains(&s.idx) || (0x46..=0x4a).contains(&s.idx)));
    assert!(forest.iter().all(|s| s.x < 80 + 5));
    let mut again = Vec::new();
    province_sprites(&p, &world.heights, &lines, 1, &mut again);
    assert_eq!(forest, again);
    let mut wet = world.heights.clone();
    for v in wet.iter_mut() {
        *v = -20.0;
    }
    let mut none = Vec::new();
    province_sprites(&p, &wet, &lines, 1, &mut none);
    assert!(none.is_empty());
    let plains = World::new(0, 0, Vec::new());
    let pp = plains.plane();
    let mut grass = Vec::new();
    province_sprites(&pp, &plains.heights, &lines, 1, &mut grass);
    assert!(grass.iter().all(|s| s.idx <= 0x4a));
    let winter = World::new(FOREST | COLDER, 0, Vec::new());
    let wp = winter.plane();
    let mut snow = Vec::new();
    province_sprites(&wp, &winter.heights, &lines, 1, &mut snow);
    assert!(snow.iter().any(|s| (0x22..0x28).contains(&s.idx)));
    assert!(snow.iter().all(|s| !(0x16..0x1c).contains(&s.idx)));
}

#[test]
fn site_and_throne_flags_pick_their_sprites() {
    let world = World::new(MANY_SITES, 1 << 16, Vec::new());
    let p = world.plane();
    let lines = HashSet::new();
    let mut sites = Vec::new();
    province_sprites(&p, &world.heights, &lines, 1, &mut sites);
    assert!(sites.iter().any(|s| (0x34..=0x3c).contains(&s.idx)));
    let mut one = Vec::new();
    province_sprites(&p, &world.heights, &lines, 2, &mut one);
    assert_eq!(one.iter().filter(|s| s.idx == 0x37).count(), 1);
}

#[test]
fn mountains_only_grow_along_mountain_lines() {
    let bare = World::new(MOUNTAIN, MOUNTAIN, Vec::new());
    let p = bare.plane();
    let set = mountain_line_set(&p);
    assert!(set.is_empty());
    let mut none = Vec::new();
    mountain_sprites(&p, &bare.heights, &set, 1, [0, 79, 0, 119], &mut none);
    assert!(none.is_empty());
    let ridge = World::new(MOUNTAIN, MOUNTAIN, vec![(1, 2)]);
    let p = ridge.plane();
    let set = mountain_line_set(&p);
    assert_eq!(set.len(), 1);
    let mut rocks = Vec::new();
    mountain_sprites(&p, &ridge.heights, &set, 1, [0, 79, 0, 119], &mut rocks);
    assert!(!rocks.is_empty());
    assert!(rocks.iter().all(|s| s.x >= 79 - 8));
    assert!(rocks.iter().all(|s| (0..=10).contains(&s.idx)));
}

#[test]
fn bridges_sit_on_the_border_crossing() {
    let mut world = World::new(FOREST, 0, Vec::new());
    world.bridges = vec![(1, 2)];
    let p = world.plane();
    let mut trees = vec![Sprite {
        x: 80,
        y: 60,
        size: 4,
        idx: 0x16,
        layer: 0,
    }];
    let mut out = Vec::new();
    dom6_simple_map_editor::decor::bridge_sprites(&p, 1, &mut trees, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].idx, 0x4b);
    assert_eq!(out[0].x, 80);
    assert_eq!(out[0].y, 60);
    assert_eq!(out[0].size, 18);
    assert!(trees.is_empty());
    let mut none = Vec::new();
    dom6_simple_map_editor::decor::bridge_sprites(&p, 2, &mut trees, &mut none);
    assert!(none.is_empty());
}

#[test]
fn drawing_sorts_far_sprites_first_and_stays_inside_the_rect() {
    let world = World::new(0, 0, Vec::new());
    let p = world.plane();
    let tex = textures();
    let mut sprites = vec![
        Sprite {
            x: 20,
            y: 20,
            size: 6,
            idx: 0x16,
            layer: 0,
        },
        Sprite {
            x: 20,
            y: 40,
            size: 6,
            idx: 0x17,
            layer: 0,
        },
    ];
    order(&mut sprites);
    assert_eq!(sprites[0].y, 40);
    let mut out = vec![0u8; (world.w * world.h * 4) as usize];
    draw_sprites(
        &p,
        &tex,
        &sprites,
        Rect {
            x0: 0,
            y0: 0,
            x1: 60,
            y1: 30,
        },
        &mut out,
    );
    let at = |x: i32, y: i32| out[((y * world.w + x) * 4 + 3) as usize];
    assert_eq!(at(20, 22), 255);
    assert_eq!(at(20, 42), 0);
    assert_eq!(at(50, 22), 0);
}

#[test]
fn rendered_map_carries_a_decor_layer() {
    let world = World::new(FOREST, 0, Vec::new());
    let p = world.plane();
    let tex = textures();
    let opts = Options::default();
    let r = Rendered::new(&p, &tex, &opts);
    assert_eq!(r.decor.len(), r.rgba.len());
    assert!(r.sprite_count() > 0);
    assert!(r.decor.chunks_exact(4).any(|px| px[3] > 0));
    let plain = Options {
        decor: false,
        ..Options::default()
    };
    let r2 = Rendered::new(&p, &tex, &plain);
    assert_eq!(r2.sprite_count(), 0);
}
