use crate::render::{province_winter, Plane, Rect, SEA_LEVEL};
use crate::terrain::*;
use crate::textures::{sample_bilinear, TexSet};
use std::collections::HashSet;

pub const WINTER_MOUNTAIN_OFFSET: i16 = 11;
const MOUNTAIN_LINE_SPECS: u64 = BORDER_MOUNTAIN_PASS | BORDER_MOUNTAIN_LINE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub x: i32,
    pub y: i32,
    pub size: i16,
    pub idx: i16,
    pub layer: u8,
}

impl Sprite {
    pub fn rect(&self) -> Rect {
        let s = self.size as i32;
        Rect {
            x0: self.x - s / 2,
            y0: self.y,
            x1: self.x - s / 2 + s - 1,
            y1: self.y + s - 1,
        }
    }
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let s = seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xD1B5_4A32_D192_ED03);
        Rng(if s == 0 { 1 } else { s })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: i32) -> i32 {
        if n <= 0 {
            0
        } else {
            (self.next() >> 33) as i32 % n
        }
    }

    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn dice(&mut self, n: i32, sides: i32) -> i32 {
        (0..n.max(0)).map(|_| self.below(sides) + 1).sum()
    }
}

struct Ctx<'a> {
    p: &'a Plane<'a>,
    heights: &'a [f32],
    lines: &'a HashSet<(u32, u32)>,
}

impl Ctx<'_> {
    #[inline]
    fn owner(&self, x: i32, y: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.p.w || y >= self.p.h {
            0
        } else {
            self.p.owners[(y * self.p.w + x) as usize] as i32
        }
    }

    #[inline]
    fn wrap(&self, x: i32, y: i32) -> (i32, i32) {
        let mut x = x;
        let mut y = y;
        if self.p.hwrap {
            if x < 0 {
                x += self.p.w;
            }
            if x >= self.p.w {
                x -= self.p.w;
            }
        }
        if self.p.vwrap {
            if y < 0 {
                y += self.p.h;
            }
            if y >= self.p.h {
                y -= self.p.h;
            }
        }
        (x, y)
    }

    #[inline]
    fn height_clamped(&self, x: i32, y: i32) -> f32 {
        let cx = x.clamp(0, self.p.w - 1);
        let cy = y.clamp(0, self.p.h - 1);
        self.heights[(cy * self.p.w + cx) as usize]
    }

    fn dry(&self, x: i32, y: i32) -> bool {
        self.height_clamped(x - 2, y) >= SEA_LEVEL
            && self.height_clamped(x, y) >= SEA_LEVEL
            && self.height_clamped(x + 2, y) >= SEA_LEVEL
    }

    fn rejected(&self, x: i32, y: i32, prov: i32, margin: i32, rng: &mut Rng) -> bool {
        let cx = x.clamp(0, self.p.w - 1);
        let cy = y.clamp(0, self.p.h - 1);
        if self.owner(cx, cy) == prov || margin > 9999 {
            return false;
        }
        if rng.below(100) < 50 {
            return true;
        }
        let mut best = 99999;
        for yy in cy - margin..=cy + margin {
            for xx in cx - margin..=cx + margin {
                if self.owner(xx, yy) == prov {
                    let d = (yy - cy).abs() + (xx - cx).abs();
                    if d <= best {
                        best = d;
                    }
                }
            }
        }
        if best <= margin {
            return rng.below(100) < best * 50 / margin;
        }
        true
    }

    fn accepts(&self, x: i32, y: i32, prov: i32, margin: i32, rng: &mut Rng) -> bool {
        self.dry(x, y) && !self.rejected(x, y, prov, margin, rng)
    }

    fn near_mountain_line(&self, x: i32, y: i32, r: i32) -> bool {
        let p = self.owner(x, y);
        if p <= 0 || self.p.flags[p as usize] & MOUNTAIN == 0 {
            return false;
        }
        let r = r.max(1);
        let half = r / 2;
        for (dx, dy) in [
            (r, 0),
            (-r, 0),
            (0, r),
            (0, -r),
            (half, 0),
            (-half, 0),
            (0, half),
            (0, -half),
        ] {
            let q = self.owner(x + dx, y + dy);
            if q <= 0 || q == p || q as usize >= self.p.flags.len() {
                continue;
            }
            if self.p.flags[q as usize] & MOUNTAIN == 0 {
                continue;
            }
            let key = ((p as u32).min(q as u32), (p as u32).max(q as u32));
            if self.lines.contains(&key) {
                return true;
            }
        }
        false
    }
}

fn pick(sel: i32, size: i32, winter: bool, rng: &mut Rng) -> Option<(i16, i32)> {
    let s = size as f32;
    let noise = |rng: &mut Rng| rng.unit() - 0.5;
    let out = match sel {
        -1 => (
            rng.below(6) + if winter { 0x22 } else { 0x16 },
            (s * 0.75) as i32,
        ),
        -2 => {
            if rng.below(100) > 0x20 {
                (0x2e + rng.below(2), size)
            } else {
                (0x32, size)
            }
        }
        -3 => (rng.below(5) + if winter { 0x28 } else { 0x1c }, size),
        -4 => {
            let mut i = rng.below(6);
            if rng.below(100) < 1 {
                i = 6 + rng.below(3);
            }
            (i, if i < 4 { size / 2 } else { size })
        }
        -6 => {
            if rng.below(100) < 0x50 {
                (0x3f, size)
            } else if rng.below(100) < 0x23 {
                let i = 0x40 + rng.below(3);
                let n = noise(rng);
                (i, (s * 0.5 + n * s * 0.2) as i32)
            } else {
                let i = 0x43 + rng.below(2);
                let n = noise(rng);
                (i, (s * 0.3 + n * s * 0.15) as i32)
            }
        }
        -7 => {
            if winter {
                return None;
            }
            if rng.below(100) < 0x4b {
                (0x46, size)
            } else {
                (0x47, size)
            }
        }
        -8 => {
            if rng.below(100) < 0x4b {
                (0x48, size)
            } else {
                (0x49 + rng.below(2), size)
            }
        }
        -9 => (0x43 + rng.below(2), size),
        -10 => (0x31, size / 2),
        -11 => (rng.below(3) + if winter { 0x52 } else { 0x4f }, size),
        -12 => {
            let i = 0x55 + rng.below(4);
            let n = noise(rng);
            (i, (n * s * 0.15 + s) as i32)
        }
        -13 => {
            let i = 0x59 + rng.below(6);
            let n = noise(rng);
            (i, (n * s * 0.75 + s) as i32)
        }
        -14 => {
            let i = rng.below(4);
            let n = noise(rng);
            (i, (n * s * 0.5 + s) as i32)
        }
        -15 => {
            let i = 0x57 + rng.below(2);
            let n = noise(rng);
            (i, (n * s * 0.15 + s) as i32)
        }
        -16 => {
            let i = 0x5f + rng.below(3);
            let n = noise(rng);
            (i, (n * s * 0.5 + s) as i32)
        }
        i if i >= 0 => (i, size),
        _ => return None,
    };
    if out.1 <= 0 {
        return None;
    }
    Some((out.0 as i16, out.1))
}

struct Scatter {
    cx: i32,
    cy: i32,
    spread: i32,
    count: i32,
    sel: i32,
    size: i32,
    margin: i32,
    layer: u8,
    winter: bool,
    cluster: i32,
    cluster_scale: f32,
}

impl Scatter {
    fn new(cx: i32, cy: i32, spread: f32, count: i32, sel: i32, size: f32) -> Scatter {
        Scatter {
            cx,
            cy,
            spread: spread as i32,
            count,
            sel,
            size: size as i32,
            margin: 0,
            layer: 0,
            winter: false,
            cluster: 0,
            cluster_scale: 0.0,
        }
    }

    fn margin(mut self, m: f32) -> Scatter {
        self.margin = m as i32;
        self
    }

    fn winter(mut self, w: bool) -> Scatter {
        self.winter = w;
        self
    }

    fn clusters(mut self, n: i32, scale: f32) -> Scatter {
        self.cluster = n;
        self.cluster_scale = scale;
        self
    }
}

fn scatter(ctx: &Ctx, prov: i32, sc: Scatter, rng: &mut Rng, out: &mut Vec<Sprite>) {
    let mut extra = 0;
    let mut x = sc.cx;
    let mut y = sc.cy;
    for _ in 0..sc.count {
        if extra < 1 {
            x = sc.cx - sc.spread / 2 + rng.below(sc.spread);
            y = sc.cy - sc.spread / 2 + rng.below(sc.spread);
            if sc.cluster > 0 {
                extra = rng.dice(sc.cluster, 2);
            }
        } else {
            extra -= 1;
            let sp = (sc.size as f32 * sc.cluster_scale) as i32;
            let half = sc.size as f32 * sc.cluster_scale * 0.5;
            x = (rng.below(sp) as f32 - half + x as f32) as i32;
            y = (rng.below(sp) as f32 - half + y as f32) as i32;
        }
        let (wx, wy) = ctx.wrap(x, y);
        x = wx;
        y = wy;
        if sc.layer == 0 && !ctx.accepts(x, y, prov, sc.margin, rng) {
            continue;
        }
        let Some((idx, size)) = pick(sc.sel, sc.size, sc.winter, rng) else {
            continue;
        };
        if (0x34..=0x3c).contains(&sc.sel) {
            let r = (sc.size as f32 * 0.4) as i32;
            out.retain(|s| (s.x - x).abs() + (s.y - y).abs() > r);
        }
        out.push(Sprite {
            x,
            y,
            size: size.min(i16::MAX as i32) as i16,
            idx,
            layer: sc.layer,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn hut_rows(
    ctx: &Ctx,
    prov: i32,
    cx: i32,
    cy: i32,
    spread: i32,
    n: i32,
    size: i32,
    winter: bool,
    rng: &mut Rng,
    out: &mut Vec<Sprite>,
) {
    if winter {
        return;
    }
    let mut x = cx;
    let mut y = cy;
    let mut found = false;
    for _ in 0..=1000 {
        let (wx, wy) = ctx.wrap(
            cx - spread / 2 + rng.below(spread),
            cy - spread / 2 + rng.below(spread),
        );
        x = wx.clamp(0, ctx.p.w - 1);
        y = wy.clamp(0, ctx.p.h - 1);
        if ctx.accepts(x, y, prov, 0, rng) {
            found = true;
            break;
        }
    }
    if !found {
        return;
    }
    let s = size as f32;
    let back = s * 0.15 * n as f32;
    let (x0, y0) = ctx.wrap((x as f32 - back) as i32, (y as f32 - back) as i32);
    let x0 = x0.clamp(0, ctx.p.w - 1);
    let mut y = y0.clamp(0, ctx.p.h - 1);
    let mut x = x0;
    let total = (n - 1).max(1) * (n + 1);
    for k in 0..total {
        x = (x as f32 + s * 0.18) as i32;
        if k % (n + 1) == n {
            y = (s * rng.unit() * 0.1 + s * 0.18 + y as f32) as i32;
            x = x0;
        }
        let (wx, wy) = ctx.wrap(x, y);
        x = wx.clamp(0, ctx.p.w - 1);
        y = wy.clamp(0, ctx.p.h - 1);
        if !ctx.accepts(x, y, prov, 0, rng) {
            continue;
        }
        let px = ((rng.unit() - 0.5) * s * 0.15 + x as f32) as i32;
        let py = ((rng.unit() - 0.5) * s * 0.15 + y as f32) as i32;
        let sz = ((rng.unit() - 0.5) * s * 0.25 + s) as i32;
        if sz <= 0 {
            continue;
        }
        out.push(Sprite {
            x: px,
            y: py,
            size: sz as i16,
            idx: 0x30,
            layer: 0,
        });
    }
}

pub fn province_sprites(
    p: &Plane,
    heights: &[f32],
    lines: &HashSet<(u32, u32)>,
    prov: usize,
    out: &mut Vec<Sprite>,
) {
    out.clear();
    if prov == 0 || prov >= p.flags.len() || prov > p.capitals.len() {
        return;
    }
    let flags = p.flags[prov];
    if flags & UNKNOWN != 0 {
        return;
    }
    let ctx = Ctx { p, heights, lines };
    let seed = (prov as u64) << 40 ^ (p.w as u64) << 20 ^ p.h as u64 ^ flags;
    let mut rng = Rng::new(seed);
    let pr = prov as i32;
    let (cx, cy) = p.capitals[prov - 1];
    let (cx, cy) = (cx as i32, cy as i32);
    let sc = p.scale;
    let s = sc.max(4.0);
    let winter = province_winter(flags);
    if flags & CAVE != 0 {
        if flags & FOREST != 0 {
            let n = rng.below(150) + 300;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 4.0, n, -12, s * 0.15).margin(1.0),
                &mut rng,
                out,
            );
            let n = rng.below(10) + 20;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 4.0, n, -15, s * 0.6).margin(1.0),
                &mut rng,
                out,
            );
        } else if flags & HIGHLAND == 0 {
            let (n, size) = if flags & SWAMP != 0 {
                (rng.below(25) + 50, s * 0.35)
            } else {
                (rng.below(10) + 20, s * 0.25)
            };
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 5.0, n, -14, size).margin(1.0),
                &mut rng,
                out,
            );
        } else {
            let n = rng.below(150) + 300;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 5.0, n, -13, s * 0.15)
                    .margin(1.0)
                    .clusters(4, 2.5),
                &mut rng,
                out,
            );
            let n = rng.below(150) + 300;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 5.0, n, -13, s * 0.1).margin(1.0),
                &mut rng,
                out,
            );
            let n = rng.below(10) + 20;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 5.0, n, -13, s * 0.3).margin(1.0),
                &mut rng,
                out,
            );
        }
        return;
    }
    if flags & FARM != 0 {
        let groups = rng.below(4) + 3;
        for _ in 0..groups {
            let n = rng.below(4) + 9;
            hut_rows(
                &ctx,
                pr,
                cx,
                cy,
                sc as i32,
                n,
                (s * 0.3) as i32,
                winter,
                &mut rng,
                out,
            );
        }
        if flags & LARGE == 0 {
            let n = rng.below(6) + 2;
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 1.25, n, -10, s)
                    .margin(sc * 0.25)
                    .winter(winter),
                &mut rng,
                out,
            );
        } else {
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, (cy as f32 - s * 0.5) as i32, 0.0, 1, 0x45, s).winter(winter),
                &mut rng,
                out,
            );
            let n = rng.below(2);
            scatter(
                &ctx,
                pr,
                Scatter::new(cx, cy, sc * 1.25, n, 0x31, s * 0.5)
                    .margin(sc * 0.25)
                    .winter(winter),
                &mut rng,
                out,
            );
        }
    }
    let flooded = flags & ALWAYS_WATER != 0;
    if flags & FOREST != 0 {
        let n = rng.below(150) + 250;
        let sel = if flooded { -3 } else { -1 };
        scatter(
            &ctx,
            pr,
            Scatter::new(cx, cy, sc * 3.0, n, sel, s * 0.5).winter(winter),
            &mut rng,
            out,
        );
    }
    if flags & HIGHLAND != 0 {
        let n = rng.below(25) + 75;
        scatter(
            &ctx,
            pr,
            Scatter::new(cx, cy, sc * 3.0, n, -11, s * 0.75)
                .winter(winter)
                .clusters(6, 1.0),
            &mut rng,
            out,
        );
    }
    if flags & SWAMP != 0 {
        let n = rng.below(200) + 300;
        scatter(
            &ctx,
            pr,
            Scatter::new(cx, cy, sc * 3.0, n, -6, s * 0.4)
                .margin(sc * 0.05)
                .winter(winter),
            &mut rng,
            out,
        );
    }
    if flags & WASTE != 0 {
        let n = rng.below(6) + 2;
        scatter(
            &ctx,
            pr,
            Scatter::new(cx, cy, sc * 1.25, n, -2, s * 0.3).winter(winter),
            &mut rng,
            out,
        );
    }
    site_sprites(&ctx, pr, cx, cy, sc, s, flags, winter, &mut rng, out);
    if flags & (WASTE | BORDER_MOUNTAIN | CAVE_WALL) == 0 {
        let (n, sel) = if flooded {
            (rng.below(6), -3)
        } else {
            (rng.below(4), -1)
        };
        scatter(
            &ctx,
            pr,
            Scatter::new(cx, cy, sc * 3.0, n, sel, s * 0.5).winter(winter),
            &mut rng,
            out,
        );
    }
    const NO_MEADOW: u64 =
        SEA | HIGHLAND | SWAMP | WASTE | FARM | CAVE | BORDER_MOUNTAIN | CAVE_WALL | ALWAYS_WATER;
    if flags & NO_MEADOW != 0 {
        return;
    }
    let n = rng.below(3);
    scatter(
        &ctx,
        pr,
        Scatter::new(cx, cy, sc * 3.0, n, -7, s * 0.1).winter(winter),
        &mut rng,
        out,
    );
    let n = rng.below(5) + 1;
    scatter(
        &ctx,
        pr,
        Scatter::new(cx, cy, sc * 3.0, n, -8, s * 0.15).winter(winter),
        &mut rng,
        out,
    );
    for _ in 0..4 {
        if rng.below(100) < 50 {
            let ox = (rng.unit() - 0.5) * sc * 2.0;
            let oy = (rng.unit() - 0.5) * sc;
            let n = rng.below(40) + 10;
            scatter(
                &ctx,
                pr,
                Scatter::new(
                    (cx as f32 + ox) as i32,
                    (cy as f32 + oy) as i32,
                    s * 0.6,
                    n,
                    -7,
                    s * 0.1,
                )
                .winter(winter),
                &mut rng,
                out,
            );
        } else if rng.below(100) < 50 {
            let ox = (rng.unit() - 0.5) * sc * 2.0;
            let oy = (rng.unit() - 0.5) * sc;
            let n = rng.below(40) + 10;
            scatter(
                &ctx,
                pr,
                Scatter::new(
                    (cx as f32 + ox) as i32,
                    (cy as f32 + oy) as i32,
                    s,
                    n,
                    -8,
                    s * 0.15,
                )
                .winter(winter),
                &mut rng,
                out,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn site_sprites(
    ctx: &Ctx,
    pr: i32,
    cx: i32,
    cy: i32,
    sc: f32,
    s: f32,
    flags: u64,
    winter: bool,
    rng: &mut Rng,
    out: &mut Vec<Sprite>,
) {
    let one = |sprite: i32, spread: f32, size: f32, rng: &mut Rng, out: &mut Vec<Sprite>| {
        scatter(
            ctx,
            pr,
            Scatter::new(cx, cy, spread, 1, sprite, size).winter(winter),
            rng,
            out,
        );
    };
    if flags & MANY_SITES != 0 {
        let sprite = 0x34 + rng.below(9);
        one(sprite, sc, s * 0.5, rng, out);
    } else if flags & (1 << 13) != 0 {
        one(0x34, sc * 0.25, s * 0.85, rng, out);
        scatter(
            ctx,
            pr,
            Scatter::new(cx, cy, sc * 0.75, 5, -4, s * 0.7).winter(winter),
            rng,
            out,
        );
    } else if flags & (1 << 14) != 0 {
        one(0x35, sc * 0.25, s * 0.85, rng, out);
        scatter(
            ctx,
            pr,
            Scatter::new(cx, cy, sc * 0.75, 10, -4, s * 0.7).winter(winter),
            rng,
            out,
        );
    } else if flags & (1 << 15) != 0 {
        if rng.below(100) < 50 {
            scatter(
                ctx,
                pr,
                Scatter::new(cx, cy, sc * 0.75, 30, -9, s * 0.1).winter(winter),
                rng,
                out,
            );
        } else {
            one(0x36, sc * 0.75, s * 0.5, rng, out);
        }
    } else if flags & (1 << 16) != 0 {
        one(0x37, sc, s * 0.4, rng, out);
    } else if flags & (1 << 17) != 0 {
        one(0x38, sc, s * 0.5, rng, out);
    } else if flags & (1 << 18) != 0 {
        one(0x39, sc, s * 0.5, rng, out);
    } else if flags & (1 << 19) != 0 {
        one(0x3a, sc, s * 0.5, rng, out);
    } else if flags & (1 << 21) != 0 {
        one(0x3b, sc, s * 0.5, rng, out);
    } else if flags & (1 << 22) != 0 {
        one(0x3c, sc, s * 0.5, rng, out);
    }
}

pub fn mountain_sprites(
    p: &Plane,
    heights: &[f32],
    lines: &HashSet<(u32, u32)>,
    prov: usize,
    bbox: [i32; 4],
    out: &mut Vec<Sprite>,
) {
    if prov == 0 || prov >= p.flags.len() || lines.is_empty() {
        return;
    }
    let flags = p.flags[prov];
    if flags & MOUNTAIN == 0 || flags & UNKNOWN != 0 || bbox[1] < 0 || bbox[3] < 0 {
        return;
    }
    let ctx = Ctx { p, heights, lines };
    let mut rng = Rng::new((prov as u64) << 40 ^ 0xA5A5 ^ (p.w as u64) << 20 ^ p.h as u64);
    let s = p.scale.max(4.0);
    let chance = (220000.0 / (s * s)).max(1.0);
    let winter = province_winter(flags);
    let base_size = (s * 0.9) as i32;
    for y in bbox[2]..=bbox[3] {
        let mut x = bbox[0] + ((bbox[0] & 1) ^ (y & 1));
        while x <= bbox[1] {
            if ctx.owner(x, y) == prov as i32 && (rng.below(2500) as f32) < chance {
                let r = ((rng.unit() + 1.0) * 0.5 * s * 0.22) as i32;
                if ctx.near_mountain_line(x, y, r) {
                    let mut idx = if rng.below(100) < 85 {
                        rng.below(4)
                    } else {
                        let mut i = 4 + rng.below(7);
                        if i == 7 || i == 10 {
                            i = 4 + rng.below(6);
                        }
                        i
                    };
                    let size = if idx < 4 { base_size / 2 } else { base_size };
                    if winter {
                        idx += WINTER_MOUNTAIN_OFFSET as i32;
                    }
                    if size > 0 {
                        out.push(Sprite {
                            x,
                            y,
                            size: size as i16,
                            idx: idx as i16,
                            layer: 0,
                        });
                    }
                }
            }
            x += 2;
        }
    }
}

fn border_crossing(p: &Plane, a: usize, b: usize) -> Option<(i32, i32)> {
    let (ax, ay) = p.capitals[a - 1];
    let (bx, by) = p.capitals[b - 1];
    let dx = (bx - ax) as f32;
    let dy = (by - ay) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0 {
        return None;
    }
    let mut t = 0.0f32;
    loop {
        let x = (t * dx / len + ax as f32) as i32;
        let y = (t * dy / len + ay as f32) as i32;
        let id = if x < 0 || y < 0 || x >= p.w || y >= p.h {
            0
        } else {
            p.owners[(y * p.w + x) as usize] as i32
        };
        if id == b as i32 {
            return Some((x, y));
        }
        t += 1.0;
        if t >= len {
            return None;
        }
    }
}

pub fn bridge_sprites(p: &Plane, prov: usize, trees: &mut Vec<Sprite>, out: &mut Vec<Sprite>) {
    if prov == 0 || prov >= p.flags.len() || prov > p.capitals.len() {
        return;
    }
    let size = (p.scale.max(4.0) * 0.6) as i32;
    if size <= 0 {
        return;
    }
    for &(a, b) in p.bridges {
        let (a, b) = (a as usize, b as usize);
        if a != prov || b == 0 || b >= p.flags.len() || b > p.capitals.len() {
            continue;
        }
        if p.flags[a] & SEA != 0 && p.flags[b] & SEA != 0 {
            continue;
        }
        let (from, to) = if p.flags[a] & SEA == 0 {
            (a, b)
        } else {
            (b, a)
        };
        let Some((x, mut y)) = border_crossing(p, from, to) else {
            continue;
        };
        let (fx, fy) = p.capitals[from - 1];
        let (tx, ty) = p.capitals[to - 1];
        let dx = (tx - fx) as i32;
        let dy = (ty - fy) as i32;
        let steep = (dx * 2).abs() < (dy * 3).abs();
        let idx = if !steep {
            0x4b
        } else if (dx <= 0 || dy <= 0) && (dx >= 0 || dy >= 0) {
            0x4c
        } else {
            0x4d
        };
        if steep {
            y = (y as f64 - size as f64 * 0.5) as i32;
        }
        let clear = (size as f64 * 0.6) as i32;
        trees.retain(|s| (s.x - x).abs() + (s.y - y).abs() > clear);
        out.push(Sprite {
            x,
            y,
            size: size as i16,
            idx,
            layer: 0,
        });
    }
}

pub fn mountain_line_set(p: &Plane) -> HashSet<(u32, u32)> {
    p.mountain_lines
        .iter()
        .filter(|&&(a, b)| {
            (a as usize) < p.flags.len()
                && (b as usize) < p.flags.len()
                && p.flags[a as usize] & MOUNTAIN != 0
                && p.flags[b as usize] & MOUNTAIN != 0
        })
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .collect()
}

pub fn is_mountain_line(spec: i64) -> bool {
    spec as u64 & MOUNTAIN_LINE_SPECS != 0
}

pub fn sprite_bounds(sprites: &[Sprite]) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    for s in sprites {
        let b = s.rect();
        r = Some(match r {
            None => b,
            Some(a) => Rect {
                x0: a.x0.min(b.x0),
                y0: a.y0.min(b.y0),
                x1: a.x1.max(b.x1),
                y1: a.y1.max(b.y1),
            },
        });
    }
    r
}

pub fn order(sprites: &mut [Sprite]) {
    sprites.sort_by(|a, b| b.layer.cmp(&a.layer).then(b.y.cmp(&a.y)));
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

#[allow(clippy::too_many_arguments)]
fn blit(tex: &TexSet, s: &Sprite, dx: i32, dy: i32, w: i32, band: Rect, out: &mut [u8], row0: i32) {
    let Some(frame) = tex.frame(s.idx) else {
        return;
    };
    let size = s.size as i32;
    let r = s.rect();
    let r = Rect {
        x0: r.x0 + dx,
        y0: r.y0 + dy,
        x1: r.x1 + dx,
        y1: r.y1 + dy,
    };
    if !intersects(r, band) {
        return;
    }
    let img = frame.level_for(size);
    let inv = 1.0 / size as f32;
    for y in r.y0.max(band.y0)..=r.y1.min(band.y1) {
        let v = ((r.y1 - y) as f32 + 0.5) * inv;
        let row = ((y - row0) * w) as usize;
        for x in r.x0.max(band.x0)..=r.x1.min(band.x1) {
            let u = ((x - r.x0) as f32 + 0.5) * inv;
            let src = sample_bilinear(img, u, v);
            if src[3] == 0 {
                continue;
            }
            let o = (row + x as usize) * 4;
            let inv_a = 255 - src[3];
            for c in 0..4 {
                let v = src[c] + (out[o + c] as u32 * inv_a + 127) / 255;
                out[o + c] = v.min(255) as u8;
            }
        }
    }
}

pub fn draw_sprites(p: &Plane, tex: &TexSet, sprites: &[Sprite], rect: Rect, out: &mut [u8]) {
    let rect = Rect {
        x0: rect.x0.max(0),
        y0: rect.y0.max(0),
        x1: rect.x1.min(p.w - 1),
        y1: rect.y1.min(p.h - 1),
    };
    if rect.is_empty() {
        return;
    }
    let w = p.w;
    let stride = (w * 4) as usize;
    for y in rect.y0..=rect.y1 {
        let row = (y * w) as usize;
        out[(row + rect.x0 as usize) * 4..(row + rect.x1 as usize + 1) * 4].fill(0);
    }
    let visible: Vec<&Sprite> = sprites
        .iter()
        .filter(|s| {
            let r = s.rect();
            let wide = Rect {
                x0: r.x0 - if p.hwrap { w } else { 0 },
                y0: r.y0 - if p.vwrap { p.h } else { 0 },
                x1: r.x1 + if p.hwrap { w } else { 0 },
                y1: r.y1 + if p.vwrap { p.h } else { 0 },
            };
            intersects(wide, rect)
        })
        .collect();
    if visible.is_empty() {
        return;
    }
    let rows = rect.y1 - rect.y0 + 1;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 32) as i32;
    let chunk = ((rows + threads - 1) / threads).max(64);
    let first = rect.y0 as usize * stride;
    let last = (rect.y1 as usize + 1) * stride;
    let slice = &mut out[first..last];
    let visible = &visible;
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
            sc.spawn(move || {
                for s in visible {
                    blit(tex, s, 0, 0, w, band, mine, y);
                    if p.hwrap {
                        blit(tex, s, -w, 0, w, band, mine, y);
                        blit(tex, s, w, 0, w, band, mine, y);
                    }
                    if p.vwrap {
                        blit(tex, s, 0, -p.h, w, band, mine, y);
                        blit(tex, s, 0, p.h, w, band, mine, y);
                    }
                }
            });
            y = y1 + 1;
        }
    });
}
