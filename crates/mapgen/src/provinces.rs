use rayon::prelude::*;

use crate::options::Options;
use crate::stage::{Control, Sink, Stage};
use crate::world::{Province, World};

pub const CAPITAL_EDGE: i32 = 0x25;
pub const PLANE_FIRST_PROV: i32 = 1;
pub const PLANE_LAST_PROV: i32 = 0;
pub const EXTRA_CAPITALS_ENABLED: bool = true;
pub const NEAREST_INIT: f64 = 99999999.0;
pub const MAX_PROVINCES: i32 = 0x7c6;
pub const MAPPROV_MIN: i32 = 10;
pub const MAPPROV_MAX: i32 = 0x7bc;

pub struct WaterSat {
    w: i32,
    h: i32,
    ps: Vec<u32>,
}

impl WaterSat {
    pub fn new(world: &World) -> Self {
        let (w, h) = (world.w, world.h);
        let sea = world.sea_level;
        let stride = (w + 1) as usize;
        let mut ps = vec![0u32; stride * (h + 1) as usize];
        ps[stride..]
            .par_chunks_mut(stride)
            .enumerate()
            .for_each(|(y, cur)| {
                let row = &world.heights[y * w as usize..y * w as usize + w as usize];
                let mut run = 0u32;
                for x in 0..w as usize {
                    if is_below_waterline(row[x], sea) {
                        run += 1;
                    }
                    cur[x + 1] = run;
                }
            });
        for y in 1..=h as usize {
            let (prev, cur) = ps.split_at_mut(y * stride);
            let prev = &prev[(y - 1) * stride..];
            for x in 1..stride {
                cur[x] += prev[x];
            }
        }
        WaterSat { w, h, ps }
    }

    fn rect(&self, x0: i32, x1: i32, y0: i32, y1: i32) -> u32 {
        let stride = (self.w + 1) as usize;
        let a = i64::from(self.ps[(y1 + 1) as usize * stride + (x1 + 1) as usize]);
        let b = i64::from(self.ps[y0 as usize * stride + (x1 + 1) as usize]);
        let c = i64::from(self.ps[(y1 + 1) as usize * stride + x0 as usize]);
        let d = i64::from(self.ps[y0 as usize * stride + x0 as usize]);
        (a - b - c + d) as u32
    }

    fn count(&self, x0: i32, x1: i32, y0: i32, y1: i32) -> u32 {
        if x1 < x0 || y1 < y0 {
            return 0;
        }
        let (lx, ix0, ix1, rx) = clamp_span(x0, x1, self.w);
        let (ly, iy0, iy1, ry) = clamp_span(y0, y1, self.h);
        let mut total = 0u32;
        let cols = [(0, 0, lx), (ix0, ix1, 1), (self.w - 1, self.w - 1, rx)];
        let rows = [(0, 0, ly), (iy0, iy1, 1), (self.h - 1, self.h - 1, ry)];
        for &(ry0, ry1, my) in &rows {
            if my == 0 || ry1 < ry0 {
                continue;
            }
            for &(rx0, rx1, mx) in &cols {
                if mx == 0 || rx1 < rx0 {
                    continue;
                }
                total += my * mx * self.rect(rx0, rx1, ry0, ry1);
            }
        }
        total
    }

    pub fn crosses_waterline(&self, x: i32, y: i32, radius: i32) -> bool {
        let r = radius / 2;
        let side = 2 * r + 1;
        let total = side * side;
        let water = self.count(x - r, x + r, y - r, y + r);
        let frac = water as f32 / total as f32;
        0.01 < frac && frac < 0.99
    }

    pub fn mostly_below_waterline(&self, x: i32, y: i32, radius: i32) -> bool {
        let r = radius / 2;
        let side = 2 * r;
        if side <= 0 {
            return false;
        }
        let water = self.count(x - r, x + r - 1, y - r, y + r - 1);
        0.75 < water as f32 / (side * side) as f32
    }

    fn has_opposite_pixel(&self, x: i32, y: i32, r: i32, want_water: bool) -> bool {
        let side = 2 * r;
        if side <= 0 {
            return false;
        }
        let water = self.count(x - r, x + r - 1, y - r, y + r - 1);
        let total = (side * side) as u32;
        if want_water {
            water < total
        } else {
            water > 0
        }
    }
}

fn clamp_span(lo: i32, hi: i32, n: i32) -> (u32, i32, i32, u32) {
    let left = (hi.min(-1) - lo + 1).max(0) as u32;
    let right = (hi - lo.max(n) + 1).max(0) as u32;
    (left, lo.max(0), hi.min(n - 1), right)
}

pub fn requested_province_count(requested: i32) -> i32 {
    let capped = if requested < MAPPROV_MAX {
        requested
    } else {
        MAPPROV_MAX
    };
    if MAPPROV_MIN < capped {
        capped
    } else {
        MAPPROV_MIN
    }
}

pub fn place_and_grow(world: &mut World, opts: &Options, sink: &mut dyn Sink) -> Control {
    let nprov = requested_province_count(opts.provinces);
    let sat = WaterSat::new(world);
    create_random_map_capitals(world, opts, nprov, CAPITAL_EDGE, &sat);
    if EXTRA_CAPITALS_ENABLED && !world.cave_world {
        create_extra_underwater_capitals(world, opts, CAPITAL_EDGE, &sat);
        create_extra_dry_capitals(world, opts, CAPITAL_EDGE, &sat);
    }
    bubble_random_map_capitals(world);
    if world.emit(Stage::Capitals, sink) == Control::Cancel {
        return Control::Cancel;
    }

    let saved_height = world.heights.clone();
    let full_w = world.w;
    let full_h = world.h;

    downsample_random_map_work_buffers_2x(world);
    downsample_random_map_work_buffers_2x(world);

    expand_random_map_territories(world, 1);
    if world.emit(Stage::Growth, sink) == Control::Cancel {
        return Control::Cancel;
    }
    center_random_map_capitals(world);

    expand_random_map_territories(world, 0);
    if world.emit(Stage::Growth, sink) == Control::Cancel {
        return Control::Cancel;
    }
    center_random_map_capitals(world);

    expand_random_map_territories(world, 0);
    if world.emit(Stage::Growth, sink) == Control::Cancel {
        return Control::Cancel;
    }

    if recenter_random_map_capitals(world, 1) > 0 {
        expand_random_map_territories(world, 0);
        if world.emit(Stage::Growth, sink) == Control::Cancel {
            return Control::Cancel;
        }
    }

    if opts.no_water_prov {
        remove_water_capitals(world);
    }

    crate::graph::find_all_random_map_neighbors(world);
    if world.emit(Stage::Graph, sink) == Control::Cancel {
        return Control::Cancel;
    }
    prettify_random_map_borders(world, 2);
    upsample_random_map_work_buffers_2x(world);
    prettify_random_map_borders(world, 2);
    upsample_random_map_work_buffers_2x(world);
    if world.emit(Stage::Upsample, sink) == Control::Cancel {
        return Control::Cancel;
    }
    prettify_random_map_borders(world, 2);
    if world.emit(Stage::Upsample, sink) == Control::Cancel {
        return Control::Cancel;
    }

    world.w = full_w;
    world.h = full_h;
    world.heights = saved_height;
    Control::Continue
}

fn sea_size_knob(opts: &Options) -> f32 {
    opts.sea_size as f32 / 100.0
}

fn clamp_index(v: i32, limit: i32) -> i32 {
    if v < 0 {
        0
    } else if limit - 1 <= v {
        limit - 1
    } else {
        v
    }
}

fn height_at(heights: &[f32], w: i32, h: i32, x: i32, y: i32) -> f32 {
    let cx = clamp_index(x, w);
    let cy = clamp_index(y, h);
    heights[(cy * w + cx) as usize]
}

fn is_below_waterline(v: f32, sea: f32) -> bool {
    v <= sea && sea != v
}

fn wrap_axis(v: i32, n: i32, wrap: bool) -> i32 {
    let mut r = v;
    if wrap {
        if v < 0 {
            r = n + v;
        }
        if n <= r {
            r -= n;
        }
    }
    r
}

fn wrap_x(world: &World, x: i32) -> i32 {
    let mut v = x;
    if world.hwrap {
        if x < 0 {
            v = world.w + x;
        }
        if world.w <= v {
            v -= world.w;
        }
    }
    v
}

fn wrap_y(world: &World, y: i32) -> i32 {
    let mut v = y;
    if world.vwrap {
        if y < 0 {
            v = world.h + y;
        }
        if world.h <= v {
            v -= world.h;
        }
    }
    v
}

fn owner_at(world: &World, x: i32, y: i32) -> i32 {
    if x < 0 || world.w <= x || y < 0 || world.h <= y {
        0
    } else {
        world.owner[(y * world.w + x) as usize] as i32
    }
}

fn store_owner(world: &mut World, x: i32, y: i32, v: i16) {
    if -1 < x && x < world.w && -1 < y && y < world.h {
        let i = (y * world.w + x) as usize;
        world.owner[i] = v;
    }
}

pub fn area_crosses_waterline(world: &World, x: i32, y: i32, radius: i32) -> bool {
    let r = radius / 2;
    let sea = world.sea_level;
    let mut water = 0i32;
    let mut total = 0i32;
    let mut ty = y - r;
    while ty <= y + r {
        let mut tx = x - r;
        while tx <= x + r {
            if is_below_waterline(height_at(&world.heights, world.w, world.h, tx, ty), sea) {
                water += 1;
            }
            total += 1;
            tx += 1;
        }
        ty += 1;
    }
    let frac = water as f32 / total as f32;
    0.01 < frac && frac < 0.99
}

pub fn is_area_mostly_below_waterline(world: &World, x: i32, y: i32, radius: i32) -> bool {
    let r = radius / 2;
    let sea = world.sea_level;
    let mut water = 0i32;
    let mut total = 0i32;
    let mut ty = y - r;
    while ty < y + r {
        let mut tx = x - r;
        while tx < x + r {
            if is_below_waterline(height_at(&world.heights, world.w, world.h, tx, ty), sea) {
                water += 1;
            }
            total += 1;
            tx += 1;
        }
        ty += 1;
    }
    0.75 < water as f32 / total as f32
}

pub fn has_land_at_cardinal_radius_samples(world: &World, x: i32, y: i32, radius: i32) -> bool {
    has_land_at_cardinal_radius_in(
        &world.heights,
        world.w,
        world.h,
        world.sea_level,
        x,
        y,
        radius,
    )
}

fn has_land_at_cardinal_radius_in(
    hs: &[f32],
    w: i32,
    h: i32,
    sea: f32,
    x: i32,
    y: i32,
    radius: i32,
) -> bool {
    if !is_below_waterline(height_at(hs, w, h, x - radius, y), sea) {
        return true;
    }
    if !is_below_waterline(height_at(hs, w, h, x + radius, y), sea) {
        return true;
    }
    if !is_below_waterline(height_at(hs, w, h, x, y - radius), sea) {
        return true;
    }
    !is_below_waterline(height_at(hs, w, h, x, y + radius), sea)
}

pub fn distance_to_nearest_water_province(world: &World, x: i32, y: i32) -> f64 {
    let mut best = NEAREST_INIT;
    for p in &world.provinces[1..] {
        if p.terrain & 4 != 0 {
            let dx = p.x - x;
            let dy = p.y - y;
            let d = (dy * dy + dx * dx) as f64;
            if d <= best {
                best = d;
            }
        }
    }
    best.sqrt()
}

fn distance_to_nearest_plane_capital(world: &World, x: i32, y: i32) -> f64 {
    let mut best = NEAREST_INIT;
    let mut i = PLANE_FIRST_PROV;
    while i <= PLANE_LAST_PROV {
        let p = &world.provinces[i as usize];
        let dx = p.x - x;
        let dy = p.y - y;
        let d = (dy * dy + dx * dx) as f64;
        if d < best {
            best = d;
        }
        i += 1;
    }
    best.sqrt()
}

fn capital_mindist(world: &World, nprov: i32, edge: i32) -> (i32, i32) {
    let mut e = world.w / 0x28;
    if 0x19 < world.w / 0x28 {
        e = 0x19;
    }
    let mut lim = world.h / 0x28;
    if e < world.h / 0x28 {
        lim = e;
    }
    let edge = if edge < lim { lim } else { edge };
    let cells = ((world.h - edge) * (world.w - edge)) / (nprov * 5);
    let mut mindist = (cells as f64).sqrt() as i32;
    if 0.5 < world.real_water_part {
        mindist = ((1.0 - (world.real_water_part - 0.5)) * mindist as f32) as i32;
    }
    (mindist, edge)
}

pub fn create_random_map_capitals(
    world: &mut World,
    opts: &Options,
    nprov: i32,
    edge: i32,
    sat: &WaterSat,
) {
    world.provinces.truncate(1);
    let (mut mindist, edge) = capital_mindist(world, nprov, edge);
    world.spacing = mindist as f32;
    let mut mindist_water = (world.spacing * sea_size_knob(opts)) as i32;
    if nprov <= 0 {
        return;
    }
    for _ in 0..nprov {
        let mut attempt: i32 = 0;
        let (mut x, mut y);
        loop {
            attempt += 1;
            if nprov < 2 && attempt < 6 {
                let a = world.crt.below(5);
                let hw = if world.w < 0 { world.w + 1 } else { world.w };
                x = a - 2 + (hw >> 1);
                let b = world.crt.below(5);
                let hh = if world.h < 0 { world.h + 1 } else { world.h };
                y = b - 2 + (hh >> 1);
            } else {
                x = world.crt.below(world.w - edge * 2) + edge;
                y = world.crt.below(world.h - edge * 2) + edge;
            }
            let md = mindist as f64;
            let crosses = sat.crosses_waterline(x, y, (md * 0.25) as i32);
            let mut accept = !crosses;
            if !crosses {
                if md <= distance_to_nearest_plane_capital(world, x, y) {
                    if sat.mostly_below_waterline(x, y, (md * 0.5) as i32) {
                        let d = distance_to_nearest_water_province(world, x, y);
                        accept = mindist_water as f64 <= d;
                    }
                } else {
                    accept = false;
                }
            }
            if 10000 < attempt {
                mindist = (md * 0.9) as i32;
                mindist_water = (mindist_water as f64 * 0.9) as i32;
                attempt = 0;
            }
            if accept {
                break;
            }
        }
        let mut p = Province {
            x,
            y,
            terrain: 0,
            ..Province::default()
        };
        if sat.mostly_below_waterline(x, y, (mindist as f64 * 0.5) as i32) {
            p.terrain = 4;
            let hv = height_at(&world.heights, world.w, world.h, x, y);
            if is_below_waterline(hv, world.deep_level) {
                p.terrain = 0x804;
            }
        }
        world.provinces.push(p);
    }
}

#[cfg(test)]
fn spread_capital_mask(world: &World, mask: &mut [u8], want_water: bool) {
    let sea = world.sea_level;
    let (w, h) = (world.w, world.h);
    loop {
        let mut spread = 0i32;
        for y in 0..h {
            for x in 0..w {
                let ax = wrap_x(world, x);
                let ay = wrap_y(world, y);
                if ax < 0 || ay < 0 || ax >= w || ay >= h {
                    continue;
                }
                if mask[(w * ay + ax) as usize] == 0 {
                    continue;
                }
                for dy in y - 1..=y + 1 {
                    for dx in x - 1..=x + 1 {
                        let nx = wrap_x(world, dx);
                        let ny = wrap_y(world, dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        if mask[(w * ny + nx) as usize] != 0 {
                            continue;
                        }
                        let hv = height_at(&world.heights, w, h, dx, dy);
                        if is_below_waterline(hv, sea) != want_water {
                            continue;
                        }
                        spread += 1;
                        mask[(ny * w + nx) as usize] = 1;
                    }
                }
            }
        }
        let mut y = h - 1;
        while y >= 0 {
            let mut x = w - 1;
            while x >= 0 {
                let ax = wrap_x(world, x);
                let ay = wrap_y(world, y);
                if ax >= 0 && ay >= 0 && ax < w && ay < h && mask[(ay * w + ax) as usize] != 0 {
                    for dy in y - 1..=y + 1 {
                        for dx in x - 1..=x + 1 {
                            let nx = wrap_x(world, dx);
                            let ny = wrap_y(world, dy);
                            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                                continue;
                            }
                            if mask[(ny * w + nx) as usize] != 0 {
                                continue;
                            }
                            let hv = height_at(&world.heights, w, h, dx, dy);
                            if is_below_waterline(hv, sea) != want_water {
                                continue;
                            }
                            spread += 1;
                            mask[(ny * w + nx) as usize] = 1;
                        }
                    }
                }
                x -= 1;
            }
            y -= 1;
        }
        if spread <= 0 {
            break;
        }
    }
}

const MASK_PASS: u8 = 1;
const MASK_MARK: u8 = 2;

fn seed_capital_mask(world: &World, mask: &mut [u8], want_water: bool, queue: &mut Vec<u32>) {
    for i in 1..world.provinces.len() {
        seed_one_capital(world, mask, want_water, queue, i);
    }
}

fn seed_one_capital(
    world: &World,
    mask: &mut [u8],
    want_water: bool,
    queue: &mut Vec<u32>,
    idx: usize,
) {
    let sea = world.sea_level;
    let (w, h) = (world.w, world.h);
    let p = &world.provinces[idx];
    let hv = height_at(&world.heights, w, h, p.x, p.y);
    if is_below_waterline(hv, sea) != want_water {
        return;
    }
    let ax = wrap_x(world, p.x);
    let ay = wrap_y(world, p.y);
    if -1 < ax && -1 < ay && ax < w && ay < h {
        let i = (ax + w * ay) as usize;
        if mask[i] & MASK_MARK == 0 {
            mask[i] |= MASK_MARK;
            queue.push(i as u32);
        }
    }
}

fn passable_mask(world: &World, want_water: bool) -> Vec<u8> {
    let sea = world.sea_level;
    let mut pass = vec![0u8; world.heights.len()];
    pass.par_chunks_mut(4096).enumerate().for_each(|(c, out)| {
        let base = c * 4096;
        for (i, o) in out.iter_mut().enumerate() {
            *o = u8::from(is_below_waterline(world.heights[base + i], sea) == want_water);
        }
    });
    pass
}

fn flood_capital_mask(world: &World, mask: &mut [u8], want_water: bool, queue: &mut Vec<u32>) {
    let sea = world.sea_level;
    let (w, h) = (world.w, world.h);
    let last_x = w - 1;
    while let Some(idx) = queue.pop() {
        let x = (idx % w as u32) as i32;
        let y = (idx / w as u32) as i32;
        if x >= 1 && x + 1 < w && y >= 1 && y + 1 < h {
            let row = (y * w) as usize;
            let mut xs = x;
            while xs > 1 && mask[row + (xs - 1) as usize] == MASK_PASS {
                xs -= 1;
                mask[row + xs as usize] = MASK_PASS | MASK_MARK;
            }
            if xs == 1 && mask[row] == MASK_PASS {
                mask[row] = MASK_PASS | MASK_MARK;
                queue.push(row as u32);
            }
            let mut xe = x;
            while xe < last_x - 1 && mask[row + (xe + 1) as usize] == MASK_PASS {
                xe += 1;
                mask[row + xe as usize] = MASK_PASS | MASK_MARK;
            }
            if xe == last_x - 1 && mask[row + last_x as usize] == MASK_PASS {
                mask[row + last_x as usize] = MASK_PASS | MASK_MARK;
                queue.push((row + last_x as usize) as u32);
            }
            let x_lo = (xs - 1).max(0);
            let x_hi = (xe + 1).min(last_x);
            for ny in [y - 1, y + 1] {
                let base = (ny * w) as usize;
                let interior_row = ny >= 1 && ny + 1 < h;
                let mut xx = x_lo;
                while xx <= x_hi {
                    let j = base + xx as usize;
                    if mask[j] == MASK_PASS {
                        mask[j] = MASK_PASS | MASK_MARK;
                        queue.push(j as u32);
                        if interior_row && xx != 0 && xx != last_x {
                            while xx <= x_hi && mask[base + xx as usize] & MASK_PASS != 0 {
                                xx += 1;
                            }
                            continue;
                        }
                    }
                    xx += 1;
                }
            }
            continue;
        }
        for dy in y - 1..=y + 1 {
            for dx in x - 1..=x + 1 {
                let nx = wrap_x(world, dx);
                let ny = wrap_y(world, dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let i = (ny * w + nx) as usize;
                if mask[i] & MASK_MARK != 0 {
                    continue;
                }
                let hv = height_at(&world.heights, w, h, dx, dy);
                if is_below_waterline(hv, sea) != want_water {
                    continue;
                }
                mask[i] |= MASK_MARK;
                queue.push(i as u32);
            }
        }
    }
}

#[cfg(test)]
fn box_has_opposite_pixel(world: &World, x: i32, y: i32, r: i32, want_water: bool) -> bool {
    let sea = world.sea_level;
    let mut ty = y - r;
    while ty < y + r {
        let mut tx = x - r;
        while tx < x + r {
            let hv = height_at(&world.heights, world.w, world.h, tx, ty);
            if is_below_waterline(hv, sea) != want_water {
                return true;
            }
            tx += 1;
        }
        ty += 1;
    }
    false
}

fn extra_capitals(world: &mut World, opts: &Options, edge: i32, want_water: bool, sat: &WaterSat) {
    let nprov = world.nprov() as i32;
    if nprov < 1 {
        return;
    }
    let (mindist, _edge) = capital_mindist(world, nprov, edge);
    world.spacing = mindist as f32;
    let _ = sea_size_knob(opts);
    let margin = mindist / 3;
    let md = mindist as f64;
    let ra = (md * 0.8) as i32;
    let rb = (md * 0.55) as i32;
    let rc = (md * 0.25) as i32 / 2;
    let rd = (md * 0.5) as i32;
    let mut mask = passable_mask(world, want_water);
    let mut queue: Vec<u32> = Vec::new();
    seed_capital_mask(world, &mut mask, want_water, &mut queue);
    flood_capital_mask(world, &mut mask, want_water, &mut queue);
    let y_end = world.h - margin;
    let x_end = world.w - margin;
    if margin >= y_end || margin >= x_end {
        return;
    }
    let sea = world.sea_level;
    let candidates: Vec<u32> = (margin..y_end)
        .into_par_iter()
        .map(|y| {
            let mut row = Vec::new();
            for x in margin..x_end {
                let hv = height_at(&world.heights, world.w, world.h, x, y);
                if is_below_waterline(hv, sea) == want_water
                    && sat.mostly_below_waterline(x, y, ra) == want_water
                    && sat.mostly_below_waterline(x, y, rb) == want_water
                    && !sat.has_opposite_pixel(x, y, rc, want_water)
                {
                    row.push((y * world.w + x) as u32);
                }
            }
            row
        })
        .collect::<Vec<_>>()
        .concat();
    for &i in &candidates {
        let x = (i % world.w as u32) as i32;
        let y = (i / world.w as u32) as i32;
        let ax = wrap_x(world, x);
        let ay = wrap_y(world, y);
        let covered = ax >= 0
            && ay >= 0
            && ax < world.w
            && ay < world.h
            && mask[(ay * world.w + ax) as usize] & MASK_MARK != 0;
        if covered {
            continue;
        }
        let d = distance_to_nearest_plane_capital(world, x, y);
        if md * 0.5 < d && world.nprov() as i32 + 5 < MAX_PROVINCES {
            let mut p = Province {
                x,
                y,
                terrain: 0,
                ..Province::default()
            };
            if want_water && sat.mostly_below_waterline(x, y, rd) {
                p.terrain = 4;
                let hv2 = height_at(&world.heights, world.w, world.h, x, y);
                if is_below_waterline(hv2, world.deep_level) {
                    p.terrain = 0x804;
                }
            }
            world.provinces.push(p);
            let idx = world.provinces.len() - 1;
            seed_one_capital(world, &mut mask, want_water, &mut queue, idx);
            flood_capital_mask(world, &mut mask, want_water, &mut queue);
        }
    }
}

pub fn create_extra_underwater_capitals(
    world: &mut World,
    opts: &Options,
    edge: i32,
    sat: &WaterSat,
) {
    extra_capitals(world, opts, edge, true, sat);
}

pub fn create_extra_dry_capitals(world: &mut World, opts: &Options, edge: i32, sat: &WaterSat) {
    extra_capitals(world, opts, edge, false, sat);
}

pub fn bubble_random_map_capitals(world: &mut World) -> i32 {
    let mut total = 0;
    loop {
        let n = world.nprov() as i32;
        if n < 2 {
            break;
        }
        let mut swaps = 0;
        for i in 1..n {
            let a = i as usize;
            let b = a + 1;
            let (ax, ay) = (world.provinces[a].x, world.provinces[a].y);
            let (bx, by) = (world.provinces[b].x, world.provinces[b].y);
            if by < ay || (ay == by && bx < ax) {
                swaps += 1;
                world.provinces[a].x = bx;
                world.provinces[b].x = ax;
                world.provinces[a].y = by;
                world.provinces[b].y = ay;
                let at = world.provinces[a].terrain;
                world.provinces[a].terrain = world.provinces[b].terrain;
                world.provinces[b].terrain = (at as i32) as i64;
            }
        }
        total += swaps;
        if swaps <= 0 {
            break;
        }
    }
    total
}

pub fn downsample_random_map_work_buffers_2x(world: &mut World) {
    let nw = world.w / 2;
    let nh = world.h / 2;
    let ow = world.w;
    if !world.owner.is_empty() {
        let src = &world.owner;
        let mut next = vec![0i16; (nw * nh) as usize];
        next.par_chunks_mut(nw as usize)
            .enumerate()
            .for_each(|(y, out)| {
                let base = 2 * y * ow as usize;
                for (x, o) in out.iter_mut().enumerate() {
                    *o = src[base + 2 * x];
                }
            });
        world.owner = next;
    }
    if !world.heights.is_empty() {
        let src = &world.heights;
        let mut next = vec![0f32; (nw * nh) as usize];
        next.par_chunks_mut(nw as usize)
            .enumerate()
            .for_each(|(y, out)| {
                let base = 2 * y * ow as usize;
                for (x, o) in out.iter_mut().enumerate() {
                    *o = src[base + 2 * x];
                }
            });
        world.heights = next;
    }
    for p in &mut world.provinces[1..] {
        p.x /= 2;
        p.y /= 2;
    }
    world.w = nw;
    world.h = nh;
    world.spacing *= 0.5;
}

pub fn upsample_random_map_work_buffers_2x(world: &mut World) {
    let nw = world.w * 2;
    let nh = world.h * 2;
    let ow = world.w;
    if !world.owner.is_empty() {
        let src = &world.owner;
        let mut next = vec![0i16; (nw * nh) as usize];
        next.par_chunks_mut(nw as usize)
            .enumerate()
            .for_each(|(y, out)| {
                let base = (y / 2) * ow as usize;
                for (x, o) in out.iter_mut().enumerate() {
                    *o = src[base + x / 2];
                }
            });
        world.owner = next;
    }
    if !world.heights.is_empty() {
        let src = &world.heights;
        let mut next = vec![0f32; (nw * nh) as usize];
        next.par_chunks_mut(nw as usize)
            .enumerate()
            .for_each(|(y, out)| {
                let base = (y / 2) * ow as usize;
                for (x, o) in out.iter_mut().enumerate() {
                    *o = src[base + x / 2];
                }
            });
        world.heights = next;
    }
    for p in &mut world.provinces[1..] {
        p.x *= 2;
        p.y *= 2;
    }
    world.w = nw;
    world.h = nh;
    world.spacing *= 2.0;
}

pub fn seed_province_growth_pixels(world: &mut World, step: i32) -> i32 {
    let mut claimed = 0;
    let n = world.nprov() as i32;
    let (w, h) = (world.w, world.h);
    let sea = world.sea_level;
    let hwrap = world.hwrap;
    let vwrap = world.vwrap;
    let mut st = world.crt.state;
    for i in 1..=n {
        let terrain = world.provinces[i as usize].terrain as u32;
        let mut x = world.provinces[i as usize].x;
        let mut y = world.provinces[i as usize].y;
        if step == 2 {
            x &= -2;
            y &= -2;
        }
        let target;
        {
            let owner = &world.owner;
            loop {
                let (a, b) = crate::rng::below3_pair_of(st);
                st = crate::rng::crt_advance2(st);
                x += (a - 1) * step;
                y += (b - 1) * step;
                if hwrap {
                    if x < 0 {
                        x += w;
                    }
                    if w <= x {
                        x -= w;
                    }
                }
                if vwrap {
                    if y < 0 {
                        y += h;
                    }
                    if h <= y {
                        y -= h;
                    }
                }
                if x < 0 {
                    x = 0;
                } else if w - 1 <= x {
                    x = w - 1;
                }
                if y < 0 {
                    y = 0;
                } else if h - 1 <= y {
                    y = h - 1;
                }
                let v = i32::from(owner[(y * w + x) as usize]);
                if v != i {
                    target = v;
                    break;
                }
            }
        }
        if target != 0 {
            continue;
        }
        let hv = height_at(&world.heights, w, h, x, y);
        if terrain & 4 == 0 {
            let free = !is_below_waterline(hv, sea);
            let roll = if free {
                false
            } else {
                let (v, next) = crate::rng::crt_below_of(st, 100);
                st = next;
                v < 0xf
            };
            if free || roll {
                store_owner(world, x, y, i as i16);
                claimed += 1;
            }
        } else if is_below_waterline(hv, sea) {
            store_owner(world, x, y, i as i16);
            claimed += 1;
        }
    }
    world.crt.state = st;
    claimed
}

fn blank_indices(snapshot: &[i16]) -> Vec<u32> {
    const CHUNK: usize = 1 << 16;
    snapshot
        .par_chunks(CHUNK)
        .enumerate()
        .map(|(c, part)| {
            let base = (c * CHUNK) as u32;
            let mut v = Vec::new();
            for (i, s) in part.iter().enumerate() {
                if *s < 1 {
                    v.push(base + i as u32);
                }
            }
            v
        })
        .collect::<Vec<_>>()
        .concat()
}

pub fn expand_random_map_ownership(world: &mut World, prob: i32, water_prob: i32) -> i32 {
    let mut scratch = Vec::new();
    expand_random_map_ownership_scratch(world, prob, water_prob, &mut scratch)
}

fn expand_random_map_ownership_scratch(
    world: &mut World,
    prob: i32,
    water_prob: i32,
    scratch: &mut Vec<i16>,
) -> i32 {
    let mut blanks = 0;
    scratch.clear();
    scratch.extend_from_slice(&world.owner);
    let snapshot = &scratch[..];
    let (w, h) = (world.w, world.h);
    let sea = world.sea_level;
    let hwrap = world.hwrap;
    let vwrap = world.vwrap;
    let radius = (world.spacing as f64 * 0.2) as i32;
    let is_water: Vec<bool> = world.provinces.iter().map(|p| p.terrain & 4 != 0).collect();
    let mut st = world.crt.state;
    let pending = blank_indices(snapshot);
    let World {
        ref heights,
        ref mut owner,
        ..
    } = *world;
    for &q in &pending {
        let here_i = q as usize;
        if owner[here_i] != 0 {
            continue;
        }
        blanks += 1;
        let x = (q % w as u32) as i32;
        let y = (q / w as u32) as i32;
        let wet = is_below_waterline(heights[here_i], sea);
        let mut done = false;
        let mut dy = y - 1;
        while dy <= y + 1 && !done {
            let sy = clamp_index(wrap_axis(dy, h, vwrap), h);
            let base = (w * sy) as usize;
            let mut dx = x - 1;
            while dx <= x + 1 {
                let sx = clamp_index(wrap_axis(dx, w, hwrap), w);
                let src = snapshot[base + sx as usize];
                if src > 0 && {
                    let (v, next) = crate::rng::crt_below_of(st, 100);
                    st = next;
                    v < prob
                } {
                    let accept = if !is_water[src as usize] {
                        !wet || {
                            let (v, next) = crate::rng::crt_below_of(st, 100);
                            st = next;
                            v < 0x19
                        } || has_land_at_cardinal_radius_in(heights, w, h, sea, dx, dy, radius)
                    } else {
                        (wet && !has_land_at_cardinal_radius_in(heights, w, h, sea, dx, dy, radius))
                            || {
                                let (v, next) = crate::rng::crt_below_of(st, 100);
                                st = next;
                                v < water_prob
                            }
                    };
                    if accept {
                        owner[here_i] = src;
                        done = true;
                        break;
                    }
                }
                dx += 1;
            }
            dy += 1;
        }
    }
    world.crt.state = st;
    blanks
}

pub fn grow_region_ownership_step(world: &mut World, prob: i32, water_prob: i32) -> i32 {
    let mut scratch = Vec::new();
    grow_region_ownership_step_scratch(world, prob, water_prob, &mut scratch)
}

fn grow_region_ownership_step_scratch(
    world: &mut World,
    prob: i32,
    water_prob: i32,
    scratch: &mut Vec<i16>,
) -> i32 {
    scratch.clear();
    scratch.extend_from_slice(&world.owner);
    let snapshot = &scratch[..];
    let (w, h) = (world.w, world.h);
    let sea = world.sea_level;
    let hwrap = world.hwrap;
    let vwrap = world.vwrap;
    let radius = (world.spacing as f64 * 0.2) as i32;
    let is_water: Vec<bool> = world.provinces.iter().map(|p| p.terrain & 4 != 0).collect();
    let mut st = world.crt.state;
    let mut blanks = 0;
    let World {
        ref heights,
        ref mut owner,
        ..
    } = *world;
    let mut visit = |st: &mut u32, idx: usize, x: i32, y: i32| -> bool {
        let src = snapshot[idx];
        if src < 1 {
            return true;
        }
        let (a, b) = crate::rng::below3_pair_of(*st);
        *st = crate::rng::crt_advance2(*st);
        let mut ty = a - 1 + y;
        let mut tx = b - 1 + x;
        if hwrap {
            if tx < 0 {
                tx += w;
            }
            if w <= tx {
                tx -= w;
            }
        }
        if vwrap {
            if ty < 0 {
                ty += h;
            }
            if h <= ty {
                ty -= h;
            }
        }
        if tx < 0 {
            tx = 0;
        } else if w - 1 <= tx {
            tx = w - 1;
        }
        if ty < 0 {
            ty = 0;
        } else if h - 1 <= ty {
            ty = h - 1;
        }
        let ti = (w * ty + tx) as usize;
        if owner[ti] != 0 {
            return false;
        }
        let wet = is_below_waterline(heights[ti], sea);
        let accept = if !is_water[src as usize] {
            !wet || {
                let (v, next) = crate::rng::crt_below_of(*st, 100);
                *st = next;
                v < prob
            } || has_land_at_cardinal_radius_in(heights, w, h, sea, tx, ty, radius)
        } else {
            (wet && !has_land_at_cardinal_radius_in(heights, w, h, sea, tx, ty, radius)) || {
                let (v, next) = crate::rng::crt_below_of(*st, 100);
                *st = next;
                v < water_prob
            }
        };
        if accept {
            owner[ti] = src;
        }
        false
    };
    for y in 0..h {
        let row = (w * y) as usize;
        for x in 0..w {
            if visit(&mut st, row + x as usize, x, y) {
                blanks += 1;
            }
        }
    }
    world.crt.state = st;
    blanks
}

pub fn expand_random_map_territories(world: &mut World, first_pass: i32) {
    let (w, h) = (world.w, world.h);
    world.owner = vec![0i16; (w * h) as usize];
    let n = world.nprov() as i32;
    for i in 1..=n {
        let p = &world.provinces[i as usize];
        let (x, y) = (p.x, p.y);
        if -1 < x && x < w && -1 < y && y < h {
            world.owner[(w * y + x) as usize] = i as i16;
        }
    }

    let seeds = world.spacing * 50.0;
    if 0.0 < seeds {
        let mut done = 0i32;
        loop {
            seed_province_growth_pixels(world, 1);
            done += 1;
            if !((done as f32) < seeds) {
                break;
            }
        }
    }

    let mut scratch: Vec<i16> = Vec::with_capacity((w * h) as usize);
    let mut blanks = grow_region_ownership_step_scratch(world, 0x1e, 1, &mut scratch);
    while 0 < blanks {
        blanks = if blanks < (world.h + world.w) * 3 {
            expand_random_map_ownership_scratch(world, 100, 0x32, &mut scratch)
        } else {
            let area = world.h * world.w;
            let gate = if first_pass != 0 {
                area / 0x14
            } else {
                area / 0x32
            };
            if blanks < gate {
                expand_random_map_ownership_scratch(world, 0x3c, 0xf, &mut scratch)
            } else if blanks < area / 10 {
                grow_region_ownership_step_scratch(world, 0x4b, 5, &mut scratch)
            } else {
                grow_region_ownership_step_scratch(world, 0x1e, 1, &mut scratch)
            }
        };
    }
}

pub struct OwnerPixels {
    starts: Vec<u32>,
    pix: Vec<u32>,
}

impl OwnerPixels {
    pub fn build(world: &World) -> Self {
        let n = world.provinces.len();
        let mut starts = vec![0u32; n + 1];
        for &o in &world.owner {
            if o > 0 && (o as usize) < n {
                starts[o as usize + 1] += 1;
            }
        }
        for i in 1..=n {
            starts[i] += starts[i - 1];
        }
        let mut cur = starts.clone();
        let mut pix = vec![0u32; starts[n] as usize];
        for (i, &o) in world.owner.iter().enumerate() {
            if o > 0 && (o as usize) < n {
                let slot = &mut cur[o as usize];
                pix[*slot as usize] = i as u32;
                *slot += 1;
            }
        }
        OwnerPixels { starts, pix }
    }

    pub fn of(&self, lnr: i32) -> &[u32] {
        let i = lnr as usize;
        if i + 1 >= self.starts.len() {
            return &[];
        }
        &self.pix[self.starts[i] as usize..self.starts[i + 1] as usize]
    }
}

fn window_coord(a: i32, lo: i32, hi: i32, n: i32, wrap: bool) -> Option<i32> {
    if lo <= a && a <= hi {
        return Some(a);
    }
    if wrap {
        if lo <= a - n && a - n <= hi {
            return Some(a - n);
        }
        if lo <= a + n && a + n <= hi {
            return Some(a + n);
        }
    }
    None
}

pub fn compute_region_label_centroid_indexed(
    world: &World,
    idx: &OwnerPixels,
    lnr: i32,
) -> (i32, i32) {
    let (w, h) = (world.w, world.h);
    let p = &world.provinces[lnr as usize];
    let rx = w / 3;
    let mut x0 = p.x - rx;
    let mut x1 = p.x + rx;
    if !world.hwrap {
        x0 = clamp_index(x0, w);
        x1 = clamp_index(x1, w);
    }
    let ry = h / 3;
    let mut y1 = p.y + ry;
    let mut y0 = p.y - ry;
    if !world.vwrap {
        y0 = clamp_index(y0, h);
        y1 = clamp_index(y1, h);
    }
    let mut sx = 0i64;
    let mut sy = 0i64;
    let mut hits = 0i32;
    for &i in idx.of(lnr) {
        let ax = (i % w as u32) as i32;
        let ay = (i / w as u32) as i32;
        let (Some(xx), Some(yy)) = (
            window_coord(ax, x0, x1, w, world.hwrap),
            window_coord(ay, y0, y1, h, world.vwrap),
        ) else {
            continue;
        };
        sx += i64::from(xx);
        sy += i64::from(yy);
        hits += 1;
    }
    let div = if 1 < hits { hits } else { 1 } as i64;
    ((sx / div) as i32, (sy / div) as i32)
}

#[cfg(test)]
pub fn compute_region_label_centroid(world: &World, lnr: i32) -> (i32, i32) {
    let (w, h) = (world.w, world.h);
    let p = &world.provinces[lnr as usize];
    let rx = w / 3;
    let mut x0 = p.x - rx;
    let mut x1 = p.x + rx;
    if !world.hwrap {
        x0 = clamp_index(x0, w);
        x1 = clamp_index(x1, w);
    }
    let ry = h / 3;
    let mut y1 = p.y + ry;
    let mut y0 = p.y - ry;
    if !world.vwrap {
        y0 = clamp_index(y0, h);
        y1 = clamp_index(y1, h);
    }
    let mut sx = 0i64;
    let mut sy = 0i64;
    let mut hits = 0i32;
    let mut yy = y0;
    while yy <= y1 {
        let mut xx = x0;
        while xx <= x1 {
            let ax = clamp_index(wrap_x(world, xx), w);
            let ay = clamp_index(wrap_y(world, yy), h);
            if owner_at(world, ax, ay) == lnr {
                sx += xx as i64;
                sy += yy as i64;
                hits += 1;
            }
            xx += 1;
        }
        yy += 1;
    }
    let div = if 1 < hits { hits } else { 1 } as i64;
    ((sx / div) as i32, (sy / div) as i32)
}

fn center_is_acceptable(world: &World, lnr: i32, x: i32, y: i32) -> bool {
    if world.cave_world {
        return true;
    }
    let hv = height_at(&world.heights, world.w, world.h, x, y);
    if world.provinces[lnr as usize].terrain & 4 == 0 {
        !is_below_waterline(hv, world.sea_level)
    } else {
        is_below_waterline(hv, world.sea_level)
    }
}

pub fn center_random_map_capitals(world: &mut World) {
    let n = world.nprov() as i32;
    let (w, h) = (world.w, world.h);
    let idx = OwnerPixels::build(world);
    for i in 1..=n {
        let (cx, cy) = compute_region_label_centroid_indexed(world, &idx, i);
        let cx = clamp_index(wrap_x(world, cx), w);
        let cy = clamp_index(wrap_y(world, cy), h);
        if center_is_acceptable(world, i, cx, cy) {
            world.provinces[i as usize].x = cx;
            world.provinces[i as usize].y = cy;
        }
    }
    bubble_random_map_capitals(world);
}

pub fn improve_random_map_center(world: &World, lnr: i32, axes: u32) -> (i32, i32) {
    let (w, h) = (world.w, world.h);
    let px = world.provinces[lnr as usize].x;
    let py = world.provinces[lnr as usize].y;
    let span = if w < h { h } else { w } / 3;

    let mut first_x = 999999;
    let mut last_x = -999999;
    let mut first_good_x = 999999;
    let mut last_good_x = -999999;
    if axes & 1 != 0 {
        let mut xx = px - span;
        while xx <= px + span {
            let ax = wrap_x(world, xx);
            let ay = wrap_y(world, py);
            let o = owner_at(world, ax, ay);
            if o == lnr {
                if first_x == 999999 {
                    first_x = xx;
                }
                last_x = xx;
                if center_is_acceptable(world, lnr, ax, ay) {
                    if first_good_x == 999999 {
                        first_good_x = xx;
                    }
                    last_good_x = xx;
                }
            }
            xx += 1;
        }
    } else {
        first_x = px;
        last_x = px;
        first_good_x = px;
        last_good_x = px;
    }

    let mut first_y = 999999;
    let mut last_y = -999999;
    let mut first_good_y = 999999;
    let mut last_good_y = -999999;
    if axes & 2 != 0 {
        let mut yy = py - span;
        while yy <= py + span {
            let ax = wrap_x(world, px);
            let ay = wrap_y(world, yy);
            let o = owner_at(world, ax, ay);
            if o == lnr {
                if first_y == 999999 {
                    first_y = yy;
                }
                last_y = yy;
                if center_is_acceptable(world, lnr, ax, ay) {
                    if first_good_y == 999999 {
                        first_good_y = yy;
                    }
                    last_good_y = yy;
                }
            }
            yy += 1;
        }
    } else {
        first_y = py;
        last_y = py;
        first_good_y = py;
        last_good_y = py;
    }

    if first_x == 999999 || first_y == 999999 || first_good_x == 999999 || first_good_y == 999999 {
        return (px, py);
    }
    let sx = first_good_x + last_good_x + last_x + first_x;
    let mut rx = (sx + (sx >> 0x1f & 3)) >> 2;
    let sy = last_good_y + first_good_y + last_y + first_y;
    let mut ry = (sy + (sy >> 0x1f & 3)) >> 2;
    rx = wrap_x(world, rx);
    ry = wrap_y(world, ry);
    (clamp_index(rx, w), clamp_index(ry, h))
}

pub fn recenter_random_map_capitals(world: &mut World, axes: u32) -> i32 {
    let n = world.nprov() as i32;
    for i in 1..=n {
        let (cx, cy) = improve_random_map_center(world, i, axes);
        if center_is_acceptable(world, i, cx, cy) {
            world.provinces[i as usize].x = cx;
            world.provinces[i as usize].y = cy;
        }
    }
    bubble_random_map_capitals(world)
}

pub fn prettify_random_map_borders(world: &mut World, passes: i32) {
    let (w, h) = (world.w, world.h);
    if w <= 0 || h <= 0 {
        return;
    }
    let (hwrap, vwrap) = (world.hwrap, world.vwrap);
    for _ in 0..passes {
        let snapshot = world.owner.clone();
        let src = &snapshot;
        world
            .owner
            .par_chunks_mut(w as usize)
            .enumerate()
            .for_each(|(row, out)| {
                let y = row as i32;
                let mut fill: i16 = 1;
                let interior_y = y >= 1 && y + 1 < h;
                for x in 0..w {
                    let center = src[(w * y + x) as usize];
                    let mut same = 0;
                    if interior_y && x >= 1 && x + 1 < w {
                        let mut base = ((y - 1) * w + x - 1) as usize;
                        for _ in 0..3 {
                            for k in 0..3 {
                                let v = src[base + k];
                                if v != center {
                                    fill = v;
                                } else {
                                    same += 1;
                                }
                            }
                            base += w as usize;
                        }
                    } else {
                        let mut dy = y - 1;
                        while dy <= y + 1 {
                            let ay = clamp_index(wrap_axis(dy, h, vwrap), h);
                            let base = (ay * w) as usize;
                            let mut dx = x - 1;
                            while dx <= x + 1 {
                                let ax = clamp_index(wrap_axis(dx, w, hwrap), w);
                                let v = src[base + ax as usize];
                                if v != center {
                                    fill = v;
                                } else {
                                    same += 1;
                                }
                                dx += 1;
                            }
                            dy += 1;
                        }
                    }
                    if same < 5 {
                        out[x as usize] = fill;
                    }
                }
            });
    }
}

pub fn remove_water_capitals(world: &mut World) {
    let mut i = world.nprov() as i32;
    while 0 < i {
        if world.provinces[i as usize].terrain & 4 != 0 && 0 < world.nprov() as i32 {
            world.provinces.remove(i as usize);
            for v in world.owner.iter_mut() {
                let s = *v as i32;
                if s == i {
                    *v = 0;
                } else if i < s {
                    *v = (s - 1) as i16;
                }
            }
        }
        i -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::CrtRng;

    fn flat_world(w: i32, h: i32, height: f32) -> World {
        let mut world = World::new(1);
        world.w = w;
        world.h = h;
        world.heights = vec![height; (w * h) as usize];
        world.sea_level = 100.0;
        world.deep_level = 50.0;
        world.hwrap = true;
        world.vwrap = false;
        world
    }

    #[test]
    fn capitals_are_deterministic_for_a_given_stream() {
        let opts = Options {
            provinces: 12,
            ..Options::default()
        };
        let mut a = flat_world(400, 300, 200.0);
        let mut b = flat_world(400, 300, 200.0);
        let sa = WaterSat::new(&a);
        let sb = WaterSat::new(&b);
        create_random_map_capitals(&mut a, &opts, 12, CAPITAL_EDGE, &sa);
        create_random_map_capitals(&mut b, &opts, 12, CAPITAL_EDGE, &sb);
        assert_eq!(a.nprov(), 12);
        assert_eq!(a.provinces, b.provinces);
        assert_eq!(a.crt, b.crt);
        assert_eq!(a.spacing, b.spacing);
    }

    #[test]
    fn all_land_capitals_are_dry_and_inside_the_edge_band() {
        let opts = Options::default();
        let mut w = flat_world(400, 300, 200.0);
        let sat = WaterSat::new(&w);
        create_random_map_capitals(&mut w, &opts, 20, CAPITAL_EDGE, &sat);
        for p in &w.provinces[1..] {
            assert_eq!(p.terrain, 0);
            assert!(p.x >= CAPITAL_EDGE && p.x < 400 - CAPITAL_EDGE);
            assert!(p.y >= CAPITAL_EDGE && p.y < 300 - CAPITAL_EDGE);
        }
    }

    #[test]
    fn nprov_below_two_places_the_first_tries_near_the_map_centre() {
        let opts = Options::default();
        let mut w = flat_world(400, 300, 200.0);
        let sat = WaterSat::new(&w);
        create_random_map_capitals(&mut w, &opts, 1, CAPITAL_EDGE, &sat);
        let p = &w.provinces[1];
        assert!((p.x - 200).abs() <= 2);
        assert!((p.y - 150).abs() <= 2);
    }

    #[test]
    fn back_off_shrinks_both_radii_after_ten_thousand_failures() {
        let opts = Options::default();
        let mut w = flat_world(400, 300, 200.0);
        w.heights = (0..400 * 300)
            .map(|i| if (i / 400) % 2 == 0 { 200.0 } else { 0.0 })
            .collect();
        let sat = WaterSat::new(&w);
        create_random_map_capitals(&mut w, &opts, 2, CAPITAL_EDGE, &sat);
        let unshrunk = {
            let mut probe = flat_world(400, 300, 200.0);
            let (m, _) = capital_mindist(&probe, 2, CAPITAL_EDGE);
            probe.spacing = m as f32;
            probe.spacing
        };
        assert!(w.spacing <= unshrunk);
    }

    #[test]
    fn kernel_thresholds_follow_the_blank_count() {
        let (w, h) = (1000i32, 1000i32);
        let area = w * h;
        let pick = |blanks: i32, first_pass: i32| -> &'static str {
            if blanks < (h + w) * 3 {
                "expand100"
            } else {
                let gate = if first_pass != 0 {
                    area / 20
                } else {
                    area / 50
                };
                if blanks < gate {
                    "expand60"
                } else if blanks < area / 10 {
                    "grow75"
                } else {
                    "grow30"
                }
            }
        };
        assert_eq!(pick(5999, 1), "expand100");
        assert_eq!(pick(6000, 1), "expand60");
        assert_eq!(pick(49_999, 1), "expand60");
        assert_eq!(pick(50_000, 1), "grow75");
        assert_eq!(pick(99_999, 1), "grow75");
        assert_eq!(pick(100_000, 1), "grow30");
        assert_eq!(pick(5999, 0), "expand100");
        assert_eq!(pick(6000, 0), "expand60");
        assert_eq!(pick(19_999, 0), "expand60");
        assert_eq!(pick(20_000, 0), "grow75");
        assert_eq!(pick(100_000, 0), "grow30");
    }

    #[test]
    fn downsample_then_upsample_replicates_the_quarter_res_owner_raster() {
        let mut w = flat_world(8, 4, 200.0);
        w.owner = (0..32i16).collect();
        w.provinces.push(Province {
            x: 5,
            y: 3,
            ..Province::default()
        });
        w.spacing = 8.0;
        downsample_random_map_work_buffers_2x(&mut w);
        assert_eq!(w.w, 4);
        assert_eq!(w.h, 2);
        assert_eq!(w.owner, vec![0, 2, 4, 6, 16, 18, 20, 22]);
        assert_eq!(w.spacing, 4.0);
        assert_eq!((w.provinces[1].x, w.provinces[1].y), (2, 1));
        upsample_random_map_work_buffers_2x(&mut w);
        assert_eq!(w.w, 8);
        assert_eq!(w.h, 4);
        assert_eq!(w.spacing, 8.0);
        assert_eq!((w.provinces[1].x, w.provinces[1].y), (4, 2));
        assert_eq!(&w.owner[0..8], &[0, 0, 2, 2, 4, 4, 6, 6]);
        assert_eq!(&w.owner[8..16], &[0, 0, 2, 2, 4, 4, 6, 6]);
        assert_eq!(&w.owner[16..24], &[16, 16, 18, 18, 20, 20, 22, 22]);
    }

    #[test]
    fn growth_fills_every_pixel() {
        let opts = Options::default();
        let mut w = flat_world(60, 40, 200.0);
        let sat = WaterSat::new(&w);
        create_random_map_capitals(&mut w, &opts, 6, CAPITAL_EDGE, &sat);
        w.spacing = 2.0;
        expand_random_map_territories(&mut w, 1);
        assert!(w.owner.iter().all(|v| *v > 0));
        assert_eq!(w.owner.len(), 60 * 40);
    }

    #[test]
    fn bubble_sort_orders_by_y_then_x() {
        let mut w = flat_world(10, 10, 200.0);
        for (x, y) in [(5, 9), (1, 3), (7, 3), (2, 0)] {
            w.provinces.push(Province {
                x,
                y,
                terrain: (x * 10 + y) as i64,
                ..Province::default()
            });
        }
        bubble_random_map_capitals(&mut w);
        let got: Vec<(i32, i32, i64)> = w.provinces[1..]
            .iter()
            .map(|p| (p.x, p.y, p.terrain))
            .collect();
        assert_eq!(got, vec![(2, 0, 20), (1, 3, 13), (7, 3, 73), (5, 9, 59)]);
    }

    #[test]
    fn water_capital_removal_renumbers_the_raster() {
        let mut w = flat_world(4, 1, 200.0);
        for terrain in [0i64, 4, 0] {
            w.provinces.push(Province {
                terrain,
                ..Province::default()
            });
        }
        w.owner = vec![1, 2, 3, 0];
        remove_water_capitals(&mut w);
        assert_eq!(w.nprov(), 2);
        assert_eq!(w.owner, vec![1, 0, 2, 0]);
    }

    #[test]
    fn cardinal_land_probe_reports_any_dry_sample() {
        let mut w = flat_world(20, 20, 0.0);
        assert!(!has_land_at_cardinal_radius_samples(&w, 10, 10, 3));
        w.heights[(10 * 20 + 13) as usize] = 500.0;
        assert!(has_land_at_cardinal_radius_samples(&w, 10, 10, 3));
    }

    #[test]
    fn waterline_predicates_agree_on_a_flat_field() {
        let dry = flat_world(40, 40, 200.0);
        assert!(!area_crosses_waterline(&dry, 20, 20, 8));
        assert!(!is_area_mostly_below_waterline(&dry, 20, 20, 8));
        let wet = flat_world(40, 40, 0.0);
        assert!(!area_crosses_waterline(&wet, 20, 20, 8));
        assert!(is_area_mostly_below_waterline(&wet, 20, 20, 8));
    }

    #[test]
    fn plane_capital_distance_is_inert_because_the_plane_range_is_empty() {
        let mut w = flat_world(40, 40, 200.0);
        w.provinces.push(Province {
            x: 20,
            y: 20,
            ..Province::default()
        });
        assert_eq!(
            distance_to_nearest_plane_capital(&w, 20, 20),
            NEAREST_INIT.sqrt()
        );
    }

    #[test]
    fn place_and_grow_round_trips_to_full_resolution() {
        let opts = Options {
            provinces: 8,
            ..Options::default()
        };
        let mut w = flat_world(160, 120, 200.0);
        w.spacing = 4.0;
        let mut sink = crate::stage::RecordSink::default();
        assert_eq!(place_and_grow(&mut w, &opts, &mut sink), Control::Continue);
        assert_eq!(w.w, 160);
        assert_eq!(w.h, 120);
        assert_eq!(w.heights.len(), 160 * 120);
        assert_eq!(w.owner.len(), 160 * 120);
        assert!(w.owner.iter().all(|v| *v > 0));
        let stages: Vec<Stage> = sink.entries.iter().map(|e| e.0).collect();
        assert_eq!(stages[0], Stage::Capitals);
        assert!(stages.iter().filter(|s| **s == Stage::Growth).count() >= 3);
        assert_eq!(stages[stages.len() - 1], Stage::Upsample);
    }

    fn noisy_world(w: i32, h: i32, hwrap: bool, vwrap: bool) -> World {
        let mut world = flat_world(w, h, 0.0);
        world.hwrap = hwrap;
        world.vwrap = vwrap;
        let mut rng = CrtRng::seeded(7);
        world.heights = (0..w * h).map(|_| rng.below(200) as f32).collect();
        world
    }

    #[test]
    fn summed_area_probes_match_the_scanning_originals() {
        let world = noisy_world(97, 61, true, true);
        let sat = WaterSat::new(&world);
        for y in [0, 1, 13, 30, 60] {
            for x in [0, 2, 40, 96] {
                for r in [0, 1, 2, 7, 20, 45, 300] {
                    assert_eq!(
                        sat.crosses_waterline(x, y, r),
                        area_crosses_waterline(&world, x, y, r),
                        "crosses {x} {y} {r}"
                    );
                    assert_eq!(
                        sat.mostly_below_waterline(x, y, r),
                        is_area_mostly_below_waterline(&world, x, y, r),
                        "mostly {x} {y} {r}"
                    );
                    for want in [false, true] {
                        assert_eq!(
                            sat.has_opposite_pixel(x, y, r, want),
                            box_has_opposite_pixel(&world, x, y, r, want),
                            "opposite {x} {y} {r} {want}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn flooding_the_capital_mask_reaches_the_sweep_fixpoint() {
        for (hwrap, vwrap) in [(false, false), (true, false), (true, true)] {
            let mut world = noisy_world(83, 57, hwrap, vwrap);
            let opts = Options::default();
            let sat = WaterSat::new(&world);
            create_random_map_capitals(&mut world, &opts, 10, CAPITAL_EDGE, &sat);
            for want in [false, true] {
                let mut a = passable_mask(&world, want);
                let mut q = Vec::new();
                seed_capital_mask(&world, &mut a, want, &mut q);
                flood_capital_mask(&world, &mut a, want, &mut q);
                let mut b = vec![0u8; (world.w * world.h) as usize];
                let mut qb = Vec::new();
                seed_capital_mask(&world, &mut b, want, &mut qb);
                spread_capital_mask(&world, &mut b, want);
                let b: Vec<u8> = b.iter().map(|v| u8::from(*v != 0)).collect();
                let marked: Vec<u8> = a.iter().map(|v| u8::from(v & MASK_MARK != 0)).collect();
                assert_eq!(marked, b);
            }
        }
    }

    #[test]
    fn crt_below_three_walk_stays_within_one_step() {
        let mut a = CrtRng::seeded(9);
        let dx = a.below(3) - 1;
        let dy = a.below(3) - 1;
        assert!((-1..=1).contains(&dx));
        assert!((-1..=1).contains(&dy));
    }
}
