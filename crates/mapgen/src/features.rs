use rayon::prelude::*;

use crate::graph::{
    self, clear_border_flags, count_region_land_water_pixels, get_border_flags, removenbor,
    set_border_flags, terrain, BORDER_BRIDGE, BORDER_MOUNTAIN, BORDER_MOUNTAIN_PASS, BORDER_RIVER,
    BORDER_WALL, TERRAIN_DEEP, TERRAIN_FRESHWATER, TERRAIN_MOUNTAIN, TERRAIN_NOSTART, TERRAIN_SEA,
};
use crate::options::{Blueprint, Options};
use crate::stage::{Control, Sink, Stage};
use crate::world::{World, MAX_NBORS};

pub const MARGIN_PX: i32 = 0xc0;
pub const ISLAND_ITER_CAP: i32 = 200;
pub const ISLAND_RAISE: f32 = 10.0;
pub const ISLAND_LOWER: f32 = -2.5;
pub const MOUNTAIN_TRY_CAP: i32 = 10_000;

fn clampi(v: i32, lo: i32, hi: i32) -> i32 {
    let v = if v < lo { lo } else { v };
    if v >= hi {
        hi
    } else {
        v
    }
}

fn owner_at(w: &World, x: i32, y: i32) -> i32 {
    if x < 0 || x >= w.w || y < 0 || y >= w.h {
        return 0;
    }
    i32::from(w.owner[(w.w * y + x) as usize])
}

fn height_clamped(w: &World, x: i32, y: i32) -> f32 {
    let px = clampi(x.max(0), 0, w.w - 1);
    let py = clampi(y.max(0), 0, w.h - 1);
    w.heights[(py * w.w + px) as usize]
}

struct IslandStep {
    first: u32,
    first_scale: f32,
    first_owned: bool,
    second: u32,
    second_scale: f32,
    second_owned: bool,
}

struct IslandPlan {
    steps: Vec<IslandStep>,
    total: i32,
}

impl IslandPlan {
    fn total(&self) -> i32 {
        self.total
    }
}

fn island_scale(dist: i32, refd: f32) -> f32 {
    if (dist as f32) < refd {
        dist as f32 / refd
    } else {
        1.0
    }
}

fn island_plan(w: &mut World, lnr: i32) -> IslandPlan {
    let mut steps = Vec::new();
    if lnr <= 0 || lnr as usize >= w.bbox.len() {
        return IslandPlan { steps, total: 0 };
    }
    let bb = w.bbox[lnr as usize];
    let field = graph::province_edge_field(w, lnr);
    let refd = w.spacing * 0.8;
    let mut prev: i32 = -1;
    let mut y = i32::from(bb[2]);
    while y <= i32::from(bb[3]) {
        let mut x = i32::from(bb[0]);
        while x <= i32::from(bb[1]) {
            if owner_at(w, x, y) == lnr {
                let d1 = graph::get_province_edge_distance_from(w, lnr, x + 1, y, &field);
                prev = if prev < 0 {
                    graph::get_province_edge_distance_from(w, lnr, x, y, &field)
                } else {
                    (d1 + prev) / 2
                };
                let ix = clampi(if x >= 0 { x + 1 } else { 0 }, 0, w.w - 1);
                let iy = clampi(y.max(0), 0, w.h - 1);
                let jx = clampi(x.max(0), 0, w.w - 1);
                let first = (iy * w.w + ix) as usize;
                let second = (iy * w.w + jx) as usize;
                steps.push(IslandStep {
                    first: first as u32,
                    first_scale: island_scale(d1, refd),
                    first_owned: i32::from(w.owner[first]) == lnr,
                    second: second as u32,
                    second_scale: island_scale(prev, refd),
                    second_owned: i32::from(w.owner[second]) == lnr,
                });
                prev = d1;
            }
            x += 2;
        }
        y += 1;
    }
    let total = count_region_land_water_pixels(w, lnr).0;
    IslandPlan { steps, total }
}

fn island_add(
    heights: &mut [f32],
    sea: f32,
    idx: u32,
    scale: f32,
    amount: f32,
    owned: bool,
) -> i32 {
    let slot = &mut heights[idx as usize];
    let before = *slot;
    let after = before + scale * amount;
    *slot = after;
    if owned {
        i32::from(after >= sea) - i32::from(before >= sea)
    } else {
        0
    }
}

impl IslandPlan {
    fn apply(&self, w: &mut World, amount: f32, land: &mut i32) {
        let sea = w.sea_level;
        let heights = &mut w.heights;
        for s in &self.steps {
            *land += island_add(heights, sea, s.first, s.first_scale, amount, s.first_owned);
            *land += island_add(
                heights,
                sea,
                s.second,
                s.second_scale,
                amount,
                s.second_owned,
            );
        }
    }
}

#[cfg(test)]
pub fn raise_extra_island_midland(w: &mut World, lnr: i32, amount: f32) {
    if lnr <= 0 || lnr as usize >= w.bbox.len() {
        return;
    }
    let bb = w.bbox[lnr as usize];
    let refd = w.spacing * 0.8;
    let mut prev: i32 = -1;
    let mut y = i32::from(bb[2]);
    while y <= i32::from(bb[3]) {
        let mut x = i32::from(bb[0]);
        while x <= i32::from(bb[1]) {
            if owner_at(w, x, y) == lnr {
                let d1 = graph::get_province_edge_distance(w, lnr, x + 1, y);
                prev = if prev < 0 {
                    graph::get_province_edge_distance(w, lnr, x, y)
                } else {
                    (d1 + prev) / 2
                };

                let a1 = if (d1 as f32) < refd {
                    (d1 as f32 / refd) * amount
                } else {
                    amount
                };
                let ix = clampi(if x >= 0 { x + 1 } else { 0 }, 0, w.w - 1);
                let iy = clampi(y.max(0), 0, w.h - 1);
                w.heights[(iy * w.w + ix) as usize] += a1;

                let a2 = if (prev as f32) < refd {
                    (prev as f32 / refd) * amount
                } else {
                    amount
                };
                let jx = clampi(x.max(0), 0, w.w - 1);
                w.heights[(iy * w.w + jx) as usize] += a2;

                prev = d1;
            }
            x += 2;
        }
        y += 1;
    }
}

pub fn create_extra_islands(w: &mut World, opts: &Options) -> i32 {
    let knob = opts.extra_islands;
    if knob < 1 {
        return 0;
    }
    let mut made = 0;
    let n = w.nprov() as i32;
    for a in 1..=n {
        if (terrain(w, a) & TERRAIN_SEA) == 0 {
            continue;
        }
        if w.crt.below(100) >= knob {
            continue;
        }
        let nb = graph::random_neighbor_province(w, a, false, true);
        if nb >= 1 {
            continue;
        }
        let (_, mut land, mut water) = count_region_land_water_pixels(w, a);
        let mut plan: Option<IslandPlan> = None;
        let mut i = 0i32;
        let mut j = 0i32;
        if land < water {
            loop {
                j = i + 1;
                if i > ISLAND_ITER_CAP - 1 {
                    break;
                }
                let p = plan.get_or_insert_with(|| island_plan(w, a));
                p.apply(w, ISLAND_RAISE, &mut land);
                water = p.total() - land;
                i = j;
                if land >= water {
                    break;
                }
            }
        }
        if water < land {
            loop {
                if j > ISLAND_ITER_CAP - 1 {
                    break;
                }
                let p = plan.get_or_insert_with(|| island_plan(w, a));
                p.apply(w, ISLAND_LOWER, &mut land);
                water = p.total() - land;
                j += 1;
                if water >= land {
                    break;
                }
            }
        }
        made += 1;
        w.provinces[a as usize].terrain &= !(TERRAIN_DEEP | TERRAIN_SEA);
    }
    made
}

pub fn classify_region_border_height_indexed(
    w: &World,
    idx: &graph::BorderIndex,
    a: i32,
    b: i32,
) -> i32 {
    let ra = graph::count_province_river_borders(w, a);
    let rb = graph::count_province_river_borders(w, b);
    if (ra > 0 && rb > 0) || ra > 1 || rb > 1 {
        return 0;
    }
    if a as usize >= w.bbox.len() {
        return 0;
    }
    let bb = w.bbox[a as usize];
    let x0 = i32::from(bb[0]) - 1;
    let x1 = i32::from(bb[1]) + 1;
    let y0 = i32::from(bb[2]) - 1;
    let y1 = i32::from(bb[3]) + 1;
    let mut out = 0i32;
    for &(_, i) in idx.pixels(a, b) {
        let x = (i % w.w as u32) as i32;
        let y = (i / w.w as u32) as i32;
        if x < x0 || x > x1 || y < y0 || y > y1 {
            continue;
        }
        let h = w.heights[i as usize];
        if h == crate::writers::RIVER_SENTINEL {
            return 2;
        }
        if h < w.sea_level {
            out = 1;
        }
    }
    if out == 1 {
        let land = graph::count_land_border_pixels_indexed(w, idx, a, b);
        if (land as f32) < w.spacing * 0.05 {
            out = 0;
        }
    }
    out
}

#[cfg(test)]
pub fn classify_region_border_height(w: &World, a: i32, b: i32) -> i32 {
    let ra = graph::count_province_river_borders(w, a);
    let rb = graph::count_province_river_borders(w, b);
    if (ra > 0 && rb > 0) || ra > 1 || rb > 1 {
        return 0;
    }
    if a as usize >= w.bbox.len() {
        return 0;
    }
    let bb = w.bbox[a as usize];
    let mut out = 0i32;
    let mut y = i32::from(bb[2]) - 1;
    while y <= i32::from(bb[3]) + 1 {
        if y >= 0 && y + 1 < w.h {
            let mut x = i32::from(bb[0]) - 1;
            while x <= i32::from(bb[1]) + 1 {
                if x >= 0 && x + 1 < w.w {
                    let left = owner_at(w, x, y);
                    let right = owner_at(w, x + 1, y);
                    let mut other = right;
                    let mut skip = false;
                    if left == right {
                        other = owner_at(w, x, y + 1);
                        if left == other {
                            skip = true;
                        }
                    }
                    if !skip && (a == left || a == other) && (b == left || b == other) {
                        let h = height_clamped(w, x, y);
                        if h == crate::writers::RIVER_SENTINEL {
                            return 2;
                        }
                        if h < w.sea_level {
                            out = 1;
                        }
                    }
                }
                x += 1;
            }
        }
        y += 1;
    }
    if out == 1 {
        let land = graph::count_land_border_pixels_between_regions(w, a, b);
        if (land as f32) < w.spacing * 0.05 {
            out = 0;
        }
    }
    out
}

pub fn carve_region_boundary_channel(w: &mut World, a: i32, b: i32) {
    if a as usize >= w.bbox.len() || b as usize >= w.bbox.len() {
        return;
    }
    let ba = w.bbox[a as usize];
    let bb = w.bbox[b as usize];
    let x0 = i32::from(ba[0].min(bb[0]));
    let x1 = i32::from(ba[1].max(bb[1]));
    let y0 = i32::from(ba[2].min(bb[2]));
    let y1 = i32::from(ba[3].max(bb[3]));
    if x0 < 0 || y0 < 0 {
        return;
    }
    let r0 = (w.spacing * 0.05) as i32;
    let q = r0 / 4;
    let mut wob = 0i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let left = owner_at(w, x, y);
            if left != a && left != b {
                continue;
            }
            let right = owner_at(w, x + 1, y);
            let other = if left == right {
                let below = owner_at(w, x, y + 1);
                if below == left {
                    continue;
                }
                below
            } else {
                right
            };
            if other != a && other != b {
                continue;
            }
            let r = r0 + wob;
            for dy in -r..=r {
                for dx in -r..=r {
                    let mut px = x + dx;
                    if w.hwrap {
                        if px < 0 {
                            px += w.w;
                        }
                        if px >= w.w {
                            px -= w.w;
                        }
                    }
                    let mut py = y + dy;
                    if w.vwrap {
                        if py < 0 {
                            py += w.h;
                        }
                        if py >= w.h {
                            py -= w.h;
                        }
                    }
                    px = clampi(clampi(px, 0, w.w - 1).max(0), 0, w.w - 1);
                    py = clampi(clampi(py, 0, w.h - 1).max(0), 0, w.h - 1);
                    let i = (py * w.w + px) as usize;
                    if w.heights[i] >= w.sea_level
                        && ((dy * dy + dx * dx) as f32) <= ((r * r) as f32)
                    {
                        w.heights[i] = crate::writers::RIVER_SENTINEL;
                    }
                }
            }
            if w.crt.below(100) < 10 {
                let v = wob + w.crt.below(3) - 1;
                wob = v.max(-q).min(q);
            }
        }
    }
}

pub fn carve_region_boundary_channel_indexed(
    w: &mut World,
    idx: &graph::BorderIndex,
    a: i32,
    b: i32,
) {
    if a as usize >= w.bbox.len() || b as usize >= w.bbox.len() {
        return;
    }
    let ba = w.bbox[a as usize];
    let bb = w.bbox[b as usize];
    let x0 = i32::from(ba[0].min(bb[0]));
    let x1 = i32::from(ba[1].max(bb[1]));
    let y0 = i32::from(ba[2].min(bb[2]));
    let y1 = i32::from(ba[3].max(bb[3]));
    if x0 < 0 || y0 < 0 {
        return;
    }
    let r0 = (w.spacing * 0.05) as i32;
    let q = r0 / 4;
    let mut wob = 0i32;
    let mut pixels: Vec<u32> = idx.pixels(a, b).iter().map(|e| e.1).collect();
    if y1 == w.h - 1 {
        let row = ((w.h - 1) * w.w) as usize;
        for x in x0..=x1.min(w.w - 1) {
            let left = owner_at(w, x, w.h - 1);
            if left != a && left != b {
                continue;
            }
            let right = owner_at(w, x + 1, w.h - 1);
            if right == left || (right != a && right != b) {
                continue;
            }
            pixels.push((row + x as usize) as u32);
        }
    }
    for i in pixels {
        let x = (i % w.w as u32) as i32;
        let y = (i / w.w as u32) as i32;
        let r = r0 + wob;
        for dy in -r..=r {
            for dx in -r..=r {
                let mut px = x + dx;
                if w.hwrap {
                    if px < 0 {
                        px += w.w;
                    }
                    if px >= w.w {
                        px -= w.w;
                    }
                }
                let mut py = y + dy;
                if w.vwrap {
                    if py < 0 {
                        py += w.h;
                    }
                    if py >= w.h {
                        py -= w.h;
                    }
                }
                px = clampi(clampi(px, 0, w.w - 1).max(0), 0, w.w - 1);
                py = clampi(clampi(py, 0, w.h - 1).max(0), 0, w.h - 1);
                let j = (py * w.w + px) as usize;
                if w.heights[j] >= w.sea_level && ((dy * dy + dx * dx) as f32) <= ((r * r) as f32) {
                    w.heights[j] = crate::writers::RIVER_SENTINEL;
                }
            }
        }
        if w.crt.below(100) < 10 {
            let v = wob + w.crt.below(3) - 1;
            wob = v.max(-q).min(q);
        }
    }
}

pub fn create_random_map_rivers(w: &mut World, opts: &Options, idx: &graph::BorderIndex) {
    let n = w.nprov() as i32;
    let target = ((n + 1) * opts.river_part) / 15;
    let tries = (n + 1) * target;
    let adj = graph::adjacency_bitset(w);
    let mut t = 0i32;
    let mut st = w.crt.state;
    while t < tries {
        let (ra, rb) = crate::rng::below_pair_of(st, n);
        st = crate::rng::crt_advance2(st);
        let a = ra + 1;
        let b = rb + 1;
        if adj.linked(a, b)
            && (terrain(w, a) & TERRAIN_SEA) == 0
            && (terrain(w, b) & TERRAIN_SEA) == 0
        {
            let q = classify_region_border_height_indexed(w, idx, a, b);
            if (t < target && q > 0) || q > 1 {
                set_border_flags(w, a, b, BORDER_RIVER);
                w.crt.state = st;
                carve_region_boundary_channel_indexed(w, idx, a, b);
                st = w.crt.state;
            }
        }
        t += 1;
    }
    w.crt.state = st;
}

pub fn infer_river_borders(w: &mut World, idx: &graph::BorderIndex) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        if (terrain(w, a) & TERRAIN_SEA) != 0 {
            continue;
        }
        let nbors = w.provinces[a as usize].nbors.clone();
        for &nb in nbors.iter().take(MAX_NBORS) {
            let b = i32::from(nb);
            if b < 1 {
                break;
            }
            if (terrain(w, b) & TERRAIN_SEA) != 0 {
                continue;
            }
            if graph::count_land_border_pixels_indexed(w, idx, a, b) >= 1 {
                continue;
            }
            let water = graph::count_water_border_pixels_indexed(w, idx, a, b);
            let carved = graph::count_carved_border_pixels_indexed(w, idx, a, b);
            if water > 0 || carved > 0 {
                set_border_flags(w, a, b, BORDER_RIVER);
            }
        }
    }
}

pub fn create_border_mountains(w: &mut World, opts: &Options, idx: &graph::BorderIndex) {
    let n = w.nprov() as i32;
    let mount_part = (f64::from(opts.mount_part) * 0.01) as f32;
    let requested = crate::provinces::requested_province_count(opts.provinces);
    let target = ((requested as f32) * mount_part * 0.5) as i32;
    let mut made = 0i32;
    let mut tries = 0i32;
    while made < target {
        let a = w.crt.below(n) + 1;
        let b = graph::choose_random_passable_neighbor(w, a);
        let ok = !(b < 1
            || (terrain(w, a) & TERRAIN_SEA) != 0
            || (terrain(w, b) & TERRAIN_SEA) != 0
            || ((terrain(w, a) & TERRAIN_MOUNTAIN) != 0
                && (terrain(w, b) & TERRAIN_MOUNTAIN) != 0)
            || graph::count_water_border_pixels_indexed(w, idx, b, a) > 0);
        if ok {
            w.provinces[a as usize].terrain |= TERRAIN_MOUNTAIN;
            w.provinces[b as usize].terrain |= TERRAIN_MOUNTAIN;
            set_border_flags(w, b, a, BORDER_MOUNTAIN);
            let (set, clr) = if w.crt.below(100) < 20 {
                (BORDER_MOUNTAIN_PASS, BORDER_WALL)
            } else {
                (BORDER_WALL, BORDER_MOUNTAIN_PASS)
            };
            set_border_flags(w, b, a, set);
            clear_border_flags(w, b, a, clr);
            if w.crt.below(100) < 90 {
                for _ in 0..10 {
                    let c = graph::choose_random_passable_neighbor(w, a);
                    if c > 0
                        && (terrain(w, c) & TERRAIN_SEA) == 0
                        && graph::count_water_border_pixels_indexed(w, idx, c, a) == 0
                    {
                        w.provinces[c as usize].terrain |= TERRAIN_MOUNTAIN;
                        set_border_flags(w, c, a, BORDER_MOUNTAIN);
                        if w.crt.below(100) < 20 {
                            set_border_flags(w, c, a, BORDER_MOUNTAIN_PASS);
                            clear_border_flags(w, c, a, BORDER_WALL);
                        } else {
                            set_border_flags(w, c, a, BORDER_WALL);
                            clear_border_flags(w, c, a, BORDER_MOUNTAIN_PASS);
                        }
                        break;
                    }
                }
            }
            made += 1;
        }
        tries += 1;
        if tries >= MOUNTAIN_TRY_CAP {
            return;
        }
    }
}

pub fn mark_freshwater(w: &mut World) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        if (terrain(w, a) & TERRAIN_SEA) != 0 {
            continue;
        }
        let p = w.provinces[a as usize].clone();
        for i in 0..p.nbors.len().min(MAX_NBORS) {
            let b = i32::from(p.nbors[i]);
            if b < 1 {
                break;
            }
            if (terrain(w, b) & TERRAIN_SEA) == 0
                && (p.border[i] & (BORDER_BRIDGE | BORDER_RIVER)) != 0
            {
                w.provinces[a as usize].terrain |= TERRAIN_FRESHWATER;
            }
        }
    }
}

pub fn find_region_entry_on_center_line(w: &World, a: i32, b: i32) -> Option<(i32, i32)> {
    if a < 0 || b < 0 {
        return None;
    }
    let ax = w.provinces[a as usize].x as i16 as i32;
    let ay = w.provinces[a as usize].y as i16 as i32;
    let bx = w.provinces[b as usize].x as i16 as i32;
    let by = w.provinces[b as usize].y as i16 as i32;
    let dx = (bx - ax) as f32;
    let dy = (by - ay) as f32;
    let len = (dy * dy + dx * dx).sqrt();
    if len <= 0.0 {
        return None;
    }
    let mut step = 0.0f32;
    let mut k = 0i32;
    loop {
        let px = (step * dx * (1.0 / len) + ax as f32) as i32;
        let py = (step * dy * (1.0 / len) + ay as f32) as i32;
        if owner_at(w, px, py) == b {
            return Some((px, py));
        }
        k += 1;
        step = k as f32;
        if len <= step {
            return None;
        }
    }
}

pub fn create_extra_island_bridges(w: &mut World, opts: &Options) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        if (terrain(w, a) & TERRAIN_SEA) != 0 {
            continue;
        }
        if w.crt.below(100) >= opts.bridges {
            continue;
        }
        for _ in 0..4 {
            let b = graph::random_neighbor_province(w, a, false, false);
            if b <= 0 || (get_border_flags(w, a, b) & 0x26) != BORDER_RIVER {
                continue;
            }
            let ax = w.provinces[a as usize].x as i16 as i32;
            let ay = w.provinces[a as usize].y as i16 as i32;
            let bx = w.provinces[b as usize].x as i16 as i32;
            let by = w.provinces[b as usize].y as i16 as i32;
            let dx = bx - ax;
            let dy = by - ay;
            let mx = ax + (if dx < 0 { dx + 1 } else { dx } >> 1);
            let my = ay + (if dy < 0 { dy + 1 } else { dy } >> 1);
            if height_clamped(w, mx, my) > w.sea_level {
                continue;
            }
            let mid = owner_at(w, mx, my);
            if mid != a && mid != b {
                continue;
            }
            let mut land_all = true;
            for t in [0.3f64, 0.4, 0.6, 0.7] {
                let px = (f64::from(dx) * t + f64::from(ax)) as i32;
                let py = (f64::from(dy) * t + f64::from(ay)) as i32;
                if height_clamped(w, px, py) < w.sea_level {
                    land_all = false;
                    break;
                }
            }
            if !land_all {
                continue;
            }
            if find_region_entry_on_center_line(w, a, b).is_some() {
                clear_border_flags(w, a, b, BORDER_RIVER);
                set_border_flags(w, a, b, BORDER_BRIDGE);
                break;
            }
        }
    }
}

pub fn chop_sea_borders(w: &mut World, idx: &graph::BorderIndex) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        if (terrain(w, a) & TERRAIN_SEA) == 0 {
            continue;
        }
        let mut i = 0usize;
        while i < MAX_NBORS {
            let b = match w.provinces[a as usize].nbors.get(i) {
                Some(&v) => i32::from(v),
                None => break,
            };
            if b < 1 {
                break;
            }
            if (terrain(w, b) & TERRAIN_SEA) != 0
                && graph::count_water_border_pixels_indexed(w, idx, a, b) < 1
            {
                removenbor(w, a, b);
                removenbor(w, b, a);
            }
            i += 1;
        }
    }
}

pub fn build_graph_and_features(w: &mut World, opts: &Options, sink: &mut dyn Sink) -> Control {
    graph::precalculate_random_map_stuff(w);

    create_extra_islands(w, opts);
    if w.emit(Stage::Islands, sink) == Control::Cancel {
        return Control::Cancel;
    }

    graph::calculate_random_map_sizes(w);
    if w.emit(Stage::Sizes, sink) == Control::Cancel {
        return Control::Cancel;
    }

    graph::find_all_random_map_neighbors(w);
    if w.emit(Stage::Graph, sink) == Control::Cancel {
        return Control::Cancel;
    }

    let bidx = graph::BorderIndex::build(w);
    create_random_map_rivers(w, opts, &bidx);
    if w.emit(Stage::Rivers, sink) == Control::Cancel {
        return Control::Cancel;
    }

    infer_river_borders(w, &bidx);
    create_border_mountains(w, opts, &bidx);
    if w.emit(Stage::Mountains, sink) == Control::Cancel {
        return Control::Cancel;
    }

    mark_freshwater(w);
    create_extra_island_bridges(w, opts);
    chop_sea_borders(w, &bidx);
    if opts.cave_world {
        return Control::Continue;
    }
    w.emit(Stage::Bridges, sink)
}

pub fn expand_random_map_work_buffers(w: &mut World, dw: i32, dh: i32) {
    let (nw, nh) = (w.w + dw, w.h + dh);
    let mut owner = vec![0i16; (nw * nh) as usize];
    let mut heights = vec![0.0f32; (nw * nh) as usize];
    let keep = w.w.min(nw) as usize;
    for y in 0..w.h.min(nh) as usize {
        owner[y * nw as usize..y * nw as usize + keep]
            .copy_from_slice(&w.owner[y * w.w as usize..y * w.w as usize + keep]);
        heights[y * nw as usize..y * nw as usize + keep]
            .copy_from_slice(&w.heights[y * w.w as usize..y * w.w as usize + keep]);
    }
    w.owner = owner;
    w.heights = heights;
    w.w = nw;
    w.h = nh;
}

pub fn translate_random_map_work_buffers(w: &mut World, dx: i32, dy: i32) {
    let mut owner = vec![0i16; (w.w * w.h) as usize];
    let mut heights = vec![0.0f32; (w.w * w.h) as usize];
    let x_lo = dx.max(0);
    let x_hi = (w.w + dx).min(w.w);
    for y in 0..w.h {
        let sy = y - dy;
        if sy < 0 || sy >= w.h || x_lo >= x_hi {
            continue;
        }
        let dst = (y * w.w + x_lo) as usize;
        let src = (sy * w.w + x_lo - dx) as usize;
        let len = (x_hi - x_lo) as usize;
        owner[dst..dst + len].copy_from_slice(&w.owner[src..src + len]);
        heights[dst..dst + len].copy_from_slice(&w.heights[src..src + len]);
    }
    w.owner = owner;
    w.heights = heights;
    for p in w.provinces.iter_mut().skip(1) {
        p.x = i32::from((p.x as i16).wrapping_add(dx as i16));
        p.y = i32::from((p.y as i16).wrapping_add(dy as i16));
    }
}

pub fn apply_margin(w: &mut World, sink: &mut dyn Sink) -> Control {
    let mx = if w.hwrap { 0 } else { MARGIN_PX };
    let my = if w.vwrap { 0 } else { MARGIN_PX };
    if mx != 0 || my != 0 {
        let (ow, oh) = (w.w, w.h);
        let (nw, nh) = (ow + mx * 2, oh + my * 2);
        let mut owner = vec![0i16; (nw * nh) as usize];
        let mut heights = vec![0.0f32; (nw * nh) as usize];
        let len = ow as usize;
        let (mx, my) = (mx as usize, my as usize);
        owner
            .par_chunks_mut(nw as usize)
            .skip(my)
            .take(oh as usize)
            .zip(w.owner.par_chunks(len))
            .for_each(|(dst, src)| dst[mx..mx + len].copy_from_slice(src));
        heights
            .par_chunks_mut(nw as usize)
            .skip(my)
            .take(oh as usize)
            .zip(w.heights.par_chunks(len))
            .for_each(|(dst, src)| dst[mx..mx + len].copy_from_slice(src));
        let (mx, my) = (mx as i32, my as i32);
        w.owner = owner;
        w.heights = heights;
        w.w = nw;
        w.h = nh;
        for p in w.provinces.iter_mut().skip(1) {
            p.x = i32::from((p.x as i16).wrapping_add(mx as i16));
            p.y = i32::from((p.y as i16).wrapping_add(my as i16));
        }
    }
    w.emit(Stage::Margin, sink)
}

pub fn build_classification_mask(bp: &Blueprint) -> Vec<i8> {
    let mut mask = vec![0i8; (bp.w * bp.h).max(0) as usize];
    for y in 0..bp.h {
        for x in 0..bp.w {
            let i = ((y * bp.w + x) * 4) as usize;
            let b = i32::from(bp.bgra[i]);
            let g = i32::from(bp.bgra[i + 1]);
            let r = i32::from(bp.bgra[i + 2]);
            let spread = (g - b).abs().max((g - r).abs());
            let code: i8 = if spread < 0x10 {
                if g >= 0x40 {
                    2
                } else {
                    -2
                }
            } else if g <= r {
                -1
            } else {
                1
            };
            mask[(y * bp.w + x) as usize] = code;
        }
    }
    mask
}

pub fn sample_classification_mask(
    mask: &[i8],
    mw: i32,
    mh: i32,
    board_w: i32,
    board_h: i32,
    x: i32,
    y: i32,
) -> i32 {
    if mw <= 0 || mh <= 0 || board_w <= 0 || board_h <= 0 {
        return 0;
    }
    let mx = clampi((mw * x) / board_w, 0, mw - 1).max(0);
    let my = clampi((mh * y) / board_h, 0, mh - 1).max(0);
    i32::from(mask[(my * mw + mx) as usize])
}

pub fn mark_no_mans_land(w: &mut World, mask: &[i8], mw: i32, mh: i32, bw: i32, bh: i32) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        let x = w.provinces[a as usize].x as i16 as i32;
        let y = w.provinces[a as usize].y as i16 as i32;
        if sample_classification_mask(mask, mw, mh, bw, bh, x, y).abs() == 2 {
            w.provinces[a as usize].terrain |= TERRAIN_NOSTART;
        }
    }
}

pub fn finish_edges(
    w: &mut World,
    alpha: &[u8],
    board_w: i32,
    board_h: i32,
    sink: &mut dyn Sink,
) -> Control {
    graph::disown_random_map_edges(w, alpha);
    graph::ensure_land_exits(w);
    if !w.blueprint_mask.is_empty() {
        let mask = std::mem::take(&mut w.blueprint_mask);
        mark_no_mans_land(
            w,
            &mask,
            w.blueprint_mask_w,
            w.blueprint_mask_h,
            board_w,
            board_h,
        );
        w.blueprint_mask = mask;
    }
    w.emit(Stage::Edges, sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::NoSink;

    fn flat(wd: i32, ht: i32, nprov: usize) -> World {
        let mut w = World::new(7);
        w.w = wd;
        w.h = ht;
        w.owner = vec![0; (wd * ht) as usize];
        w.heights = vec![100.0; (wd * ht) as usize];
        w.sea_level = 0.0;
        w.spacing = 40.0;
        w.provinces = vec![Default::default(); nprov + 1];
        w
    }

    fn blobby(seed: u32) -> World {
        let (wd, ht) = (48, 40);
        let mut w = flat(wd, ht, 3);
        let mut rng = crate::rng::CrtRng::seeded(seed);
        for y in 0..ht {
            for x in 0..wd {
                let i = (y * wd + x) as usize;
                let dx = x - 22;
                let dy = y - 19;
                let r = dx * dx + dy * dy;
                w.owner[i] = if r < 190 {
                    1
                } else if x < 6 {
                    2
                } else {
                    3
                };
                w.heights[i] = rng.below(100) as f32 - 60.0;
            }
        }
        w.sea_level = 0.0;
        w.spacing = 12.0;
        w.provinces[1].terrain = TERRAIN_SEA;
        graph::precalculate_random_map_stuff(&mut w);
        w
    }

    #[test]
    fn the_island_plan_replays_the_scan_bit_for_bit() {
        for seed in [1u32, 5, 9] {
            let mut slow = blobby(seed);
            let mut fast = blobby(seed);
            let before = count_region_land_water_pixels(&slow, 1);
            let plan = island_plan(&mut fast, 1);
            assert_eq!(plan.total, before.0);
            let mut land = before.1;
            for amount in [
                ISLAND_RAISE,
                ISLAND_RAISE,
                ISLAND_LOWER,
                ISLAND_RAISE,
                ISLAND_LOWER,
            ] {
                raise_extra_island_midland(&mut slow, 1, amount);
                plan.apply(&mut fast, amount, &mut land);
                let c = count_region_land_water_pixels(&slow, 1);
                assert_eq!(land, c.1, "seed {seed} amount {amount}");
                assert_eq!(slow.heights, fast.heights, "seed {seed} amount {amount}");
            }
            assert_eq!(slow.edge_dist, fast.edge_dist);
        }
    }

    #[test]
    fn margin_frames_and_shifts_the_world() {
        let mut w = flat(8, 8, 1);
        w.hwrap = false;
        w.vwrap = false;
        w.provinces[1].x = 3;
        w.provinces[1].y = 4;
        w.owner[(4 * 8 + 3) as usize] = 1;
        apply_margin(&mut w, &mut NoSink);
        assert_eq!((w.w, w.h), (8 + 384, 8 + 384));
        assert_eq!(w.provinces[1].x, 3 + MARGIN_PX);
        assert_eq!(w.provinces[1].y, 4 + MARGIN_PX);
        let i = ((4 + MARGIN_PX) * w.w + 3 + MARGIN_PX) as usize;
        assert_eq!(w.owner[i], 1);
        assert_eq!(w.owner[0], 0);
    }

    #[test]
    fn margin_is_skipped_on_a_fully_wrapped_map() {
        let mut w = flat(8, 8, 1);
        w.hwrap = true;
        w.vwrap = true;
        apply_margin(&mut w, &mut NoSink);
        assert_eq!((w.w, w.h), (8, 8));
    }

    #[test]
    fn carve_writes_the_river_sentinel_along_the_seam() {
        let mut w = flat(16, 16, 2);
        for y in 0..16 {
            for x in 0..16 {
                w.owner[(y * 16 + x) as usize] = if x < 8 { 1 } else { 2 };
            }
        }
        w.spacing = 40.0;
        graph::precalculate_random_map_stuff(&mut w);
        carve_region_boundary_channel(&mut w, 1, 2);
        assert!(w.heights.contains(&crate::writers::RIVER_SENTINEL));
    }

    #[test]
    fn indexed_carving_matches_the_bounding_box_scan() {
        for (hwrap, vwrap) in [(false, false), (true, true)] {
            let mut a = flat(48, 40, 3);
            a.hwrap = hwrap;
            a.vwrap = vwrap;
            let mut r = crate::rng::CrtRng::seeded(4);
            for y in 0..40 {
                for x in 0..48 {
                    a.heights[(y * 48 + x) as usize] = 2.0 + r.float() * 6.0;
                }
            }
            for y in 0..40 {
                for x in 0..48 {
                    let o = if x * 2 + (y % 5) < 48 { 1 } else { 2 };
                    a.owner[(y * 48 + x) as usize] = o;
                }
            }
            a.spacing = 40.0;
            a.sea_level = 3.0;
            graph::precalculate_random_map_stuff(&mut a);
            let mut b = a.clone();
            let idx = graph::BorderIndex::build(&a);
            carve_region_boundary_channel_indexed(&mut a, &idx, 1, 2);
            carve_region_boundary_channel(&mut b, 1, 2);
            assert_eq!(a.heights, b.heights);
            assert_eq!(a.crt, b.crt);
        }
    }

    #[test]
    fn chop_sea_borders_drops_dry_sea_pairs() {
        let mut w = flat(8, 8, 2);
        w.provinces[1].terrain = TERRAIN_SEA;
        w.provinces[2].terrain = TERRAIN_SEA;
        graph::addnbor(&mut w, 1, 2);
        graph::precalculate_random_map_stuff(&mut w);
        let idx = graph::BorderIndex::build(&w);
        chop_sea_borders(&mut w, &idx);
        assert!(w.provinces[1].nbors.is_empty());
        assert!(w.provinces[2].nbors.is_empty());
    }

    #[test]
    fn classification_mask_codes_grey_as_two() {
        let bp = Blueprint {
            w: 2,
            h: 1,
            bgra: vec![100, 100, 100, 255, 0, 10, 200, 255],
        };
        let m = build_classification_mask(&bp);
        assert_eq!(m[0], 2);
        assert_eq!(m[1], -1);
    }

    #[test]
    fn freshwater_needs_a_river_or_bridge_to_a_land_neighbour() {
        let mut w = flat(8, 8, 2);
        graph::addnbor(&mut w, 1, 2);
        mark_freshwater(&mut w);
        assert_eq!(w.provinces[1].terrain & TERRAIN_FRESHWATER, 0);
        set_border_flags(&mut w, 1, 2, BORDER_RIVER);
        mark_freshwater(&mut w);
        assert_eq!(
            w.provinces[1].terrain & TERRAIN_FRESHWATER,
            TERRAIN_FRESHWATER
        );
    }
}
