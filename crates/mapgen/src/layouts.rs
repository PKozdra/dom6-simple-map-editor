use crate::options::{Blueprint, Layout};

pub const LAYOUT_SIZE: i32 = 1024;

const LAND: [u8; 4] = [32, 78, 5, 255];
const SEA: [u8; 4] = [144, 5, 33, 255];
const NOSTART_LAND: [u8; 4] = [67, 67, 67, 255];
const NOSTART_SEA: [u8; 4] = [40, 40, 40, 255];

const TAU: f32 = std::f32::consts::TAU;

pub fn layout_blue_acc(kind: Layout) -> i32 {
    match kind {
        Layout::ForbiddenCenter => 2,
        _ => 1,
    }
}

pub fn layout_variants(kind: Layout) -> u32 {
    match kind {
        Layout::Standard => 0,
        Layout::CircleWorld | Layout::ForbiddenCenter => 1,
        Layout::OneSea | Layout::TwoSeas | Layout::TwirlingSea => 2,
        Layout::SmallLakes | Layout::NoMansLand => 3,
    }
}

pub fn layout_variant_for_seed(kind: Layout, seed: u32) -> u32 {
    let n = layout_variants(kind);
    if n < 2 {
        0
    } else {
        seed % n
    }
}

pub fn layout_blueprint(kind: Layout, w: i32, h: i32) -> Option<Blueprint> {
    layout_blueprint_variant(kind, 0, w, h)
}

pub fn layout_blueprint_variant(kind: Layout, variant: u32, w: i32, h: i32) -> Option<Blueprint> {
    if kind == Layout::Standard || w < 1 || h < 1 {
        return None;
    }
    let variant = variant % layout_variants(kind).max(1);
    let mut bgra = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            bgra.extend_from_slice(&paint(kind, variant, u, v));
        }
    }
    Some(Blueprint { w, h, bgra })
}

fn polar(u: f32, v: f32) -> (f32, f32) {
    let dx = u - 0.5;
    let dy = v - 0.5;
    ((dx * dx + dy * dy).sqrt(), dy.atan2(dx))
}

fn wobble(theta: f32, phase: f32, a: f32, b: f32) -> f32 {
    1.0 + a * (3.0 * theta + phase).sin() + b * (7.0 * theta - phase * 2.0).sin()
}

fn in_blob(u: f32, v: f32, cx: f32, cy: f32, rx: f32, ry: f32, phase: f32) -> bool {
    let dx = (u - cx) / rx;
    let dy = (v - cy) / ry;
    let r = (dx * dx + dy * dy).sqrt();
    r < wobble(dy.atan2(dx), phase, 0.09, 0.05)
}

fn any_blob(u: f32, v: f32, blobs: &[(f32, f32)], rx: f32, ry: f32, phase: f32) -> bool {
    blobs
        .iter()
        .enumerate()
        .any(|(i, (cx, cy))| in_blob(u, v, *cx, *cy, rx, ry, phase + i as f32 * 1.3))
}

fn paint(kind: Layout, variant: u32, u: f32, v: f32) -> [u8; 4] {
    let phase = variant as f32 * 1.7;
    match kind {
        Layout::Standard => LAND,
        Layout::CircleWorld => circle_world(u, v, phase),
        Layout::SmallLakes => small_lakes(u, v, variant, phase),
        Layout::OneSea => one_sea(u, v, variant, phase),
        Layout::TwoSeas => two_seas(u, v, variant, phase),
        Layout::TwirlingSea => twirling_sea(u, v, variant),
        Layout::NoMansLand => no_mans_land(u, v, variant, phase),
        Layout::ForbiddenCenter => forbidden_center(u, v, phase),
    }
}

fn circle_world(u: f32, v: f32, phase: f32) -> [u8; 4] {
    let (r, t) = polar(u, v);
    let inner = 0.24 * wobble(t, phase, 0.07, 0.04);
    let outer = 0.46 * wobble(t, phase + 1.1, 0.05, 0.03);
    if r > inner && r < outer {
        LAND
    } else {
        SEA
    }
}

fn small_lakes(u: f32, v: f32, variant: u32, phase: f32) -> [u8; 4] {
    let lakes: &[(f32, f32)] = match variant {
        0 => &[(0.27, 0.28), (0.70, 0.24), (0.24, 0.72), (0.73, 0.70)],
        1 => &[(0.30, 0.30), (0.66, 0.35), (0.42, 0.70), (0.78, 0.72)],
        _ => &[(0.22, 0.45), (0.52, 0.22), (0.55, 0.72), (0.80, 0.45)],
    };
    if any_blob(u, v, lakes, 0.105, 0.105, phase) {
        SEA
    } else {
        LAND
    }
}

fn one_sea(u: f32, v: f32, variant: u32, phase: f32) -> [u8; 4] {
    let (cx, cy, rx, ry) = if variant == 0 {
        (0.44, 0.50, 0.40, 0.30)
    } else {
        (0.55, 0.45, 0.34, 0.35)
    };
    if in_blob(u, v, cx, cy, rx, ry, phase) {
        SEA
    } else {
        LAND
    }
}

fn two_seas(u: f32, v: f32, variant: u32, phase: f32) -> [u8; 4] {
    let seas: &[(f32, f32)] = if variant == 0 {
        &[(0.28, 0.30), (0.72, 0.70)]
    } else {
        &[(0.30, 0.68), (0.70, 0.30)]
    };
    if any_blob(u, v, seas, 0.26, 0.215, phase) {
        SEA
    } else {
        LAND
    }
}

fn twirling_sea(u: f32, v: f32, variant: u32) -> [u8; 4] {
    let (r, t) = polar(u, v);
    let t = if variant == 1 { -t } else { t };
    let pitch = 0.17 + variant as f32 * 0.03;
    let s = (t / TAU + r / pitch).rem_euclid(1.0);
    if s < 0.32 {
        SEA
    } else {
        LAND
    }
}

fn no_mans_land(u: f32, v: f32, variant: u32, phase: f32) -> [u8; 4] {
    let jitter = 0.018 * ((v * 9.0 + phase) * TAU * 0.5).sin()
        + 0.010 * ((v * 21.0 - phase) * TAU * 0.5).sin();
    let spread = match variant {
        0 => 0.0,
        1 => 0.02,
        _ => -0.02,
    };
    let b0 = 0.13 + jitter + spread;
    let b1 = 0.29 + jitter + spread;
    let b2 = 0.71 - jitter - spread;
    let b3 = 0.87 - jitter - spread;
    if u < b0 || u >= b3 {
        return SEA;
    }
    if u < b1 || u >= b2 {
        return LAND;
    }
    let holes: &[(f32, f32)] = match variant {
        0 => &[(0.42, 0.22), (0.58, 0.55), (0.44, 0.82)],
        1 => &[(0.50, 0.18), (0.44, 0.50), (0.56, 0.84)],
        _ => &[(0.38, 0.30), (0.60, 0.30), (0.48, 0.76)],
    };
    if any_blob(u, v, holes, 0.09, 0.10, phase) {
        NOSTART_SEA
    } else {
        NOSTART_LAND
    }
}

fn forbidden_center(u: f32, v: f32, phase: f32) -> [u8; 4] {
    let (r, t) = polar(u, v);
    let outer = 0.26 * wobble(t, phase, 0.10, 0.05);
    let inner = 0.16 * wobble(t, phase + 2.0, 0.12, 0.06);
    if r < inner {
        return NOSTART_SEA;
    }
    if r < outer {
        return NOSTART_LAND;
    }
    let lakes = [(0.20, 0.22), (0.80, 0.24), (0.18, 0.78), (0.82, 0.76)];
    if any_blob(u, v, &lakes, 0.09, 0.09, phase) {
        SEA
    } else {
        LAND
    }
}

const CAVE_FLOOR: [u8; 4] = [0, 255, 0, 255];
const CAVE_ROCK: [u8; 4] = [255, 0, 27, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaveLayout {
    Random,
    SmallCaves,
    OneCave,
    TwoCaves,
    CircleCave,
}

impl CaveLayout {
    pub const ALL: [CaveLayout; 5] = [
        CaveLayout::Random,
        CaveLayout::SmallCaves,
        CaveLayout::OneCave,
        CaveLayout::TwoCaves,
        CaveLayout::CircleCave,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CaveLayout::Random => "Random",
            CaveLayout::SmallCaves => "Small Caves",
            CaveLayout::OneCave => "One Cave",
            CaveLayout::TwoCaves => "Two Caves",
            CaveLayout::CircleCave => "Circle Cave",
        }
    }

    pub fn engine_index(self) -> i32 {
        match self {
            CaveLayout::Random => 1,
            CaveLayout::SmallCaves => 2,
            CaveLayout::OneCave => 3,
            CaveLayout::TwoCaves => 4,
            CaveLayout::CircleCave => 5,
        }
    }
}

pub fn cave_layout_blue_acc(_kind: CaveLayout) -> i32 {
    1
}

pub fn cave_layout_variants(kind: CaveLayout) -> u32 {
    match kind {
        CaveLayout::Random => 0,
        CaveLayout::SmallCaves => 5,
        CaveLayout::OneCave => 3,
        CaveLayout::TwoCaves => 4,
        CaveLayout::CircleCave => 1,
    }
}

pub fn cave_layout_variant_for_seed(kind: CaveLayout, seed: u32) -> u32 {
    let n = cave_layout_variants(kind);
    if n < 2 {
        0
    } else {
        seed % n
    }
}

pub fn cave_layout_blueprint(kind: CaveLayout, w: i32, h: i32) -> Option<Blueprint> {
    cave_layout_blueprint_variant(kind, 0, w, h)
}

pub fn cave_layout_blueprint_variant(
    kind: CaveLayout,
    variant: u32,
    w: i32,
    h: i32,
) -> Option<Blueprint> {
    if kind == CaveLayout::Random || w < 1 || h < 1 {
        return None;
    }
    let variant = variant % cave_layout_variants(kind).max(1);
    let mut bgra = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let floor = match kind {
                CaveLayout::Random => false,
                CaveLayout::SmallCaves => small_caves(u, v, variant),
                CaveLayout::OneCave => one_cave(u, v, variant),
                CaveLayout::TwoCaves => two_caves(u, v, variant),
                CaveLayout::CircleCave => circle_cave(u, v),
            };
            bgra.extend_from_slice(if floor { &CAVE_FLOOR } else { &CAVE_ROCK });
        }
    }
    Some(Blueprint { w, h, bgra })
}

fn ripple(n: u32, k: u32) -> f32 {
    let x = (n as f32 * 12.9898 + k as f32 * 78.233).sin() * 43758.547;
    x - x.floor()
}

fn segment_distance(u: f32, v: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((u - a.0) * dx + (v - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (px, py) = (a.0 + t * dx - u, a.1 + t * dy - v);
    (px * px + py * py).sqrt()
}

fn near_path(u: f32, v: f32, path: &[(f32, f32)], half: f32, phase: f32) -> bool {
    let edge = half * wobble(u * 9.0 + v * 7.0, phase, 0.10, 0.06);
    path.windows(2)
        .any(|s| segment_distance(u, v, s[0], s[1]) < edge)
}

fn small_caves(u: f32, v: f32, variant: u32) -> bool {
    let phase = variant as f32 * 1.7;
    for j in 0..3 {
        for i in 0..4 {
            let n = variant * 16 + j * 4 + i;
            let cx = (i as f32 + 0.5) / 4.0 + (ripple(n, 1) - 0.5) * 0.12;
            let cy = (j as f32 + 0.5) / 3.0 + (ripple(n, 2) - 0.5) * 0.14;
            let r = 0.042 + ripple(n, 3) * 0.022;
            if in_blob(u, v, cx, cy, r, r, phase + n as f32) {
                return true;
            }
        }
    }
    false
}

fn one_cave(u: f32, v: f32, variant: u32) -> bool {
    let phase = variant as f32 * 1.7;
    if variant == 1 && in_blob(u, v, 0.52, 0.47, 0.085, 0.075, phase + 4.0) {
        return false;
    }
    if in_blob(u, v, 0.5, 0.5, 0.20, 0.17, phase) {
        return true;
    }
    let arms: &[(f32, f32)] = match variant {
        0 => &[
            (0.10, 0.55),
            (0.30, 0.15),
            (0.72, 0.12),
            (0.88, 0.50),
            (0.60, 0.90),
        ],
        1 => &[
            (0.12, 0.30),
            (0.45, 0.08),
            (0.85, 0.30),
            (0.80, 0.80),
            (0.25, 0.85),
        ],
        _ => &[
            (0.08, 0.50),
            (0.30, 0.10),
            (0.70, 0.10),
            (0.92, 0.55),
            (0.70, 0.92),
            (0.30, 0.90),
        ],
    };
    arms.iter()
        .enumerate()
        .any(|(k, end)| near_path(u, v, &[(0.5, 0.5), *end], 0.075, phase + k as f32 * 1.3))
}

fn two_caves(u: f32, v: f32, variant: u32) -> bool {
    let phase = variant as f32 * 1.7;
    let bodies: [&[(f32, f32)]; 2] = match variant {
        0 => [
            &[(0.12, 0.30), (0.30, 0.15), (0.52, 0.28), (0.45, 0.45)],
            &[(0.20, 0.72), (0.45, 0.80), (0.70, 0.72), (0.85, 0.55)],
        ],
        1 => [
            &[(0.15, 0.25), (0.40, 0.18), (0.60, 0.35), (0.85, 0.25)],
            &[(0.15, 0.75), (0.35, 0.65), (0.60, 0.80), (0.85, 0.70)],
        ],
        2 => [
            &[(0.25, 0.15), (0.30, 0.45), (0.22, 0.80)],
            &[(0.72, 0.15), (0.80, 0.45), (0.70, 0.80)],
        ],
        _ => [
            &[(0.15, 0.20), (0.45, 0.30), (0.60, 0.15)],
            &[(0.20, 0.85), (0.50, 0.65), (0.85, 0.80)],
        ],
    };
    near_path(u, v, bodies[0], 0.075, phase) || near_path(u, v, bodies[1], 0.075, phase + 2.1)
}

fn circle_cave(u: f32, v: f32) -> bool {
    let (r, t) = polar(u, v);
    let inner = 0.23 * wobble(t, 0.4, 0.04, 0.02);
    let outer = 0.35 * wobble(t, 1.9, 0.03, 0.02);
    r > inner && r < outer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::fnv64;
    use crate::height::{bottom_up_rgba, build_randboard_rgb_classification_mask};

    fn counts(kind: Layout, variant: u32) -> (usize, [usize; 4]) {
        let bp = layout_blueprint_variant(kind, variant, 256, 256).unwrap();
        let ordered = bottom_up_rgba(&bp);
        let mask = build_randboard_rgb_classification_mask(&ordered.bgra, ordered.w, ordered.h);
        let mut c = [0usize; 4];
        for cell in &mask.cells {
            match cell {
                1 => c[0] += 1,
                -1 => c[1] += 1,
                2 => c[2] += 1,
                _ => c[3] += 1,
            }
        }
        (mask.cells.len(), c)
    }

    #[test]
    fn standard_has_no_blueprint() {
        assert!(layout_blueprint(Layout::Standard, 64, 64).is_none());
        assert_eq!(layout_variants(Layout::Standard), 0);
        assert_eq!(layout_variant_for_seed(Layout::Standard, 7), 0);
    }

    #[test]
    fn every_layout_but_standard_draws_an_image() {
        for kind in Layout::ALL {
            if kind == Layout::Standard {
                continue;
            }
            let bp = layout_blueprint(kind, 32, 24).unwrap();
            assert_eq!((bp.w, bp.h), (32, 24));
            assert_eq!(bp.bgra.len(), 32 * 24 * 4);
        }
    }

    #[test]
    fn sea_fractions_stay_in_a_playable_range() {
        for kind in Layout::ALL {
            if kind == Layout::Standard {
                continue;
            }
            for variant in 0..layout_variants(kind) {
                let bp = layout_blueprint_variant(kind, variant, 256, 256).unwrap();
                let ordered = bottom_up_rgba(&bp);
                let f =
                    build_randboard_rgb_classification_mask(&ordered.bgra, ordered.w, ordered.h)
                        .sea_frac;
                assert!(
                    f > 0.08 && f < 0.60,
                    "{} variant {variant} sea fraction {f}",
                    kind.label()
                );
            }
        }
    }

    #[test]
    fn only_two_layouts_reserve_no_mans_land() {
        for kind in Layout::ALL {
            if kind == Layout::Standard {
                continue;
            }
            for variant in 0..layout_variants(kind) {
                let (n, c) = counts(kind, variant);
                let reserved = c[2] + c[3];
                let wants = matches!(kind, Layout::NoMansLand | Layout::ForbiddenCenter);
                assert_eq!(
                    reserved > 0,
                    wants,
                    "{} variant {variant} reserved {reserved} of {n}",
                    kind.label()
                );
                if wants {
                    assert!(c[2] > 0 && c[3] > 0);
                }
            }
        }
    }

    #[test]
    fn no_mans_land_keeps_both_edges_startable() {
        let (_, c) = counts(Layout::NoMansLand, 0);
        assert!(c[0] > 0);
        let bp = layout_blueprint(Layout::NoMansLand, 256, 256).unwrap();
        let at = |x: usize, y: usize| {
            let p = (y * 256 + x) * 4;
            [bp.bgra[p], bp.bgra[p + 1], bp.bgra[p + 2], bp.bgra[p + 3]]
        };
        assert_eq!(at(55, 128), LAND);
        assert_eq!(at(200, 128), LAND);
        assert_eq!(at(4, 128), SEA);
        assert_eq!(at(251, 128), SEA);
    }

    #[test]
    fn forbidden_center_reserves_the_middle() {
        let bp = layout_blueprint(Layout::ForbiddenCenter, 256, 256).unwrap();
        let p = (128 * 256 + 128) * 4;
        assert_eq!(bp.bgra[p..p + 4], NOSTART_SEA);
        let q = (10 * 256 + 128) * 4;
        assert_eq!(bp.bgra[q..q + 4], LAND);
    }

    #[test]
    fn drawing_is_deterministic_and_variants_differ() {
        for kind in Layout::ALL {
            if kind == Layout::Standard {
                continue;
            }
            let a = layout_blueprint_variant(kind, 0, 128, 128).unwrap();
            let b = layout_blueprint_variant(kind, 0, 128, 128).unwrap();
            assert_eq!(a.bgra, b.bgra);
            for variant in 1..layout_variants(kind) {
                let other = layout_blueprint_variant(kind, variant, 128, 128).unwrap();
                assert_ne!(a.bgra, other.bgra, "{} variant {variant}", kind.label());
            }
            assert_ne!(fnv64(&a.bgra), 0);
        }
    }

    #[test]
    fn a_variant_index_wraps_and_follows_the_seed() {
        let n = layout_variants(Layout::SmallLakes);
        assert_eq!(n, 3);
        assert_eq!(
            layout_blueprint_variant(Layout::SmallLakes, n, 32, 32),
            layout_blueprint_variant(Layout::SmallLakes, 0, 32, 32)
        );
        assert_eq!(layout_variant_for_seed(Layout::SmallLakes, 7), 1);
        assert_eq!(layout_variant_for_seed(Layout::CircleWorld, 7), 0);
    }

    #[test]
    fn the_accuracy_table_is_two_for_the_forbidden_centre_and_one_elsewhere() {
        for kind in Layout::ALL {
            let want = if kind == Layout::ForbiddenCenter {
                2
            } else {
                1
            };
            assert_eq!(layout_blue_acc(kind), want);
        }
    }

    fn floor_fraction(kind: CaveLayout, variant: u32) -> f32 {
        let bp = cave_layout_blueprint_variant(kind, variant, 256, 256).unwrap();
        let ordered = bottom_up_rgba(&bp);
        let mask = build_randboard_rgb_classification_mask(&ordered.bgra, ordered.w, ordered.h);
        for cell in &mask.cells {
            assert!(*cell == 1 || *cell == -1, "cave blueprints hold no grey");
        }
        1.0 - mask.sea_frac
    }

    #[test]
    fn the_random_cave_layout_has_no_blueprint() {
        assert!(cave_layout_blueprint(CaveLayout::Random, 64, 64).is_none());
        assert_eq!(cave_layout_variants(CaveLayout::Random), 0);
        assert_eq!(cave_layout_variant_for_seed(CaveLayout::Random, 9), 0);
    }

    #[test]
    fn cave_layouts_follow_the_engine_index_and_variant_files() {
        let want = [(1, 0), (2, 5), (3, 3), (4, 4), (5, 1)];
        for (kind, (index, files)) in CaveLayout::ALL.iter().zip(want) {
            assert_eq!(kind.engine_index(), index);
            assert_eq!(cave_layout_variants(*kind), files);
            assert_eq!(cave_layout_blue_acc(*kind), 1);
        }
        assert_eq!(cave_layout_variant_for_seed(CaveLayout::SmallCaves, 7), 2);
        assert_eq!(
            cave_layout_blueprint_variant(CaveLayout::OneCave, 3, 32, 32),
            cave_layout_blueprint_variant(CaveLayout::OneCave, 0, 32, 32)
        );
    }

    #[test]
    fn cave_floors_are_green_on_blue_rock() {
        let bp = cave_layout_blueprint(CaveLayout::CircleCave, 256, 256).unwrap();
        let at = |x: usize, y: usize| {
            let p = (y * 256 + x) * 4;
            [bp.bgra[p], bp.bgra[p + 1], bp.bgra[p + 2], bp.bgra[p + 3]]
        };
        assert_eq!(at(128, 128), CAVE_ROCK);
        assert_eq!(at(128, 52), CAVE_FLOOR);
        assert_eq!(at(3, 3), CAVE_ROCK);
    }

    #[test]
    fn cave_floor_fractions_match_the_shipped_shapes() {
        let ranges = [
            (CaveLayout::SmallCaves, 0.06, 0.16),
            (CaveLayout::OneCave, 0.30, 0.55),
            (CaveLayout::TwoCaves, 0.15, 0.35),
            (CaveLayout::CircleCave, 0.15, 0.30),
        ];
        for (kind, lo, hi) in ranges {
            for variant in 0..cave_layout_variants(kind) {
                let f = floor_fraction(kind, variant);
                assert!(
                    f > lo && f < hi,
                    "{} variant {variant} floor fraction {f}",
                    kind.label()
                );
            }
        }
    }

    #[test]
    fn cave_drawing_is_deterministic_and_variants_differ() {
        for kind in CaveLayout::ALL {
            if kind == CaveLayout::Random {
                continue;
            }
            let a = cave_layout_blueprint_variant(kind, 0, 128, 128).unwrap();
            let b = cave_layout_blueprint_variant(kind, 0, 128, 128).unwrap();
            assert_eq!(a.bgra, b.bgra);
            for variant in 1..cave_layout_variants(kind) {
                let other = cave_layout_blueprint_variant(kind, variant, 128, 128).unwrap();
                assert_ne!(a.bgra, other.bgra, "{} variant {variant}", kind.label());
            }
            assert_ne!(fnv64(&a.bgra), 0);
        }
    }
}
