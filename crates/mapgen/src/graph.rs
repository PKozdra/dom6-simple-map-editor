use rayon::prelude::*;

use crate::world::{World, MAX_NBORS};

pub const PROV_LIMIT: i32 = 0x7c5;
pub const BBOX_INIT: [i16; 4] = [0x7d01, -1, 0x7d01, -1];
pub const NBOR_PIXEL_MIN: i32 = 3;

pub const BORDER_MOUNTAIN_PASS: i64 = 0x1;
pub const BORDER_RIVER: i64 = 0x2;
pub const BORDER_WALL: i64 = 0x4;
pub const BORDER_BRIDGE: i64 = 0x10;
pub const BORDER_MOUNTAIN: i64 = 0x20;
pub const BORDER_BLOCKED: i64 = BORDER_MOUNTAIN_PASS | BORDER_MOUNTAIN;

pub const TERRAIN_SMALL: i64 = 0x1;
pub const TERRAIN_LARGE: i64 = 0x2;
pub const TERRAIN_SEA: i64 = 0x4;
pub const TERRAIN_FRESHWATER: i64 = 0x8;
pub const TERRAIN_HIGHLAND: i64 = 0x10;
pub const TERRAIN_SWAMP: i64 = 0x20;
pub const TERRAIN_WASTE: i64 = 0x40;
pub const TERRAIN_FOREST: i64 = 0x80;
pub const TERRAIN_FARM: i64 = 0x100;
pub const TERRAIN_NOSTART: i64 = 0x200;
pub const TERRAIN_DEEP: i64 = 0x800;
pub const TERRAIN_CAVE: i64 = 0x1000;
pub const TERRAIN_MOUNTAIN: i64 = 0x0080_0000;
pub const TERRAIN_HIDDEN: i64 = 0x1_0000_0000;
pub const TERRAIN_CAVE_WALL: i64 = 0x10_0000_0000;
pub const TERRAIN_DESERT: i64 = 0x2000_0000_0000_0000;
pub const TERRAIN_VARIANT: i64 = 0x4000_0000_0000_0000;

pub fn isqrt(v: i32) -> i32 {
    if v < 1 {
        return 0;
    }
    let mut i: i32 = 1;
    while i < 40000 {
        if v < i * i {
            return i - 1;
        }
        i += 1;
    }
    40000
}

fn valid(w: &World, lnr: i32) -> bool {
    lnr > 0 && (lnr as usize) < w.provinces.len()
}

pub fn addnbor(w: &mut World, a: i32, b: i32) {
    if a != b {
        addnbor_oneway(w, a, b);
        addnbor_oneway(w, b, a);
    }
}

pub fn addnbor_oneway(w: &mut World, a: i32, b: i32) {
    if a == b || a <= 0 || b <= 0 || a >= PROV_LIMIT || b >= PROV_LIMIT {
        return;
    }
    if !valid(w, a) || !valid(w, b) {
        return;
    }
    let p = &mut w.provinces[a as usize];
    if p.nbors.iter().any(|&n| i32::from(n) == b) {
        return;
    }
    if p.nbors.len() >= MAX_NBORS {
        return;
    }
    p.nbors.push(b as u16);
    p.border.push(0);
}

pub fn removenbor(w: &mut World, src: i32, dst: i32) {
    if src <= 0 || dst <= 0 || src >= PROV_LIMIT || dst >= PROV_LIMIT || !valid(w, src) {
        return;
    }
    let p = &mut w.provinces[src as usize];
    if let Some(i) = p.nbors.iter().position(|&n| i32::from(n) == dst) {
        p.nbors.remove(i);
        p.border.remove(i);
    }
}

pub fn nbor_slot(w: &World, a: i32, b: i32) -> Option<usize> {
    if a < 0 || b < 0 || !valid(w, a) {
        return None;
    }
    w.provinces[a as usize]
        .nbors
        .iter()
        .position(|&n| i32::from(n) == b)
}

pub fn get_border_flags(w: &World, a: i32, b: i32) -> i64 {
    match nbor_slot(w, a, b) {
        Some(i) => w.provinces[a as usize].border[i],
        None => 0,
    }
}

pub fn set_border_flags(w: &mut World, a: i32, b: i32, flags: i64) {
    if a < 0 || b < 0 || a == b {
        return;
    }
    if let Some(i) = nbor_slot(w, a, b) {
        w.provinces[a as usize].border[i] |= flags;
    }
    if let Some(i) = nbor_slot(w, b, a) {
        w.provinces[b as usize].border[i] |= flags;
    }
}

pub fn clear_border_flags(w: &mut World, a: i32, b: i32, flags: i64) {
    if a < 0 || b < 0 {
        return;
    }
    if let Some(i) = nbor_slot(w, a, b) {
        w.provinces[a as usize].border[i] &= !flags;
    }
    if let Some(i) = nbor_slot(w, b, a) {
        w.provinces[b as usize].border[i] &= !flags;
    }
}

pub fn provinces_are_neighbors(w: &World, a: i32, b: i32) -> bool {
    if !(1..PROV_LIMIT + 1).contains(&a) || !(1..PROV_LIMIT + 1).contains(&b) {
        return false;
    }
    nbor_slot(w, a, b).is_some()
}

pub struct AdjacencyBits {
    n: usize,
    words: usize,
    bits: Vec<u64>,
}

impl AdjacencyBits {
    pub fn linked(&self, a: i32, b: i32) -> bool {
        if a < 1 || b < 1 || a as usize > self.n || b as usize > self.n {
            return false;
        }
        let i = (a as usize - 1) * self.words * 64 + (b as usize - 1);
        self.bits[i >> 6] & (1u64 << (i & 63)) != 0
    }
}

pub fn adjacency_bitset(w: &World) -> AdjacencyBits {
    let n = w.nprov();
    let words = n.div_ceil(64).max(1);
    let mut bits = vec![0u64; n * words];
    for a in 1..=n {
        if !valid(w, a as i32) || a as i32 > PROV_LIMIT {
            continue;
        }
        for &nb in &w.provinces[a].nbors {
            let b = i32::from(nb);
            if !(1..=PROV_LIMIT).contains(&b) || b as usize > n {
                continue;
            }
            let i = (a - 1) * words * 64 + (b as usize - 1);
            bits[i >> 6] |= 1u64 << (i & 63);
        }
    }
    AdjacencyBits { n, words, bits }
}

pub fn is_border_impassable(w: &World, a: i32, b: i32) -> bool {
    if a <= 0 || b <= 0 {
        return true;
    }
    (get_border_flags(w, a, b) & BORDER_WALL) != 0
}

pub fn count_province_river_borders(w: &World, a: i32) -> i32 {
    if a < 1 || !valid(w, a) {
        return 0;
    }
    let p = &w.provinces[a as usize];
    let mut n = 0;
    for i in 0..p.nbors.len().min(MAX_NBORS) {
        if (p.border[i] & BORDER_RIVER) != 0 {
            n += 1;
        }
    }
    n
}

pub fn terrain(w: &World, lnr: i32) -> i64 {
    if valid(w, lnr) {
        w.provinces[lnr as usize].terrain
    } else {
        0
    }
}

pub fn is_coastal_province(w: &World, a: i32) -> bool {
    if a > PROV_LIMIT || !valid(w, a) {
        return false;
    }
    let t = terrain(w, a);
    if (t & TERRAIN_SEA) != 0 {
        return false;
    }
    if (t >> 0x1c) & 1 != 0 {
        return true;
    }
    let nbors = w.provinces[a as usize].nbors.clone();
    for b in nbors {
        let b = i32::from(b);
        if b < 1 {
            break;
        }
        if !is_border_impassable(w, a, b) && (terrain(w, b) & TERRAIN_SEA) != 0 {
            return true;
        }
    }
    false
}

pub fn random_neighbor_province(w: &mut World, a: i32, allow_sea: bool, ignore_walls: bool) -> i32 {
    if !valid(w, a) {
        return -1;
    }
    let mut pick: Vec<i32> = Vec::new();
    let p = w.provinces[a as usize].clone();
    for i in 0..p.nbors.len().min(MAX_NBORS) {
        let b = i32::from(p.nbors[i]);
        if b < 1 {
            break;
        }
        let tb = terrain(w, b);
        if ((tb & TERRAIN_SEA) == 0 || allow_sea) && ((tb & TERRAIN_CAVE_WALL) == 0 || ignore_walls)
        {
            let f = get_border_flags(w, a, b);
            if (f & BORDER_WALL) == 0 || ignore_walls {
                pick.push(b);
            }
        }
    }
    if pick.is_empty() {
        return -1;
    }
    let k = w.pool.rnd(pick.len() as u32) as usize;
    pick[k]
}

pub fn choose_random_passable_neighbor(w: &mut World, a: i32) -> i32 {
    if a <= 0 || !valid(w, a) {
        return -1;
    }
    let mut pick: Vec<i32> = Vec::new();
    let p = w.provinces[a as usize].clone();
    for i in 0..p.nbors.len().min(MAX_NBORS) {
        let b = i32::from(p.nbors[i]);
        if b < 1 {
            break;
        }
        if !is_border_impassable(w, a, b) && (terrain(w, b) & TERRAIN_CAVE_WALL) == 0 {
            pick.push(b);
        }
    }
    if pick.is_empty() {
        return -1;
    }
    let k = w.pool.rnd(pick.len() as u32) as usize;
    pick[k]
}

fn owner_at(w: &World, x: i32, y: i32) -> i32 {
    if x < 0 || x >= w.w || y < 0 || y >= w.h {
        return 0;
    }
    i32::from(w.owner[(w.w * y + x) as usize])
}

fn clampi(v: i32, lo: i32, hi: i32) -> i32 {
    let v = if v < lo { lo } else { v };
    if v >= hi {
        hi
    } else {
        v
    }
}

fn wrap_clamp_x(w: &World, x: i32) -> i32 {
    let mut v = x;
    if w.hwrap {
        if v < 0 {
            v += w.w;
        }
        if v >= w.w {
            v -= w.w;
        }
    }
    clampi(v, 0, w.w - 1)
}

fn wrap_clamp_y(w: &World, y: i32) -> i32 {
    let mut v = y;
    if w.vwrap {
        if v < 0 {
            v += w.h;
        }
        if v >= w.h {
            v -= w.h;
        }
    }
    clampi(v, 0, w.h - 1)
}

pub fn compute_neighbor_pixels(w: &World, lnr: i32, counts: &mut [i32]) {
    for c in counts.iter_mut() {
        *c = 0;
    }
    if lnr <= 0 || w.h <= 0 {
        return;
    }
    let mut last_row: i32 = -1;
    let mut y = 0;
    loop {
        let mut x = y & 1;
        while x < w.w {
            if owner_at(w, x, y) == lnr {
                let probes = [
                    (wrap_clamp_x(w, x + 1), wrap_clamp_y(w, y)),
                    (wrap_clamp_x(w, x - 1), wrap_clamp_y(w, y)),
                    (wrap_clamp_x(w, x), wrap_clamp_y(w, y + 1)),
                    (wrap_clamp_x(w, x), wrap_clamp_y(w, y - 1)),
                ];
                for (px, py) in probes {
                    let id = owner_at(w, px, py);
                    if id != lnr && id >= 0 && (id as usize) < counts.len() {
                        counts[id as usize] += 1;
                    }
                }
                last_row = y;
            }
            x += 2;
        }
        let keep = last_row < 0 || y <= last_row + 5 || w.vwrap;
        y += 1;
        if !keep || y >= w.h {
            break;
        }
    }
}

pub fn find_all_random_map_neighbors(w: &mut World) {
    let n = w.nprov() as i32;
    for p in w.provinces.iter_mut() {
        p.nbors.clear();
        p.border.clear();
    }
    if n <= 0 || w.h <= 0 {
        return;
    }
    let nslots = w.provinces.len();
    let mut last_row = vec![-1i32; nslots];
    let mut cut = vec![i32::MAX; nslots];
    if !w.vwrap {
        for y in 0..w.h {
            let mut x = y & 1;
            while x < w.w {
                let id = owner_at(w, x, y);
                if id > 0 && (id as usize) < nslots {
                    let i = id as usize;
                    if last_row[i] != y {
                        if last_row[i] >= 0 && cut[i] == i32::MAX && y > last_row[i] + 6 {
                            cut[i] = last_row[i];
                        }
                        last_row[i] = y;
                    }
                }
                x += 2;
            }
        }
    }
    let mut pairs: Vec<u64> = Vec::new();
    for y in 0..w.h {
        let mut x = y & 1;
        while x < w.w {
            let id = owner_at(w, x, y);
            if id > 0 && (id as usize) < nslots && id <= n && y <= cut[id as usize] {
                let probes = [
                    (wrap_clamp_x(w, x + 1), wrap_clamp_y(w, y)),
                    (wrap_clamp_x(w, x - 1), wrap_clamp_y(w, y)),
                    (wrap_clamp_x(w, x), wrap_clamp_y(w, y + 1)),
                    (wrap_clamp_x(w, x), wrap_clamp_y(w, y - 1)),
                ];
                for (px, py) in probes {
                    let b = owner_at(w, px, py);
                    if b != id && b > 0 && b <= n {
                        pairs.push(((id as u64) << 32) | b as u64);
                    }
                }
            }
            x += 2;
        }
    }
    pairs.par_sort_unstable();
    let mut i = 0;
    while i < pairs.len() {
        let key = pairs[i];
        let mut j = i;
        while j < pairs.len() && pairs[j] == key {
            j += 1;
        }
        if (j - i) as i32 > NBOR_PIXEL_MIN {
            addnbor(w, (key >> 32) as i32, (key & 0xffff_ffff) as i32);
        }
        i = j;
    }
}

pub fn precalculate_random_map_stuff(w: &mut World) {
    let n = w.provinces.len();
    w.bbox = vec![BBOX_INIT; n.max(1)];
    for y in 0..w.h {
        for x in 0..w.w {
            let id = i32::from(w.owner[(w.w * y + x) as usize]);
            if id < 0 || id as usize >= w.bbox.len() {
                continue;
            }
            let b = &mut w.bbox[id as usize];
            if (x as i16) <= b[0] {
                b[0] = x as i16;
            }
            if b[1] <= x as i16 {
                b[1] = x as i16;
            }
            if (y as i16) <= b[2] {
                b[2] = y as i16;
            }
            if b[3] <= y as i16 {
                b[3] = y as i16;
            }
        }
    }
    w.edge_dist = vec![-1i16; (w.w * w.h).max(0) as usize];
}

pub fn get_province_edge_distance(w: &mut World, prov: i32, x: i32, y: i32) -> i32 {
    if owner_at(w, x, y) != prov {
        return 0;
    }
    if w.edge_dist.is_empty() {
        return 0;
    }
    let ci = (w.h * y + x) as usize;
    if ci >= w.edge_dist.len() {
        return 0;
    }
    let cached = i32::from(w.edge_dist[ci]);
    if cached >= 0 {
        return cached;
    }
    let bb = w.bbox[prov as usize];
    let mut best = 9_999_999i32;
    let mut sy = i32::from(bb[2]) - 1;
    while sy <= i32::from(bb[3]) + 1 {
        let mut sx = i32::from(bb[0]) - 1;
        while sx <= i32::from(bb[1]) + 1 {
            let id = owner_at(w, sx, sy);
            if id != prov {
                let mut dx = (sx - x).abs();
                if w.hwrap {
                    let alt = (w.w - dx).abs();
                    if alt < dx {
                        dx = alt;
                    }
                }
                let mut dy = (sy - y).abs();
                if w.hwrap {
                    let alt = (w.h - dy).abs();
                    if alt < dy {
                        dy = alt;
                    }
                }
                let d = dy * dy + dx * dx;
                if d < best {
                    best = d;
                }
            }
            sx += 2;
        }
        sy += 2;
    }
    let out = isqrt(best);
    w.edge_dist[ci] = out as i16;
    out
}

pub struct EdgeField {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    dist: Vec<i32>,
}

impl EdgeField {
    fn at(&self, x: i32, y: i32) -> Option<i32> {
        let (lx, ly) = (x - self.x0, y - self.y0);
        if lx < 0 || ly < 0 || lx >= self.w || ly >= self.h {
            return None;
        }
        Some(self.dist[(ly * self.w + lx) as usize])
    }
}

fn axis_gap(a: i32, b: i32, span: i32, wrap: bool) -> i32 {
    let mut d = (a - b).abs();
    if wrap {
        let alt = (span - d).abs();
        if alt < d {
            d = alt;
        }
    }
    d
}

pub fn province_edge_field(w: &World, prov: i32) -> EdgeField {
    let bb = w.bbox[prov as usize];
    let (x0, x1) = (i32::from(bb[0]), i32::from(bb[1]));
    let (y0, y1) = (i32::from(bb[2]), i32::from(bb[3]));
    let (fw, fh) = ((x1 - x0 + 1).max(0), (y1 - y0 + 1).max(0));
    let mut rows: Vec<(i32, Vec<i32>)> = Vec::new();
    let mut sy = y0 - 1;
    while sy <= y1 + 1 {
        let mut xs = Vec::new();
        let mut sx = x0 - 1;
        while sx <= x1 + 1 {
            if owner_at(w, sx, sy) != prov {
                xs.push(sx);
            }
            sx += 2;
        }
        if !xs.is_empty() {
            rows.push((sy, xs));
        }
        sy += 2;
    }
    let wrap = w.hwrap;
    let mut dist = vec![0i32; (fw * fh) as usize];
    let mut column = vec![0i32; rows.len()];
    for lx in 0..fw {
        let x = x0 + lx;
        for (k, (_, xs)) in rows.iter().enumerate() {
            let at = xs.partition_point(|&sx| sx < x);
            let mut best = i32::MAX;
            let mut consider = |sx: i32| {
                let d = axis_gap(sx, x, w.w, wrap);
                let d = d * d;
                if d < best {
                    best = d;
                }
            };
            if at < xs.len() {
                consider(xs[at]);
            }
            if at > 0 {
                consider(xs[at - 1]);
            }
            if wrap {
                consider(xs[0]);
                consider(xs[xs.len() - 1]);
            }
            column[k] = best;
        }
        for ly in 0..fh {
            let y = y0 + ly;
            let mut best = 9_999_999i32;
            for (k, (sy, _)) in rows.iter().enumerate() {
                let dy = axis_gap(*sy, y, w.h, wrap);
                let d = dy * dy + column[k];
                if d < best {
                    best = d;
                }
            }
            dist[(ly * fw + lx) as usize] = isqrt(best);
        }
    }
    EdgeField {
        x0,
        y0,
        w: fw,
        h: fh,
        dist,
    }
}

pub fn get_province_edge_distance_from(
    w: &mut World,
    prov: i32,
    x: i32,
    y: i32,
    field: &EdgeField,
) -> i32 {
    if owner_at(w, x, y) != prov {
        return 0;
    }
    if w.edge_dist.is_empty() {
        return 0;
    }
    let ci = (w.h * y + x) as usize;
    if ci >= w.edge_dist.len() {
        return 0;
    }
    let cached = i32::from(w.edge_dist[ci]);
    if cached >= 0 {
        return cached;
    }
    let out = match field.at(x, y) {
        Some(d) => d,
        None => return get_province_edge_distance(w, prov, x, y),
    };
    w.edge_dist[ci] = out as i16;
    out
}

pub fn count_region_land_water_pixels(w: &World, lnr: i32) -> (i32, i32, i32) {
    if lnr <= 0 || lnr as usize >= w.bbox.len() {
        return (0, 0, 0);
    }
    let bb = w.bbox[lnr as usize];
    let (x0, x1) = (i32::from(bb[0]), i32::from(bb[1]));
    let (y0, y1) = (i32::from(bb[2]), i32::from(bb[3]));
    let (mut total, mut land, mut water) = (0, 0, 0);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x < 0 || x >= w.w || y < 0 || y >= w.h {
                continue;
            }
            if i32::from(w.owner[(w.w * y + x) as usize]) != lnr {
                continue;
            }
            total += 1;
            let px = clampi(x.max(0), 0, w.w - 1);
            let py = clampi(y.max(0), 0, w.h - 1);
            if w.heights[(py * w.w + px) as usize] >= w.sea_level {
                land += 1;
            } else {
                water += 1;
            }
        }
    }
    (total, land, water)
}

pub struct BorderIndex {
    entries: Vec<(u64, u32)>,
}

fn pair_key(a: i32, b: i32) -> u64 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    ((lo as u64) << 32) | hi as u64
}

impl BorderIndex {
    pub fn build(w: &World) -> Self {
        let mut entries: Vec<(u64, u32)> = (0..w.h - 1)
            .into_par_iter()
            .map(|y| {
                let row = (w.w * y) as usize;
                let mut out = Vec::new();
                for x in 0..w.w - 1 {
                    let left = i32::from(w.owner[row + x as usize]);
                    let right = i32::from(w.owner[row + x as usize + 1]);
                    let other = if left != right {
                        right
                    } else {
                        let below = i32::from(w.owner[row + w.w as usize + x as usize]);
                        if below == left {
                            continue;
                        }
                        below
                    };
                    if left <= 0 || other <= 0 {
                        continue;
                    }
                    out.push((pair_key(left, other), (row + x as usize) as u32));
                }
                out
            })
            .collect::<Vec<_>>()
            .concat();
        entries.par_sort_unstable();
        BorderIndex { entries }
    }

    pub fn pixels(&self, a: i32, b: i32) -> &[(u64, u32)] {
        let key = pair_key(a, b);
        let lo = self.entries.partition_point(|e| e.0 < key);
        let hi = self.entries.partition_point(|e| e.0 <= key);
        &self.entries[lo..hi]
    }
}

pub fn count_border_pixels_indexed<F: Fn(f32, f32) -> bool>(
    w: &World,
    idx: &BorderIndex,
    a: i32,
    b: i32,
    pred: F,
) -> i32 {
    if a <= 0 || a as usize >= w.bbox.len() {
        return 0;
    }
    let bb = w.bbox[a as usize];
    let x0 = (i32::from(bb[0]) - 1).max(0);
    let y0 = (i32::from(bb[2]) - 1).max(0);
    let x1 = i32::from(bb[1]);
    let y1 = i32::from(bb[3]);
    if x0 >= x1 {
        return 0;
    }
    let mut n = 0;
    for &(_, i) in idx.pixels(a, b) {
        let x = (i % w.w as u32) as i32;
        let y = (i / w.w as u32) as i32;
        if x < x0 || x > x1 - 1 || y < y0 || y > y1 - 1 {
            continue;
        }
        if pred(w.heights[i as usize], w.sea_level) {
            n += 1;
        }
    }
    n
}

pub fn count_land_border_pixels_indexed(w: &World, idx: &BorderIndex, a: i32, b: i32) -> i32 {
    count_border_pixels_indexed(w, idx, a, b, |h, s| h >= s)
}

pub fn count_water_border_pixels_indexed(w: &World, idx: &BorderIndex, a: i32, b: i32) -> i32 {
    count_border_pixels_indexed(w, idx, a, b, |h, s| h < s)
}

pub fn count_carved_border_pixels_indexed(w: &World, idx: &BorderIndex, a: i32, b: i32) -> i32 {
    count_border_pixels_indexed(w, idx, a, b, |h, _| h == crate::writers::RIVER_SENTINEL)
}

fn count_border_pixels<F: Fn(f32, f32) -> bool>(w: &World, a: i32, b: i32, pred: F) -> i32 {
    if a <= 0 || a as usize >= w.bbox.len() {
        return 0;
    }
    let bb = w.bbox[a as usize];
    let x0 = (i32::from(bb[0]) - 1).max(0);
    let y0 = (i32::from(bb[2]) - 1).max(0);
    let x1 = i32::from(bb[1]);
    let y1 = i32::from(bb[3]);
    let mut n = 0;
    let mut y = y0;
    while y < y1 {
        if x0 < x1 {
            let mut x = x0;
            let mut left = owner_at(w, x, y);
            loop {
                let right = owner_at(w, x + 1, y);
                let other = if left != right {
                    right
                } else {
                    owner_at(w, x, y + 1)
                };
                if (left != right || left != other)
                    && (a == left || a == other)
                    && (b == left || b == other)
                {
                    let px = clampi(x.max(0), 0, w.w - 1);
                    let py = clampi(y.max(0), 0, w.h - 1);
                    if pred(w.heights[(py * w.w + px) as usize], w.sea_level) {
                        n += 1;
                    }
                }
                left = right;
                if x + 2 > x1 {
                    break;
                }
                x += 1;
            }
        }
        y += 1;
    }
    n
}

pub fn count_land_border_pixels_between_regions(w: &World, a: i32, b: i32) -> i32 {
    count_border_pixels(w, a, b, |h, s| h >= s)
}

pub fn count_water_border_pixels_between_regions(w: &World, a: i32, b: i32) -> i32 {
    count_border_pixels(w, a, b, |h, s| h < s)
}

pub fn count_carved_border_pixels(w: &World, a: i32, b: i32) -> i32 {
    count_border_pixels(w, a, b, |h, _| h == crate::writers::RIVER_SENTINEL)
}

pub fn calculate_random_map_sizes(w: &mut World) {
    let n = w.nprov() as i32;
    let totals = count_all_region_pixels(w);
    let (mut sea_sum, mut land_sum) = (0.0f32, 0.0f32);
    let (mut sea_n, mut land_n) = (0i32, 0i32);
    for a in 1..=n {
        let total = totals[a as usize];
        if (terrain(w, a) & TERRAIN_SEA) == 0 {
            land_n += 1;
            land_sum += total as f32;
        } else {
            sea_n += 1;
            sea_sum += total as f32;
        }
    }
    w.mean_sea_area = sea_sum / sea_n as f32;
    w.mean_land_area = land_sum / land_n as f32;
    for a in 1..=n {
        let t = terrain(w, a);
        let mean = if (t & TERRAIN_SEA) != 0 {
            w.mean_sea_area
        } else {
            w.mean_land_area
        };
        let area = totals[a as usize] as f32;
        if area < mean * 0.75 {
            w.provinces[a as usize].terrain = t | TERRAIN_SMALL;
        } else if area > mean * 1.3 {
            w.provinces[a as usize].terrain = t | TERRAIN_LARGE;
        }
    }
}

fn count_all_region_pixels(w: &World) -> Vec<i32> {
    let slots = w.provinces.len();
    let width = w.w as usize;
    w.owner
        .par_chunks(width)
        .enumerate()
        .fold(
            || vec![0i32; slots],
            |mut acc, (row, cells)| {
                let y = row as i32;
                for (x, &o) in cells.iter().enumerate() {
                    let id = i32::from(o);
                    if id <= 0 || id as usize >= w.bbox.len() {
                        continue;
                    }
                    let bb = w.bbox[id as usize];
                    if y < i32::from(bb[2]) || y > i32::from(bb[3]) {
                        continue;
                    }
                    let x = x as i32;
                    if x < i32::from(bb[0]) || x > i32::from(bb[1]) {
                        continue;
                    }
                    acc[id as usize] += 1;
                }
                acc
            },
        )
        .reduce(
            || vec![0i32; slots],
            |mut a, b| {
                for (x, y) in a.iter_mut().zip(b.iter()) {
                    *x += y;
                }
                a
            },
        )
}

pub fn ensure_land_exits(w: &mut World) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        if !w.provinces[a as usize].nbors.is_empty() {
            continue;
        }
        let (ax, ay) = (
            w.provinces[a as usize].x as i16 as i32,
            w.provinces[a as usize].y as i16 as i32,
        );
        let mut best = -1i32;
        let mut bestd = 99_999_999.0f64;
        for b in 1..=n {
            if b == a {
                continue;
            }
            let bx = w.provinces[b as usize].x as i16 as i32;
            let by = w.provinces[b as usize].y as i16 as i32;
            let d = f64::from((bx - ax) * (bx - ax) + (by - ay) * (by - ay));
            if d < bestd {
                bestd = d;
                best = b;
            }
        }
        addnbor(w, a, best);
        if (get_border_flags(w, a, best) & BORDER_BLOCKED) != 0 {
            set_border_flags(w, a, best, BORDER_MOUNTAIN_PASS);
        }
    }
}

pub fn disown_random_map_edges(w: &mut World, alpha: &[u8]) {
    if alpha.len() != (w.w * w.h).max(0) as usize {
        return;
    }
    for y in 0..w.h {
        for x in 0..w.w {
            let i = (w.w * y + x) as usize;
            if alpha[i] < 0x32 {
                w.owner[i] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_adjacency_bitset_agrees_with_the_neighbour_scan() {
        let mut w = World::new(5);
        w.w = 8;
        w.h = 8;
        w.owner = vec![0; 64];
        w.heights = vec![0.0; 64];
        w.provinces = vec![crate::world::Province::default(); 7];
        for (a, b) in [(1, 2), (2, 3), (3, 5), (5, 6), (1, 6), (4, 4)] {
            addnbor(&mut w, a, b);
        }
        let bits = adjacency_bitset(&w);
        for a in 0..8 {
            for b in 0..8 {
                assert_eq!(
                    bits.linked(a, b),
                    provinces_are_neighbors(&w, a, b),
                    "{a} {b}"
                );
            }
        }
    }

    fn blob_world(seed: u32, hwrap: bool, wd: i32, ht: i32) -> World {
        let mut w = World::new(seed);
        w.w = wd;
        w.h = ht;
        w.hwrap = hwrap;
        w.owner = vec![0; (wd * ht) as usize];
        w.heights = vec![0.0; (wd * ht) as usize];
        w.provinces = vec![Default::default(); 3];
        let mut rng = crate::rng::CrtRng::seeded(seed);
        let cx = rng.below(wd);
        let cy = rng.below(ht);
        for y in 0..ht {
            for x in 0..wd {
                let dx = axis_gap(x, cx, wd, hwrap);
                let dy = axis_gap(y, cy, ht, hwrap);
                let inside = dx * dx + dy * dy < 150 + rng.below(40);
                w.owner[(y * wd + x) as usize] = if inside { 1 } else { 2 };
            }
        }
        precalculate_random_map_stuff(&mut w);
        w
    }

    #[test]
    fn the_edge_field_matches_the_scan_for_every_owned_pixel() {
        for (seed, hwrap, wd, ht) in [
            (3u32, false, 40, 36),
            (4, true, 40, 36),
            (8, true, 30, 50),
            (11, false, 64, 20),
        ] {
            let mut slow = blob_world(seed, hwrap, wd, ht);
            let mut fast = blob_world(seed, hwrap, wd, ht);
            let field = province_edge_field(&fast, 1);
            let bb = slow.bbox[1];
            for y in i32::from(bb[2])..=i32::from(bb[3]) {
                for x in i32::from(bb[0])..=i32::from(bb[1]) + 1 {
                    let a = get_province_edge_distance(&mut slow, 1, x, y);
                    let b = get_province_edge_distance_from(&mut fast, 1, x, y, &field);
                    assert_eq!(a, b, "seed {seed} wrap {hwrap} at {x},{y}");
                }
            }
            assert_eq!(slow.edge_dist, fast.edge_dist);
        }
    }

    fn tiny() -> World {
        let mut w = World::new(1);
        w.w = 4;
        w.h = 4;
        w.owner = vec![0; 16];
        w.heights = vec![0.0; 16];
        w.provinces = vec![Default::default(); 4];
        w
    }

    #[test]
    fn isqrt_is_floor_sqrt() {
        for n in [0, 1, 2, 3, 4, 15, 16, 17, 2499, 2500, 10_000, 99_999] {
            let want = (n as f64).sqrt().floor() as i32;
            assert_eq!(isqrt(n), want.max(0), "n={n}");
        }
    }

    #[test]
    fn adjacency_is_symmetric_and_capped() {
        let mut w = tiny();
        w.provinces = vec![Default::default(); 30];
        for b in 1..29 {
            addnbor(&mut w, 1, b + 1);
        }
        assert_eq!(w.provinces[1].nbors.len(), MAX_NBORS);
        for &b in &w.provinces[1].nbors.clone() {
            assert!(w.provinces[b as usize].nbors.contains(&1u16));
        }
        assert_eq!(w.provinces[1].border.len(), MAX_NBORS);
    }

    #[test]
    fn border_flags_are_shared_both_ways_and_clearable() {
        let mut w = tiny();
        addnbor(&mut w, 1, 2);
        set_border_flags(&mut w, 1, 2, BORDER_RIVER | BORDER_WALL);
        assert_eq!(get_border_flags(&w, 2, 1), BORDER_RIVER | BORDER_WALL);
        clear_border_flags(&mut w, 2, 1, BORDER_WALL);
        assert_eq!(get_border_flags(&w, 1, 2), BORDER_RIVER);
        removenbor(&mut w, 1, 2);
        assert_eq!(get_border_flags(&w, 1, 2), 0);
        assert_eq!(get_border_flags(&w, 2, 1), BORDER_RIVER);
    }

    #[test]
    fn checkerboard_scan_finds_the_split_neighbour() {
        let mut w = tiny();
        for y in 0..4 {
            for x in 0..4 {
                w.owner[(y * 4 + x) as usize] = if x < 2 { 1 } else { 2 };
            }
        }
        let mut counts = vec![0i32; 4];
        compute_neighbor_pixels(&w, 1, &mut counts);
        assert!(counts[2] > 0);
        assert_eq!(counts[1], 0);
    }

    #[test]
    fn bounding_boxes_cover_every_owned_pixel() {
        let mut w = tiny();
        w.owner[5] = 1;
        w.owner[(3 * 4 + 2) as usize] = 1;
        precalculate_random_map_stuff(&mut w);
        assert_eq!(w.bbox[1], [1, 2, 1, 3]);
        assert_eq!(w.edge_dist.len(), 16);
    }

    #[test]
    fn size_classes_split_on_the_class_mean() {
        let mut w = tiny();
        w.provinces = vec![Default::default(); 4];
        w.owner = vec![0; 16];
        w.owner[0] = 1;
        for i in 1..12 {
            w.owner[i] = 2;
        }
        for i in 12..16 {
            w.owner[i] = 3;
        }
        precalculate_random_map_stuff(&mut w);
        calculate_random_map_sizes(&mut w);
        assert_eq!(w.provinces[1].terrain & TERRAIN_SMALL, TERRAIN_SMALL);
        assert_eq!(w.provinces[2].terrain & TERRAIN_LARGE, TERRAIN_LARGE);
    }

    #[test]
    fn ensure_land_exits_links_the_nearest_centre() {
        let mut w = tiny();
        w.provinces = vec![Default::default(); 4];
        w.provinces[1].x = 0;
        w.provinces[1].y = 0;
        w.provinces[2].x = 10;
        w.provinces[2].y = 0;
        w.provinces[3].x = 3;
        w.provinces[3].y = 0;
        ensure_land_exits(&mut w);
        assert!(w.provinces[1].nbors.contains(&3u16));
    }
}
