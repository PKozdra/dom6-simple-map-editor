use crate::options::Options;
use crate::rng::{wrap_noise_cursor, CrtRng};
use crate::stage::{Control, Sink, Stage};
use crate::world::World;

pub const PAINT_STAGE_FUNCTIONS: [&str; 11] = [
    "height_to_color",
    "dirtify_random_map_circles",
    "draw_province_terrain_sprites",
    "draw_random_map_mountains",
    "draw_random_map_borders_final",
    "bubble_random_map_trees",
    "draw_map_for_real",
    "draw_random_map_top_borders",
    "mark_province_border_mask_rows",
    "illuminate_map_edges",
    "draw_all_random_terrains",
];

pub const ILLUMINATE_WIDTH: i32 = 0x19;
pub const OPAQUE: u8 = 0xff;
pub const MARGIN_PX: i32 = 0xc0;

pub const COLOR_RANDOM_MAP_ADVANCES: usize = 9;
pub const MAP_DIRT: i32 = 100;
pub const MAP_DIRT_SIZE: i32 = 100;
pub const TREE_DISK_ALPHA: i32 = 5;
pub const ROCK_DISK_ALPHA: i32 = 0x32;
pub const ROCK_DENSITY_NUMERATOR: f32 = 220000.0;
pub const MAP_NOISE: i32 = 15;
pub const DIRT_DISK_HEAD_ADVANCES: usize = 4;
pub const TREE_DISK_SPREAD: f32 = 1.2;
pub const SIZED_DISK_SPREAD: f32 = 0.5;
pub const SITE_DISK_ALPHA: i32 = 10;
pub const ROCK_DISK_SPREAD: f32 = 0.25;
pub const ROCK_DISK_RADIUS_PART: f32 = 0.6;

const TERRAIN_SEA: i64 = 0x4;
const TERRAIN_HIGHLAND: i64 = 0x10;
const TERRAIN_SWAMP: i64 = 0x20;
const TERRAIN_WASTE: i64 = 0x40;
const TERRAIN_FOREST: i64 = 0x80;
const TERRAIN_FARM: i64 = 0x100;
const TERRAIN_MANY_SITES: i64 = 0x400;
const TERRAIN_CAVE: i64 = 0x1000;
const TERRAIN_MOUNTAIN: i64 = 0x0080_0000;
const TERRAIN_HIDDEN: i64 = 0x1_0000_0000;
const TERRAIN_NO_MEADOW: i64 = 0x1000_0014_0000_1174;
const TERRAIN_NO_PLAIN_TREE: i64 = 0x14_0000_0040;
const TERRAIN_VARIANT: i64 = 0x4000_0000_0000_0000;
const BORDER_MOUNTAIN_MASK: i64 = 0x21;

pub fn apply_paint_writes(world: &mut World, _opts: &Options, sink: &mut dyn Sink) -> Control {
    let (cursor, crt) = compute_paint_state(world);
    if world.paint_noise_cursor.is_none() {
        world.paint_noise_cursor = Some(cursor);
    }
    world.noise.cursor = cursor;
    world.crt = crt;
    world.emit(Stage::Paint, sink)
}

pub fn compute_paint_noise_cursor(world: &World) -> usize {
    compute_paint_state(world).0
}

pub fn compute_paint_state(world: &World) -> (usize, CrtRng) {
    let mut pass = PaintPass {
        w: world,
        cursor: world.noise.cursor,
        crt: world.crt,
    };
    pass.color_random_map();
    pass.dirtify_random_map();
    pass.dirtify_random_map_noise_rows();
    for prov in 1..=world.nprov() as i32 {
        pass.draw_province_terrain_sprites(prov);
    }
    pass.draw_random_map_mountains();
    pass.draw_random_map_borders_final();
    pass.draw_map_for_real();
    pass.draw_random_map_borders_final();
    (pass.cursor, pass.crt)
}

struct PaintPass<'a> {
    w: &'a World,
    cursor: usize,
    crt: CrtRng,
}

impl PaintPass<'_> {
    fn advance(&mut self) -> f32 {
        self.cursor = wrap_noise_cursor(self.cursor);
        self.w.noise.values[self.cursor]
    }

    fn width(&self) -> i32 {
        self.w.w
    }

    fn height(&self) -> i32 {
        self.w.h
    }

    fn terrain(&self, prov: i32) -> i64 {
        if prov < 0 || prov as usize >= self.w.provinces.len() {
            0
        } else {
            self.w.provinces[prov as usize].terrain
        }
    }

    fn owner_at(&self, x: i32, y: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            0
        } else {
            i32::from(self.w.owner[(y * self.width() + x) as usize])
        }
    }

    fn height_at(&self, x: i32, y: i32) -> f32 {
        let cx = x.clamp(0, self.width() - 1);
        let cy = y.clamp(0, self.height() - 1);
        self.w.heights[(cy * self.width() + cx) as usize]
    }

    fn is_dry(&self, x: i32, y: i32) -> bool {
        self.height_at(x - 2, y) >= self.w.sea_level
            && self.height_at(x, y) >= self.w.sea_level
            && self.height_at(x + 2, y) >= self.w.sea_level
    }

    fn wrap_pixel(&self, x: i32, y: i32) -> (i32, i32) {
        let (mut x, mut y) = (x, y);
        if self.w.hwrap {
            if x < 0 {
                x += self.width();
            }
            if x >= self.width() {
                x -= self.width();
            }
        }
        if self.w.vwrap {
            if y < 0 {
                y += self.height();
            }
            if y >= self.height() {
                y -= self.height();
            }
        }
        (x, y)
    }

    fn get_province_value_cached(&self, _prov: i32, _idx: i32) -> i32 {
        0
    }

    fn get_border_flags(&self, a: i32, b: i32) -> i64 {
        if a < 1 || a as usize >= self.w.provinces.len() {
            return 0;
        }
        let p = &self.w.provinces[a as usize];
        for (i, n) in p.nbors.iter().enumerate() {
            if i32::from(*n) == b {
                return p.border[i];
            }
        }
        0
    }

    fn roll_dice(&mut self, n: i32, sides: i32) -> i32 {
        (0..n.max(0)).map(|_| self.crt.below(sides) + 1).sum()
    }

    fn color_random_map(&mut self) {
        for _ in 0..COLOR_RANDOM_MAP_ADVANCES {
            self.advance();
        }
    }

    fn dirtify_random_map(&mut self) {
        if MAP_DIRT <= 0 || MAP_DIRT_SIZE <= 0 {
            return;
        }
        let lines = self.height();
        let spacing = self.w.spacing;
        let blobs =
            ((0.02 / (spacing * spacing)) * (lines * self.width() * MAP_DIRT) as f32) as i32;
        if blobs <= 0 {
            return;
        }
        for _ in 0..blobs {
            let nx = self.advance();
            let ny = self.advance();
            let x = (self.width() as f32 * nx) as i32;
            let y = (lines as f32 * ny) as i32;
            if x < 0 || x >= self.width() || y < 0 || y >= self.height() {
                continue;
            }
            let prov = self.owner_at(x, y);
            if prov <= 0 || (self.terrain(prov) & TERRAIN_SEA) != 0 {
                continue;
            }
            let n = self.advance();
            let r = ((spacing * 0.5 * n * MAP_DIRT_SIZE as f32) * 0.01) as i32;
            self.paint_randomized_map_color_disk(x, y, r);
        }
    }

    fn draw_random_map_borders_final(&mut self) {
        self.crt.rand();
    }

    fn draw_map_for_real(&mut self) {}

    fn dirtify_random_map_noise_rows(&mut self) {
        if MAP_NOISE <= 0 {
            return;
        }
        for _ in 0..self.height() {
            self.crt.rand();
        }
    }

    fn paint_randomized_map_color_disk(&mut self, cx: i32, cy: i32, r: i32) {
        for _ in 0..DIRT_DISK_HEAD_ADVANCES {
            self.advance();
        }
        let r2 = (r * r) as f32;
        let mut yy = cy - r;
        while yy <= cy + r {
            let mut xx = cx - r;
            while xx <= cx + r {
                let d2 = ((xx - cx) * (xx - cx) + (yy - cy) * (yy - cy)) as f32;
                if d2 < r2 {
                    let (px, py) = self.wrap_pixel(xx, yy);
                    if self.height_at(px, py) >= self.w.sea_level {
                        self.advance();
                    }
                }
                xx += 1;
            }
            yy += 1;
        }
    }

    fn blend_randomized_map_color_disk(&mut self, cx: f32, cy: f32, radius: f32, alpha: i32) {
        let mut ext = alpha as f32 * radius * 0.066_666_67;
        if radius <= ext {
            ext = radius;
        }
        if ext <= 0.0 {
            return;
        }
        let mut dy = 0i32;
        loop {
            let mut dx = 0i32;
            loop {
                if ((dx * dx + dy * dy) as f32) < ext * ext {
                    let xm = (cx - dx as f32) as i32;
                    let ym = (cy - dy as f32) as i32;
                    let xp = (dx as f32 + cx) as i32;
                    let yp = (dy as f32 + cy) as i32;
                    self.blend_random_map_rgb_pixel(xm, ym);
                    if dx != 0 {
                        self.blend_random_map_rgb_pixel(xp, ym);
                    }
                    if dy != 0 {
                        self.blend_random_map_rgb_pixel(xm, yp);
                        if dx != 0 {
                            self.blend_random_map_rgb_pixel(xp, yp);
                        }
                    }
                }
                dx += 1;
                if (dx as f32) >= ext {
                    break;
                }
            }
            dy += 1;
            if (dy as f32) >= ext {
                break;
            }
        }
    }

    fn blend_random_map_rgb_pixel(&mut self, x: i32, y: i32) {
        let (x, y) = self.wrap_pixel(x, y);
        if self.height_at(x, y) >= self.w.sea_level {
            self.advance();
        }
    }

    fn accept_pixel_by_region_proximity(&mut self, x: i32, y: i32, prov: i32, margin: i32) -> bool {
        let x1 = x.clamp(0, self.width() - 1);
        let y1 = y.clamp(0, self.height() - 1);
        if self.owner_at(x1, y1) == prov || margin > 9999 {
            return false;
        }
        if self.crt.below(100) < 0x32 {
            return true;
        }
        let mut best = 99999;
        let mut yy = y1 - margin;
        while yy <= y1 + margin {
            let mut xx = x1 - margin;
            while xx <= x1 + margin {
                if self.owner_at(xx, yy) == prov {
                    let d = (yy - y1).abs() + (xx - x1).abs();
                    if d <= best {
                        best = d;
                    }
                }
                xx += 1;
            }
            yy += 1;
        }
        if best <= margin {
            return self.crt.below(100) < (best * 0x32) / margin.max(1);
        }
        true
    }

    fn blend_tree_disk(&mut self, x: i32, y: i32, size: i32, noise: f32) {
        let radius = ((noise - 0.5) * TREE_DISK_SPREAD + 1.0) * size as f32;
        self.blend_randomized_map_color_disk(x as f32, y as f32, radius, TREE_DISK_ALPHA);
    }

    fn tree_color_disk(&mut self, x: i32, y: i32, size: i32) {
        let n = self.advance();
        self.blend_tree_disk(x, y, size, n);
    }

    fn sized_tree_color_disk(&mut self, x: i32, y: i32, size: i32) {
        self.advance();
        let n = self.advance();
        let radius = ((n - 0.5) * SIZED_DISK_SPREAD + 1.0) * size as f32;
        self.blend_randomized_map_color_disk(x as f32, y as f32, radius, TREE_DISK_ALPHA);
    }

    fn plain_color_disk(&mut self, x: i32, y: i32, size: i32) {
        let n = self.advance();
        let radius = ((n - 0.5) * SIZED_DISK_SPREAD + 1.0) * size as f32;
        self.blend_randomized_map_color_disk(x as f32, y as f32, radius, TREE_DISK_ALPHA);
    }

    fn site_color_disk(&mut self, x: i32, y: i32, size: i32) {
        let n = self.advance();
        let radius = ((n - 0.5) * SIZED_DISK_SPREAD + 1.0) * (size as f32 * 0.5);
        self.blend_randomized_map_color_disk(x as f32, y as f32, radius, SITE_DISK_ALPHA);
    }

    fn consume_sprite_selection(&mut self, sel: i32, size: i32, winter: bool, x: i32, y: i32) {
        match sel {
            -1 => {
                self.crt.below(6);
                self.tree_color_disk(x, y, size);
            }
            -2 => {
                if self.crt.below(100) > 0x20 {
                    self.crt.below(2);
                }
                self.tree_color_disk(x, y, size);
            }
            -3 => {
                self.crt.below(5);
                self.tree_color_disk(x, y, size);
            }
            -4 => {
                self.crt.below(6);
                if self.crt.below(100) < 1 {
                    self.crt.below(3);
                }
                self.tree_color_disk(x, y, size);
            }
            -6 => {
                if self.crt.below(100) >= 0x50 {
                    if self.crt.below(100) < 0x23 {
                        self.crt.below(3);
                    } else {
                        self.crt.below(2);
                    }
                    self.advance();
                }
                self.plain_color_disk(x, y, size);
            }
            -7 => {
                if !winter {
                    self.crt.below(100);
                }
            }
            -8 => {
                if self.crt.below(100) >= 0x4b {
                    self.crt.below(2);
                }
            }
            -9 => {
                self.crt.below(2);
            }
            -10 => {
                self.plain_color_disk(x, y, size);
            }
            -11 => {
                self.crt.below(3);
            }
            -12 => {
                self.crt.below(4);
                self.sized_tree_color_disk(x, y, size);
            }
            -13 => {
                self.crt.below(6);
                self.sized_tree_color_disk(x, y, size);
            }
            -14 => {
                self.crt.below(4);
                self.sized_tree_color_disk(x, y, size);
            }
            -15 => {
                self.crt.below(2);
                self.sized_tree_color_disk(x, y, size);
            }
            -16 => {
                self.crt.below(3);
                self.sized_tree_color_disk(x, y, size);
            }
            s => {
                let u = s as u32;
                if s == 0x30 || u <= 0x15 {
                    self.plain_color_disk(x, y, size);
                } else if u == 0x34 || u.wrapping_sub(0x34) <= 8 {
                    self.site_color_disk(x, y, size);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_forest_sprites(
        &mut self,
        prov: i32,
        cx: i32,
        cy: i32,
        spread: i32,
        count: i32,
        sel: i32,
        size: i32,
        margin: i32,
        layer: i32,
        winter: bool,
        cluster: i32,
        cluster_scale: f32,
    ) {
        if count <= 0 {
            return;
        }
        let mut extra = 0i32;
        let mut x = 0i32;
        let mut y = 0i32;
        for _ in 0..count {
            if extra < 1 {
                x = cx - spread / 2 + self.crt.below(spread);
                y = cy - spread / 2 + self.crt.below(spread);
                if cluster > 0 {
                    extra = self.roll_dice(cluster, 2);
                }
            } else {
                extra -= 1;
                let span = (size as f32 * cluster_scale) as i32;
                let half = size as f32 * cluster_scale * 0.5;
                let jx = self.crt.below(span);
                x = ((jx as f32 - half) + x as f32) as i32;
                let jy = self.crt.below(span);
                y = ((jy as f32 - half) + y as f32) as i32;
            }
            let (wx, wy) = self.wrap_pixel(x, y);
            x = wx;
            y = wy;
            if layer == 0 {
                if !self.is_dry(x, y) {
                    continue;
                }
                if self.accept_pixel_by_region_proximity(x, y, prov, margin) {
                    continue;
                }
            }
            self.consume_sprite_selection(sel, size, winter, x, y);
        }
    }
}

impl PaintPass<'_> {
    #[allow(clippy::too_many_arguments)]
    fn allocate_random_map_tree(
        &mut self,
        prov: i32,
        cx: i32,
        cy: i32,
        spread: i32,
        rows: i32,
        size: i32,
        margin: i32,
    ) {
        let mut x;
        let mut y;
        let mut tries = 0;
        loop {
            x = cx - spread / 2 + self.crt.below(spread);
            y = cy - spread / 2 + self.crt.below(spread);
            let (wx, wy) = self.wrap_pixel(x, y);
            x = wx.clamp(0, self.width() - 1);
            y = wy.clamp(0, self.height() - 1);
            if self.is_dry(x, y) && !self.accept_pixel_by_region_proximity(x, y, prov, margin) {
                break;
            }
            tries += 1;
            if tries > 1000 {
                break;
            }
        }
        let back = size as f32 * 0.15 * rows as f32;
        let x0 = (x as f32 - back) as i32;
        let mut cur_y = (y as f32 - back) as i32;
        let (wx0, wy0) = self.wrap_pixel(x0, cur_y);
        let x0 = wx0.clamp(0, self.width() - 1);
        cur_y = wy0.clamp(0, self.height() - 1);
        let mut cur_x = x0;
        let span = (rows - 1).max(1) * (rows + 1);
        for k in 0..span {
            cur_x = (cur_x as f32 + size as f32 * 0.18) as i32;
            if k % (rows + 1) == rows {
                let n = self.advance();
                cur_y = (size as f32 * n * 0.1 + size as f32 * 0.18 + cur_y as f32) as i32;
                cur_x = x0;
            }
            let (wx, wy) = self.wrap_pixel(cur_x, cur_y);
            cur_x = wx.clamp(0, self.width() - 1);
            cur_y = wy.clamp(0, self.height() - 1);
            if !self.is_dry(cur_x, cur_y) {
                continue;
            }
            if self.accept_pixel_by_region_proximity(cur_x, cur_y, prov, margin) {
                continue;
            }
            self.advance();
            self.advance();
            self.advance();
        }
    }

    fn draw_province_terrain_sprites(&mut self, prov: i32) {
        let flags = self.terrain(prov);
        if flags & TERRAIN_HIDDEN != 0 {
            return;
        }
        let spacing = self.w.spacing;
        let mut base = spacing;
        if base <= 4.0 {
            base = 4.0;
        }
        let px = self.w.provinces[prov as usize].x as i16 as i32;
        let py = self.w.provinces[prov as usize].y as i16 as i32;

        if flags & TERRAIN_CAVE != 0 {
            let count;
            let sel;
            let spread;
            let size;
            if flags & TERRAIN_FOREST != 0 {
                let c = self.crt.below(0x96);
                let sp = (spacing * 4.0) as i32;
                self.draw_forest_sprites(
                    prov,
                    px,
                    py,
                    sp,
                    c + 300,
                    -12,
                    (base * 0.15) as i32,
                    1,
                    0,
                    false,
                    0,
                    0.0,
                );
                count = self.crt.below(10) + 0x14;
                sel = -15;
                spread = sp;
                size = (base * 0.6) as i32;
            } else if flags & TERRAIN_HIGHLAND == 0 {
                if flags & TERRAIN_SWAMP != 0 {
                    let c = self.crt.below(0x19) + 0x32;
                    self.draw_forest_sprites(
                        prov,
                        px,
                        py,
                        (spacing * 5.0) as i32,
                        c,
                        -14,
                        (base * 0.35) as i32,
                        1,
                        0,
                        false,
                        0,
                        0.0,
                    );
                    return;
                }
                count = self.crt.below(10) + 0x14;
                sel = -14;
                spread = (spacing * 5.0) as i32;
                size = (base * 0.25) as i32;
            } else {
                let sp = (spacing * 5.0) as i32;
                let c = self.crt.below(0x96);
                self.draw_forest_sprites(
                    prov,
                    px,
                    py,
                    sp,
                    c + 300,
                    -13,
                    (base * 0.15) as i32,
                    1,
                    0,
                    false,
                    4,
                    2.5,
                );
                let c2 = self.crt.below(0x96);
                self.draw_forest_sprites(
                    prov,
                    px,
                    py,
                    sp,
                    c2 + 300,
                    -13,
                    (base * 0.1) as i32,
                    1,
                    0,
                    false,
                    0,
                    0.0,
                );
                count = self.crt.below(10) + 0x14;
                sel = -13;
                spread = sp;
                size = (base * 0.3) as i32;
            }
            self.draw_forest_sprites(prov, px, py, spread, count, sel, size, 1, 0, false, 0, 0.0);
            return;
        }

        if flags & TERRAIN_FARM != 0 {
            let hot = self.get_province_value_cached(prov, 2) > 0;
            let groups = self.crt.below(4) + 3;
            for _ in 0..groups {
                let rows = self.crt.below(4) + 9;
                self.allocate_random_map_tree(
                    prov,
                    px,
                    py,
                    spacing as i32,
                    rows,
                    (base * 0.3) as i32,
                    0,
                );
            }
            let mut size = base as i32;
            let sel;
            let count;
            if flags & 0x2 == 0 {
                count = self.crt.below(6) + 2;
                sel = -10;
            } else {
                self.draw_forest_sprites(
                    prov,
                    px,
                    (py as f32 - base * 0.5) as i32,
                    0,
                    1,
                    0x45,
                    size,
                    0,
                    0,
                    hot,
                    0,
                    0.0,
                );
                count = self.crt.below(2);
                size = (base * 0.5) as i32;
                sel = 0x31;
            }
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 1.25) as i32,
                count,
                sel,
                size,
                (spacing * 0.25) as i32,
                0,
                hot,
                0,
                0.0,
            );
        }

        let mut count = 1;
        let hot = self.get_province_value_cached(prov, 2) > 0;
        let variant = flags & TERRAIN_VARIANT != 0;
        if flags & TERRAIN_FOREST != 0 {
            let c = self.crt.below(0x96);
            let sel = if variant { -3 } else { -1 };
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 3.0) as i32,
                c + 0xfa,
                sel,
                (base * 0.5) as i32,
                0,
                0,
                hot,
                0,
                0.0,
            );
        }
        if flags & TERRAIN_HIGHLAND != 0 {
            let c = self.crt.below(0x19);
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 3.0) as i32,
                c + 0x4b,
                -11,
                (base * 0.75) as i32,
                0,
                0,
                hot,
                6,
                1.0,
            );
        }
        if flags & TERRAIN_SWAMP != 0 {
            let margin = (spacing * 0.05) as i32;
            let c = self.crt.below(200);
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 3.0) as i32,
                c + 300,
                -6,
                (base * 0.4) as i32,
                margin,
                0,
                hot,
                0,
                0.0,
            );
        }
        if flags & TERRAIN_WASTE != 0 {
            let c = self.crt.below(6);
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 1.25) as i32,
                c + 2,
                -2,
                (base * 0.3) as i32,
                0,
                0,
                hot,
                0,
                0.0,
            );
        }

        let mut site_sprite = None;
        let mut spread = spacing as i32;
        let mut size = 0;
        if flags & TERRAIN_MANY_SITES != 0 {
            site_sprite = Some(self.crt.below(9) + 0x34);
            size = (base * 0.5) as i32;
        } else if flags & 0x2000 != 0 {
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 0.25) as i32,
                1,
                0x34,
                (base * 0.85) as i32,
                0,
                0,
                hot,
                0,
                0.0,
            );
            site_sprite = Some(-4);
            size = (base * 0.7) as i32;
            count = 5;
            spread = (spacing * 0.75) as i32;
        } else if flags & 0x4000 != 0 {
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 0.25) as i32,
                1,
                0x35,
                (base * 0.85) as i32,
                0,
                0,
                hot,
                0,
                0.0,
            );
            site_sprite = Some(-4);
            size = (base * 0.7) as i32;
            count = 10;
            spread = (spacing * 0.75) as i32;
        } else if flags & 0x8000 != 0 {
            if self.crt.below(100) < 0x32 {
                count = 0x1e;
                size = (base * 0.1) as i32;
                site_sprite = Some(-9);
            } else {
                size = (base * 0.5) as i32;
                site_sprite = Some(0x36);
            }
            spread = (spacing * 0.75) as i32;
        } else if flags & 0x10000 != 0 {
            site_sprite = Some(0x37);
            size = (base * 0.4) as i32;
        } else if flags & 0x20000 != 0 {
            site_sprite = Some(0x38);
            size = (base * 0.5) as i32;
        } else if flags & 0x40000 != 0 {
            site_sprite = Some(0x39);
            size = (base * 0.5) as i32;
        } else if flags & 0x80000 != 0 {
            site_sprite = Some(0x3a);
            size = (base * 0.5) as i32;
        } else if flags & 0x20_0000 != 0 {
            site_sprite = Some(0x3b);
            size = (base * 0.5) as i32;
        } else if flags & 0x40_0000 != 0 {
            site_sprite = Some(0x3c);
            size = (base * 0.5) as i32;
        }
        if let Some(sel) = site_sprite {
            self.draw_forest_sprites(prov, px, py, spread, count, sel, size, 0, 0, hot, 0, 0.0);
        }

        if flags & TERRAIN_NO_PLAIN_TREE == 0 {
            let (c, sel) = if variant {
                (self.crt.below(6), -3)
            } else {
                (self.crt.below(4), -1)
            };
            self.draw_forest_sprites(
                prov,
                px,
                py,
                (spacing * 3.0) as i32,
                c,
                sel,
                (base * 0.5) as i32,
                0,
                0,
                hot,
                0,
                0.0,
            );
        }
        if flags & TERRAIN_NO_MEADOW != 0 {
            return;
        }
        let hot = self.get_province_value_cached(prov, 2) > 0;
        let c = self.crt.below(3);
        self.draw_forest_sprites(
            prov,
            px,
            py,
            (spacing * 3.0) as i32,
            c,
            -7,
            (base * 0.1) as i32,
            0,
            0,
            hot,
            0,
            0.0,
        );
        let c = self.crt.below(5) + 1;
        self.draw_forest_sprites(
            prov,
            px,
            py,
            (spacing * 3.0) as i32,
            c,
            -8,
            (base * 0.15) as i32,
            0,
            0,
            hot,
            0,
            0.0,
        );
        for _ in 0..4 {
            let meadow = self.crt.below(100) < 0x32;
            let clover = !meadow && self.crt.below(100) < 0x32;
            if !meadow && !clover {
                continue;
            }
            let ny = self.advance();
            let nx = self.advance();
            let c = self.crt.below(0x28);
            let ox = (nx - 0.5) * spacing;
            let ox = ox + ox;
            let oy = (ny - 0.5) * spacing + py as f32;
            let (sel, spread, size) = if meadow {
                (-7, base * 0.6, (base * 0.1) as i32)
            } else {
                (-8, base, (base * 0.15) as i32)
            };
            self.draw_forest_sprites(
                prov,
                (px as f32 + ox) as i32,
                oy as i32,
                spread as i32,
                c + 10,
                sel,
                size,
                0,
                0,
                hot,
                0,
                0.0,
            );
        }
    }

    fn is_mountain_border_near_pixel(&self, x: i32, y: i32, radius: i32) -> bool {
        if x < 0 || x >= self.width() || y < 0 || y >= self.height() {
            return false;
        }
        let p = self.owner_at(x, y);
        if p < 1 || self.terrain(p) & TERRAIN_MOUNTAIN == 0 {
            return false;
        }
        let r = radius.max(1);
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
            let q = self.owner_at(x + dx, y + dy);
            if q == p || self.terrain(q) & TERRAIN_MOUNTAIN == 0 {
                continue;
            }
            if self.get_border_flags(p, q) & BORDER_MOUNTAIN_MASK != 0 {
                return true;
            }
        }
        false
    }

    fn draw_random_map_mountains(&mut self) {
        let mut base = self.w.spacing;
        if base <= 4.0 {
            base = 4.0;
        }
        let mut density = ROCK_DENSITY_NUMERATOR / (base * base);
        if density <= 1.0 {
            density = 1.0;
        }
        for y in 0..self.height() {
            let mut x = y & 1;
            while x < self.width() {
                let p = self.owner_at(x, y);
                let t = self.terrain(p);
                if p > 0 && t & TERRAIN_HIDDEN == 0 && t & TERRAIN_MOUNTAIN != 0 {
                    let heat = self.get_province_value_cached(p, 2);
                    let roll = self.crt.below(0x9c4);
                    if (roll as f32) < density {
                        let n = self.advance();
                        let r = ((n + 1.0) * 0.5 * base * 0.22) as i32;
                        if self.is_mountain_border_near_pixel(x, y, r) {
                            if self.crt.below(100) < 0x55 {
                                self.crt.below(4);
                            } else {
                                let k = self.crt.below(7) + 4;
                                if k == 7 || k == 10 {
                                    self.crt.below(6);
                                }
                            }
                            if heat < 1 {
                                let n2 = self.advance();
                                let radius = ((n2 - 0.5) * ROCK_DISK_SPREAD + 1.0)
                                    * (base * ROCK_DISK_RADIUS_PART);
                                self.blend_randomized_map_color_disk(
                                    x as f32,
                                    y as f32,
                                    radius,
                                    ROCK_DISK_ALPHA,
                                );
                            }
                        }
                    }
                }
                x += 2;
            }
        }
    }
}

pub fn illuminate_map_edges(world: &mut World, board_w: i32, board_h: i32) -> Vec<u8> {
    let mut alpha = vec![OPAQUE; (world.w * world.h).max(0) as usize];
    if let Some(c) = world.paint_noise_cursor {
        world.noise.cursor = c % world.noise.values.len();
    }
    let ox = if world.hwrap { 0 } else { MARGIN_PX };
    let oy = if world.vwrap { 0 } else { MARGIN_PX };
    let width = ILLUMINATE_WIDTH;
    let step = (255 / (width >> 2)).max(1) as f32;
    let stride = world.w;
    if !world.hwrap {
        for near in [true, false] {
            let mut walk = EdgeWalk::new(world.noise.cursor, width);
            for y in 0..board_h {
                let n = walk.ramp();
                let mut a = 255.0f32;
                for k in (0..=n).rev() {
                    a -= step;
                    let v = (a as i32).clamp(0, 255) as u8;
                    let x = if near { k } else { board_w - 1 - k };
                    let i = ((oy + y) * stride + ox + x) as usize;
                    if v < alpha[i] {
                        alpha[i] = v;
                    }
                }
                walk.advance(&world.noise.values);
            }
            world.noise.cursor = walk.cursor;
        }
    }
    if !world.vwrap {
        for near in [true, false] {
            let mut walk = EdgeWalk::new(world.noise.cursor, width);
            for x in 0..board_w {
                let n = walk.ramp();
                let mut a = 255.0f32;
                for k in (0..=n).rev() {
                    a -= step;
                    let v = (a as i32).clamp(0, 255) as u8;
                    let y = if near { k } else { board_h - 1 - k };
                    let i = ((oy + y) * stride + ox + x) as usize;
                    if v < alpha[i] {
                        alpha[i] = v;
                    }
                }
                walk.advance(&world.noise.values);
            }
            world.noise.cursor = walk.cursor;
        }
    }
    alpha
}

pub struct EdgeWalk {
    cursor: usize,
    colour: f32,
    ramp: f32,
    width: f32,
    floor: f32,
    ceil: f32,
}

impl EdgeWalk {
    pub fn new(cursor: usize, width: i32) -> Self {
        let half = (width / 2) as f32;
        EdgeWalk {
            cursor,
            colour: half,
            ramp: half,
            width: width as f32,
            floor: (width >> 2) as f32,
            ceil: ((width * 3) >> 2) as f32,
        }
    }

    pub fn ramp(&self) -> i32 {
        self.ramp.max(1.0) as i32
    }

    pub fn advance(&mut self, noise: &[f32]) {
        let len = noise.len();
        let i = if self.cursor + 1 < len {
            self.cursor + 1
        } else {
            0
        };
        let j = if i + 1 < len { i + 1 } else { 0 };
        self.cursor = j;
        let a = (noise[i] - 0.5) * self.width * 0.1 + self.colour;
        let b = (noise[j] - 0.5) * self.width * 0.1 + self.ramp;
        self.colour = if a >= 1.0 { a.min(self.width) } else { 1.0 };
        self.ramp = if b >= self.floor {
            b.min(self.ceil)
        } else {
            self.floor
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{NoSink, RecordSink};
    use crate::world::Province;

    fn seeded_world() -> World {
        let mut w = World::new(20260908);
        w.w = 8;
        w.h = 6;
        w.hwrap = true;
        w.sea_level = 100.0;
        w.spacing = 12.0;
        w.heights = (0..48).map(|i| 90.0 + i as f32).collect();
        w.owner = (0..48).map(|i| (i % 5) as i16).collect();
        w.provinces = vec![Province::default()];
        for i in 1..5i32 {
            w.provinces.push(Province {
                x: i,
                y: i,
                terrain: i as i64,
                nbors: vec![],
                border: vec![],
            });
        }
        w
    }

    #[test]
    fn paint_leaves_owner_height_and_provinces_untouched() {
        let mut w = seeded_world();
        let before_owner = w.owner.clone();
        let before_heights = w.heights.clone();
        let before_provinces = w.provinces.clone();
        assert_eq!(
            apply_paint_writes(&mut w, &Options::default(), &mut NoSink),
            Control::Continue
        );
        assert_eq!(w.owner, before_owner);
        assert_eq!(w.heights, before_heights);
        assert_eq!(w.provinces, before_provinces);
    }

    #[test]
    fn paint_leaves_the_pool_alone_and_publishes_its_own_end_state() {
        let mut w = seeded_world();
        let pool = w.pool;
        let (cursor, crt) = compute_paint_state(&w);
        apply_paint_writes(&mut w, &Options::default(), &mut NoSink);
        assert_eq!(w.pool, pool);
        assert_eq!(w.noise.cursor, cursor);
        assert_eq!(w.crt, crt);
        assert_eq!(w.paint_noise_cursor, Some(cursor));
    }

    #[test]
    fn paint_emits_one_stage_whose_hash_matches_upsample() {
        let mut w = seeded_world();
        let upsample = w.hash_for(Stage::Upsample);
        let mut sink = RecordSink::default();
        apply_paint_writes(&mut w, &Options::default(), &mut sink);
        assert_eq!(sink.entries.len(), 1);
        assert_eq!(sink.entries[0].0, Stage::Paint);
        assert_eq!(sink.entries[0].1, 0);
        assert_eq!(sink.entries[0].2, upsample);
    }

    fn edge_world(hwrap: bool, vwrap: bool) -> World {
        let mut w = World::new(1234);
        let ox = if hwrap { 0 } else { MARGIN_PX };
        let oy = if vwrap { 0 } else { MARGIN_PX };
        w.w = 128 + 2 * ox;
        w.h = 128 + 2 * oy;
        w.hwrap = hwrap;
        w.vwrap = vwrap;
        w.owner = vec![1i16; (w.w * w.h) as usize];
        w.heights = vec![0.0; (w.w * w.h) as usize];
        w
    }

    #[test]
    fn wrapped_axes_get_no_ramp() {
        let mut w = edge_world(true, true);
        let a = illuminate_map_edges(&mut w, 128, 128);
        assert!(a.iter().all(|v| *v == OPAQUE));
    }

    #[test]
    fn open_axis_ramp_hugs_the_board_edges_only() {
        let mut w = edge_world(false, true);
        let a = illuminate_map_edges(&mut w, 128, 128);
        let faded: Vec<usize> = (0..a.len()).filter(|i| a[*i] < 0x32).collect();
        assert!(!faded.is_empty());
        for i in faded {
            let x = i as i32 % w.w - MARGIN_PX;
            assert!((0..128).contains(&x));
            let depth = x.min(127 - x);
            assert!(depth <= 14, "depth {depth}");
        }
    }

    #[test]
    fn the_first_three_columns_of_an_open_edge_always_fade() {
        let mut w = edge_world(false, true);
        let a = illuminate_map_edges(&mut w, 128, 128);
        for y in 0..128 {
            for d in 0..3 {
                let l = (y * w.w + MARGIN_PX + d) as usize;
                let r = (y * w.w + MARGIN_PX + 127 - d) as usize;
                assert!(a[l] < 0x32 && a[r] < 0x32);
            }
        }
    }

    #[test]
    fn ramp_run_lengths_stay_between_the_walk_clamps() {
        let mut w = edge_world(false, true);
        w.paint_noise_cursor = Some(1533);
        let a = illuminate_map_edges(&mut w, 128, 128);
        let run = |y: i32| {
            (0..128)
                .take_while(|d| a[(y * w.w + MARGIN_PX + d) as usize] < 0x32)
                .count()
        };
        assert_eq!(run(0), 9);
        for y in 0..128 {
            assert!((3..=15).contains(&run(y)), "row {y} run {}", run(y));
        }
    }

    #[test]
    fn the_entry_cursor_can_be_pinned() {
        let mut a = edge_world(false, true);
        let mut b = edge_world(false, true);
        a.paint_noise_cursor = Some(1533);
        b.noise.cursor = 1533;
        assert_eq!(
            illuminate_map_edges(&mut a, 128, 128),
            illuminate_map_edges(&mut b, 128, 128)
        );
    }

    #[test]
    fn both_axes_open_consumes_four_walks_of_noise() {
        let mut w = edge_world(false, false);
        let before = w.noise.cursor;
        illuminate_map_edges(&mut w, 128, 128);
        assert_eq!(
            w.noise.cursor,
            (before + 2 * 128 * 4) % crate::rng::NOISE_LEN
        );
    }

    #[test]
    fn paint_stage_function_list_is_the_surveyed_set() {
        assert_eq!(PAINT_STAGE_FUNCTIONS.len(), 11);
        assert!(PAINT_STAGE_FUNCTIONS.contains(&"draw_map_for_real"));
    }
}
