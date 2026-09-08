use rayon::prelude::*;

use crate::options::{Blueprint, Options};
use crate::rng::{CrtRng, PoolRng};
use crate::stage::{Control, Sink, Stage};
use crate::world::World;

pub const BASE_HEIGHT: f32 = 150.0;
pub const BLOB_AMP: f32 = 0.2;
pub const AUTO_WIDTH: i32 = 0x800;
pub const AUTO_HEIGHT: i32 = 0x600;
pub const MIN_AXIS: i32 = 50;
pub const WRAP_GRID: i32 = 0x200;

pub const FLAG_COAST_FADE: u32 = 1;
pub const FLAG_BOARD_VSEAM: u32 = 2;
pub const FLAG_BOARD_HSEAM: u32 = 4;
pub const FLAG_BONUS_HILLS: u32 = 8;

pub const SEAM_FRACTION: f32 = 0.25;
pub const SEAM_FRACTION_BLUEPRINT: f32 = 0.05;
pub const SEA_STEP: f32 = 2.0;
pub const SEA_CEILING: f32 = 100000.0;
pub const MOUNT_START: f32 = 450.0;
pub const MOUNT_STEP: f32 = -2.5;
pub const BONUS_HILL_AMP: f32 = 0.75;
pub const DEFAULT_JITTER: f32 = 0.3;

const PAR_MAX_DISCS: usize = 1 << 18;
const PAR_MIN_ITEMS: usize = 1 << 14;

fn par_chunk_len(n: usize) -> usize {
    let threads = rayon::current_num_threads().max(1);
    n.div_ceil(threads * 4).max(1)
}

fn clamp_index(v: i32, dim: i32) -> i32 {
    if v < 0 {
        0
    } else if dim - 1 <= v {
        dim - 1
    } else {
        v
    }
}

#[derive(Clone, Debug, Default)]
pub struct Randboard {
    pub w: i32,
    pub h: i32,
    pub oob: f32,
    pub cells: Vec<f32>,
}

impl Randboard {
    fn offset(&self, x: i32, y: i32) -> usize {
        (self.w * y + x) as usize
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    pub fn get(&self, x: i32, y: i32) -> f32 {
        if self.inside(x, y) {
            self.cells[self.offset(x, y)]
        } else {
            self.oob
        }
    }

    pub fn put(&mut self, x: i32, y: i32, v: f32) {
        if self.inside(x, y) {
            let i = self.offset(x, y);
            self.cells[i] = v;
        }
    }

    fn add(&mut self, x: i32, y: i32, v: f32) {
        let i = self.offset(x, y);
        self.cells[i] += v;
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClassificationMask {
    pub w: i32,
    pub h: i32,
    pub cells: Vec<i8>,
    pub sea_frac: f32,
}

impl ClassificationMask {
    pub fn raw(&self, board_w: i32, board_h: i32, x: i32, y: i32) -> i32 {
        if self.w < 1 || self.h < 1 {
            return 0;
        }
        let bx = clamp_index((self.w * x) / board_w, self.w);
        let by = clamp_index((self.h * y) / board_h, self.h);
        i32::from(self.cells[(self.w * by + bx) as usize])
    }

    pub fn unit(&self, board_w: i32, board_h: i32, x: i32, y: i32) -> i32 {
        let v = self.raw(board_w, board_h, x, y);
        if v > -2 {
            if v < 1 {
                v
            } else {
                1
            }
        } else {
            -1
        }
    }

    pub fn signum2(&self, board_w: i32, board_h: i32, x: i32, y: i32) -> i32 {
        let v = self.raw(board_w, board_h, x, y) * 2;
        if v < -1 {
            -1
        } else if v < 1 {
            v
        } else {
            1
        }
    }
}

pub fn bottom_up_rgba(bp: &Blueprint) -> Blueprint {
    let row = (bp.w * 4).max(0) as usize;
    let mut px = Vec::with_capacity(bp.bgra.len());
    for y in (0..bp.h.max(0) as usize).rev() {
        px.extend_from_slice(&bp.bgra[y * row..(y + 1) * row]);
    }
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    Blueprint {
        w: bp.w,
        h: bp.h,
        bgra: px,
    }
}

pub fn build_randboard_rgb_classification_mask(rgba: &[u8], w: i32, h: i32) -> ClassificationMask {
    let mut cells = vec![0i8; (w * h).max(0) as usize];
    let mut sea = 0i32;
    let mut land = 0i32;
    for y in 0..h {
        for x in 0..w {
            let p = ((y * w + x) * 4) as usize;
            let r = i32::from(rgba[p]);
            let g = i32::from(rgba[p + 1]);
            let b = i32::from(rgba[p + 2]);
            let m = (g - r).abs().max((g - b).abs());
            let code: i32 = if m < 0x10 {
                if g > 0x3f {
                    2
                } else {
                    -2
                }
            } else if g <= b {
                -1
            } else {
                1
            };
            if code < 1 {
                sea += 1;
            } else {
                land += 1;
            }
            cells[(y * w + x) as usize] = code as i8;
        }
    }
    let sea_frac = if sea + land > 0 {
        sea as f32 / (sea + land) as f32
    } else {
        0.0
    };
    ClassificationMask {
        w,
        h,
        cells,
        sea_frac,
    }
}

pub fn zoom_up_32c(src: &[u8], sw: i32, sh: i32, crt: &mut CrtRng) -> Vec<u8> {
    let dw = sw * 2;
    let dh = sh * 2;
    let mut dst = vec![0u8; (dw * dh * 4) as usize];
    for y in 0..dh {
        for x in 0..dw {
            let s = (((x / 2) + (y / 2) * sw) * 4) as usize;
            let d = ((y * dw + x) * 4) as usize;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    let base = dst.clone();
    for y in 0..dh {
        for x in 0..dw {
            if (x | y) & 1 == 0 {
                continue;
            }
            let sx = clamp_index(crt.below(3) - 1 + x, dw);
            let sy = clamp_index(crt.below(3) - 1 + y, dh);
            let s = ((sy * dw + sx) * 4) as usize;
            let d = ((y * dw + x) * 4) as usize;
            dst[d..d + 4].copy_from_slice(&base[s..s + 4]);
        }
    }
    dst
}

pub fn upscale_blueprint(bp: &Blueprint, req_w: i32, req_h: i32, crt: &mut CrtRng) -> Blueprint {
    if bp.w >= req_w / 2 && bp.h >= req_h / 2 {
        return bp.clone();
    }
    let mut w = bp.w * 2;
    let mut h = bp.h * 2;
    let mut px = zoom_up_32c(&bp.bgra, bp.w, bp.h, crt);
    if w < req_w / 2 || h < req_h / 2 {
        px = zoom_up_32c(&px, w, h, crt);
        w *= 2;
        h *= 2;
    }
    Blueprint { w, h, bgra: px }
}

pub fn round_map_dimensions(
    width: i32,
    height: i32,
    hwrap: bool,
    vwrap: bool,
) -> Result<(i32, i32), &'static str> {
    let (rw, rh) = if width < 1 || height < 1 {
        (AUTO_WIDTH, AUTO_HEIGHT)
    } else {
        (width, height)
    };
    let snap = |v: i32| {
        let t = ((v & !1) + 0x100) & !(WRAP_GRID - 1);
        if t > WRAP_GRID {
            t
        } else {
            WRAP_GRID
        }
    };
    let w = if hwrap { snap(rw) } else { rw & !1 };
    let h = if vwrap { snap(rh) } else { rh & !1 };
    if w < MIN_AXIS || h < MIN_AXIS {
        return Err("maparea too small");
    }
    Ok((w, h))
}

pub fn init_randboard(w: i32, h: i32, base: f32, jit: f32, pool: &mut PoolRng) -> Randboard {
    let mut board = Randboard {
        w,
        h,
        oob: base,
        cells: vec![0.0; (w * h) as usize],
    };
    if base != -1.0 {
        let base_rng = *pool;
        let n = board.cells.len();
        let chunk = par_chunk_len(n);
        board
            .cells
            .par_chunks_mut(chunk)
            .enumerate()
            .for_each(|(ci, out)| {
                let mut p = base_rng.advanced((ci * chunk) as u64);
                for c in out.iter_mut() {
                    *c = (p.rndfloat() - 0.5) * jit + base;
                }
            });
        *pool = base_rng.advanced(n as u64);
    }
    board
}

fn stamp_disc(board: &mut Randboard, cx: i32, cy: i32, r: i32, a: f32) {
    if r == 1 {
        if cx >= 0 && cx < board.w && cy >= 0 && cy < board.h {
            let i = (board.w * cy + cx) as usize;
            board.cells[i] += a;
        }
        return;
    }
    let r2 = (r * r) as f32;
    let y0 = clamp_index(cy - r, board.h);
    let y1 = clamp_index(cy + r, board.h);
    let x0 = clamp_index(cx - r, board.w);
    let x1 = clamp_index(cx + r, board.w);
    if y0 > y1 {
        return;
    }
    let mut dy = cy - y0;
    for y in y0..=y1 {
        if x0 <= x1 {
            let mut dx = cx - x0;
            for x in x0..=x1 {
                let d2 = (dx * dx + dy * dy) as f32;
                if d2 < r2 {
                    board.add(x, y, ((r2 - d2) / r2) * a);
                }
                dx -= 1;
            }
        }
        dy -= 1;
    }
}

fn stamp_discs(board: &mut Randboard, discs: &[(i32, i32, i32, f32)]) {
    let threads = rayon::current_num_threads();
    if threads < 2 || board.h < 2 * threads as i32 {
        for &(cx, cy, r, a) in discs {
            stamp_disc(board, cx, cy, r, a);
        }
        return;
    }
    let (bw, bh) = (board.w, board.h);
    let w = bw as usize;
    let band = (bh as usize).div_ceil(threads).max(1);
    let nbands = (bh as usize).div_ceil(band);
    let band_span = |d: &(i32, i32, i32, f32)| -> Option<(usize, usize)> {
        let (cx, cy, r, _) = *d;
        let (y0, y1) = if r == 1 {
            if cx < 0 || cx >= bw || cy < 0 || cy >= bh {
                return None;
            }
            (cy, cy)
        } else {
            let y0 = clamp_index(cy - r, bh);
            let y1 = clamp_index(cy + r, bh);
            if y0 > y1 {
                return None;
            }
            let x0 = clamp_index(cx - r, bw);
            let x1 = clamp_index(cx + r, bw);
            if x0 > x1 {
                return None;
            }
            (y0, y1)
        };
        Some((y0 as usize / band, (y1 as usize / band).min(nbands - 1)))
    };
    let mut buckets: Vec<Vec<u32>> = (0..nbands).map(|_| Vec::new()).collect();
    for (i, d) in discs.iter().enumerate() {
        if let Some((b0, b1)) = band_span(d) {
            for b in buckets.iter_mut().take(b1 + 1).skip(b0) {
                b.push(i as u32);
            }
        }
    }
    board
        .cells
        .par_chunks_mut(band * w)
        .zip(buckets.par_iter())
        .enumerate()
        .for_each(|(bi, (chunk, mine))| {
            let y_lo = (bi * band) as i32;
            let y_hi = y_lo + (chunk.len() / w) as i32 - 1;
            for &di in mine {
                let (cx, cy, r, a) = discs[di as usize];
                if r == 1 {
                    chunk[(cy - y_lo) as usize * w + cx as usize] += a;
                    continue;
                }
                let r2 = (r * r) as f32;
                let y0 = clamp_index(cy - r, bh);
                let y1 = clamp_index(cy + r, bh);
                let x0 = clamp_index(cx - r, bw);
                let x1 = clamp_index(cx + r, bw);
                let ys = y0.max(y_lo);
                let ye = y1.min(y_hi);
                let mut y = ys;
                while y <= ye {
                    let dy = cy - y;
                    let row = &mut chunk[(y - y_lo) as usize * w..][..w];
                    let mut dx = cx - x0;
                    for x in x0..=x1 {
                        let d2 = (dx * dx + dy * dy) as f32;
                        if d2 < r2 {
                            row[x as usize] += ((r2 - d2) / r2) * a;
                        }
                        dx -= 1;
                    }
                    y += 1;
                }
            }
        });
}

pub fn raise_randboard_region(board: &mut Randboard, cx: i32, cy: i32, r: i32, a: f32) {
    stamp_disc(board, cx, cy, r, a);
}

pub fn smooth_randboard(board: &mut Randboard, amp: f32, pool: &mut PoolRng) {
    let mut r = board.w.max(board.h) / 2;
    let mut n: u32 = 1;
    let mut discs: Vec<(i32, i32, i32, f32)> = Vec::new();
    loop {
        if n as i32 > 0 {
            let scaled = r as f32 * amp;
            discs.clear();
            discs.reserve(n as usize);
            let (bw, bh) = (board.w as u32, board.h as u32);
            let base_rng = *pool;
            if n as usize >= PAR_MIN_ITEMS && rayon::current_num_threads() > 1 {
                (0..n as usize)
                    .into_par_iter()
                    .map(|i| {
                        let mut p = base_rng.advanced(3 * i as u64);
                        let x = p.rnd(bw) as i32;
                        let y = p.rnd(bh) as i32;
                        let u = p.rndfloat();
                        let mut a = (u - 0.5) * scaled;
                        a += a;
                        (x, y, r, a)
                    })
                    .collect_into_vec(&mut discs);
                *pool = base_rng.advanced(3 * n as u64);
            } else {
                for _ in 0..n {
                    let x = pool.rnd(bw) as i32;
                    let y = pool.rnd(bh) as i32;
                    let u = pool.rndfloat();
                    let mut a = (u - 0.5) * scaled;
                    a += a;
                    discs.push((x, y, r, a));
                }
            }
            stamp_discs(board, &discs);
        }
        n = n.wrapping_mul(4);
        r /= 2;
        if r <= 0 {
            break;
        }
    }
}

struct MaskLut {
    w: i32,
    off_x: i32,
    off_y: i32,
    bx: Vec<i32>,
    by: Vec<i32>,
}

impl MaskLut {
    fn build(mask: &ClassificationMask, bw: i32, bh: i32, pad_x: i32, pad_y: i32) -> Self {
        let off_x = pad_x;
        let off_y = pad_y;
        let bx = (-off_x..bw + off_x)
            .map(|x| clamp_index((mask.w * x) / bw, mask.w))
            .collect();
        let by = (-off_y..bh + off_y)
            .map(|y| clamp_index((mask.h * y) / bh, mask.h))
            .collect();
        MaskLut {
            w: mask.w,
            off_x,
            off_y,
            bx,
            by,
        }
    }

    fn raw(&self, mask: &ClassificationMask, x: i32, y: i32) -> i32 {
        if self.w < 1 || mask.h < 1 {
            return 0;
        }
        let bx = self.bx[(x + self.off_x) as usize];
        let by = self.by[(y + self.off_y) as usize];
        i32::from(mask.cells[(self.w * by + bx) as usize])
    }

    fn unit(&self, mask: &ClassificationMask, x: i32, y: i32) -> i32 {
        let v = self.raw(mask, x, y);
        if v > -2 {
            if v < 1 {
                v
            } else {
                1
            }
        } else {
            -1
        }
    }

    fn signum2(&self, mask: &ClassificationMask, x: i32, y: i32) -> i32 {
        let v = self.raw(mask, x, y) * 2;
        if v < -1 {
            -1
        } else if v < 1 {
            v
        } else {
            1
        }
    }
}

pub fn generate_multiscale_randboard_height(
    board: &mut Randboard,
    mask: &ClassificationMask,
    amp: f32,
    level: i32,
    pool: &mut PoolRng,
) {
    let (bw, bh) = (board.w, board.h);
    {
        let lut = MaskLut::build(mask, bw, bh, 1, 1);
        let w = bw as usize;
        board
            .cells
            .par_chunks_mut(w)
            .enumerate()
            .for_each(|(row, out)| {
                let y = row as i32;
                for x in 0..bw {
                    let v = lut.unit(mask, x, y);
                    out[x as usize] = (v * level * 10) as f32;
                }
            });
    }
    if level >= 9 {
        return;
    }
    let mut n: u32 = 2;
    let mut discs: Vec<(i32, i32, i32, f32)> = Vec::new();
    let mut r = bw.max(bh) / 2;
    let skip = if level < 2 { level } else { 2 };
    for _ in 0..skip.max(0) {
        n = n.wrapping_mul(4);
        r /= 2;
    }
    loop {
        let half = r / 2;
        let scaled = r as f32 * amp;
        let third = r / 3;
        if n as i32 > 0 {
            discs.clear();
            discs.reserve(n as usize);
            let lut = MaskLut::build(mask, bw, bh, half + third + 1, half + third + 1);
            let (nx, ny) = ((half * 2 + bw) as u32, (half * 2 + bh) as u32);
            let base_rng = *pool;
            let make = |i: usize| {
                let mut p = base_rng.advanced(3 * i as u64);
                let x = p.rnd(nx) as i32 - half;
                let y = p.rnd(ny) as i32 - half;
                let s = lut.signum2(mask, x, y)
                    + lut.unit(mask, x - third, y)
                    + lut.unit(mask, x + third, y)
                    + lut.unit(mask, x, y - third)
                    + lut.unit(mask, x, y + third);
                let u = p.rndfloat();
                let a = if s == 0 {
                    (u - 0.5) * scaled * 0.16666667
                } else {
                    s as f32 * scaled * u * 0.33333334
                };
                (x, y, r, a)
            };
            if n as usize >= PAR_MIN_ITEMS && rayon::current_num_threads() > 1 {
                (0..n as usize)
                    .into_par_iter()
                    .map(make)
                    .collect_into_vec(&mut discs);
            } else {
                discs.extend((0..n as usize).map(make));
            }
            *pool = base_rng.advanced(3 * n as u64);
            stamp_discs(board, &discs);
        }
        n = n.wrapping_mul(4);
        r = half;
        if half <= 0 {
            break;
        }
    }
}

pub fn push_multiscale_signed_height_blobs(
    discs: &mut Vec<(i32, i32, i32, f32)>,
    cx: i32,
    cy: i32,
    r0: i32,
    amp: f32,
    pool: &mut PoolRng,
) {
    let mut n: u32 = 1;
    let mut r = r0;
    while r > 0 {
        if n as i32 > 0 {
            let span = r0 - r;
            let off = -(span / 2);
            let scaled = r as f32 * amp;
            for _ in 0..n {
                let x = off + cx + pool.rnd(span as u32) as i32;
                let y = off + cy + pool.rnd(span as u32) as i32;
                let u = pool.rndfloat();
                let mut a = (u - 0.5) * scaled;
                a += a;
                discs.push((x, y, r, a));
            }
        }
        n = n.wrapping_mul(3);
        r /= 2;
    }
}

pub fn add_multiscale_signed_height_blobs(
    board: &mut Randboard,
    cx: i32,
    cy: i32,
    r0: i32,
    amp: f32,
    pool: &mut PoolRng,
) {
    let mut discs = Vec::new();
    push_multiscale_signed_height_blobs(&mut discs, cx, cy, r0, amp, pool);
    for &(x, y, r, a) in &discs {
        stamp_disc(board, x, y, r, a);
    }
}

pub fn add_multiscale_negative_height_blobs(
    board: &mut Randboard,
    cx: i32,
    cy: i32,
    r0: i32,
    amp: f32,
    pool: &mut PoolRng,
) {
    let mut n: u32 = 1;
    let mut r = r0;
    while r > 0 {
        if n as i32 > 0 {
            let span = r0 - r;
            let off = -(span / 2);
            let r2 = (r * r) as f32;
            let scaled = r as f32 * amp;
            for _ in 0..n {
                let x = off + cx + pool.rnd(span as u32) as i32;
                let y = off + cy + pool.rnd(span as u32) as i32;
                let strength = pool.rndfloat() * scaled;
                let y0 = clamp_index(y - r, board.h);
                let y1 = clamp_index(y + r, board.h);
                let x0 = clamp_index(x - r, board.w);
                let x1 = clamp_index(x + r, board.w);
                if y0 > y1 {
                    continue;
                }
                let mut dy = y - y0;
                for yy in y0..=y1 {
                    if x0 <= x1 {
                        let mut dx = x - x0;
                        for xx in x0..=x1 {
                            let d2 = (dx * dx + dy * dy) as f32;
                            if d2 < r2 {
                                board.add(xx, yy, (r2 - d2) * (-1.0 / r2) * strength);
                            }
                            dx -= 1;
                        }
                    }
                    dy -= 1;
                }
            }
        }
        n = n.wrapping_mul(3);
        r /= 2;
    }
}

pub fn maybe_bonus_hills(board: &mut Randboard, w: i32, h: i32, flags: u32, pool: &mut PoolRng) {
    let f = flags & 0xff;
    if f & 10 == 8 {
        for x in 0..w {
            if pool.rnd(100) < 5 {
                let y = pool.rnd(5) as i32;
                let m = w.min(h);
                let u = pool.rndfloat();
                let r = ((f64::from(u) * 0.03 + 0.01) * f64::from(m)) as i32;
                add_multiscale_negative_height_blobs(board, x, y, r.max(1), BONUS_HILL_AMP, pool);
                let y2 = pool.rnd(5) as i32;
                let u = pool.rndfloat();
                let r = ((f64::from(u) * 0.03 + 0.01) * f64::from(m)) as i32;
                add_multiscale_negative_height_blobs(
                    board,
                    x,
                    (h - y2) - 1,
                    r.max(1),
                    BONUS_HILL_AMP,
                    pool,
                );
            }
        }
    }
    if f & 0xc == 8 {
        for y in 0..h {
            if pool.rnd(100) < 5 {
                let x = pool.rnd(5) as i32;
                let m = w.min(h);
                let u = pool.rndfloat();
                let r = ((f64::from(u) * 0.03 + 0.01) * f64::from(m)) as i32;
                add_multiscale_negative_height_blobs(board, x, y, r.max(1), BONUS_HILL_AMP, pool);
                let x2 = pool.rnd(5) as i32;
                let u = pool.rndfloat();
                let r = ((f64::from(u) * 0.03 + 0.01) * f64::from(m)) as i32;
                add_multiscale_negative_height_blobs(
                    board,
                    (w - x2) - 1,
                    y,
                    r.max(1),
                    BONUS_HILL_AMP,
                    pool,
                );
            }
        }
    }
}

fn scatter_blobs(board: &mut Randboard, w: i32, h: i32, nblobs: u32, amp: f32, pool: &mut PoolRng) {
    if nblobs as i32 <= 0 {
        return;
    }
    let m = w.min(h);
    let mut discs = Vec::new();
    for _ in 0..nblobs {
        let rx = pool.rnd((f64::from(w) * 0.8) as i32 as u32);
        let ry = pool.rnd((f64::from(h) * 0.8) as i32 as u32);
        let u = pool.rndfloat();
        let cx = (f64::from(rx) + f64::from(w) * 0.1) as i32;
        let cy = (f64::from(ry) + f64::from(h) * 0.1) as i32;
        let r0 = ((f64::from(u) * 0.09 + 0.01) * f64::from(m)) as i32;
        push_multiscale_signed_height_blobs(&mut discs, cx, cy, r0, amp, pool);
    }
    for chunk in discs.chunks(PAR_MAX_DISCS) {
        stamp_discs(board, chunk);
    }
}

fn copy_out(board: &Randboard, w: i32, h: i32) -> Vec<f32> {
    if board.w == w && board.h == h {
        return board.cells.clone();
    }
    let mut out = vec![0.0f32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            out[(y * w + x) as usize] = board.get(x, y);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn create_random_heightmap(
    w: i32,
    h: i32,
    base: f32,
    jitter: f32,
    nblobs: u32,
    blob_amp: f32,
    flags: u32,
    pool: &mut PoolRng,
) -> Vec<f32> {
    let jit = (f64::from(jitter) * 0.2) as f32;
    let mut board = init_randboard(w, h, base, jit, pool);
    smooth_randboard(&mut board, jitter, pool);
    scatter_blobs(&mut board, w, h, nblobs, blob_amp, pool);
    maybe_bonus_hills(&mut board, w, h, flags, pool);
    copy_out(&board, w, h)
}

#[allow(clippy::too_many_arguments)]
pub fn create_random_heightmap2(
    w: i32,
    h: i32,
    base: f32,
    jitter: f32,
    nblobs: u32,
    blob_amp: f32,
    flags: u32,
    blueprint: &Blueprint,
    level: i32,
    pool: &mut PoolRng,
) -> (Vec<f32>, ClassificationMask) {
    let jit = (f64::from(jitter) * 0.2) as f32;
    let mut board = init_randboard(w, h, base, jit, pool);
    let mask = build_randboard_rgb_classification_mask(&blueprint.bgra, blueprint.w, blueprint.h);
    generate_multiscale_randboard_height(&mut board, &mask, jitter, level, pool);
    scatter_blobs(&mut board, w, h, nblobs, blob_amp, pool);
    maybe_bonus_hills(&mut board, w, h, flags, pool);
    (copy_out(&board, w, h), mask)
}

pub fn blend_heightmap_horizontal_seam(heights: &mut [f32], w: i32, h: i32, frac: f32) {
    if frac <= 0.0 {
        return;
    }
    let n = (w as f32 * frac) as i32;
    if n <= 0 {
        return;
    }
    if 2 * n <= w {
        heights
            .par_chunks_mut(w as usize)
            .take(h as usize)
            .for_each(|row| {
                for x in 0..n {
                    let weight = x as f32 * (1.0 / n as f32);
                    let col = clamp_index(x, w) as usize;
                    let mirror = clamp_index((w - x) - 1, w) as usize;
                    row[col] = (1.0 - weight) * row[mirror] + weight * row[col];
                }
            });
        return;
    }
    for x in 0..n {
        let weight = x as f32 * (1.0 / n as f32);
        for y in 0..h {
            let row = clamp_index(y, h);
            let col = clamp_index(x, w);
            let mirror = clamp_index((w - x) - 1, w);
            let here = (row * w + col) as usize;
            let there = (row * w + mirror) as usize;
            heights[here] = (1.0 - weight) * heights[there] + weight * heights[here];
        }
    }
}

pub fn blend_heightmap_vertical_seam(heights: &mut [f32], w: i32, h: i32, frac: f32) {
    if frac <= 0.0 {
        return;
    }
    let n = (h as f32 * frac) as i32;
    if n <= 0 {
        return;
    }
    let inv = 1.0 / n as f32;
    for x in 0..w {
        let col = clamp_index(x, w);
        for y in 0..n {
            let weight = y as f32 * inv;
            let row = clamp_index(y, h);
            let mirror = clamp_index((h - y) - 1, h);
            let here = (row * w + col) as usize;
            let there = (mirror * w + col) as usize;
            heights[here] = (1.0 - weight) * heights[there] + weight * heights[here];
        }
    }
}

pub fn height_sample_grid(heights: &[f32], w: i32, h: i32) -> Vec<f32> {
    let mut out = Vec::with_capacity(((h as usize).div_ceil(4)) * ((w as usize).div_ceil(2)));
    let mut y = 0;
    while y < h {
        let row = (y * w) as usize;
        let mut x = 0;
        while x < w {
            out.push(heights[row + x as usize]);
            x += 2;
        }
        y += 4;
    }
    out
}

pub fn fraction_below_threshold(samples: &[f32], threshold: f32) -> f32 {
    let below: usize = samples
        .par_chunks(1 << 15)
        .map(|part| part.iter().filter(|v| !(threshold <= **v)).count())
        .sum();
    below as f32 / samples.len() as f32
}

pub fn sample_height_fraction_below_threshold(
    heights: &[f32],
    w: i32,
    h: i32,
    threshold: f32,
) -> f32 {
    let (below, total) = (0..h)
        .into_par_iter()
        .step_by(4)
        .map(|y| {
            let row = clamp_index(y, h);
            let mut below = 0i32;
            let mut total = 0i32;
            let mut x = 0;
            while x < w {
                total += 1;
                let col = clamp_index(x, w);
                if !(threshold <= heights[(row * w + col) as usize]) {
                    below += 1;
                }
                x += 2;
            }
            (below, total)
        })
        .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    below as f32 / total as f32
}

pub fn scan_min_height(heights: &[f32], w: i32, h: i32) -> f32 {
    let mut lowest = 9999999.0f32;
    let mut y = 0;
    while y < h {
        let row = clamp_index(y, h);
        let mut x = 0;
        while x < w {
            let col = clamp_index(x, w);
            let v = heights[(row * w + col) as usize];
            if v < lowest {
                lowest = v;
            }
            x += 2;
        }
        y += 4;
    }
    lowest
}

pub struct HeightParams {
    pub jitter: f32,
    pub nblobs: u32,
    pub blob_amp: f32,
    pub base: f32,
    pub sea_target: f32,
    pub mount_target: f32,
    pub flags: u32,
    pub level: i32,
}

impl HeightParams {
    pub fn from_options(opts: &Options) -> Self {
        HeightParams {
            jitter: opts.rugedness_f32.unwrap_or(DEFAULT_JITTER),
            nblobs: opts.hills.max(0) as u32,
            blob_amp: BLOB_AMP,
            base: BASE_HEIGHT,
            sea_target: (f64::from(opts.sea_part) * 0.01) as f32,
            mount_target: (f64::from(opts.mount_part) * 0.01) as f32,
            flags: if opts.cave_world { FLAG_BONUS_HILLS } else { 0 },
            level: opts.blue_acc,
        }
    }
}

fn resolve_sea_and_mountain_levels(world: &mut World, target: f32, mount_target: f32) {
    let (w, h) = (world.w, world.h);
    let samples = height_sample_grid(&world.heights, w, h);
    let mut level = scan_min_height(&world.heights, w, h);
    world.deep_level = 0.0;
    world.deep_frac = 0.0;
    let part = target * 0.4;
    let cap = if target >= 1.0 { 1.0 } else { target };
    let mut frac = 0.0f32;
    if cap > 0.0 {
        loop {
            frac = fraction_below_threshold(&samples, level);
            if frac < part {
                world.deep_level = level;
                world.deep_frac = frac;
            }
            if !(level <= SEA_CEILING) {
                break;
            }
            level += SEA_STEP;
            if !(frac < cap) {
                break;
            }
        }
    }
    if cap < frac {
        level -= SEA_STEP;
    }
    world.sea_level = level;
    world.real_water_part = fraction_below_threshold(&samples, level);

    let mut mount = MOUNT_START;
    let mut mfrac = fraction_below_threshold(&samples, mount);
    while 1.0 - f64::from(mfrac) < (1.0 - f64::from(cap)) * f64::from(mount_target) {
        mount += MOUNT_STEP;
        mfrac = fraction_below_threshold(&samples, mount);
    }
    world.mount_level = mount;
    world.mount_frac = 1.0 - fraction_below_threshold(&samples, mount);
}

pub fn build_height_field(world: &mut World, opts: &Options, sink: &mut dyn Sink) -> Control {
    let p = HeightParams::from_options(opts);
    let req_w = if opts.width < 1 || opts.height < 1 {
        AUTO_WIDTH
    } else {
        opts.width
    };
    let req_h = if opts.width < 1 || opts.height < 1 {
        AUTO_HEIGHT
    } else {
        opts.height
    };
    let blueprint = opts
        .blueprint
        .as_ref()
        .map(bottom_up_rgba)
        .map(|bp| upscale_blueprint(&bp, req_w, req_h, &mut world.crt));

    let (w, h) = match round_map_dimensions(opts.width, opts.height, opts.hwrap, opts.vwrap) {
        Ok(dims) => dims,
        Err(_) => return Control::Cancel,
    };
    world.w = w;
    world.h = h;
    world.hwrap = opts.hwrap;
    world.vwrap = opts.vwrap;
    world.cave_world = opts.cave_world;

    let usable = blueprint.as_ref().filter(|bp| bp.w >= 1 && bp.h >= 1);
    let mut sea_target = p.sea_target;
    match usable {
        Some(bp) => {
            let (heights, mask) = create_random_heightmap2(
                w,
                h,
                p.base,
                p.jitter,
                p.nblobs,
                p.blob_amp,
                p.flags,
                bp,
                p.level,
                &mut world.pool,
            );
            world.heights = heights;
            world.blueprint_sea_frac = mask.sea_frac;
            world.blueprint_mask_w = mask.w;
            world.blueprint_mask_h = mask.h;
            world.blueprint_mask = mask.cells;
        }
        None => {
            world.heights = create_random_heightmap(
                w,
                h,
                p.base,
                p.jitter,
                p.nblobs,
                p.blob_amp,
                p.flags,
                &mut world.pool,
            );
            world.blueprint_mask = Vec::new();
            world.blueprint_mask_w = 0;
            world.blueprint_mask_h = 0;
            world.blueprint_sea_frac = 0.0;
        }
    }
    if world.emit(Stage::Height, sink) == Control::Cancel {
        return Control::Cancel;
    }

    let frac = if blueprint.is_some() {
        SEAM_FRACTION_BLUEPRINT
    } else {
        SEAM_FRACTION
    };
    if opts.hwrap {
        blend_heightmap_horizontal_seam(&mut world.heights, w, h, frac);
    }
    if opts.vwrap {
        blend_heightmap_vertical_seam(&mut world.heights, w, h, frac);
    }
    if world.emit(Stage::Seams, sink) == Control::Cancel {
        return Control::Cancel;
    }

    if usable.is_some() {
        sea_target = world.blueprint_sea_frac;
        let jitter = 0.2 - p.level as f32 * 0.05;
        if jitter > 0.0 {
            let noise = world.noise.advance();
            sea_target = ((1.0 - jitter * 0.5) + jitter * noise) * world.blueprint_sea_frac;
        }
    }
    resolve_sea_and_mountain_levels(world, sea_target, p.mount_target);
    world.emit(Stage::SeaLevel, sink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_grid_reproduces_the_strided_fraction() {
        let (w, h) = (61, 43);
        let mut r = CrtRng::seeded(11);
        let heights: Vec<f32> = (0..w * h).map(|_| r.float() * 300.0).collect();
        let samples = height_sample_grid(&heights, w, h);
        for t in [0.0f32, 15.0, 75.5, 150.0, 299.0, 1000.0] {
            assert_eq!(
                fraction_below_threshold(&samples, t),
                sample_height_fraction_below_threshold(&heights, w, h, t),
                "threshold {t}"
            );
        }
    }

    #[test]
    fn scattered_blobs_stamp_the_same_in_bands_and_in_series() {
        for seed in [1u32, 2, 3] {
            let (w, h) = (96, 80);
            let mut serial = init_randboard(w, h, 5.0, 0.0, &mut PoolRng::seeded(seed));
            let mut banded = init_randboard(w, h, 5.0, 0.0, &mut PoolRng::seeded(seed));
            let mut p1 = PoolRng::seeded(seed + 10);
            let mut p2 = PoolRng::seeded(seed + 10);
            let m = w.min(h);
            for _ in 0..40 {
                let rx = p1.rnd((f64::from(w) * 0.8) as i32 as u32);
                let ry = p1.rnd((f64::from(h) * 0.8) as i32 as u32);
                let u = p1.rndfloat();
                let cx = (f64::from(rx) + f64::from(w) * 0.1) as i32;
                let cy = (f64::from(ry) + f64::from(h) * 0.1) as i32;
                let r0 = ((f64::from(u) * 0.09 + 0.01) * f64::from(m)) as i32;
                add_multiscale_signed_height_blobs(&mut serial, cx, cy, r0, 3.0, &mut p1);
            }
            scatter_blobs(&mut banded, w, h, 40, 3.0, &mut p2);
            assert_eq!(serial.cells, banded.cells, "seed {seed}");
            assert_eq!(copy_out(&banded, w, h), banded.cells);
        }
    }
    use crate::stage::NoSink;

    fn pool() -> PoolRng {
        PoolRng::seeded(20260908)
    }

    fn draws(before: &PoolRng, after: &PoolRng) -> u64 {
        let a = before.index as i64 + 500 * (before.rotate as i64 + 30 * i64::from(before.xor_key));
        let b = after.index as i64 + 500 * (after.rotate as i64 + 30 * i64::from(after.xor_key));
        (b - a) as u64
    }

    #[test]
    fn dimension_rounding_snaps_wrapped_axes_to_the_nearest_512() {
        assert_eq!(round_map_dimensions(-1, -1, true, false), Ok((2048, 1536)));
        assert_eq!(
            round_map_dimensions(2000, 1000, true, false),
            Ok((2048, 1000))
        );
        assert_eq!(
            round_map_dimensions(2000, 1001, false, false),
            Ok((2000, 1000))
        );
        assert_eq!(round_map_dimensions(600, 600, true, true), Ok((512, 512)));
        assert_eq!(round_map_dimensions(100, 100, true, true), Ok((512, 512)));
        assert!(round_map_dimensions(48, 600, false, false).is_err());
        assert!(round_map_dimensions(600, 49, false, false).is_err());
        assert_eq!(round_map_dimensions(600, 50, false, false), Ok((600, 50)));
    }

    #[test]
    fn init_randboard_draws_one_float_per_cell_and_skips_the_uninitialised_branch() {
        let mut p = pool();
        let before = p;
        let b = init_randboard(8, 4, BASE_HEIGHT, 6.0, &mut p);
        assert_eq!(draws(&before, &p), 32);
        assert_eq!(b.cells.len(), 32);
        assert!(b.cells.iter().all(|v| (*v - BASE_HEIGHT).abs() <= 3.0));

        let mut q = pool();
        let before = q;
        let b = init_randboard(8, 4, -1.0, 6.0, &mut q);
        assert_eq!(draws(&before, &q), 0);
        assert!(b.cells.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn smooth_randboard_spends_three_draws_per_disc_over_all_octaves() {
        let mut p = pool();
        let mut b = init_randboard(64, 32, BASE_HEIGHT, 0.0, &mut p);
        let before = p;
        smooth_randboard(&mut b, 0.3, &mut p);
        let mut discs = 0u64;
        let mut r = 32;
        let mut n = 1u64;
        loop {
            discs += n;
            n *= 4;
            r /= 2;
            if r <= 0 {
                break;
            }
        }
        assert_eq!(draws(&before, &p), discs * 3);
    }

    #[test]
    fn signed_blobs_skip_the_first_octave_position_draws_because_the_span_is_zero() {
        let mut p = pool();
        let mut b = init_randboard(64, 64, BASE_HEIGHT, 0.0, &mut p);
        let before = p;
        add_multiscale_signed_height_blobs(&mut b, 32, 32, 1, 0.2, &mut p);
        assert_eq!(draws(&before, &p), 1);

        let mut q = pool();
        let mut c = init_randboard(64, 64, BASE_HEIGHT, 0.0, &mut q);
        let before = q;
        add_multiscale_signed_height_blobs(&mut c, 32, 32, 3, 0.2, &mut q);
        assert_eq!(draws(&before, &q), 1 + 3 * 3);
    }

    #[test]
    fn a_full_blueprint_level_replaces_every_octave() {
        let mask = ClassificationMask {
            w: 2,
            h: 2,
            cells: vec![1, -1, -1, 1],
            sea_frac: 0.5,
        };
        let mut p = pool();
        let mut b = init_randboard(16, 16, BASE_HEIGHT, 0.0, &mut p);
        let before = p;
        generate_multiscale_randboard_height(&mut b, &mask, 0.3, 9, &mut p);
        assert_eq!(draws(&before, &p), 0);
        assert_eq!(b.get(0, 0), 90.0);
        assert_eq!(b.get(15, 0), -90.0);
    }

    #[test]
    fn blueprint_octaves_spend_two_draws_per_sample_and_skip_coarse_levels() {
        let mask = ClassificationMask {
            w: 2,
            h: 2,
            cells: vec![1, -1, -1, 1],
            sea_frac: 0.5,
        };
        let mut p = pool();
        let mut b = init_randboard(32, 32, BASE_HEIGHT, 0.0, &mut p);
        let before = p;
        generate_multiscale_randboard_height(&mut b, &mask, 0.3, 2, &mut p);
        let mut n = 2u64 * 4 * 4;
        let mut r = 16 / 2 / 2;
        let mut samples = 0u64;
        loop {
            samples += n;
            let half = r / 2;
            n *= 4;
            r = half;
            if half <= 0 {
                break;
            }
        }
        assert_eq!(draws(&before, &p), samples * 3);
    }

    #[test]
    fn classification_mask_reads_bgra_and_reports_the_sea_fraction() {
        let bp = Blueprint {
            w: 2,
            h: 2,
            bgra: vec![
                10, 10, 10, 255, 200, 200, 200, 255, 200, 10, 10, 255, 10, 200, 10, 255,
            ],
        };
        let e = bottom_up_rgba(&bp);
        assert_eq!(&e.bgra[0..8], &[10, 10, 200, 255, 10, 200, 10, 255]);
        let m = build_randboard_rgb_classification_mask(&e.bgra, 2, 2);
        assert_eq!(m.cells, vec![-1, 1, -2, 2]);
        assert_eq!(m.sea_frac, 0.5);
        assert_eq!(m.unit(4, 4, 0, 0), -1);
        assert_eq!(m.unit(4, 4, 2, 2), 1);
        assert_eq!(m.signum2(4, 4, 0, 2), -1);
    }

    #[test]
    fn the_horizontal_seam_makes_the_first_column_match_the_last() {
        let (w, h) = (16, 4);
        let mut heights: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let mirror: Vec<f32> = (0..h).map(|y| heights[(y * w + w - 1) as usize]).collect();
        blend_heightmap_horizontal_seam(&mut heights, w, h, 0.25);
        for y in 0..h {
            assert_eq!(heights[(y * w) as usize], mirror[y as usize]);
        }
    }

    #[test]
    fn the_vertical_seam_makes_the_first_row_match_the_last() {
        let (w, h) = (8, 16);
        let mut heights: Vec<f32> = (0..w * h).map(|i| i as f32).collect();
        let mirror: Vec<f32> = (0..w)
            .map(|x| heights[((h - 1) * w + x) as usize])
            .collect();
        blend_heightmap_vertical_seam(&mut heights, w, h, 0.25);
        for x in 0..w {
            assert_eq!(heights[x as usize], mirror[x as usize]);
        }
    }

    #[test]
    fn the_fraction_below_a_threshold_is_monotonic_and_uses_the_one_in_eight_grid() {
        let (w, h) = (16, 8);
        let heights: Vec<f32> = (0..w * h).map(|i| (i % 32) as f32).collect();
        let mut last = -1.0;
        for t in 0..34 {
            let f = sample_height_fraction_below_threshold(&heights, w, h, t as f32);
            assert!(f >= last);
            last = f;
        }
        assert_eq!(last, 1.0);
        assert_eq!(
            sample_height_fraction_below_threshold(&heights, w, h, -1.0),
            0.0
        );
    }

    #[test]
    fn the_sea_ramp_lands_on_the_two_unit_grid_above_the_lowest_sample() {
        let mut world = World::new(7);
        let opts = Options {
            width: 128,
            height: 96,
            hills: 4,
            hwrap: false,
            ..Options::default()
        };
        assert_eq!(
            build_height_field(&mut world, &opts, &mut NoSink),
            Control::Continue
        );
        assert_eq!(world.heights.len(), (world.w * world.h) as usize);
        assert_eq!(world.w, 128);
        assert_eq!(world.h, 96);
        let low = scan_min_height(&world.heights, world.w, world.h);
        assert!(world.sea_level >= low);
        assert!(((world.sea_level - low) / SEA_STEP).fract().abs() < 1e-3);
        assert!(world.real_water_part > 0.0 && world.real_water_part < 1.0);
        assert!(world.mount_level <= MOUNT_START);
        assert!(world.deep_level <= world.sea_level);
    }

    #[test]
    fn a_run_is_reproducible_from_the_seed_and_the_wrap_flags_change_the_field() {
        let opts = Options {
            width: 96,
            height: 96,
            hills: 3,
            hwrap: false,
            ..Options::default()
        };
        let mut a = World::new(11);
        build_height_field(&mut a, &opts, &mut NoSink);
        let mut b = World::new(11);
        build_height_field(&mut b, &opts, &mut NoSink);
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.sea_level, b.sea_level);

        let wrapped = Options {
            hwrap: true,
            ..opts.clone()
        };
        let mut c = World::new(11);
        build_height_field(&mut c, &wrapped, &mut NoSink);
        assert_eq!(c.w, 512);
        assert_ne!(c.heights.len(), a.heights.len());
    }

    #[test]
    fn blueprint_upscaling_consumes_two_crt_draws_per_interpolated_pixel() {
        let bp = Blueprint {
            w: 4,
            h: 4,
            bgra: vec![32; 4 * 4 * 4],
        };
        let mut crt = CrtRng::seeded(3);
        let before = crt;
        let up = upscale_blueprint(&bp, 64, 64, &mut crt);
        assert_eq!((up.w, up.h), (16, 16));
        let jittered = |w: i32, h: i32| ((w * h) - (w / 2) * (h / 2)) as u32;
        let expected = jittered(8, 8) + jittered(16, 16);
        let mut check = before;
        for _ in 0..expected * 2 {
            check.rand();
        }
        assert_eq!(check, crt);
    }

    #[test]
    fn percent_knobs_are_marshalled_through_double() {
        let p = HeightParams::from_options(&Options::default());
        assert_eq!(p.jitter, DEFAULT_JITTER);
        assert_eq!(p.sea_target, 0.45f32);
        assert_eq!(p.mount_target, 0.2f32);
        assert_eq!(p.nblobs, 150);
        assert_ne!((f64::from(30) * 0.01) as f32, 30f32 * 0.01f32);
        let hi = HeightParams::from_options(&Options {
            rugedness: 100,
            ..Options::default()
        });
        assert_eq!(hi.jitter, p.jitter);
    }
}
