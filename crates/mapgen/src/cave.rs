use crate::options::Options;
use crate::stage::{Control, Sink, Stage};
use crate::world::{World, MAX_NBORS};

pub const CAVE_LAND_SET: i64 = 0x0800_0000_0000_1000u64 as i64;
pub const CAVE_WALL_KEEP: i64 = 0xffff_ffff_ffff_f76bu64 as i64;
pub const CAVE_WALL_SET: i64 = 0x0800_0010_0000_0000u64 as i64;
pub const SEA_CONVERT_KEEP: i64 = 0xffff_fffb_ffff_ee0fu64 as i64;
pub const TERRAIN_SEA: i64 = 4;
pub const TERRAIN_CAVE: i64 = 0x1000;
pub const TERRAIN_CAVE_WALL: i64 = 0x10_0000_0000;
pub const BORDER_MOUNTAIN: i64 = 4;
pub const TERRAIN_HIGHLAND: i64 = 0x10;
pub const TERRAIN_SWAMP: i64 = 0x20;
pub const TERRAIN_FOREST: i64 = 0x80;
pub const TERRAIN_GATE: i64 = 0x20_0000_0000;
pub const UNDERCAVE_FOREST_PCT: u32 = 15;
pub const UNDERCAVE_HIGHLAND_PCT: u32 = 20;
pub const UNDERCAVE_SWAMP_PCT: u32 = 28;
pub const GATE_PASSES: i32 = 2;
pub const GATE_ATTEMPTS: i32 = 250;
pub const GATE_JITTER: u32 = 200;
pub const GATE_ABORT_ROLL: u32 = 0x18;
pub const GATE_FLOOD_KEEP: i64 = !0xa0;
pub const GATE_FLOOD_SET: i64 = 0x804;
pub const NEAREST_START: i32 = 99999;
pub const GATE_WRAP: u8 = 0;
pub const CULL_ITERATIONS: i32 = 5000;
pub const RELAX_AFTER: i32 = 0x7d1;

pub fn make_cave_world(world: &mut World, opts: &Options, sink: &mut dyn Sink) -> Control {
    if !opts.cave_world {
        return Control::Continue;
    }
    cull_land_provinces(world);
    rewrite_cave_flags(world);
    make_wall_borders(world);
    if connect_caves(world, sink) == Control::Cancel {
        return Control::Cancel;
    }
    world.emit(Stage::Cave, sink)
}

fn count_land_provinces(world: &World) -> i32 {
    world.provinces[1..]
        .iter()
        .filter(|p| p.terrain & TERRAIN_SEA == 0)
        .count() as i32
}

fn cull_land_provinces(world: &mut World) {
    let nprov = world.nprov() as i32;
    let target = nprov / 10 + 1;
    let mut lands = count_land_provinces(world);
    if lands <= target {
        return;
    }
    let mean = world.mean_land_area;
    let mut land_px_of: Vec<Option<i32>> = vec![None; nprov as usize + 1];
    let mut iter = 0i32;
    loop {
        let lnr = world.crt.below(nprov) + 1;
        if world.provinces[lnr as usize].terrain & TERRAIN_SEA == 0 {
            let land_px = match land_px_of[lnr as usize] {
                Some(v) => v,
                None => {
                    let v = count_region_land_water_pixels(world, lnr).1;
                    land_px_of[lnr as usize] = Some(v);
                    v
                }
            };
            let nbors = count_province_neighbors_matching_borders(world, lnr, 0, 0, 0);
            let size = land_px as f32;
            let mut small = nbors < 1 && size < mean * 0.5;
            if size < mean * 0.2 {
                small = true;
            }
            let convert = if iter < RELAX_AFTER || lands <= target * 2 {
                small
            } else {
                if size < mean * 0.5 {
                    small = true;
                }
                if mean <= size || nbors > 0 {
                    small
                } else {
                    true
                }
            };
            if convert {
                let t = &mut world.provinces[lnr as usize].terrain;
                *t = (*t & SEA_CONVERT_KEEP) | TERRAIN_SEA;
                lands = count_land_provinces(world);
                if lands <= target {
                    return;
                }
            }
        }
        if iter >= CULL_ITERATIONS {
            return;
        }
        iter += 1;
    }
}

fn rewrite_cave_flags(world: &mut World) {
    for p in world.provinces[1..].iter_mut() {
        if p.terrain & TERRAIN_SEA != 0 {
            p.terrain = (p.terrain & CAVE_WALL_KEEP) | CAVE_WALL_SET;
        } else {
            p.terrain |= CAVE_LAND_SET;
        }
    }
}

fn make_wall_borders(world: &mut World) {
    let nprov = world.nprov();
    for lnr in 1..=nprov {
        if world.provinces[lnr].terrain & TERRAIN_CAVE_WALL == 0 {
            continue;
        }
        let nbors: Vec<u16> = world.provinces[lnr].nbors.clone();
        for nbor in nbors {
            if nbor < 1 {
                break;
            }
            set_border_flags(world, lnr as i32, nbor as i32, BORDER_MOUNTAIN);
        }
    }
}

fn connect_caves(world: &mut World, sink: &mut dyn Sink) -> Control {
    let nprov = world.nprov();
    for a in 1..=nprov {
        if world.provinces[a].terrain & TERRAIN_CAVE == 0 {
            continue;
        }
        if sink.progress(Stage::Cave, a as u32, nprov as u32) == Control::Cancel {
            return Control::Cancel;
        }
        let nbors: Vec<u16> = world.provinces[a].nbors.clone();
        for nbor in nbors {
            if nbor < 1 {
                break;
            }
            let b = nbor as usize;
            if b > a && world.provinces[b].terrain & TERRAIN_CAVE != 0 {
                draw_wrapped_province_connection(world, a as i32, b as i32);
            }
        }
    }
    Control::Continue
}

pub fn set_border_flags(world: &mut World, a: i32, b: i32, flags: i64) {
    if a < 0 || b < 0 || a == b {
        return;
    }
    for (from, to) in [(a, b), (b, a)] {
        let p = &mut world.provinces[from as usize];
        for i in 0..p.nbors.len() {
            if p.nbors[i] < 1 {
                break;
            }
            if p.nbors[i] as i32 == to {
                p.border[i] |= flags;
                break;
            }
        }
    }
}

pub fn count_province_neighbors_matching_borders(
    world: &World,
    lnr: i32,
    allow_mountain: i32,
    allow_river: i32,
    allow_cross_class: i32,
) -> i32 {
    if lnr < 0 {
        return 0;
    }
    let p = &world.provinces[lnr as usize];
    let mut n = 0;
    for i in 0..p.nbors.len().min(20) {
        let nbor = p.nbors[i];
        if nbor < 1 {
            return n;
        }
        let mut border = 0i64;
        for j in 0..p.nbors.len().min(20) {
            if p.nbors[j] < 1 {
                break;
            }
            if p.nbors[j] == nbor {
                border = p.border[j];
                break;
            }
        }
        let cross = (world.provinces[nbor as usize].terrain ^ p.terrain) & TERRAIN_SEA;
        if (allow_mountain != 0 || border & 4 == 0)
            && (allow_river != 0 || border & 2 == 0)
            && (allow_cross_class != 0 || cross == 0)
        {
            n += 1;
        }
    }
    n
}

pub fn count_region_land_water_pixels(world: &World, lnr: i32) -> (i32, i32, i32) {
    if lnr < 1 {
        return (0, 0, 0);
    }
    let bb = world.bbox[lnr as usize];
    let (x0, x1, y0, y1) = (bb[0] as i32, bb[1] as i32, bb[2] as i32, bb[3] as i32);
    let (w, h) = (world.w, world.h);
    let mut total = 0;
    let mut land = 0;
    let mut water = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if world.owner[(y * w + x) as usize] as i32 != lnr {
                continue;
            }
            total += 1;
            let sx = x.max(0).min(w - 1);
            let sy = y.max(0).min(h - 1);
            if world.heights[(sy * w + sx) as usize] >= world.sea_level {
                land += 1;
            } else {
                water += 1;
            }
        }
    }
    (total, land, water)
}

pub fn draw_wrapped_province_connection(world: &mut World, a: i32, b: i32) {
    let (xa, ya) = (world.provinces[a as usize].x, world.provinces[a as usize].y);
    let (xb, yb) = (world.provinces[b as usize].x, world.provinces[b as usize].y);
    let hw = i32::from(world.hwrap);
    let vw = i32::from(world.vwrap);
    let mut best = 99_999_999i32;
    let mut bx = xb;
    let mut by = yb;
    for i in -hw..=hw {
        let tx = i * world.w - xa + xb;
        for j in -vw..=vw {
            let ty = j * world.h - ya + yb;
            let d = ty * ty + tx * tx;
            if d < best {
                best = d;
                bx = tx + xa;
                by = ty + ya;
            }
        }
    }
    let fill = world.sea_level + 100.0;
    let radius = world.spacing * 0.15;
    let dx = (bx - xa) as f32;
    let dy = (by - ya) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    subdivide_random_map_height_segment(
        world, xa as f32, ya as f32, bx as f32, by as f32, fill, radius, len, a, b,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn subdivide_random_map_height_segment(
    world: &mut World,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    fill: f32,
    radius: f32,
    len: f32,
    owner_a: i32,
    owner_b: i32,
) {
    let (mut x0, mut y0, mut radius, mut len, mut owner_a) = (x0, y0, radius, len, owner_a);
    let floor = world.spacing * 0.08;
    while len > 1.0 {
        let mx = (world.crt.float() - 0.5) * len * 0.2 + (x1 - x0) * 0.5 + x0;
        let my = (world.crt.float() - 0.5) * len * 0.2 + (y1 - y0) * 0.5 + y0;
        let r = radius * (world.crt.float() * 0.5 + 0.75);
        len *= 0.5;
        radius = if floor <= r { r } else { floor };
        subdivide_random_map_height_segment(
            world, x0, y0, mx, my, fill, radius, len, owner_a, owner_a,
        );
        x0 = mx;
        y0 = my;
        owner_a = owner_b;
    }
    rasterize_random_map_height_segment(world, x0, y0, x1, y1, fill, radius, owner_a);
}

#[allow(clippy::too_many_arguments)]
pub fn rasterize_random_map_height_segment(
    world: &mut World,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    fill: f32,
    radius: f32,
    owner: i32,
) {
    let dx = (x1 as i32) as f32 - x0;
    let dy = (y1 as i32) as f32 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0 {
        return;
    }
    let inv = 1.0 / len;
    let r = (radius + 0.5) as i32;
    let mut step = 0i32;
    loop {
        let t = step as f32;
        if t >= len {
            return;
        }
        let cx = (t * dx * inv + x0) as i32;
        let cy = (t * dy * inv + y0) as i32;
        paint_height_owner_disk(world, cx, cy, r, fill, owner);
        step += 1;
    }
}

pub fn paint_height_owner_disk(world: &mut World, cx: i32, cy: i32, r: i32, fill: f32, owner: i32) {
    let (w, h) = (world.w, world.h);
    for py in (cy - r)..=(cy + r) {
        for px in (cx - r)..=(cx + r) {
            let mut wx = px;
            if world.hwrap {
                if wx < 0 {
                    wx += w;
                }
                if wx >= w {
                    wx -= w;
                }
            }
            let mut wy = py;
            if world.vwrap {
                if wy < 0 {
                    wy += h;
                }
                if wy >= h {
                    wy -= h;
                }
            }
            let sx = wx.max(0).min(w - 1);
            let sy = wy.max(0).min(h - 1);
            let idx = (sy * w + sx) as usize;
            let ddx = px - cx;
            let ddy = py - cy;
            if world.heights[idx] <= world.sea_level
                && (ddy * ddy + ddx * ddx) as f32 <= (r * r) as f32
            {
                world.heights[idx] = fill;
                if owner >= 0 {
                    world.owner[idx] = owner as i16;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gate {
    pub surface: u16,
    pub cave: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct PlaneRange {
    pub first: usize,
    pub last: usize,
}

impl PlaneRange {
    pub fn holds(&self, p: usize) -> bool {
        p >= self.first && p <= self.last
    }
}

pub fn roll_undercave_terrain(world: &mut World, range: PlaneRange) {
    for i in range.first..=range.last {
        if world.pool.rnd(100) < UNDERCAVE_FOREST_PCT {
            world.provinces[i].terrain |= TERRAIN_FOREST;
        } else if world.pool.rnd(100) < UNDERCAVE_HIGHLAND_PCT {
            world.provinces[i].terrain |= TERRAIN_HIGHLAND;
        } else if world.pool.rnd(100) < UNDERCAVE_SWAMP_PCT {
            world.provinces[i].terrain |= TERRAIN_SWAMP;
        }
    }
}

pub fn nearest_province_to_pixel(
    world: &World,
    range: PlaneRange,
    x: f32,
    y: f32,
    map_w: i32,
    map_h: i32,
    wrap: u8,
) -> i32 {
    let hwrap = i32::from(wrap & 1 != 0);
    let vwrap = i32::from(wrap >> 1 & 1 != 0);
    let mut best = NEAREST_START;
    let mut found = -1i32;
    for a in range.first..=range.last {
        let p = &world.provinces[a];
        for dy in -vwrap..=vwrap {
            let ady = ((p.y + dy * map_h) as f32 - y).abs();
            for dx in -hwrap..=hwrap {
                let adx = ((p.x + dx * map_w) as f32 - x).abs();
                let d = (adx + ady) as i32;
                if d < best {
                    best = d;
                    found = a as i32;
                }
            }
        }
    }
    found
}

pub fn neighbour_with_terrain_mask(world: &World, a: i32, mask: i64) -> i32 {
    if a < 1 {
        return -1;
    }
    for &n in world.provinces[a as usize].nbors.iter().take(MAX_NBORS) {
        let b = i32::from(n);
        if b < 1 {
            return -1;
        }
        if world.provinces[b as usize].terrain & mask == mask {
            return b;
        }
    }
    -1
}

fn random_province_list(world: &mut World, range: PlaneRange) -> Vec<usize> {
    let mut list: Vec<usize> = (range.first..=range.last)
        .filter(|&i| world.provinces[i].terrain & TERRAIN_CAVE_WALL == 0)
        .collect();
    let last = list.len() as i32 - 1;
    if last >= 0 {
        for i in 0..=last as usize {
            let r = world.pool.rnd((last + 1) as u32) as usize;
            list.swap(i, r);
        }
    }
    list
}

fn cave_region(world: &World, seed: usize) -> (i32, i32) {
    if seed < 1 || world.provinces[seed].terrain & TERRAIN_CAVE == 0 {
        return (0, 0);
    }
    let mut seen = vec![false; world.provinces.len()];
    seen[seed] = true;
    let mut size = 1;
    let mut gated = i32::from(world.provinces[seed].terrain & TERRAIN_GATE != 0);
    let mut queue = vec![seed];
    while let Some(a) = queue.pop() {
        for &n in world.provinces[a].nbors.iter().take(MAX_NBORS) {
            let b = n as usize;
            if b < 1 || b >= seen.len() || seen[b] {
                continue;
            }
            let t = world.provinces[b].terrain;
            if t & TERRAIN_CAVE == 0 {
                continue;
            }
            seen[b] = true;
            size += 1;
            if t & TERRAIN_GATE != 0 {
                gated += 1;
            }
            queue.push(b);
        }
    }
    (size, gated)
}

fn flood_cave_region_to_sea(world: &mut World, seed: usize) {
    if seed < 1 || world.provinces[seed].terrain & TERRAIN_CAVE == 0 {
        return;
    }
    let mut seen = vec![false; world.provinces.len()];
    seen[seed] = true;
    world.provinces[seed].terrain =
        world.provinces[seed].terrain & GATE_FLOOD_KEEP | GATE_FLOOD_SET;
    let mut queue = vec![seed];
    while let Some(a) = queue.pop() {
        let nbors: Vec<u16> = world.provinces[a]
            .nbors
            .iter()
            .take(MAX_NBORS)
            .copied()
            .collect();
        for n in nbors {
            let b = n as usize;
            if b < 1 || b >= seen.len() || seen[b] {
                continue;
            }
            let t = world.provinces[b].terrain;
            if t & TERRAIN_CAVE == 0 {
                continue;
            }
            seen[b] = true;
            world.provinces[b].terrain = t & GATE_FLOOD_KEEP | GATE_FLOOD_SET;
            queue.push(b);
        }
    }
}

pub fn generate_cave_gates(
    world: &mut World,
    surface: PlaneRange,
    cave: PlaneRange,
    map_w: i32,
    map_h: i32,
) -> Vec<Gate> {
    let mut gates = Vec::new();
    let mut taken = vec![0i32; world.provinces.len()];
    for pass in 0..GATE_PASSES {
        for prov in random_province_list(world, cave) {
            if world.provinces[prov].terrain & TERRAIN_CAVE_WALL != 0 {
                continue;
            }
            let (size, gated) = cave_region(world, prov);
            if gated > size / 4 {
                continue;
            }
            if gated >= 1 && world.pool.rnd(100) <= GATE_ABORT_ROLL {
                continue;
            }
            if pass != 0 && gated >= 1 {
                continue;
            }
            for attempt in 0..GATE_ATTEMPTS {
                let mut x = world.provinces[prov].x;
                let mut y = world.provinces[prov].y;
                if attempt > 0 {
                    x += world.pool.rnd(GATE_JITTER) as i32 - 100;
                    y += world.pool.rnd(GATE_JITTER) as i32 - 100;
                }
                let here = nearest_province_to_pixel(
                    world, cave, x as f32, y as f32, map_w, map_h, GATE_WRAP,
                );
                let there = nearest_province_to_pixel(
                    world, surface, x as f32, y as f32, map_w, map_h, GATE_WRAP,
                );
                if here != prov as i32 || there < 1 {
                    continue;
                }
                let there = there as usize;
                if taken[there] >= 1 {
                    continue;
                }
                let wet = world.provinces[there].terrain & TERRAIN_SEA != 0;
                if wet && pass == 0 {
                    continue;
                }
                if gated >= 1 && neighbour_with_terrain_mask(world, there as i32, TERRAIN_GATE) >= 1
                {
                    continue;
                }
                crate::graph::addnbor(world, prov as i32, there as i32);
                gates.push(Gate {
                    surface: (there - surface.first + 1) as u16,
                    cave: (prov - cave.first + 1) as u16,
                });
                taken[prov] += 1;
                taken[there] += 1;
                world.provinces[prov].terrain |= TERRAIN_GATE;
                world.provinces[there].terrain |= TERRAIN_GATE;
                if wet {
                    flood_cave_region_to_sea(world, prov);
                }
                break;
            }
        }
    }
    gates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{NoSink, RecordSink};
    use crate::world::Province;

    const GRID: i32 = 5;
    const SIDE: i32 = 60;

    fn ring_world() -> World {
        let mut w = World::new(20260908);
        w.w = SIDE;
        w.h = SIDE;
        w.hwrap = true;
        w.vwrap = false;
        w.sea_level = 100.0;
        w.spacing = 8.0;
        w.mean_land_area = 25.0;
        w.heights = vec![50.0; (SIDE * SIDE) as usize];
        w.owner = vec![0i16; (SIDE * SIDE) as usize];
        w.provinces = vec![Province::default()];
        w.bbox = vec![[0, 0, 0, 0]];
        for j in 0..GRID {
            for i in 0..GRID {
                let lnr = j * GRID + i + 1;
                let x = 6 + i * 12;
                let y = 6 + j * 12;
                let mut nbors = Vec::new();
                if i > 0 {
                    nbors.push((lnr - 1) as u16);
                }
                if i < GRID - 1 {
                    nbors.push((lnr + 1) as u16);
                }
                if j > 0 {
                    nbors.push((lnr - GRID) as u16);
                }
                if j < GRID - 1 {
                    nbors.push((lnr + GRID) as u16);
                }
                let border = vec![0i64; nbors.len()];
                w.provinces.push(Province {
                    x,
                    y,
                    terrain: 0,
                    nbors,
                    border,
                });
                for yy in (y - 2)..=(y + 2) {
                    for xx in (x - 2)..=(x + 2) {
                        w.owner[(yy * SIDE + xx) as usize] = lnr as i16;
                        w.heights[(yy * SIDE + xx) as usize] = 150.0;
                    }
                }
                w.bbox.push([
                    (x - 2) as i16,
                    (x + 2) as i16,
                    (y - 2) as i16,
                    (y + 2) as i16,
                ]);
            }
        }
        w
    }

    fn cullable_world() -> World {
        let mut w = ring_world();
        w.mean_land_area = 200.0;
        w
    }

    fn cave_opts() -> Options {
        Options {
            cave_world: true,
            ..Options::default()
        }
    }

    #[test]
    fn disabled_when_not_a_cave_world() {
        let mut w = ring_world();
        let before = w.provinces.clone();
        let mut sink = RecordSink::default();
        assert_eq!(
            make_cave_world(&mut w, &Options::default(), &mut sink),
            Control::Continue
        );
        assert_eq!(w.provinces, before);
        assert!(sink.entries.is_empty());
    }

    #[test]
    fn region_pixel_counts_split_on_sea_level() {
        let w = ring_world();
        let (total, land, water) = count_region_land_water_pixels(&w, 1);
        assert_eq!(total, 25);
        assert_eq!(land, 25);
        assert_eq!(water, 0);
        let mut sunk = ring_world();
        for h in sunk.heights.iter_mut() {
            *h = 50.0;
        }
        let (total, land, water) = count_region_land_water_pixels(&sunk, 3);
        assert_eq!(total, 25);
        assert_eq!(land, 0);
        assert_eq!(water, 25);
    }

    #[test]
    fn neighbour_count_rejects_mountain_river_and_cross_class() {
        let mut w = ring_world();
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 0, 0, 0), 2);
        set_border_flags(&mut w, 1, 2, BORDER_MOUNTAIN);
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 0, 0, 0), 1);
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 1, 0, 0), 2);
        set_border_flags(&mut w, 1, 6, 2);
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 1, 0, 0), 1);
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 1, 1, 0), 2);
        w.provinces[2].terrain |= TERRAIN_SEA;
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 1, 1, 0), 1);
        assert_eq!(count_province_neighbors_matching_borders(&w, 1, 1, 1, 1), 2);
    }

    #[test]
    fn set_border_flags_is_symmetric_and_ors() {
        let mut w = ring_world();
        set_border_flags(&mut w, 2, 3, BORDER_MOUNTAIN);
        set_border_flags(&mut w, 2, 3, 2);
        let i = w.provinces[2].nbors.iter().position(|n| *n == 3).unwrap();
        let j = w.provinces[3].nbors.iter().position(|n| *n == 2).unwrap();
        assert_eq!(w.provinces[2].border[i], 6);
        assert_eq!(w.provinces[3].border[j], 6);
    }

    #[test]
    fn every_province_becomes_cave_land_or_cave_wall() {
        let mut w = cullable_world();
        make_cave_world(&mut w, &cave_opts(), &mut NoSink);
        for p in &w.provinces[1..] {
            let land = p.terrain & TERRAIN_CAVE != 0;
            let wall = p.terrain & TERRAIN_CAVE_WALL != 0;
            assert!(land ^ wall);
            assert!(p.terrain & CAVE_LAND_SET & !TERRAIN_CAVE != 0);
        }
    }

    #[test]
    fn culling_reduces_land_towards_one_tenth() {
        let mut w = cullable_world();
        make_cave_world(&mut w, &cave_opts(), &mut NoSink);
        let caves = w.provinces[1..]
            .iter()
            .filter(|p| p.terrain & TERRAIN_CAVE != 0)
            .count();
        assert!(caves <= 3);
        assert!(caves >= 1);
    }

    #[test]
    fn wall_provinces_get_mountain_borders_both_ways() {
        let mut w = cullable_world();
        make_cave_world(&mut w, &cave_opts(), &mut NoSink);
        for lnr in 1..=w.nprov() {
            if w.provinces[lnr].terrain & TERRAIN_CAVE_WALL == 0 {
                continue;
            }
            for (i, nbor) in w.provinces[lnr].nbors.clone().iter().enumerate() {
                assert_eq!(
                    w.provinces[lnr].border[i] & BORDER_MOUNTAIN,
                    BORDER_MOUNTAIN
                );
                let back = &w.provinces[*nbor as usize];
                let j = back.nbors.iter().position(|n| *n as usize == lnr).unwrap();
                assert_eq!(back.border[j] & BORDER_MOUNTAIN, BORDER_MOUNTAIN);
            }
        }
    }

    #[test]
    fn tunnels_raise_height_and_claim_owner_pixels() {
        let mut w = ring_world();
        let before_heights = w.heights.clone();
        let before_owner = w.owner.clone();
        make_cave_world(&mut w, &cave_opts(), &mut NoSink);
        assert_ne!(w.heights, before_heights);
        assert_ne!(w.owner, before_owner);
        let raised = w
            .heights
            .iter()
            .filter(|h| **h == w.sea_level + 100.0)
            .count();
        assert!(raised > 0);
    }

    #[test]
    fn cave_world_is_deterministic_for_a_seed() {
        let mut a = ring_world();
        let mut b = ring_world();
        let mut sa = RecordSink::default();
        let mut sb = RecordSink::default();
        make_cave_world(&mut a, &cave_opts(), &mut sa);
        make_cave_world(&mut b, &cave_opts(), &mut sb);
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.owner, b.owner);
        assert_eq!(a.provinces, b.provinces);
        assert_eq!(a.crt, b.crt);
        assert_eq!(sa.entries.last(), sb.entries.last());
    }

    #[test]
    fn cave_stage_is_emitted_once() {
        let mut w = ring_world();
        let mut sink = RecordSink::default();
        make_cave_world(&mut w, &cave_opts(), &mut sink);
        let stages: Vec<_> = sink.entries.iter().filter(|e| e.0 == Stage::Cave).collect();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].1, 0);
    }

    #[test]
    fn disk_paint_clamps_an_edge_overrun_onto_the_border_column() {
        let mut w = ring_world();
        w.hwrap = false;
        w.vwrap = false;
        w.heights = vec![50.0; (SIDE * SIDE) as usize];
        w.owner = vec![0i16; (SIDE * SIDE) as usize];
        paint_height_owner_disk(&mut w, 0, 16, 2, 300.0, 7);
        assert_eq!(w.owner[(16 * SIDE) as usize], 7);
        assert_eq!(w.heights[(16 * SIDE) as usize], 300.0);
        assert_eq!(w.owner[(14 * SIDE) as usize], 7);
        assert_eq!(w.owner[(16 * SIDE + SIDE - 1) as usize], 0);
        let mut w = ring_world();
        w.hwrap = false;
        w.vwrap = false;
        w.heights = vec![50.0; (SIDE * SIDE) as usize];
        w.owner = vec![0i16; (SIDE * SIDE) as usize];
        paint_height_owner_disk(&mut w, 16, 0, 2, 300.0, 7);
        assert_eq!(w.owner[16], 7);
        assert_eq!(w.owner[14], 7);
        assert_eq!(w.heights[16], 300.0);
    }

    #[test]
    fn disk_paint_wraps_horizontally_but_not_vertically() {
        let mut w = ring_world();
        w.heights = vec![50.0; (SIDE * SIDE) as usize];
        w.owner = vec![0i16; (SIDE * SIDE) as usize];
        paint_height_owner_disk(&mut w, 0, 16, 2, 300.0, 7);
        assert_eq!(w.owner[(16 * SIDE + SIDE - 1) as usize], 7);
        assert_eq!(w.heights[(16 * SIDE + SIDE - 1) as usize], 300.0);
        w.heights = vec![50.0; (SIDE * SIDE) as usize];
        w.owner = vec![0i16; (SIDE * SIDE) as usize];
        paint_height_owner_disk(&mut w, 16, 0, 2, 300.0, 7);
        assert_eq!(w.owner[((SIDE - 1) * SIDE + 16) as usize], 0);
        assert_eq!(w.owner[16], 7);
    }
}
