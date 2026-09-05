use crate::d6m::RIVER_SENTINEL;
use crate::decor::{self, Sprite};
use crate::terrain::*;
use crate::textures::{Tex, TexSet};

pub const SEA_LEVEL: f32 = 0.0;
pub const WATER_BAND: f32 = 30.0;
pub const EDGE_FADE: i32 = 25;

pub struct Plane<'a> {
    pub w: i32,
    pub h: i32,
    pub heights: &'a [f32],
    pub owners: &'a [i16],
    pub flags: &'a [u64],
    pub scale: f32,
    pub hwrap: bool,
    pub vwrap: bool,
    pub capitals: &'a [(i16, i16)],
    pub rivers: &'a [(u32, u32)],
    pub mountain_lines: &'a [(u32, u32)],
    pub bridges: &'a [(u32, u32)],
    pub cave_plane: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub rivers: bool,
    pub borders: bool,
    pub capitals: bool,
    pub edge_fade: bool,
    pub decor: bool,
    pub border_percent: i32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            rivers: true,
            borders: true,
            capitals: true,
            edge_fade: true,
            decor: true,
            border_percent: 100,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub fn full(w: i32, h: i32) -> Rect {
        Rect {
            x0: 0,
            y0: 0,
            x1: w - 1,
            y1: h - 1,
        }
    }
    pub fn expand(&self, m: i32, w: i32, h: i32) -> Rect {
        Rect {
            x0: (self.x0 - m).max(0),
            y0: (self.y0 - m).max(0),
            x1: (self.x1 + m).min(w - 1),
            y1: (self.y1 + m).min(h - 1),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.x1 < self.x0 || self.y1 < self.y0
    }
    pub fn union(self, o: Rect) -> Rect {
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
    pub fn intersects(&self, o: Rect) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    pub fn clamp_to(self, w: i32, h: i32) -> Rect {
        Rect {
            x0: self.x0.max(0),
            y0: self.y0.max(0),
            x1: self.x1.min(w - 1),
            y1: self.y1.min(h - 1),
        }
    }
}

pub fn province_winter(flags: u64) -> bool {
    let mut v = 0i32;
    if flags & COLDER != 0 {
        v += 1;
    } else if flags & WARMER != 0 {
        v -= 1;
    }
    v > 0 && flags & CAVE_WALL == 0
}

pub fn land_texture(u: u64, winter: bool) -> Tex {
    let lo = u as u32;
    if !winter {
        if u & 0x40_0000_0040 == 0x40_0000_0040 {
            Tex::Floodedwaste
        } else if u & 0x40_0000_0020 == 0x40_0000_0020 {
            Tex::Floodedswamp
        } else if u & 0x40_0000_0000 == 0 {
            if lo & 0x1014 == 0x1014 {
                Tex::Floodcrys
            } else if lo & 0x1004 == 0x1004 {
                Tex::Floodcave
            } else if u & 0x10_0000_0000 == 0 {
                if lo & 0x1020 == 0x1020 {
                    Tex::Dripcave
                } else if lo & 0x1080 == 0x1080 {
                    Tex::Caveforest
                } else if lo & 0x1010 == 0x1010 {
                    Tex::Waste
                } else if (u >> 12) & 1 == 0 {
                    if u & 0x80 != 0 {
                        Tex::Forest
                    } else if u & 0x2000_0000_0000_0040 == 0x2000_0000_0000_0040 {
                        Tex::Desert
                    } else if u & 0x40 == 0 {
                        if u & 0x20 == 0 {
                            if (u >> 8) & 1 == 0 {
                                if u & 0x10 == 0 {
                                    if u & 0x4_0000_0000 != 0 {
                                        Tex::Ash
                                    } else {
                                        Tex::Plain
                                    }
                                } else {
                                    Tex::Highland
                                }
                            } else {
                                Tex::Farm
                            }
                        } else {
                            Tex::Swamp
                        }
                    } else {
                        Tex::Waste
                    }
                } else {
                    Tex::Cavefloor
                }
            } else {
                Tex::Cave
            }
        } else {
            Tex::Floodedplain
        }
    } else if (u >> 8) & 1 == 0 {
        if u & 0x80 != 0 {
            Tex::Winterwood
        } else if lo & 0x1020 == 0x1020 {
            Tex::Frozendrip
        } else {
            Tex::Winter
        }
    } else {
        Tex::Winterfarm
    }
}

#[inline]
fn wrap_clamp(v: i32, n: i32, wrap: bool) -> i32 {
    let mut r = v;
    if wrap {
        if r < 0 {
            r += n;
        }
        if r >= n {
            r -= n;
        }
    }
    r.clamp(0, n - 1)
}

#[inline]
fn owner_at(p: &Plane, x: i32, y: i32) -> i32 {
    if x < 0 || x >= p.w || y < 0 || y >= p.h {
        0
    } else {
        p.owners[(y * p.w + x) as usize] as i32
    }
}

pub fn province_bboxes(p: &Plane) -> Vec<[i32; 4]> {
    let n = p.flags.len().max(1);
    let mut b = vec![[32001, -1, 32001, -1]; n];
    for y in 0..p.h {
        for x in 0..p.w {
            let id = owner_at(p, x, y).max(0) as usize;
            if id >= n {
                continue;
            }
            let e = &mut b[id];
            e[0] = e[0].min(x);
            e[1] = e[1].max(x);
            e[2] = e[2].min(y);
            e[3] = e[3].max(y);
        }
    }
    b
}

pub fn carve_rivers(p: &Plane, work: &mut [f32], bboxes: &[[i32; 4]], within: Option<Rect>) {
    let r = (p.scale * 0.05) as i32;
    let w = p.w;
    let h = p.h;
    for &(a, b) in p.rivers {
        let (a, b) = (a as usize, b as usize);
        if a >= bboxes.len() || b >= bboxes.len() {
            continue;
        }
        let x0 = bboxes[a][0].min(bboxes[b][0]);
        let x1 = bboxes[a][1].max(bboxes[b][1]);
        let y0 = bboxes[a][2].min(bboxes[b][2]);
        let y1 = bboxes[a][3].max(bboxes[b][3]);
        if x0 < 0 || y0 < 0 || x1 < 0 || y1 < 0 {
            continue;
        }
        if let Some(lim) = within {
            let reach = Rect { x0, y0, x1, y1 }.expand(r + 1, w, h);
            if !reach.intersects(lim) {
                continue;
            }
        }
        let (a, b) = (a as i32, b as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let id = owner_at(p, x, y);
                if id != a && id != b {
                    continue;
                }
                let right = owner_at(p, x + 1, y);
                let other = if id == right {
                    let down = owner_at(p, x, y + 1);
                    if id == down {
                        continue;
                    }
                    down
                } else {
                    right
                };
                if other != a && other != b {
                    continue;
                }
                let rr = r;
                for dy in -rr..=rr {
                    for dx in -rr..=rr {
                        let xx = wrap_clamp(x + dx, w, p.hwrap);
                        let yy = wrap_clamp(y + dy, h, p.vwrap);
                        let i = (yy * w + xx) as usize;
                        if work[i] >= SEA_LEVEL
                            && ((dy * dy + dx * dx) as f32) <= ((rr * rr) as f32)
                        {
                            work[i] = RIVER_SENTINEL;
                        }
                    }
                }
            }
        }
    }
}

struct ProvLook {
    land: Tex,
    winter: bool,
    skip: bool,
}

fn province_looks(p: &Plane) -> Vec<ProvLook> {
    p.flags
        .iter()
        .map(|&f| {
            let winter = province_winter(f);
            ProvLook {
                land: land_texture(f, winter),
                winter,
                skip: f & UNKNOWN != 0,
            }
        })
        .collect()
}

#[inline]
fn blend_channels(deep: [u8; 4], water: [u8; 4], t: f32) -> [i32; 4] {
    let u = 1.0 - t;
    let mut o = [0i32; 4];
    for i in 0..4 {
        o[i] = (deep[i] as f32 * t + water[i] as f32 * u) as i32;
    }
    o
}

pub fn color_rows(p: &Plane, work: &[f32], tex: &TexSet, rect: Rect, out: &mut [u8]) {
    let looks = province_looks(p);
    color_rows_into(p, work, tex, &looks, rect, out, 0);
}

fn color_rows_into(
    p: &Plane,
    work: &[f32],
    tex: &TexSet,
    looks: &[ProvLook],
    rect: Rect,
    out: &mut [u8],
    row0: i32,
) {
    let (y0, y1, x0, x1) = (rect.y0, rect.y1, rect.x0, rect.x1);
    let w = p.w;
    for y in y0.max(0)..=y1.min(p.h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let i = (y * w + x) as usize;
            let oi = ((y - row0) * w + x) as usize;
            let id = p.owners[i];
            if id <= 0 {
                out[oi * 4..oi * 4 + 4].fill(0);
                continue;
            }
            let id = id as usize;
            if id >= looks.len() || looks[id].skip {
                out[oi * 4..oi * 4 + 4].fill(0);
                continue;
            }
            let f = p.flags[id];
            let look = &looks[id];
            let hv = work[i];
            let b60 = f & ALWAYS_WATER != 0;
            let mut c: [i32; 4];
            if hv < SEA_LEVEL || b60 {
                let mut depth = SEA_LEVEL - hv;
                if b60 && depth <= 10.1 {
                    depth = 10.1;
                }
                let t = if look.winter && (hv == RIVER_SENTINEL || depth < 10.0) {
                    Some(Tex::Frozen)
                } else if hv == RIVER_SENTINEL && !b60 {
                    Some(Tex::Shallowsea)
                } else if f & CAVE_LOOK != 0 {
                    Some(Tex::Cave)
                } else if f == KELP_EXACT {
                    Some(Tex::Kelpforest)
                } else if depth < 10.0 {
                    Some(Tex::Shallowsea)
                } else if depth <= WATER_BAND {
                    Some(Tex::Water)
                } else {
                    None
                };
                c = match t {
                    Some(t) => {
                        let s = tex.sample(t, x, y);
                        [s[0] as i32, s[1] as i32, s[2] as i32, s[3] as i32]
                    }
                    None => {
                        let k = (depth - WATER_BAND) * 0.166_666_67;
                        let deep = tex.sample(Tex::Deepsea, x, y);
                        if k < 1.0 {
                            let water = tex.sample(Tex::Water, x, y);
                            blend_channels(deep, water, k)
                        } else {
                            [
                                deep[0] as i32,
                                deep[1] as i32,
                                deep[2] as i32,
                                deep[3] as i32,
                            ]
                        }
                    }
                };
                if (f as u8) & 0x14 == 0x14 {
                    c[0] = (c[0] as f64 * 0.9) as i32;
                    c[1] = (c[1] as f64 * 0.9) as i32;
                    c[2] = (c[2] as f64 * 0.9) as i32;
                }
            } else {
                let s = tex.sample(look.land, x, y);
                c = [s[0] as i32, s[1] as i32, s[2] as i32, s[3] as i32];
            }
            if c[0] == 255 && c[1] == 255 && c[2] == 255 {
                c[0] = 254;
                c[1] = 254;
                c[2] = 254;
            }
            let o = oi * 4;
            out[o] = c[0].min(255) as u8;
            out[o + 1] = c[1].min(255) as u8;
            out[o + 2] = c[2].min(255) as u8;
            out[o + 3] = c[3] as u8;
        }
    }
}

pub fn border_width(scale: f32, percent: i32) -> f32 {
    let base = scale * 0.03;
    percent as f32 * base * 0.01
}

fn par_bands(
    rect: Rect,
    w: i32,
    out: &mut [u8],
    bytes_per_px: usize,
    f: impl Fn(Rect, &mut [u8], i32) + Sync,
) {
    if rect.is_empty() {
        return;
    }
    let rows = rect.y1 - rect.y0 + 1;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 32) as i32;
    let chunk = ((rows + threads - 1) / threads).max(32);
    let stride = w as usize * bytes_per_px;
    let first = rect.y0 as usize * stride;
    let last = (rect.y1 as usize + 1) * stride;
    let slice = &mut out[first..last];
    let f = &f;
    std::thread::scope(|sc| {
        let mut y = rect.y0;
        let mut rest = slice;
        while y <= rect.y1 {
            let y1 = (y + chunk - 1).min(rect.y1);
            let len = (y1 - y + 1) as usize * stride;
            let (mine, tail) = rest.split_at_mut(len);
            rest = tail;
            let band = Rect {
                x0: rect.x0,
                y0: y,
                x1: rect.x1,
                y1,
            };
            sc.spawn(move || f(band, mine, y));
            y = y1 + 1;
        }
    });
}

fn seam_rows(p: &Plane, work: &[f32], band: Rect, out: &mut [u8], row0: i32) {
    let w = p.w;
    let h = p.h;
    for y in band.y0.max(0)..=band.y1.min(h - 1) {
        for x in band.x0.max(0)..=band.x1.min(w - 1) {
            let i = (y * w + x) as usize;
            if work[i] == RIVER_SENTINEL {
                continue;
            }
            let id = p.owners[i];
            if id <= 0 {
                continue;
            }
            if (id as usize) < p.flags.len() && p.flags[id as usize] & UNKNOWN != 0 {
                continue;
            }
            let mut hit = false;
            'outer: for yy in y..=y + 1 {
                for xx in x..=x + 1 {
                    let nx = wrap_clamp(xx, w, p.hwrap);
                    let ny = wrap_clamp(yy, h, p.vwrap);
                    let nid = p.owners[(ny * w + nx) as usize];
                    if nid > 0 && nid != id {
                        hit = true;
                        break 'outer;
                    }
                }
            }
            if hit {
                out[((y - row0) * w + x) as usize] = 2;
            }
        }
    }
}

fn dilate_rows(w: i32, wrap: bool, r: i32, band: Rect, src: &[u8], out: &mut [u8], row0: i32) {
    for y in band.y0..=band.y1 {
        let row = (y * w) as usize;
        let orow = ((y - row0) * w) as usize;
        for x in band.x0..=band.x1 {
            let mut near = false;
            for d in -r..=r {
                let nx = wrap_clamp(x + d, w, wrap);
                if src[row + nx as usize] == 2 {
                    near = true;
                    break;
                }
            }
            if near {
                out[orow + x as usize] = 1;
            }
        }
    }
}

fn dilate_cols(p: &Plane, r: i32, band: Rect, wide: &[u8], out: &mut [u8], row0: i32) {
    let w = p.w;
    let h = p.h;
    for y in band.y0..=band.y1 {
        let orow = ((y - row0) * w) as usize;
        for x in band.x0..=band.x1 {
            let oi = orow + x as usize;
            if out[oi] >= 1 {
                continue;
            }
            for d in -r..=r {
                let ny = wrap_clamp(y + d, h, p.vwrap);
                if wide[(ny * w + x) as usize] != 0 {
                    out[oi] = 1;
                    break;
                }
            }
        }
    }
}

pub fn border_mask_rows(p: &Plane, work: &[f32], width: f32, rect: Rect, mask: &mut [u8]) {
    let rect = rect.clamp_to(p.w, p.h);
    if rect.is_empty() {
        return;
    }
    let w = p.w;
    let rr = (width + 0.5) as i32;
    par_bands(rect, w, mask, 1, |band, out, row0| {
        seam_rows(p, work, band, out, row0)
    });
    let mut wide = vec![0u8; (w * p.h) as usize];
    let src: &[u8] = mask;
    par_bands(rect, w, &mut wide, 1, |band, out, row0| {
        dilate_rows(w, p.hwrap, rr, band, src, out, row0)
    });
    let wide: &[u8] = &wide;
    par_bands(rect, w, mask, 1, |band, out, row0| {
        dilate_cols(p, rr, band, wide, out, row0)
    });
}

#[inline]
fn blend_pixel(out: &mut [u8], i: usize, r: i32, g: i32, b: i32, a: i32) {
    let o = i * 4;
    let inv = 255 - a;
    let mut nr = (out[o] as i32 * inv + r * a) / 255;
    let mut ng = (out[o + 1] as i32 * inv + g * a) / 255;
    let mut nb = (out[o + 2] as i32 * inv + b * a) / 255;
    if nr == 255 && ng == 255 && nb == 255 {
        nr = 254;
        ng = 254;
        nb = 254;
    }
    out[o] = nr as u8;
    out[o + 1] = ng as u8;
    out[o + 2] = nb as u8;
}

pub fn draw_border_rows(
    p: &Plane,
    mask: &[u8],
    width: f32,
    base_alpha: i32,
    rect: Rect,
    out: &mut [u8],
) {
    let rect = rect.clamp_to(p.w, p.h);
    par_bands(rect, p.w, out, 4, |band, slice, row0| {
        draw_border_band(p, mask, width, base_alpha, band, slice, row0)
    });
}

fn draw_border_band(
    p: &Plane,
    mask: &[u8],
    width: f32,
    base_alpha: i32,
    rect: Rect,
    out: &mut [u8],
    row0: i32,
) {
    let (y0, y1, x0, x1) = (rect.y0, rect.y1, rect.x0, rect.x1);
    let w = p.w;
    let h = p.h;
    let rad = (width * 1.5) as i32;
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let i = (y * w + x) as usize;
            if mask[i] == 0 {
                continue;
            }
            let sy0 = (y - rad).max(0);
            let sy1 = (y + rad).min(h - 1);
            let sx0 = (x - rad).max(0);
            let sx1 = (x + rad).min(w - 1);
            let mut best = 999_999.0f32;
            let mut dist = 999_999.0f32;
            let mut close = false;
            'search: for yy in sy0..=sy1 {
                for xx in sx0..=sx1 {
                    if mask[(yy * w + xx) as usize] == 2 {
                        let d = ((y - yy) * (y - yy) + (x - xx) * (x - xx)) as f32;
                        if d <= 1.0 {
                            dist = d;
                            close = true;
                            break 'search;
                        }
                        if d < best {
                            best = d;
                        }
                    }
                }
            }
            if !close {
                dist = best;
                if best < 999_999.0 && best <= (rad * rad) as f32 {
                    dist = best.sqrt();
                }
            }
            let t;
            if width <= dist {
                if width * 1.5 <= dist {
                    continue;
                }
                t = (dist - width) / (width * 0.5);
                if t < 0.0 {
                    continue;
                }
            } else {
                t = 0.0;
            }
            let a = ((1.0 - t) * base_alpha as f32) as i32;
            blend_pixel(out, ((y - row0) * w + x) as usize, 255, 255, 255, a);
        }
    }
}

pub fn mark_capitals(p: &Plane, out: &mut [u8]) {
    for &(x, y) in p.capitals {
        let (x, y) = (x as i32, y as i32);
        if x >= 0 && y >= 0 && x < p.w && y < p.h {
            let o = ((y * p.w + x) * 4) as usize;
            out[o] = 255;
            out[o + 1] = 255;
            out[o + 2] = 255;
            out[o + 3] = 255;
        }
    }
}

pub fn hide_unknown_rows(p: &Plane, tex: &TexSet, rect: Rect, out: &mut [u8]) {
    let (y0, y1, x0, x1) = (rect.y0, rect.y1, rect.x0, rect.x1);
    if !p.flags.iter().any(|&f| f & UNKNOWN != 0) {
        return;
    }
    let img = tex.get(if p.cave_plane {
        Tex::Cave
    } else {
        Tex::Unknown
    });
    let w = p.w;
    for y in y0.max(0)..=y1.min(p.h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let i = (y * w + x) as usize;
            let id = p.owners[i];
            let hidden =
                id <= 0 || (id as usize) < p.flags.len() && p.flags[id as usize] & UNKNOWN != 0;
            if !hidden {
                continue;
            }
            let mut s = img.tiled(x, y);
            if s[0] == 255 && s[1] == 255 && s[2] == 255 {
                s[0] = 254;
                s[1] = 254;
                s[2] = 254;
            }
            out[i * 4..i * 4 + 4].copy_from_slice(&s);
        }
    }
}

pub fn edge_fade_rows(p: &Plane, rect: Rect, out: &mut [u8]) {
    let (y0, y1, x0, x1) = (rect.y0, rect.y1, rect.x0, rect.x1);
    let w = p.w;
    let h = p.h;
    let n = EDGE_FADE;
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let mut d = i32::MAX;
            if !p.hwrap {
                d = d.min(x).min(w - 1 - x);
            }
            if !p.vwrap {
                d = d.min(y).min(h - 1 - y);
            }
            if d >= n {
                continue;
            }
            let k = d as f32 / n as f32;
            let i = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                out[i + c] = (out[i + c] as f32 * k) as u8;
            }
        }
    }
}

pub struct Rendered {
    pub w: i32,
    pub h: i32,
    pub rgba: Vec<u8>,
    pub carved: Vec<f32>,
    pub mask: Vec<u8>,
    pub bboxes: Vec<[i32; 4]>,
    pub width: f32,
    pub decor: Vec<u8>,
    pub sprites: Vec<Vec<Sprite>>,
    pub mountains: Vec<Vec<Sprite>>,
    pub touched: Rect,
}

impl Rendered {
    pub fn empty() -> Rendered {
        Rendered {
            w: 0,
            h: 0,
            rgba: Vec::new(),
            carved: Vec::new(),
            mask: Vec::new(),
            bboxes: Vec::new(),
            width: 0.0,
            decor: Vec::new(),
            sprites: Vec::new(),
            mountains: Vec::new(),
            touched: Rect::full(0, 0),
        }
    }

    pub fn new(p: &Plane, tex: &TexSet, opts: &Options) -> Rendered {
        let n = (p.w * p.h) as usize;
        let mut r = Rendered {
            w: p.w,
            h: p.h,
            rgba: vec![0u8; n * 4],
            carved: vec![0.0; n],
            mask: vec![0u8; n],
            bboxes: province_bboxes(p),
            width: border_width(p.scale, opts.border_percent),
            decor: vec![0u8; n * 4],
            sprites: Vec::new(),
            mountains: Vec::new(),
            touched: Rect::full(p.w, p.h),
        };
        r.render(p, tex, opts, Rect::full(p.w, p.h));
        r
    }

    pub fn margin(&self) -> i32 {
        (self.width * 1.5) as i32 + 3
    }

    pub fn render(&mut self, p: &Plane, tex: &TexSet, opts: &Options, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.width = border_width(p.scale, opts.border_percent);
        let m = self.margin();
        let outer = rect.expand(m * 2, p.w, p.h);
        let inner = rect.expand(m, p.w, p.h);
        let full = outer.x0 <= 0 && outer.y0 <= 0 && outer.x1 >= p.w - 1 && outer.y1 >= p.h - 1;
        let reach = outer.expand((p.scale * 0.05) as i32 + 1, p.w, p.h);
        for y in reach.y0..=reach.y1 {
            let row = (y * p.w) as usize;
            let (a, b) = (row + reach.x0 as usize, row + reach.x1 as usize + 1);
            self.carved[a..b].copy_from_slice(&p.heights[a..b]);
        }
        if opts.rivers {
            let within = if full { None } else { Some(reach) };
            carve_rivers(p, &mut self.carved, &self.bboxes, within);
        }
        self.color(p, tex, inner);
        for y in inner.y0..=inner.y1 {
            let row = (y * p.w) as usize;
            self.mask[row + inner.x0 as usize..=row + inner.x1 as usize].fill(0);
        }
        for y in outer.y0..=outer.y1 {
            let row = (y * p.w) as usize;
            self.mask[row + outer.x0 as usize..=row + outer.x1 as usize].fill(0);
        }
        if opts.borders {
            border_mask_rows(p, &self.carved, self.width, outer, &mut self.mask);
            draw_border_rows(p, &self.mask, self.width, 15, inner, &mut self.rgba);
            draw_border_rows(p, &self.mask, self.width, 30, inner, &mut self.rgba);
        }
        if opts.capitals {
            mark_capitals(p, &mut self.rgba);
        }
        hide_unknown_rows(p, tex, inner, &mut self.rgba);
        if opts.edge_fade {
            edge_fade_rows(p, inner, &mut self.rgba);
        }
        self.touched = inner;
        if opts.decor {
            let full = rect.x0 <= 0 && rect.y0 <= 0 && rect.x1 >= p.w - 1 && rect.y1 >= p.h - 1;
            self.decorate(p, tex, inner, full);
        }
    }

    pub fn composed(&self, capitals: &[(i16, i16)]) -> Vec<u8> {
        let mut out = self.rgba.clone();
        let has_decor = self.decor.len() == out.len();
        for (i, dst) in out.chunks_exact_mut(4).enumerate() {
            if dst[3] == 0 {
                dst.copy_from_slice(&[0, 0, 0, 255]);
                continue;
            }
            if has_decor {
                let src = &self.decor[i * 4..i * 4 + 4];
                let a = src[3] as u32;
                if a > 0 {
                    for c in 0..3 {
                        dst[c] = (src[c] as u32 + (dst[c] as u32 * (255 - a) + 127) / 255).min(255)
                            as u8;
                    }
                }
            }
            dst[3] = 255;
            if dst[0] == 255 && dst[1] == 255 && dst[2] == 255 {
                dst[0] = 254;
                dst[1] = 254;
                dst[2] = 254;
            }
        }
        for &(x, y) in capitals {
            let (x, y) = (x as i32, y as i32);
            if x >= 0 && y >= 0 && x < self.w && y < self.h {
                let o = ((y * self.w + x) * 4) as usize;
                out[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        out
    }

    pub fn sprite_count(&self) -> usize {
        self.sprites.iter().map(Vec::len).sum::<usize>()
            + self.mountains.iter().map(Vec::len).sum::<usize>()
    }

    fn decorate(&mut self, p: &Plane, tex: &TexSet, rect: Rect, full: bool) {
        let n = p.flags.len();
        if self.sprites.len() != n {
            self.sprites = vec![Vec::new(); n];
            self.mountains = vec![Vec::new(); n];
        }
        if self.decor.len() != (p.w * p.h * 4) as usize {
            self.decor = vec![0u8; (p.w * p.h * 4) as usize];
        }
        let lines = decor::mountain_line_set(p);
        let reach = (p.scale * 3.0) as i32 + 4;
        let probe = rect.expand(reach, p.w, p.h);
        let affected: Vec<usize> = (1..n)
            .filter(|&i| {
                if full {
                    return true;
                }
                let b = self.bboxes[i];
                if b[1] < 0 || b[3] < 0 {
                    return !self.sprites[i].is_empty() || !self.mountains[i].is_empty();
                }
                let br = Rect {
                    x0: b[0],
                    y0: b[2],
                    x1: b[1],
                    y1: b[3],
                };
                br.intersects(probe)
            })
            .collect();
        let mut redraw = if full { Rect::full(p.w, p.h) } else { rect };
        for &i in &affected {
            if let Some(r) = decor::sprite_bounds(&self.sprites[i]) {
                redraw = redraw.union(r);
            }
            if let Some(r) = decor::sprite_bounds(&self.mountains[i]) {
                redraw = redraw.union(r);
            }
        }
        let carved = &self.carved;
        let bboxes = &self.bboxes;
        let lines = &lines;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 32);
        let chunk = affected.len().div_ceil(threads).max(8);
        let results: Vec<(usize, Vec<Sprite>, Vec<Sprite>)> = std::thread::scope(|s| {
            let handles: Vec<_> = affected
                .chunks(chunk)
                .map(|ids| {
                    s.spawn(move || {
                        let mut out = Vec::with_capacity(ids.len());
                        for &i in ids {
                            let mut trees = Vec::new();
                            let mut rocks = Vec::new();
                            decor::province_sprites(p, carved, lines, i, &mut trees);
                            decor::mountain_sprites(p, carved, lines, i, bboxes[i], &mut rocks);
                            decor::bridge_sprites(p, i, &mut trees, &mut rocks);
                            out.push((i, trees, rocks));
                        }
                        out
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        for (i, trees, rocks) in results {
            if let Some(r) = decor::sprite_bounds(&trees) {
                redraw = redraw.union(r);
            }
            if let Some(r) = decor::sprite_bounds(&rocks) {
                redraw = redraw.union(r);
            }
            self.sprites[i] = trees;
            self.mountains[i] = rocks;
        }
        let redraw = redraw.clamp_to(p.w, p.h);
        let mut all: Vec<Sprite> = Vec::with_capacity(self.sprite_count());
        for v in self.sprites.iter().chain(self.mountains.iter()) {
            all.extend_from_slice(v);
        }
        decor::order(&mut all);
        decor::draw_sprites(p, tex, &all, redraw, &mut self.decor);
        self.touched = self.touched.union(redraw).clamp_to(p.w, p.h);
    }

    fn color(&mut self, p: &Plane, tex: &TexSet, rect: Rect) {
        let looks = province_looks(p);
        let rows = rect.y1 - rect.y0 + 1;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 32) as i32;
        let chunk = ((rows + threads - 1) / threads).max(32);
        let w = p.w as usize;
        let stride = w * 4;
        let carved = &self.carved;
        let looks = &looks;
        let first = rect.y0 as usize * stride;
        let last = (rect.y1 as usize + 1) * stride;
        let slice = &mut self.rgba[first..last];
        std::thread::scope(|s| {
            let mut y = rect.y0;
            let mut rest = slice;
            while y <= rect.y1 {
                let y1 = (y + chunk - 1).min(rect.y1);
                let len = (y1 - y + 1) as usize * stride;
                let (mine, tail) = rest.split_at_mut(len);
                rest = tail;
                s.spawn(move || {
                    color_rows_into(
                        p,
                        carved,
                        tex,
                        looks,
                        Rect {
                            x0: rect.x0,
                            y0: y,
                            x1: rect.x1,
                            y1,
                        },
                        mine,
                        y,
                    );
                });
                y = y1 + 1;
            }
        });
    }
}

pub fn flip_to_top_down(w: i32, h: i32, rgba: &[u8]) -> Vec<u8> {
    let stride = (w * 4) as usize;
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h as usize {
        let src = (h as usize - 1 - y) * stride;
        out[y * stride..(y + 1) * stride].copy_from_slice(&rgba[src..src + stride]);
    }
    out
}
