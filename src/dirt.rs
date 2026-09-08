use crate::render::{cold_scale, wrap_clamp, Plane, Rect, SEA_LEVEL};
use crate::terrain::SEA;
use dom6_mapgen::rng::{CrtRng, NOISE_LEN};
use std::sync::OnceLock;

pub const DIRT_COUNT: i32 = 100;
pub const DIRT_COLOR: i32 = 5;
pub const DIRT_SIZE: i32 = 100;
pub const DIRT_NOISE: i32 = 15;

const NOISE_SEED: u32 = 0x0d6d_0001;

fn noise() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut r = CrtRng::seeded(NOISE_SEED);
        (0..NOISE_LEN).map(|_| r.float()).collect()
    })
}

#[inline]
fn step(c: usize) -> usize {
    if c + 1 < NOISE_LEN {
        c + 1
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Disk {
    pub x: i32,
    pub y: i32,
    pub r: i32,
    pub rgb: [i32; 3],
    pub cursor: usize,
    pub winter: bool,
}

impl Disk {
    pub fn rect(&self) -> Rect {
        Rect {
            x0: self.x - self.r,
            y0: self.y - self.r,
            x1: self.x + self.r,
            y1: self.y + self.r,
        }
    }
}

pub fn disk_count(p: &Plane) -> i32 {
    if p.scale <= 0.0 || DIRT_COUNT <= 0 || DIRT_SIZE <= 0 {
        return 0;
    }
    let cells = p.h.saturating_mul(p.w).saturating_mul(DIRT_COUNT);
    ((0.02 / (p.scale * p.scale)) * cells as f32) as i32
}

pub fn disks(p: &Plane, season: bool) -> Vec<Disk> {
    let count = disk_count(p);
    if count <= 0 {
        return Vec::new();
    }
    let noise = noise();
    let nflags = p.flags.len();
    let mut cursor = 0usize;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let cx = step(cursor);
        let cy = step(cx);
        cursor = cy;
        let x = (p.w as f32 * noise[cx]) as i32;
        let y = (p.h as f32 * noise[cy]) as i32;
        if x < 0 || x >= p.w || y < 0 || y >= p.h {
            continue;
        }
        let prov = p.owners[(y * p.w + x) as usize];
        if prov <= 0 || prov as usize >= nflags {
            continue;
        }
        let flags = p.flags[prov as usize];
        if flags & SEA != 0 {
            continue;
        }
        let winter = cold_scale(flags, season) >= 1;
        let cr = step(cursor);
        cursor = cr;
        let r = (p.scale * 0.5 * noise[cr] * DIRT_SIZE as f32 * 0.01) as i32;
        let ir = step(cursor);
        let ig = step(ir);
        let ib = step(ig);
        let ik = step(ib);
        cursor = ik;
        if r <= 0 {
            continue;
        }
        let k = noise[ik] * 0.2 + 0.8;
        let chan = |i: usize, m: f32, b: f32| {
            let base = f64::from((DIRT_COLOR * 2) as f32 * noise[i]) as i32;
            let t = (base - DIRT_COLOR) as f32 * m + b;
            ((t as i32) as f32 * k) as i32
        };
        out.push(Disk {
            x,
            y,
            r,
            rgb: [
                chan(ir, 1.56, 156.0).clamp(0, 255),
                chan(ig, 1.35, 135.0).clamp(0, 255),
                chan(ib, 1.06, 106.0).clamp(0, 255),
            ],
            cursor,
            winter,
        });
    }
    out
}

#[inline]
fn div255(x: i32) -> i32 {
    (x * 32897) >> 23
}

#[inline]
fn blend(px: &mut [u8], rgb: [i32; 3], a: i32) {
    if a <= 0 || px[3] == 0 {
        return;
    }
    let a = a.min(255);
    let inv = 255 - a;
    let r = div255(px[0] as i32 * inv + rgb[0] * a);
    let g = div255(px[1] as i32 * inv + rgb[1] * a);
    let b = div255(px[2] as i32 * inv + rgb[2] * a);
    if r == 255 && g == 255 && b == 255 {
        px[0] = 254;
        px[1] = 254;
        px[2] = 254;
    } else {
        px[0] = r as u8;
        px[1] = g as u8;
        px[2] = b as u8;
    }
}

fn row_alpha() -> &'static [u8] {
    static A: OnceLock<Vec<u8>> = OnceLock::new();
    A.get_or_init(|| {
        let strength = DIRT_NOISE as f32 + 0.9;
        noise().iter().map(|n| (n * strength) as u8).collect()
    })
}

fn darken() -> &'static [[u8; 256]] {
    static D: OnceLock<Vec<[u8; 256]>> = OnceLock::new();
    D.get_or_init(|| {
        (0..=DIRT_NOISE.max(0) as usize + 1)
            .map(|a| {
                let inv = 255 - a as i32;
                let mut row = [0u8; 256];
                for (v, slot) in row.iter_mut().enumerate() {
                    *slot = div255(v as i32 * inv) as u8;
                }
                row
            })
            .collect()
    })
}

pub fn dirt_band(p: &Plane, work: &[f32], disks: &[Disk], rect: Rect, out: &mut [u8], row0: i32) {
    let noise = noise();
    let w = p.w;
    let h = p.h;
    let (mx, my) = (if p.hwrap { w } else { 0 }, if p.vwrap { h } else { 0 });
    let mut falloff: Vec<f32> = Vec::new();
    for d in disks {
        let b = d.rect();
        let reach = Rect {
            x0: b.x0 - mx,
            y0: b.y0 - my,
            x1: b.x1 + mx,
            y1: b.y1 + my,
        };
        if !reach.intersects(rect) {
            continue;
        }
        let span = d.r * 2 + 1;
        let rr = d.r * d.r;
        let scale = if d.winter { 25.0 } else { 50.0 };
        falloff.clear();
        falloff.extend((0..rr).map(|d2| (d.r as f32 - (d2 as f32).sqrt()) / d.r as f32 * scale));
        for y in b.y0..=b.y1 {
            let dy = y - d.y;
            let yy = wrap_clamp(y, h, p.vwrap);
            if yy < rect.y0 || yy > rect.y1 {
                continue;
            }
            let row = (dy + d.r) * span + d.cursor as i32 + 1 + d.r;
            let dy2 = dy * dy;
            for x in b.x0..=b.x1 {
                let dx = x - d.x;
                let d2 = dx * dx + dy2;
                if d2 >= rr {
                    continue;
                }
                let xx = wrap_clamp(x, w, p.hwrap);
                if xx < rect.x0 || xx > rect.x1 {
                    continue;
                }
                if work[(yy * w + xx) as usize] < SEA_LEVEL {
                    continue;
                }
                let mut i = (row + dx) as usize;
                if i >= NOISE_LEN {
                    i -= NOISE_LEN;
                }
                let a = (falloff[d2 as usize] * (noise[i] + 1.0)) as i32;
                let o = (((yy - row0) * w + xx) * 4) as usize;
                blend(&mut out[o..o + 4], d.rgb, a);
            }
        }
    }
    if DIRT_NOISE <= 0 {
        return;
    }
    let alpha = row_alpha();
    let dark = darken();
    for y in rect.y0..=rect.y1 {
        let off = row_noise_offset(y);
        let base = ((y - row0) * w * 4) as usize;
        for x in rect.x0..=rect.x1 {
            let mut i = off + x as usize;
            if i >= NOISE_LEN {
                i -= NOISE_LEN;
            }
            let a = alpha[i] as usize;
            let o = base + (x * 4) as usize;
            if a == 0 || out[o + 3] == 0 {
                continue;
            }
            let t = &dark[a];
            out[o] = t[out[o] as usize];
            out[o + 1] = t[out[o + 1] as usize];
            out[o + 2] = t[out[o + 2] as usize];
        }
    }
}

fn row_noise_offset(y: i32) -> usize {
    let mut r = CrtRng::seeded(
        (y as u32)
            .wrapping_mul(2_654_435_761)
            .wrapping_add(NOISE_SEED),
    );
    r.rand();
    r.rand() as usize % NOISE_LEN
}
