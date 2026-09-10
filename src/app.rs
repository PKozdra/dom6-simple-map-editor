use crate::generator_panel::{GeneratedMap, GeneratorPanel};
use crate::keycap;

const KEY_HELP: &[(&[&str], &str)] = &[
    (&["Ctrl", "Z"], "Undo"),
    (&["Ctrl", "Y"], "Redo"),
    (&["Ctrl", "S"], "Save"),
    (&["Ctrl", "O"], "Open"),
    (&["Ctrl", "N"], "Random map generator"),
    (&["G"], "Generate a random map"),
    (&["F4"], "Random terrain"),
    (&["S"], "Select"),
    (
        &["L"],
        "Link; right-click a connected province to change the border type",
    ),
    (&["P"], "Paint area"),
    (&["H"], "Heights"),
    (&["Tab"], "Next tool"),
    (&["PgUp"], "Previous plane"),
    (&["PgDn"], "Next plane"),
    (&["Home"], "Fit the map"),
    (&["Esc"], "Back"),
    (&["F1"], "Help"),
];
use crate::layout::{self, Kind, Layout, Sheet};
use crate::mapfile::plane_file_name;
use crate::project::{union, wrap_delta, wrap_point, FlagOp, HeightOp, PlaneDoc, Project};
use crate::render::{Options, Rect};
use crate::settings::Settings;
use crate::terrain::{self, *};
use crate::textures::{decode_png, TexSet};
use crate::theme;
use egui::{Align2, Color32, FontId, Key, PointerButton, Pos2, Sense, StrokeKind, Vec2};
use std::path::{Path, PathBuf};

const TILE: usize = 1024;

const ICON_CELL: f32 = 48.0;
pub const REPO_URL: &str = "https://github.com/PKozdra/dom6-simple-map-editor";

struct TileGrid {
    w: usize,
    h: usize,
    ox: usize,
    oy: usize,
    map_h: usize,
    cols: usize,
    rows: usize,
    handles: Vec<Option<egui::TextureHandle>>,
    dirty: Vec<Option<[usize; 4]>>,
}

const FULL_TILE: [usize; 4] = [0, 0, usize::MAX, usize::MAX];

impl TileGrid {
    fn new(w: i32, h: i32) -> TileGrid {
        TileGrid::new_at(0, 0, w as usize, h as usize, h as usize)
    }

    fn new_at(ox: usize, oy: usize, w: usize, h: usize, map_h: usize) -> TileGrid {
        let cols = w.div_ceil(TILE).max(1);
        let rows = h.div_ceil(TILE).max(1);
        TileGrid {
            w,
            h,
            ox,
            oy,
            map_h,
            cols,
            rows,
            handles: (0..cols * rows).map(|_| None).collect(),
            dirty: vec![Some(FULL_TILE); cols * rows],
        }
    }

    fn mark(&mut self, r: Rect) {
        let mh = self.map_h as i64;
        let sy0 = (mh - 1 - r.y1 as i64).max(0) - self.oy as i64;
        let sy1 = (mh - 1 - r.y0 as i64).max(0) - self.oy as i64;
        let sx0 = (r.x0 as i64).max(0) - self.ox as i64;
        let sx1 = (r.x1 as i64).max(0) - self.ox as i64;
        let sx0 = sx0.max(0) as usize;
        let sy0 = sy0.max(0) as usize;
        if sx1 < 0 || sy1 < 0 {
            return;
        }
        let sx1 = (sx1 as usize).min(self.w.saturating_sub(1));
        let sy1 = (sy1 as usize).min(self.h.saturating_sub(1));
        if sx0 > sx1 || sy0 > sy1 {
            return;
        }
        for ty in sy0 / TILE..=(sy1 / TILE).min(self.rows.saturating_sub(1)) {
            for tx in sx0 / TILE..=(sx1 / TILE).min(self.cols.saturating_sub(1)) {
                let i = ty * self.cols + tx;
                let local = [
                    sx0.saturating_sub(tx * TILE),
                    sy0.saturating_sub(ty * TILE),
                    (sx1 - tx * TILE).min(TILE - 1),
                    (sy1 - ty * TILE).min(TILE - 1),
                ];
                self.dirty[i] = Some(match self.dirty[i] {
                    None => local,
                    Some(d) => [
                        d[0].min(local[0]),
                        d[1].min(local[1]),
                        d[2].max(local[2]),
                        d[3].max(local[3]),
                    ],
                });
            }
        }
    }

    fn mark_all(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = Some(FULL_TILE));
    }

    fn upload(&mut self, ctx: &egui::Context, rgba: &[u8], name: &str, premultiplied: bool) {
        let w = self.w;
        let h = self.h;
        for ty in 0..self.rows {
            for tx in 0..self.cols {
                let i = ty * self.cols + tx;
                let Some(d) = self.dirty[i].take() else {
                    continue;
                };
                let x0 = tx * TILE;
                let y0 = ty * TILE;
                let tw = (w - x0).min(TILE);
                let th = (h - y0).min(TILE);
                let partial = self.handles[i].is_some() && d != FULL_TILE;
                let (px, py, pw, ph) = if partial {
                    let px = d[0].min(tw - 1);
                    let py = d[1].min(th - 1);
                    (px, py, d[2].min(tw - 1) - px + 1, d[3].min(th - 1) - py + 1)
                } else {
                    (0, 0, tw, th)
                };
                let mut buf = vec![0u8; pw * ph * 4];
                for row in 0..ph {
                    let eng_y = h - 1 - (y0 + py + row);
                    let src = (eng_y * w + x0 + px) * 4;
                    buf[row * pw * 4..(row + 1) * pw * 4].copy_from_slice(&rgba[src..src + pw * 4]);
                }
                let image = if premultiplied {
                    egui::ColorImage::from_rgba_premultiplied([pw, ph], &buf)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied([pw, ph], &buf)
                };
                let opts = egui::TextureOptions::LINEAR;
                match &mut self.handles[i] {
                    Some(hnd) if partial => hnd.set_partial([px, py], image, opts),
                    Some(hnd) => hnd.set(image, opts),
                    None => {
                        self.handles[i] = Some(ctx.load_texture(format!("{name}_{i}"), image, opts))
                    }
                }
            }
        }
    }
}

struct Overlay {
    map_w: i32,
    map_h: i32,
    rect: Option<Rect>,
    rgba: Vec<u8>,
    tiles: Option<TileGrid>,
}

impl Overlay {
    fn new(w: i32, h: i32) -> Overlay {
        Overlay {
            map_w: w,
            map_h: h,
            rect: None,
            rgba: Vec::new(),
            tiles: None,
        }
    }

    fn clear(&mut self) {
        self.rect = None;
        self.rgba = Vec::new();
        self.tiles = None;
    }

    fn alloc(&mut self, want: Rect) {
        let span = (want.x1 - want.x0 + 1).max(want.y1 - want.y0 + 1);
        let r = want.expand((span / 8).max(32), self.map_w, self.map_h);
        let bw = (r.x1 - r.x0 + 1) as usize;
        let bh = (r.y1 - r.y0 + 1) as usize;
        let mut buf = vec![0u8; bw * bh * 4];
        if let Some(old) = self.rect {
            let ow = (old.x1 - old.x0 + 1) as usize;
            for y in old.y0..=old.y1 {
                let src = (y - old.y0) as usize * ow * 4;
                let dst = ((y - r.y0) as usize * bw + (old.x0 - r.x0) as usize) * 4;
                buf[dst..dst + ow * 4].copy_from_slice(&self.rgba[src..src + ow * 4]);
            }
        }
        self.rect = Some(r);
        self.rgba = buf;
        self.tiles = Some(TileGrid::new_at(
            r.x0 as usize,
            (self.map_h - 1 - r.y1) as usize,
            bw,
            bh,
            self.map_h as usize,
        ));
    }

    fn paint_selection(&mut self, doc: &PlaneDoc, prov: u32) {
        self.clear();
        let Some(b) = doc.bbox(prov) else {
            return;
        };
        self.alloc(b);
        let dst = self.rect.unwrap_or(b);
        crate::render::selection_rows(&doc.plane(), prov, b, dst, &mut self.rgba);
    }

    fn paint_selection_in(&mut self, doc: &PlaneDoc, prov: u32, rect: Rect) {
        let (w, h) = (doc.width(), doc.height());
        let mut r = rect.expand(2, w, h);
        if doc.hwrap() && (r.x0 <= 2 || r.x1 >= w - 3) {
            r.x0 = 0;
            r.x1 = w - 1;
        }
        if doc.vwrap() && (r.y0 <= 2 || r.y1 >= h - 3) {
            r.y0 = 0;
            r.y1 = h - 1;
        }
        if r.is_empty() {
            return;
        }
        let Some(cur) = self.rect else {
            self.paint_selection(doc, prov);
            return;
        };
        let inside = r.x0 >= cur.x0 && r.y0 >= cur.y0 && r.x1 <= cur.x1 && r.y1 <= cur.y1;
        if !inside {
            self.alloc(cur.union(r));
        }
        let dst = self.rect.unwrap_or(cur);
        crate::render::selection_rows(&doc.plane(), prov, r, dst, &mut self.rgba);
        if let Some(t) = &mut self.tiles {
            t.mark(r);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Preset {
    DeepSea,
    Sea,
    Shallows,
    Land,
}

impl Preset {
    fn label(self) -> &'static str {
        match self {
            Preset::DeepSea => "Deep sea",
            Preset::Sea => "Sea",
            Preset::Shallows => "Shallows",
            Preset::Land => "Land",
        }
    }
    fn target(self) -> f32 {
        match self {
            Preset::DeepSea => -60.0,
            Preset::Sea => -20.0,
            Preset::Shallows => -5.0,
            Preset::Land => 30.0,
        }
    }
    fn water(self) -> bool {
        self.target() < 0.0
    }
    fn flag_op(self) -> FlagOp {
        match self {
            Preset::DeepSea => FlagOp::DeepSea,
            Preset::Sea | Preset::Shallows => FlagOp::Sea,
            Preset::Land => FlagOp::Land,
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Preset::DeepSea => "Sink the province to deep water and mark it Sea and Deep sea",
            Preset::Sea => "Sink the province to open water and mark it Sea",
            Preset::Shallows => "Sink the province just below the surface and mark it Sea",
            Preset::Land => "Raise the province above the water and clear the Sea marks. The land picture comes from the terrain flags, not from the height",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Select,
    Link,
    Paint,
    Height,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Editor,
    Generator,
}

#[derive(Clone, PartialEq)]
enum Pending {
    None,
    Open(Option<PathBuf>),
    AddPlane,
    Close,
}

const BORDER_KINDS: [(i64, &str); 6] = [
    (0, "Normal"),
    (
        (BORDER_MOUNTAIN_LINE | BORDER_IMPASSABLE) as i64,
        "Mountains",
    ),
    (
        (BORDER_MOUNTAIN_LINE | BORDER_MOUNTAIN_PASS) as i64,
        "Mountain pass",
    ),
    (BORDER_RIVER as i64, "River"),
    (BORDER_BRIDGE as i64, "Bridge"),
    (BORDER_IMPASSABLE as i64, "Impassable"),
];

const ROAD: i64 = 8;

const ICONS: [(u16, &[u8]); 12] = [
    (184, include_bytes!("../assets/icons/184.png")),
    (185, include_bytes!("../assets/icons/185.png")),
    (186, include_bytes!("../assets/icons/186.png")),
    (187, include_bytes!("../assets/icons/187.png")),
    (188, include_bytes!("../assets/icons/188.png")),
    (189, include_bytes!("../assets/icons/189.png")),
    (190, include_bytes!("../assets/icons/190.png")),
    (228, include_bytes!("../assets/icons/228.png")),
    (266, include_bytes!("../assets/icons/266.png")),
    (272, include_bytes!("../assets/icons/272.png")),
    (276, include_bytes!("../assets/icons/276.png")),
    (277, include_bytes!("../assets/icons/277.png")),
];

fn load_icons(ctx: &egui::Context) -> Vec<(u16, egui::TextureHandle)> {
    ICONS
        .iter()
        .filter_map(|(id, bytes)| {
            let img = decode_png(bytes).ok()?;
            let (mut x0, mut y0, mut x1, mut y1) = (img.w, img.h, 0usize, 0usize);
            for y in 0..img.h {
                for x in 0..img.w {
                    if img.rgba[(y * img.w + x) * 4 + 3] > 8 {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            if x1 < x0 {
                return None;
            }
            let side = (x1 - x0 + 1).max(y1 - y0 + 1) + 2;
            let cx = (x0 + x1).div_ceil(2);
            let cy = (y0 + y1).div_ceil(2);
            let mut out = vec![0u8; side * side * 4];
            for y in 0..side {
                for x in 0..side {
                    let sx = (cx + x).checked_sub(side / 2);
                    let sy = (cy + y).checked_sub(side / 2);
                    if let (Some(sx), Some(sy)) = (sx, sy) {
                        if sx < img.w && sy < img.h {
                            let src = (sy * img.w + sx) * 4;
                            out[(y * side + x) * 4..(y * side + x) * 4 + 4]
                                .copy_from_slice(&img.rgba[src..src + 4]);
                        }
                    }
                }
            }
            let ci = egui::ColorImage::from_rgba_unmultiplied([side, side], &out);
            Some((
                *id,
                ctx.load_texture(format!("icon_{id}"), ci, egui::TextureOptions::LINEAR),
            ))
        })
        .collect()
}

fn terrain_icons(f: u64) -> Vec<u16> {
    let mut out = Vec::new();
    let sea = f & SEA != 0;
    if sea {
        out.push(if f & DEEP_SEA != 0 { 190 } else { 189 });
    }
    if f & FOREST != 0 {
        out.push(if sea { 276 } else { 184 });
    }
    if f & SWAMP != 0 {
        out.push(185);
    }
    if f & CAVE != 0 {
        out.push(228);
    } else if f & MOUNTAIN != 0 {
        out.push(186);
    }
    if f & HIGHLAND != 0 {
        out.push(if sea { 277 } else { 272 });
    }
    if f & WASTE != 0 {
        out.push(187);
    }
    if f & FARM != 0 {
        out.push(188);
    }
    if f & FRESH_WATER != 0 && !sea {
        out.push(266);
    }
    out
}

const MIN_ZOOM: f32 = 0.02;
const THUMB_W: usize = 320;
const LABEL_MIN_ZOOM: f32 = 0.25;
const DOT_MIN_ZOOM: f32 = 0.12;
const MAX_WRAP_COPIES: i32 = 96;

fn wrap_fits(span: f32, extent: f32) -> bool {
    span > 1.0 && ((extent / span).ceil() as i32) < MAX_WRAP_COPIES
}

fn wrap_axis_shifts(base: f32, span: f32, lo: f32, hi: f32, wrap: bool) -> Vec<i32> {
    if !wrap || span <= 1.0 {
        return vec![0];
    }
    let first = ((lo - base) / span).floor() as i32;
    let last = ((hi - base) / span).floor() as i32;
    let last = last.max(first).min(first + MAX_WRAP_COPIES);
    (first..=last).collect()
}

fn wrap_shifts(
    canvas: egui::Rect,
    offset: Vec2,
    zoom: f32,
    w: i32,
    h: i32,
    hwrap: bool,
    vwrap: bool,
) -> Vec<Vec2> {
    let span_x = w as f32 * zoom;
    let span_y = h as f32 * zoom;
    let xs = wrap_axis_shifts(
        canvas.min.x + offset.x,
        span_x,
        canvas.min.x,
        canvas.max.x,
        hwrap,
    );
    let ys = wrap_axis_shifts(
        canvas.min.y + offset.y,
        span_y,
        canvas.min.y,
        canvas.max.y,
        vwrap,
    );
    let mut out = Vec::new();
    for ky in &ys {
        for kx in &xs {
            out.push(Vec2::new(*kx as f32 * span_x, *ky as f32 * span_y));
        }
    }
    out
}

fn plane_label(index: u32) -> String {
    match index {
        1 => "Surface".to_owned(),
        2 => "Caves".to_owned(),
        n => format!("Plane {n}"),
    }
}

fn terrain_label(f: u64) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if f & UNKNOWN != 0 {
        parts.push("Unknown");
    }
    if f & CAVE_WALL != 0 {
        parts.push("Cave wall");
    }
    if f & SEA != 0 {
        parts.push(if f & DEEP_SEA != 0 { "Deep sea" } else { "Sea" });
        if f & FOREST != 0 {
            parts.push("Kelp");
        }
        if f & HIGHLAND != 0 {
            parts.push("Gorge");
        }
    } else if f & FRESH_WATER != 0 {
        parts.push("Fresh water");
    }
    for (bit, name) in [
        (CAVE, "Cave"),
        (HIGHLAND, "Highlands"),
        (SWAMP, "Swamp"),
        (WASTE, "Waste"),
        (FOREST, "Forest"),
        (FARM, "Farm"),
        (MOUNTAIN, "Mountain"),
        (WARMER, "Warmer"),
        (COLDER, "Colder"),
    ] {
        let at_sea = f & SEA != 0 && (bit == FOREST || bit == HIGHLAND);
        if f & bit != 0 && !at_sea {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        parts.push("Plains");
    }
    parts.join(", ")
}

fn link_colour(spec: i64) -> Color32 {
    if spec & (BORDER_RIVER as i64) != 0 {
        Color32::from_rgb(90, 160, 255)
    } else if spec & (BORDER_IMPASSABLE as i64) != 0 {
        Color32::from_rgb(230, 80, 60)
    } else if spec & (BORDER_MOUNTAIN_PASS as i64) != 0 {
        Color32::from_rgb(230, 150, 60)
    } else {
        Color32::from_rgb(240, 220, 60)
    }
}

const BASIC_FLAGS: [(u64, &str); 11] = [
    (SEA, "Sea"),
    (DEEP_SEA, "Deep sea"),
    (FRESH_WATER, "Fresh water"),
    (SMALL, "Small prov."),
    (LARGE, "Large prov."),
    (MOUNTAIN, "Mountain"),
    (GOOD_START, "Start"),
    (NO_START, "No start"),
    (MANY_SITES, "Many sites"),
    (GOOD_THRONE, "Throne site"),
    (BAD_THRONE, "No throne"),
];

const ADVANCED_FLAGS: [(u64, &str); 10] = [
    (HIGHLAND, "Highlands"),
    (SWAMP, "Swamp"),
    (WASTE, "Waste"),
    (FOREST, "Forest"),
    (FARM, "Farm"),
    (CAVE, "Cave"),
    (CAVE_WALL, "Cave wall"),
    (CAVE_LOOK, "Cave look"),
    (WARMER, "Warmer"),
    (COLDER, "Colder"),
];

fn toggle_flag(flags: u64, bit: u64) -> u64 {
    let on = flags & bit == 0;
    let mut f = if on { flags | bit } else { flags & !bit };
    if on {
        match bit {
            SEA => f &= !FRESH_WATER,
            DEEP_SEA => f = (f | SEA) & !FRESH_WATER,
            FRESH_WATER => f &= !DEEP_SEA,
            SMALL => f &= !LARGE,
            LARGE => f &= !SMALL,
            GOOD_THRONE => f &= !BAD_THRONE,
            BAD_THRONE => f &= !GOOD_THRONE,
            WARMER => f &= !COLDER,
            COLDER => f &= !WARMER,
            _ => {}
        }
    } else if bit == SEA {
        f &= !DEEP_SEA;
    }
    f
}

pub struct App {
    tex: TexSet,
    opts: Options,
    project: Option<Project>,
    tiles: Vec<TileGrid>,
    overlays: Vec<Overlay>,
    active: usize,
    zoom: f32,
    offset: Vec2,
    fit_pending: bool,
    center_pending: Option<u32>,
    wrap_view: bool,
    gen_wrap: (bool, bool),
    last_canvas: Option<Vec2>,
    last_wrap_axes: (bool, bool),
    renaming: bool,
    name_draft: String,
    last_click_prov: Option<(usize, u32)>,
    selected: Option<u32>,
    hover: Option<u32>,
    decor_tiles: Vec<TileGrid>,
    relief_tiles: Vec<TileGrid>,
    relief: Vec<Vec<u8>>,
    relief_range: Vec<(f32, f32)>,
    relief_stale: Vec<bool>,
    thumbs: Vec<Option<egui::TextureHandle>>,
    thumb_img: Vec<Option<crate::render::Thumb>>,
    thumb_stale: Vec<bool>,
    thumb_dirty: Vec<Option<Rect>>,
    thumb_at: web_time::Instant,
    rendered_opts: Vec<Options>,
    relief_in_height: bool,
    keep_rivers: bool,
    terrain_follows_height: bool,
    repair_rivers: bool,
    stroke_last: Option<(i32, i32)>,
    stroke_decor: Option<Rect>,
    stroke_decor_at: web_time::Instant,
    stroke_decor_cost: web_time::Duration,
    tool: Tool,
    brush: i32,
    paint_empty: bool,
    painting: Option<PointerButton>,
    stroke_phys: Option<PointerButton>,
    paint_erase: bool,
    lay: Layout,
    sheet: Option<Sheet>,
    style_kind: Option<Kind>,
    show_markers: bool,
    show_links: bool,
    show_terrain: bool,
    placing_capital: bool,
    goto: u32,
    placing_new: bool,
    icons: Vec<(u16, egui::TextureHandle)>,
    github: Option<egui::TextureHandle>,
    flatten: bool,
    show_names: bool,
    custom: f32,
    step: f32,
    nostart_min: f32,
    nostart_crossing: f32,
    name_edit: String,
    name_for: Option<u32>,
    gate_edit: i32,
    status: String,
    error: Option<String>,
    pending: Pending,
    confirm_close: bool,
    show_help: bool,
    gen: GeneratorPanel,
    mode: Mode,
    pending_generate: bool,
    confirm_generate: bool,
    settings: Settings,
    confirm_overwrite: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    ctx: egui::Context,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    folder_files: Vec<String>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        preselect: Option<u32>,
        random: bool,
    ) -> App {
        theme::install(&cc.egui_ctx);
        #[cfg(target_arch = "wasm32")]
        crate::web::install_drop(cc.egui_ctx.clone());
        cc.egui_ctx.options_mut(|o| {
            o.input_options.max_double_click_delay = 0.5;
            o.input_options.max_click_dist = 8.0;
        });
        let mut app = App {
            tex: TexSet::embedded(),
            opts: Options::default(),
            project: None,
            tiles: Vec::new(),
            overlays: Vec::new(),
            active: 0,
            zoom: 1.0,
            offset: Vec2::ZERO,
            fit_pending: true,
            center_pending: None,
            wrap_view: false,
            gen_wrap: (false, false),
            last_canvas: None,
            last_wrap_axes: (false, false),
            renaming: false,
            name_draft: String::new(),
            last_click_prov: None,
            selected: None,
            hover: None,
            tool: Tool::Select,
            decor_tiles: Vec::new(),
            relief_tiles: Vec::new(),
            relief: Vec::new(),
            relief_range: Vec::new(),
            relief_stale: Vec::new(),
            thumbs: Vec::new(),
            thumb_img: Vec::new(),
            thumb_stale: Vec::new(),
            thumb_dirty: Vec::new(),
            thumb_at: web_time::Instant::now(),
            rendered_opts: Vec::new(),
            relief_in_height: true,
            keep_rivers: true,
            terrain_follows_height: true,
            repair_rivers: false,
            stroke_last: None,
            stroke_decor: None,
            stroke_decor_at: web_time::Instant::now(),
            stroke_decor_cost: web_time::Duration::ZERO,
            brush: 10,
            paint_empty: false,
            painting: None,
            show_markers: true,
            show_links: false,
            show_terrain: false,
            placing_capital: false,
            goto: 1,
            placing_new: false,
            icons: load_icons(&cc.egui_ctx),
            github: load_github_mark(&cc.egui_ctx),
            flatten: false,
            show_names: false,
            custom: -20.0,
            step: 10.0,
            nostart_min: 4.0,
            nostart_crossing: 0.5,
            name_edit: String::new(),
            name_for: None,
            gate_edit: 0,
            status: if crate::io::IS_WEB {
                "Open a map: drop its folder, or the .d6m and .map files together".to_owned()
            } else {
                "Open a map, or drop a .d6m or .map file on the window".to_owned()
            },
            error: None,
            pending: Pending::None,
            confirm_close: false,
            show_help: false,
            gen: GeneratorPanel::default(),
            mode: if random {
                Mode::Generator
            } else {
                Mode::Editor
            },
            pending_generate: false,
            confirm_generate: false,
            settings: Settings::load(),
            confirm_overwrite: false,
            ctx: cc.egui_ctx.clone(),
            folder_files: Vec::new(),
            stroke_phys: None,
            paint_erase: false,
            lay: Layout::probe(&cc.egui_ctx),
            sheet: None,
            style_kind: None,
        };
        if let Some(p) = initial {
            app.open(&p);
            if let Some(sel) = preselect {
                if app
                    .doc()
                    .map(|d| sel >= 1 && sel as usize <= d.province_count())
                    .unwrap_or(false)
                {
                    app.select(Some(sel));
                }
            }
        }
        app
    }

    fn adopt_project(&mut self, p: Project) {
        self.tiles = p
            .planes
            .iter()
            .map(|d| TileGrid::new(d.width(), d.height()))
            .collect();
        self.overlays = p
            .planes
            .iter()
            .map(|d| Overlay::new(d.width(), d.height()))
            .collect();
        self.decor_tiles = p
            .planes
            .iter()
            .map(|d| TileGrid::new(d.width(), d.height()))
            .collect();
        self.relief_tiles = p
            .planes
            .iter()
            .map(|d| TileGrid::new(d.width(), d.height()))
            .collect();
        self.relief = p.planes.iter().map(|_| Vec::new()).collect();
        self.relief_range = p.planes.iter().map(|_| (-1.0, 1.0)).collect();
        self.relief_stale = p.planes.iter().map(|_| true).collect();
        self.thumbs = p.planes.iter().map(|_| None).collect();
        self.thumb_img = p.planes.iter().map(|_| None).collect();
        self.thumb_stale = p.planes.iter().map(|_| true).collect();
        self.thumb_dirty = p.planes.iter().map(|_| None).collect();
        self.rendered_opts = p.planes.iter().map(|_| self.opts).collect();
        self.project = Some(p);
        self.active = 0;
        self.selected = None;
        self.name_for = None;
        self.error = None;
        self.fit_pending = true;
        self.tool = Tool::Select;
        let (hwrap, vwrap) = self.plane_wraps();
        if hwrap || vwrap {
            self.wrap_view = true;
        }
        self.apply_river_repair();
    }

    fn apply_river_repair(&mut self) {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let on = self.repair_rivers;
        let mut total = 0;
        if let Some(p) = &mut self.project {
            for d in &mut p.planes {
                total += d.set_river_repair(on, &tex, &opts);
            }
        }
        self.tex = tex;
        if total > 0 {
            for t in self.tiles.iter_mut().chain(self.decor_tiles.iter_mut()) {
                t.mark_all();
            }
            for st in &mut self.relief_stale {
                *st = true;
            }
            self.refresh_selection();
        }
    }

    fn place_starts(&mut self, wants: &[crate::starts::Want], generic: bool) {
        if wants.is_empty() {
            self.status = "Add players to the list first".to_owned();
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let (planes, gates) = project.graph();
        let seed = self.gen.seed() as u64;
        let placed = crate::starts::place(&planes, &gates, wants, seed);
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let n = self
            .project
            .as_mut()
            .map(|p| p.apply_starts(&placed, generic, &tex, &opts))
            .unwrap_or(0);
        self.tex = tex;
        self.mark_tiles(None);
        self.refresh_selection();
        let missing = wants.len().saturating_sub(n);
        self.status = if missing > 0 {
            format!("Placed {n} starts; {missing} players found no fitting province")
        } else if generic {
            format!("Placed {n} start provinces")
        } else {
            format!("Placed {n} nation starts")
        };
    }

    fn remove_empty_provinces(&mut self) -> usize {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let mut removed = 0;
        let mut orphan_gates: Vec<i32> = Vec::new();
        if let Some(p) = &mut self.project {
            for d in &mut p.planes {
                while let Some(&e) = d.empty_provinces().first() {
                    let g = d.gate(e);
                    if !d.remove_province(e, &tex, &opts) {
                        break;
                    }
                    if g != 0 {
                        orphan_gates.push(g);
                    }
                    removed += 1;
                }
            }
            for g in orphan_gates {
                for d in &mut p.planes {
                    for q in 1..=d.province_count() as u32 {
                        if d.gate(q) == g {
                            d.set_gate(q, 0, &tex, &opts);
                        }
                    }
                }
            }
        }
        self.tex = tex;
        if removed > 0 {
            self.mark_tiles(None);
            self.refresh_selection();
        }
        removed
    }

    fn link_isolated_provinces(&mut self) -> usize {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let mut linked = 0;
        if let Some(p) = &mut self.project {
            for d in &mut p.planes {
                linked += d.link_isolated(&tex, &opts).len();
            }
        }
        self.tex = tex;
        if linked > 0 {
            self.mark_tiles(None);
            self.refresh_selection();
        }
        linked
    }

    fn open_generated(&mut self, g: GeneratedMap) {
        let dir = self.settings.maps_dir();
        let planes: Vec<(&[u8], &str)> = g
            .planes
            .iter()
            .map(|p| (p.d6m.as_slice(), p.map_text.as_str()))
            .collect();
        let gates: Vec<(u16, u16)> = g.gates.iter().map(|g| (g.surface, g.cave)).collect();
        match Project::from_generated(dir, &g.name, &planes, &gates, &self.tex, &self.opts) {
            Ok(mut p) => {
                p.set_generator_settings(&self.gen.settings_lines());
                self.adopt_project(p);
                let dropped = self.remove_empty_provinces();
                let linked = self.link_isolated_provinces();
                let counts = self
                    .project
                    .as_ref()
                    .and_then(|p| p.planes.first())
                    .map(terrain_tally)
                    .unwrap_or_default();
                let first = &g.planes[0];
                let caves = if g.planes.len() > 1 {
                    format!(
                        ", {} cave provinces behind {} gateways",
                        g.planes[1].provinces,
                        gates.len()
                    )
                } else {
                    String::new()
                };
                self.status = format!(
                    "Generated {} px x {} px with {} provinces{} from seed {} in {:.1} s. {}",
                    first.width,
                    first.height,
                    first.provinces,
                    caves,
                    g.seed,
                    g.elapsed.as_secs_f32(),
                    counts
                );
                if dropped > 0 {
                    self.status = format!(
                        "{}. Dropped {dropped} province{} that had no area",
                        self.status,
                        if dropped == 1 { "" } else { "s" }
                    );
                }
                if linked > 0 {
                    self.status = format!(
                        "{}. Connected {linked} province{} that had no neighbour",
                        self.status,
                        if linked == 1 { "" } else { "s" }
                    );
                }
                if !g.wants.is_empty() {
                    let summary = std::mem::take(&mut self.status);
                    self.place_starts(&g.wants, g.generic);
                    self.status = format!("{summary}. {}", self.status);
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn open(&mut self, path: &Path) {
        match Project::open(path, &self.tex, &self.opts) {
            Ok(p) => {
                let planes = p.planes.len();
                let notes = p.notes.clone();
                self.status = if planes > 1 {
                    format!("Loaded {} with {} planes", p.base, planes)
                } else {
                    format!("Loaded {}", p.base)
                };
                if !notes.is_empty() {
                    self.status = format!("{} ({})", self.status, notes.join("; "));
                }
                let restored = self.gen.apply_settings(&p.generator_settings());
                if let Some(note) = restored {
                    self.status = format!("{}; {}", self.status, note);
                }
                let status = std::mem::take(&mut self.status);
                self.adopt_project(p);
                self.status = status;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn doc(&self) -> Option<&PlaneDoc> {
        self.project
            .as_ref()
            .and_then(|p| p.planes.get(self.active))
    }

    fn with_doc<R>(&mut self, f: impl FnOnce(&mut PlaneDoc, &TexSet, &Options) -> R) -> Option<R> {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let i = self.active;
        let out = self
            .project
            .as_mut()
            .and_then(|p| p.planes.get_mut(i))
            .map(|d| f(d, &tex, &opts));
        self.tex = tex;
        out
    }

    fn map_to_screen_shift(&self, canvas: egui::Rect, x: f32, y_img: f32, shift: Vec2) -> Pos2 {
        canvas.min + self.offset + shift + Vec2::new(x, y_img) * self.zoom
    }

    fn wrap_axes(&self) -> (bool, bool) {
        if !self.wrap_view {
            return (false, false);
        }
        self.doc()
            .map(|d| (d.hwrap(), d.vwrap()))
            .unwrap_or((false, false))
    }

    fn plane_wraps(&self) -> (bool, bool) {
        self.doc()
            .map(|d| (d.hwrap(), d.vwrap()))
            .unwrap_or((false, false))
    }

    fn screen_to_map(&self, canvas: egui::Rect, p: Pos2) -> (i32, i32) {
        let v = (p - canvas.min - self.offset) / self.zoom;
        let (w, h) = self
            .doc()
            .map(|d| (d.width(), d.height()))
            .unwrap_or((0, 0));
        let (hw, vw) = self.wrap_axes();
        let (x, y_img) = wrap_point(v.x.floor() as i32, v.y.floor() as i32, w, h, hw, vw);
        (x, h - 1 - y_img)
    }

    fn fit(&mut self, canvas: egui::Rect) {
        let Some(doc) = self.doc() else {
            return;
        };
        let w = doc.width() as f32;
        let h = doc.height() as f32;
        let z = (canvas.width() / w).min(canvas.height() / h) * 0.985;
        self.zoom = z.max(MIN_ZOOM);
        self.offset = Vec2::new(
            (canvas.width() - w * self.zoom) * 0.5,
            (canvas.height() - h * self.zoom) * 0.5,
        );
    }

    fn center_on(&mut self, canvas: egui::Rect, prov: u32) {
        let Some(doc) = self.doc() else {
            return;
        };
        let Some(&(cx, cy)) = doc.capitals.get(prov as usize - 1) else {
            return;
        };
        let h = doc.height();
        let target = Vec2::new(cx as f32 + 0.5, (h - 1 - cy as i32) as f32 + 0.5) * self.zoom;
        self.offset = canvas.size() * 0.5 - target;
    }

    fn jump_to_gateway(&mut self, prov: u32) {
        let gate = self.doc().map(|d| d.gate(prov)).unwrap_or(0);
        if gate == 0 {
            return;
        }
        let next = self
            .project
            .as_ref()
            .and_then(|p| p.next_gateway(self.active, prov));
        let Some((plane, target)) = next else {
            self.status =
                format!("Gate {gate}: province {prov} is the only gateway with that number");
            return;
        };
        if plane != self.active {
            self.switch_plane(plane);
            self.fit_pending = false;
        }
        self.select(Some(target));
        self.goto = target;
        self.center_pending = Some(target);
        let where_to = self
            .doc()
            .map(|d| plane_label(d.index))
            .unwrap_or_else(|| format!("plane {}", plane + 1));
        self.status = format!("Gate {gate}: jumped to province {target} on {where_to}");
    }

    fn select(&mut self, prov: Option<u32>) {
        self.selected = prov;
        self.name_for = None;
        let active = self.active;
        if let (Some(project), Some(ov)) = (&self.project, self.overlays.get_mut(active)) {
            match prov {
                Some(p) => ov.paint_selection(&project.planes[active], p),
                None => ov.clear(),
            }
        }
    }

    fn refresh_selection(&mut self) {
        if let Some(p) = self.selected {
            let active = self.active;
            if let (Some(project), Some(ov)) = (&self.project, self.overlays.get_mut(active)) {
                ov.paint_selection(&project.planes[active], p);
            }
        }
    }

    fn refresh_selection_in(&mut self, rect: Rect) {
        if let Some(p) = self.selected {
            let active = self.active;
            if let (Some(project), Some(ov)) = (&self.project, self.overlays.get_mut(active)) {
                ov.paint_selection_in(&project.planes[active], p, rect);
            }
        }
    }

    fn mark_tiles(&mut self, rect: Option<Rect>) {
        let active = self.active;
        let touched = rect.map(|r| self.doc().map(|d| d.rendered.touched.union(r)).unwrap_or(r));
        for grid in [self.tiles.get_mut(active), self.decor_tiles.get_mut(active)]
            .into_iter()
            .flatten()
        {
            match touched {
                Some(r) => grid.mark(r.expand(4, i32::MAX, i32::MAX)),
                None => grid.mark_all(),
            }
        }
        if self.relief_shown() && !self.relief_stale.get(active).copied().unwrap_or(true) {
            self.refresh_relief(touched);
        } else if let Some(st) = self.relief_stale.get_mut(active) {
            *st = true;
        }
        match touched {
            Some(r) => {
                if let Some(d) = self.thumb_dirty.get_mut(active) {
                    *d = union(*d, Some(r));
                }
            }
            None => {
                if let Some(st) = self.thumb_stale.get_mut(active) {
                    *st = true;
                }
            }
        }
    }

    fn refresh_thumb(&mut self, ctx: &egui::Context) {
        let active = self.active;
        let full = self.thumb_stale.get(active).copied().unwrap_or(false);
        let dirty = self.thumb_dirty.get(active).copied().flatten();
        if !full && dirty.is_none() {
            return;
        }
        let has = self
            .thumbs
            .get(active)
            .map(|t| t.is_some())
            .unwrap_or(false);
        if has && self.thumb_at.elapsed() < web_time::Duration::from_millis(400) {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let Some(doc) = project.planes.get(active) else {
            return;
        };
        let (w, h) = (doc.width() as usize, doc.height() as usize);
        if w == 0 || h == 0 || doc.rendered.rgba.len() != w * h * 4 {
            return;
        }
        let prev = self.thumb_img.get_mut(active).and_then(Option::take);
        let thumb = crate::render::thumbnail(
            &doc.rendered,
            self.opts.decor,
            THUMB_W,
            prev,
            if full { None } else { dirty },
        );
        let (ow, oh) = (thumb.w, thumb.h);
        let top = crate::render::flip_to_top_down(ow as i32, oh as i32, &thumb.rgba);
        let image = egui::ColorImage::from_rgba_unmultiplied([ow, oh], &top);
        if let Some(slot) = self.thumb_img.get_mut(active) {
            *slot = Some(thumb);
        }
        if let Some(d) = self.thumb_dirty.get_mut(active) {
            *d = None;
        }
        let name = format!("thumb{}", doc.index);
        match self.thumbs.get_mut(active) {
            Some(Some(t)) => t.set(image, egui::TextureOptions::LINEAR),
            Some(slot) => *slot = Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR)),
            None => {}
        }
        self.thumb_stale[active] = false;
        self.thumb_at = web_time::Instant::now();
    }

    fn relief_shown(&self) -> bool {
        self.tool == Tool::Height && self.relief_in_height
    }

    fn refresh_relief(&mut self, rect: Option<Rect>) {
        let active = self.active;
        let Some(project) = &self.project else {
            return;
        };
        let Some(doc) = project.planes.get(active) else {
            return;
        };
        if self.relief.len() <= active {
            return;
        }
        let n = doc.rendered.rgba.len();
        let full = rect.is_none() || self.relief[active].len() != n || self.relief_stale[active];
        if full {
            self.relief[active] = vec![0u8; n];
            self.relief_range[active] =
                crate::render::height_range(&doc.rendered.carved, &doc.d6m.owners);
        }
        let (lo, hi) = self.relief_range[active];
        let area = match rect {
            Some(r) if !full => r.expand(2, i32::MAX, i32::MAX),
            _ => Rect::full(doc.width(), doc.height()),
        };
        let plane = doc.plane();
        let buf = &mut self.relief[active];
        crate::render::relief_rows(&plane, &doc.rendered.carved, area, lo, hi, buf);
        if self.opts.borders {
            let bw = doc.rendered.width;
            crate::render::draw_border_rows(&plane, &doc.rendered.mask, bw, 15, area, buf);
            crate::render::draw_border_rows(&plane, &doc.rendered.mask, bw, 30, area, buf);
        }
        if self.opts.capitals {
            crate::render::mark_capitals(&plane, buf);
        }
        self.relief_stale[active] = false;
        if let Some(t) = self.relief_tiles.get_mut(active) {
            if full {
                t.mark_all();
            } else {
                t.mark(area.expand(4, i32::MAX, i32::MAX));
            }
        }
    }

    fn after_edit(&mut self, changed: bool, rect: Option<Rect>, label: &str) {
        if changed {
            self.mark_tiles(rect);
            self.refresh_selection();
            self.status = label.to_owned();
        } else {
            self.status = format!("{label}: nothing to change");
        }
    }

    fn apply(&mut self, op: HeightOp, flag_op: FlagOp, label: &str) {
        let Some(prov) = self.selected else {
            return;
        };
        let name = self
            .doc()
            .map(|d| d.name(prov).to_owned())
            .unwrap_or_default();
        let res = self.with_doc(|d, tex, opts| {
            let changed = d.apply(prov, op, flag_op, label, tex, opts);
            (
                changed,
                if matches!(flag_op, FlagOp::Keep) {
                    d.bbox(prov)
                } else {
                    None
                },
            )
        });
        if let Some((changed, rect)) = res {
            self.after_edit(changed, rect, &format!("{label}: province {prov} {name}"));
        }
    }

    fn preset(&mut self, p: Preset) {
        let op = if self.flatten {
            HeightOp::Flat(p.target())
        } else if p.water() {
            HeightOp::Below(p.target())
        } else {
            HeightOp::Above(p.target())
        };
        self.apply(op, p.flag_op(), p.label());
    }

    fn set_flags(&mut self, prov: u32, new: u64, label: &str) {
        let res = self.with_doc(|d, tex, opts| d.set_flags(prov, new, label, tex, opts));
        self.after_edit(res.unwrap_or(false), None, label);
    }

    fn undo(&mut self) {
        let msg = self
            .with_doc(|d, tex, opts| d.undo_last(tex, opts))
            .flatten();
        self.after_edit(
            msg.is_some(),
            None,
            &msg.map(|m| format!("Undid {m}"))
                .unwrap_or_else(|| "Nothing to undo".to_owned()),
        );
    }

    fn redo(&mut self) {
        let msg = self
            .with_doc(|d, tex, opts| d.redo_last(tex, opts))
            .flatten();
        self.after_edit(
            msg.is_some(),
            None,
            &msg.map(|m| format!("Redid {m}"))
                .unwrap_or_else(|| "Nothing to redo".to_owned()),
        );
    }

    fn rerender_all(&mut self) {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        if let Some(p) = &mut self.project {
            for d in &mut p.planes {
                d.rerender(&tex, &opts);
            }
        }
        self.tex = tex;
        for t in self.tiles.iter_mut().chain(self.decor_tiles.iter_mut()) {
            t.mark_all();
        }
        for st in &mut self.relief_stale {
            *st = true;
        }
        for st in &mut self.thumb_stale {
            *st = true;
        }
        for o in &mut self.rendered_opts {
            *o = opts;
        }
    }

    fn apply_view(&mut self, i: usize) {
        let to = self.opts;
        let Some(from) = self.rendered_opts.get(i).copied() else {
            return;
        };
        if from == to {
            return;
        }
        let winter = from.winter != to.winter;
        let grey = from.grey_no_start != to.grey_no_start;
        let ground = from.borders != to.borders
            || from.dirt != to.dirt
            || from.edge_fade != to.edge_fade
            || from.rivers != to.rivers
            || from.border_percent != to.border_percent;
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let mut ground_changed = false;
        let mut decor_changed = false;
        if let Some(d) = self.project.as_mut().and_then(|p| p.planes.get_mut(i)) {
            if winter {
                d.rerender(&tex, &to);
                ground_changed = true;
                decor_changed = true;
            } else if grey {
                d.rerender_quick(&tex, &to);
                ground_changed = true;
                decor_changed = true;
            } else if ground {
                d.rerender_ground(&tex, &to);
                ground_changed = true;
            } else if from.capitals != to.capitals {
                d.set_capitals(to.capitals);
                ground_changed = true;
            }
            if to.decor && d.decor_stale() {
                d.refresh_decor_full(&tex, &to);
                decor_changed = true;
            }
            if from.decor && !to.decor {
                d.rendered.drop_decor();
                let (dw, dh) = (d.width(), d.height());
                if let Some(t) = self.decor_tiles.get_mut(i) {
                    *t = TileGrid::new(dw, dh);
                }
            }
        }
        self.tex = tex;
        self.rendered_opts[i] = to;
        if ground_changed {
            if let Some(t) = self.tiles.get_mut(i) {
                t.mark_all();
            }
            if let Some(st) = self.relief_stale.get_mut(i) {
                *st = true;
            }
        }
        if decor_changed {
            if let Some(t) = self.decor_tiles.get_mut(i) {
                t.mark_all();
            }
        }
        if ground_changed || decor_changed || from.decor != to.decor {
            if let Some(st) = self.thumb_stale.get_mut(i) {
                *st = true;
            }
        }
    }

    fn view_changed(&mut self) {
        let i = self.active;
        self.apply_view(i);
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_save_target(&mut self) -> bool {
        false
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_save_target(&mut self) -> bool {
        let (dir, base) = match &self.project {
            Some(p) if p.unsaved => (self.settings.maps_dir(), p.base.clone()),
            Some(p) => (p.dir.clone(), p.base.clone()),
            None => return false,
        };
        let _ = crate::settings::ensure(dir.clone());
        let dlg = rfd::FileDialog::new()
            .add_filter("Dominions 6 recipe", &["d6m"])
            .set_directory(&dir)
            .set_file_name(format!("{base}.d6m"));
        let Some(path) = dlg.save_file() else {
            return false;
        };
        let dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            self.error = Some("that file name cannot be used".to_owned());
            return false;
        };
        let stem = stem.to_owned();
        if let Some(p) = &mut self.project {
            p.retarget(dir, &stem);
        }
        true
    }

    fn default_target(&self) -> Option<(PathBuf, String)> {
        let p = self.project.as_ref()?;
        Some((self.settings.maps_dir(), p.base.clone()))
    }

    fn target_taken(&self, dir: &Path, base: &str) -> bool {
        let Some(p) = &self.project else {
            return false;
        };
        p.planes.iter().any(|d| {
            dir.join(plane_file_name(base, d.index, "d6m")).exists()
                || dir.join(plane_file_name(base, d.index, "map")).exists()
        })
    }

    fn retarget_to(&mut self, dir: PathBuf, base: &str) -> bool {
        match crate::settings::ensure(dir) {
            Ok(dir) => {
                if let Some(p) = &mut self.project {
                    p.retarget(dir, base);
                }
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn save(&mut self) -> bool {
        if self.project.as_ref().map(|p| p.unsaved).unwrap_or(false) {
            let Some((dir, base)) = self.default_target() else {
                return false;
            };
            if self.target_taken(&dir, &base) {
                self.confirm_overwrite = true;
                return false;
            }
            if !self.retarget_to(dir, &base) {
                return false;
            }
        }
        self.write_planes()
    }

    fn write_planes(&mut self) -> bool {
        let Some(p) = &mut self.project else {
            return false;
        };
        let mut written = Vec::new();
        let mut warnings = Vec::new();
        for d in &mut p.planes {
            if !d.dirty {
                continue;
            }
            match d.save() {
                Ok(files) => {
                    written.extend(files);
                    if let Some(w) = d.area_warning() {
                        warnings.push(format!("plane {}: {w}", d.index));
                    }
                }
                Err(e) => {
                    self.error = Some(e);
                    return false;
                }
            }
        }
        self.status = if written.is_empty() {
            "Nothing to save".to_owned()
        } else {
            let names: Vec<String> = written
                .iter()
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            if crate::io::IS_WEB {
                self.export(&written);
                format!("Saving {}", names.join(", "))
            } else {
                let dir = written
                    .first()
                    .and_then(|f| f.parent())
                    .map(crate::settings::shown)
                    .unwrap_or_default();
                format!("Saved {} in {dir}", names.join(", "))
            }
        };
        if !warnings.is_empty() {
            self.status = format!("{}. {}", self.status, warnings.join("; "));
        }
        true
    }

    #[cfg(target_arch = "wasm32")]
    fn export(&self, paths: &[PathBuf]) {
        crate::web::export(paths.to_vec(), self.ctx.clone());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export(&self, _paths: &[PathBuf]) {}

    #[cfg(target_arch = "wasm32")]
    fn pick_file(&mut self) {
        crate::web::pick(
            crate::web::Purpose::Open,
            crate::web::MAP_FILES,
            true,
            self.ctx.clone(),
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_web(&mut self) {
        use crate::web::{Event, Purpose};
        for event in crate::web::take_events() {
            match event {
                Event::Picked(Purpose::Open, files) => {
                    let paths: Vec<PathBuf> =
                        files.into_iter().map(crate::web::store_file).collect();
                    if let Some(p) = primary_map_file(&paths) {
                        self.open(&p);
                    }
                }
                Event::Picked(Purpose::AddPlane, files) => {
                    let paths: Vec<PathBuf> =
                        files.into_iter().map(crate::web::store_file).collect();
                    if let Some(p) = primary_map_file(&paths) {
                        self.add_plane_from(&p);
                    }
                }
                Event::Picked(Purpose::Blueprint { cave }, files) => {
                    if let Some(f) = files.into_iter().next() {
                        if let Err(e) = self.gen.set_own(cave, f.name, &f.bytes) {
                            self.error = Some(e);
                        }
                    }
                }
                Event::Picked(Purpose::Mod, files) => {
                    if let Some(f) = files.into_iter().next() {
                        let text = String::from_utf8_lossy(&f.bytes).into_owned();
                        self.gen.load_mod_text(&f.name, &text);
                    }
                }
                Event::Directory(Some(name)) => {
                    self.error = None;
                    self.status = format!("Maps are read from and saved into {name}");
                }
                Event::Directory(None) => {
                    self.status = "No folder was chosen".to_owned();
                }
                Event::Listed(names) => self.folder_files = names,
                Event::Status(text) => {
                    self.error = None;
                    self.status = text;
                }
                Event::Error(text) => self.error = Some(text),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_file(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Dominions 6 map", &["d6m", "map"]);
        if let Some(p) = &self.project {
            dlg = dlg.set_directory(&p.dir);
        } else {
            let maps = self.settings.maps_dir();
            if maps.is_dir() {
                dlg = dlg.set_directory(maps);
            } else if let Some(d) = default_maps_dir() {
                dlg = dlg.set_directory(d);
            }
        }
        if let Some(path) = dlg.pick_file() {
            self.open(&path);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if typing {
            return;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::N)) && !self.gen.is_running() {
            self.mode = Mode::Generator;
        }
        let generate =
            ctx.input(|i| !i.modifiers.command && !i.modifiers.alt && i.key_pressed(Key::G));
        if generate && !self.gen.is_running() {
            self.mode = Mode::Generator;
            self.pending_generate = true;
        }
        let (undo, redo, save, open, fit, esc, help, tab) = ctx.input(|i| {
            let c = i.modifiers.command;
            (
                c && i.key_pressed(Key::Z) && !i.modifiers.shift,
                c && (i.key_pressed(Key::Y) || (i.key_pressed(Key::Z) && i.modifiers.shift)),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::O),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::F1),
                i.key_pressed(Key::Tab),
            )
        });
        let (select_key, link_key, paint_key, height_key, random_key, next_plane, prev_plane) = ctx
            .input(|i| {
                let plain = !i.modifiers.command && !i.modifiers.alt;
                (
                    plain && i.key_pressed(Key::S),
                    plain && i.key_pressed(Key::L),
                    plain && i.key_pressed(Key::P),
                    plain && i.key_pressed(Key::H),
                    i.key_pressed(Key::F4),
                    i.key_pressed(Key::PageDown),
                    i.key_pressed(Key::PageUp),
                )
            });
        if height_key {
            self.tool = if self.tool == Tool::Height {
                Tool::Select
            } else {
                Tool::Height
            };
        }
        if random_key {
            self.randomize_terrain();
        }
        if select_key {
            self.tool = Tool::Select;
        }
        if link_key {
            self.tool = Tool::Link;
        }
        if paint_key {
            self.tool = if self.tool == Tool::Paint {
                Tool::Select
            } else {
                Tool::Paint
            };
        }
        if next_plane || prev_plane {
            let n = self.project.as_ref().map(|p| p.planes.len()).unwrap_or(0);
            if n > 1 {
                let i = if next_plane {
                    (self.active + 1) % n
                } else {
                    (self.active + n - 1) % n
                };
                self.switch_plane(i);
            }
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if save {
            self.save();
        }
        if open {
            self.pending = Pending::Open(None);
        }
        if fit {
            self.fit_pending = true;
        }
        if esc && self.renaming {
            self.renaming = false;
        } else if esc {
            if self.show_help {
                self.show_help = false;
            } else if self.placing_capital || self.placing_new {
                self.placing_capital = false;
                self.placing_new = false;
                self.status = "Cancelled".to_owned();
            } else if self.tool != Tool::Select {
                self.tool = Tool::Select;
            } else {
                self.select(None);
            }
        }
        if help {
            self.show_help = !self.show_help;
        }
        if tab {
            self.tool = match self.tool {
                Tool::Select => Tool::Link,
                Tool::Link => Tool::Paint,
                Tool::Paint => Tool::Height,
                Tool::Height => Tool::Select,
            };
        }
        let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            self.dropped(dropped);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dropped(&mut self, files: Vec<egui::DroppedFile>) {
        if let Some(p) = files.into_iter().filter_map(|f| f.path).next() {
            self.pending = Pending::Open(Some(p));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn dropped(&mut self, files: Vec<egui::DroppedFile>) {
        let paths: Vec<PathBuf> = files
            .into_iter()
            .filter_map(|f| {
                let bytes = f.bytes?;
                Some(crate::web::store_file(crate::web::PickedFile {
                    name: f.name,
                    bytes: bytes.to_vec(),
                    handle: None,
                }))
            })
            .collect();
        if let Some(p) = primary_map_file(&paths) {
            self.pending = Pending::Open(Some(p));
        }
    }

    fn switch_plane(&mut self, i: usize) {
        if self.active == i {
            return;
        }
        self.active = i;
        self.apply_view(i);
        self.fit_pending = true;
        self.selected = None;
        self.name_for = None;
        if let Some(d) = self.doc() {
            self.status = format!(
                "Plane {}: {} x {} px, {} provinces",
                d.index,
                d.width(),
                d.height(),
                d.province_count()
            );
        }
    }

    fn capital_near(&self, canvas: egui::Rect, pos: Pos2) -> Option<u32> {
        let (hw, vw) = self.wrap_axes();
        let doc = self.doc()?;
        let h = doc.height();
        let wf = doc.width() as f32;
        let hf = h as f32;
        let v = (pos - canvas.min - self.offset) / self.zoom;
        let mut best = None;
        let mut best_d = 8.0_f32 * 8.0;
        for (i, &(cx, cy)) in doc.capitals.iter().enumerate() {
            let dx = wrap_delta(v.x - (cx as f32 + 0.5), wf, hw) * self.zoom;
            let dy = wrap_delta(v.y - ((h - 1 - cy as i32) as f32 + 0.5), hf, vw) * self.zoom;
            let d = dx * dx + dy * dy;
            if d < best_d {
                best_d = d;
                best = Some(i as u32 + 1);
            }
        }
        best
    }

    fn remove_province(&mut self) {
        let Some(sel) = self.selected else {
            return;
        };
        let res = self.with_doc(|d, tex, opts| d.remove_province(sel, tex, opts));
        if res == Some(true) {
            self.select(None);
            self.after_edit(true, None, &format!("Removed province {sel}; its area went to the neighbours and later provinces moved down by one"));
        } else {
            self.status = "The last province cannot be removed".to_owned();
        }
    }

    fn place_capital(&mut self, x: i32, y: i32) {
        self.placing_capital = false;
        let Some(sel) = self.selected else {
            return;
        };
        let under = self.doc().map(|d| d.owner_at(x, y)).unwrap_or(0);
        if under != sel {
            self.status = if under == 0 {
                format!(
                    "That pixel belongs to no province; the capital of {sel} must lie inside it"
                )
            } else {
                format!("That pixel belongs to province {under}; the capital of {sel} must lie inside it")
            };
            return;
        }
        let res =
            self.with_doc(|d, tex, opts| (d.set_capital(sel, x, y, tex, opts), d.rendered.touched));
        if let Some((ok, rect)) = res {
            self.after_edit(
                ok,
                Some(rect),
                &format!("Moved the capital of {sel} to {x}, {y}"),
            );
        }
    }

    fn centre_capital(&mut self) {
        let Some(sel) = self.selected else {
            return;
        };
        let res =
            self.with_doc(|d, tex, opts| (d.centre_capital(sel, tex, opts), d.rendered.touched));
        if let Some((ok, rect)) = res {
            self.after_edit(ok, Some(rect), &format!("Centred the capital of {sel}"));
        }
    }

    fn place_new_province(&mut self, x: i32, y: i32) {
        self.placing_new = false;
        let r = self.brush;
        let res = self
            .with_doc(|d, tex, opts| d.add_province(x, y, r, tex, opts))
            .flatten();
        match res {
            Some(p) => {
                self.mark_tiles(None);
                self.select(Some(p));
                self.tool = Tool::Paint;
                self.paint_empty = false;
                self.status =
                    format!("Province {p} added with its capital at {x}, {y}; paint its area now");
            }
            None => self.status = "Could not add a province there".to_owned(),
        }
    }

    fn canvas_click(&mut self, prov: u32, x: i32, y: i32) {
        match self.tool {
            Tool::Select => self.select(Some(prov)),
            Tool::Link => {
                let Some(sel) = self.selected else {
                    self.select(Some(prov));
                    return;
                };
                if sel == prov {
                    return;
                }
                let linked = self.doc().map(|d| d.linked(sel, prov)).unwrap_or(false);
                let res = self.with_doc(|d, tex, opts| {
                    let ok = d.set_link(sel, prov, !linked, tex, opts);
                    (ok, union(d.bbox(sel), d.bbox(prov)))
                });
                if let Some((ok, rect)) = res {
                    let verb = if linked {
                        "Removed the connection between"
                    } else {
                        "Connected"
                    };
                    self.after_edit(ok, rect, &format!("{verb} {sel} and {prov}"));
                }
            }
            Tool::Paint | Tool::Height => {
                let _ = (x, y);
            }
        }
    }

    fn cycle_link(&mut self, prov: u32) {
        let Some(sel) = self.selected else {
            self.status = "Select a province first, then right-click a connected one".to_owned();
            return;
        };
        if sel == prov {
            return;
        }
        let Some(doc) = self.doc() else {
            return;
        };
        if !doc.linked(sel, prov) {
            self.status = format!("{sel} and {prov} are not connected");
            return;
        }
        let current = doc.spec(sel, prov);
        let i = BORDER_KINDS
            .iter()
            .position(|(v, _)| *v == current)
            .unwrap_or(0);
        let (next, label) = BORDER_KINDS[(i + 1) % BORDER_KINDS.len()];
        let res = self.with_doc(|d, tex, opts| {
            let ok = d.set_spec(sel, prov, next, tex, opts);
            (ok, union(d.bbox(sel), d.bbox(prov)))
        });
        if let Some((ok, rect)) = res {
            self.after_edit(ok, rect, &format!("{sel} to {prov}: {label}"));
        }
    }

    fn undo_all(&mut self) {
        let n = self
            .with_doc(|d, tex, opts| d.undo_all(tex, opts))
            .unwrap_or(0);
        self.after_edit(n > 0, None, &format!("Undid all {n} edits on this plane"));
    }

    fn randomize_terrain(&mut self) {
        let res = self.with_doc(|d, tex, opts| d.randomize_terrain(tex, opts));
        let n = res.unwrap_or(0);
        self.after_edit(
            n > 0,
            None,
            &format!("Random terrain: {n} provinces changed on this plane"),
        );
    }

    fn set_no_starts(&mut self) {
        let (min, crossing) = (self.nostart_min, self.nostart_crossing);
        let res = self.with_doc(|d, tex, opts| d.set_no_starts(min, crossing, tex, opts));
        let n = res.unwrap_or(0);
        self.after_edit(
            n > 0,
            None,
            &format!("No start set on {n} provinces with fewer than {min:.1} connections"),
        );
    }

    fn clear_no_starts(&mut self) {
        let res = self.with_doc(|d, tex, opts| d.clear_no_starts(tex, opts));
        let n = res.unwrap_or(0);
        self.after_edit(n > 0, None, &format!("No start cleared on {n} provinces"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn add_plane(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Dominions 6 map", &["d6m", "map"]);
        if let Some(p) = &self.project {
            dlg = dlg.set_directory(&p.dir);
        }
        let Some(path) = dlg.pick_file() else {
            return;
        };
        self.add_plane_from(&path);
    }

    #[cfg(target_arch = "wasm32")]
    fn add_plane(&mut self) {
        crate::web::pick(
            crate::web::Purpose::AddPlane,
            crate::web::MAP_FILES,
            true,
            self.ctx.clone(),
        );
    }

    fn add_plane_from(&mut self, path: &Path) {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let res = self
            .project
            .as_mut()
            .map(|p| p.add_plane(path, &tex, &opts));
        self.tex = tex;
        match res {
            Some(Ok(n)) => {
                if let Some(p) = &self.project {
                    let d = p.planes.last().unwrap();
                    self.tiles.push(TileGrid::new(d.width(), d.height()));
                    self.overlays.push(Overlay::new(d.width(), d.height()));
                    self.decor_tiles.push(TileGrid::new(d.width(), d.height()));
                    self.relief_tiles.push(TileGrid::new(d.width(), d.height()));
                    self.relief.push(Vec::new());
                    self.relief_range.push((-1.0, 1.0));
                    self.relief_stale.push(true);
                    self.thumbs.push(None);
                    self.thumb_img.push(None);
                    self.thumb_stale.push(true);
                    self.thumb_dirty.push(None);
                    self.rendered_opts.push(self.opts);
                    let last = p.planes.len() - 1;
                    self.switch_plane(last);
                }
                self.apply_river_repair();
                self.status = format!("Added plane {n} from {}", crate::settings::shown(path));
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(e),
            None => {}
        }
    }

    fn remove_last_plane(&mut self) {
        let res = self.project.as_mut().map(|p| p.remove_last_plane());
        match res {
            Some(Ok(moved)) => {
                self.tiles.pop();
                self.overlays.pop();
                self.decor_tiles.pop();
                self.relief_tiles.pop();
                self.relief.pop();
                self.relief_range.pop();
                self.relief_stale.pop();
                self.thumbs.pop();
                self.thumb_img.pop();
                self.thumb_stale.pop();
                self.thumb_dirty.pop();
                self.rendered_opts.pop();
                let n = self.project.as_ref().map(|p| p.planes.len()).unwrap_or(0);
                if self.active >= n {
                    self.active = n.saturating_sub(1);
                    self.fit_pending = true;
                    self.selected = None;
                    self.name_for = None;
                }
                let names: Vec<String> = moved
                    .iter()
                    .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .collect();
                self.status = if crate::io::IS_WEB {
                    "Removed the last plane; its files on disk were left alone".to_owned()
                } else {
                    format!(
                        "Removed the last plane; its files were kept as {}",
                        names.join(", ")
                    )
                };
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(e),
            None => {}
        }
    }

    fn stroke_points(&mut self, x: i32, y: i32, spacing: f32) -> Vec<(i32, i32, f32)> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        let (w, h) = (doc.width(), doc.height());
        let (hw, vw) = (doc.hwrap(), doc.vwrap());
        let out = match self.stroke_last {
            None => vec![(x, y, 1.0)],
            Some((lx, ly)) => {
                let dx = wrap_delta((x - lx) as f32, w as f32, hw);
                let dy = wrap_delta((y - ly) as f32, h as f32, vw);
                let dist = (dx * dx + dy * dy).sqrt();
                let n = (dist / spacing).floor() as i32;
                (1..=n)
                    .map(|k| {
                        let t = k as f32 * spacing / dist;
                        let (px, py) = wrap_point(
                            (lx as f32 + dx * t).round() as i32,
                            (ly as f32 + dy * t).round() as i32,
                            w,
                            h,
                            hw,
                            vw,
                        );
                        (px, py, 0.0)
                    })
                    .collect()
            }
        };
        if let Some(&(sx, sy, _)) = out.last() {
            self.stroke_last = Some((sx, sy));
        }
        out
    }

    fn paint_at(&mut self, x: i32, y: i32, remove: bool) {
        let r = self.brush;
        let res = if self.tool == Tool::Height {
            let sign = if remove { -1.0 } else { 1.0 };
            let keep = self.keep_rivers;
            let step = self.step * sign;
            let from = self.stroke_last;
            self.stroke_last = Some((x, y));
            match from {
                None => self
                    .with_doc(|d, tex, opts| {
                        d.paint_height_stamps(&[(x, y, step)], r, keep, tex, opts)
                    })
                    .flatten(),
                Some((fx, fy)) if (fx, fy) == (x, y) => None,
                Some((fx, fy)) => self
                    .with_doc(|d, tex, opts| {
                        d.paint_height_path((fx, fy), (x, y), r, step, keep, tex, opts)
                    })
                    .flatten(),
            }
        } else {
            let prov = if self.paint_empty {
                0
            } else {
                self.selected.unwrap_or(0)
            };
            if !remove && !self.paint_empty && prov == 0 {
                return;
            }
            let spacing = (r as f32 / 2.0).max(1.0);
            let points: Vec<(i32, i32)> = self
                .stroke_points(x, y, spacing)
                .into_iter()
                .map(|(sx, sy, _)| (sx, sy))
                .collect();
            let target = if remove { None } else { Some(prov) };
            self.with_doc(|d, tex, opts| d.paint_many(target, &points, r, tex, opts))
                .flatten()
        };
        if let Some(rect) = res {
            self.mark_tiles(Some(rect));
            self.stroke_decor = union(self.stroke_decor, Some(rect));
            if self.tool == Tool::Paint {
                self.refresh_selection_in(rect);
            }
        }
    }

    fn stroke_decor_tick(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.stroke_decor else {
            return;
        };
        let follow = self.tool == Tool::Height && self.terrain_follows_height;
        if !self.opts.decor && !follow {
            self.stroke_decor = None;
            return;
        }
        let wait = (self.stroke_decor_cost * 4).max(web_time::Duration::from_millis(150));
        let since = self.stroke_decor_at.elapsed();
        if since < wait {
            ctx.request_repaint_after(wait - since);
            return;
        }
        let started = web_time::Instant::now();
        let mut touched = None;
        if follow {
            let (r, became) = self
                .with_doc(|d, tex, opts| d.follow_terrain_in(rect, tex, opts))
                .unwrap_or((None, Vec::new()));
            touched = union(touched, r);
            if !became.is_empty() {
                self.status = became_status(&became);
            }
        }
        if self.opts.decor {
            let area = union(Some(rect), touched).unwrap_or(rect);
            let done = self
                .with_doc(|d, tex, opts| d.refresh_decor(area, tex, opts))
                .flatten();
            touched = union(touched, done);
        }
        self.stroke_decor_cost = started.elapsed();
        self.stroke_decor_at = web_time::Instant::now();
        self.stroke_decor = None;
        if let Some(r) = touched {
            self.mark_tiles(Some(r));
        }
    }

    fn zoom_about(&mut self, canvas: egui::Rect, pos: Pos2, factor: f32) {
        if factor == 1.0 || !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, 16.0);
        let k = new_zoom / self.zoom;
        let rel = pos - canvas.min;
        self.offset = rel - (rel - self.offset) * k;
        self.zoom = new_zoom;
    }

    fn end_stroke(&mut self) {
        self.painting = None;
        self.stroke_phys = None;
        self.stroke_last = None;
        self.stroke_decor = None;
        let follow = self.tool == Tool::Height && self.terrain_follows_height;
        let done = self.with_doc(|d, tex, opts| d.paint_end_follow(follow, tex, opts));
        if let Some((rect, became)) = done {
            if let Some(rect) = rect {
                self.mark_tiles(Some(rect));
            }
            if !became.is_empty() {
                self.status = became_status(&became);
            }
        }
        self.refresh_selection();
    }

    fn draw_canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::CANVAS))
            .show(ctx, |ui| {
                let (canvas, resp) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if self.project.is_none() {
                    if resp.clicked() {
                        self.pending = Pending::Open(None);
                    }
                    let p = canvas.center();
                    let main = match (self.lay.compact(), crate::io::IS_WEB) {
                        (true, true) => "Tap here to open a map: pick its .d6m and .map together",
                        (true, false) => "Tap here to open a map, or use Generate",
                        (false, true) => "Drop a map folder here, or its .d6m and .map together",
                        (false, false) => "Drop a .d6m or .map here, or use Open map",
                    };
                    ui.painter().text(
                        p,
                        Align2::CENTER_CENTER,
                        main,
                        FontId::proportional(20.0),
                        theme::INK_DIM,
                    );
                    return;
                }
                let size = canvas.size();
                if let Some(old) = self.last_canvas {
                    if old != size {
                        self.offset += (size - old) * 0.5;
                    }
                }
                self.last_canvas = Some(size);
                let axes_now = self.wrap_axes();
                if axes_now != self.last_wrap_axes {
                    self.fit_pending = true;
                }
                self.last_wrap_axes = axes_now;
                if self.fit_pending {
                    self.fit(canvas);
                    self.fit_pending = false;
                }
                if let Some(p) = self.center_pending.take() {
                    self.center_on(canvas, p);
                }
                let (want_hwrap, want_vwrap) = self.wrap_axes();
                let (map_w, map_h) = self
                    .doc()
                    .map(|d| (d.width(), d.height()))
                    .unwrap_or((0, 0));
                let hwrap = want_hwrap && wrap_fits(map_w as f32 * self.zoom, canvas.width());
                let vwrap = want_vwrap && wrap_fits(map_h as f32 * self.zoom, canvas.height());
                if map_w > 0 {
                    let span = map_w as f32 * self.zoom;
                    if hwrap {
                        self.offset.x = self.offset.x.rem_euclid(span) - span;
                    } else if span <= canvas.width() {
                        self.offset.x = (canvas.width() - span) * 0.5;
                    }
                }
                if map_h > 0 {
                    let span = map_h as f32 * self.zoom;
                    if vwrap {
                        self.offset.y = self.offset.y.rem_euclid(span) - span;
                    } else if span <= canvas.height() {
                        self.offset.y = (canvas.height() - span) * 0.5;
                    }
                }
                let paint_tool = matches!(self.tool, Tool::Paint | Tool::Height);
                let ctrl = ctx.input(|i| i.modifiers.command);
                let placing = self.placing_capital || self.placing_new;
                let jump_click = self.tool == Tool::Select
                    && !placing
                    && (resp.double_clicked_by(PointerButton::Primary)
                        || resp.triple_clicked_by(PointerButton::Primary));
                let multi = ctx.input(|i| i.multi_touch());
                if let Some(mt) = multi {
                    if self.painting.is_some() {
                        self.end_stroke();
                    }
                    self.offset += mt.translation_delta;
                    self.zoom_about(canvas, mt.center_pos, mt.zoom_delta);
                }
                let gesture = multi.is_some();
                let pan = !gesture
                    && (resp.dragged_by(PointerButton::Middle)
                        || (!paint_tool && resp.dragged_by(PointerButton::Secondary))
                        || ((!paint_tool || ctrl)
                            && !jump_click
                            && resp.dragged_by(PointerButton::Primary)));
                if pan {
                    self.offset += resp.drag_delta();
                }
                if let Some(pos) = resp.hover_pos().filter(|_| !gesture) {
                    let scroll = ctx.input(|i| i.raw_scroll_delta.y);
                    let zd = ctx.input(|i| i.zoom_delta());
                    let factor = if scroll != 0.0 {
                        (scroll / 120.0 * 0.25).exp()
                    } else {
                        zd
                    };
                    self.zoom_about(canvas, pos, factor);
                    let (x, y) = self.screen_to_map(canvas, pos);
                    self.hover = self.doc().map(|d| d.owner_at(x, y)).filter(|&p| p > 0);
                    if placing && resp.clicked_by(PointerButton::Primary) {
                        if self.placing_new {
                            self.place_new_province(x, y);
                        } else {
                            self.place_capital(x, y);
                        }
                    } else if jump_click {
                        let under = self.capital_near(canvas, pos).or_else(|| {
                            self.doc().and_then(|d| {
                                let p = d.owner_at(x, y);
                                (p > 0).then_some(p)
                            })
                        });
                        let same_spot = under.is_some()
                            && self.last_click_prov == under.map(|p| (self.active, p));
                        self.last_click_prov = under.map(|p| (self.active, p));
                        if let Some(p) = under {
                            self.select(Some(p));
                            if same_spot {
                                self.jump_to_gateway(p);
                                self.last_click_prov = None;
                            }
                        }
                    } else if self.tool == Tool::Link && resp.clicked_by(PointerButton::Secondary) {
                        if let Some(p) = self.hover {
                            self.cycle_link(p);
                        }
                    } else if !paint_tool && resp.clicked_by(PointerButton::Primary) {
                        let dot = if self.tool == Tool::Select {
                            self.capital_near(canvas, pos)
                        } else {
                            None
                        };
                        self.last_click_prov = dot.or(self.hover).map(|p| (self.active, p));
                        if let Some(p) = dot {
                            self.select(Some(p));
                        } else if let Some(p) = self.hover {
                            self.canvas_click(p, x, y);
                        } else if self.tool == Tool::Select {
                            self.select(None);
                        }
                    }
                    if paint_tool && !ctrl && !placing {
                        let (primary, secondary) =
                            ctx.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
                        let phys = if primary {
                            Some(PointerButton::Primary)
                        } else if secondary {
                            Some(PointerButton::Secondary)
                        } else {
                            None
                        };
                        if let Some(pb) = phys {
                            let b = if pb == PointerButton::Primary && self.paint_erase {
                                PointerButton::Secondary
                            } else {
                                pb
                            };
                            let started = resp.contains_pointer() || self.stroke_phys == Some(pb);
                            if started {
                                if self.stroke_phys != Some(pb) {
                                    self.painting = Some(b);
                                    self.stroke_phys = Some(pb);
                                    self.stroke_last = None;
                                    let label = match (self.tool, b) {
                                        (Tool::Height, _) => "Height brush",
                                        (_, PointerButton::Primary) => "Paint area",
                                        _ => "Remove area",
                                    };
                                    self.with_doc(|d, _, _| d.paint_begin(label));
                                }
                                self.paint_at(x, y, b == PointerButton::Secondary);
                                self.stroke_decor_tick(ctx);
                            }
                        }
                    }
                } else {
                    self.hover = None;
                }
                if let Some(pb) = self.stroke_phys {
                    let still = ctx.input(|i| i.pointer.button_down(pb));
                    if !still {
                        self.end_stroke();
                    }
                }
                let active = self.active;
                let relief_shown = self.relief_shown();
                if relief_shown && self.relief_stale.get(active).copied().unwrap_or(false) {
                    self.refresh_relief(None);
                }
                self.refresh_thumb(ctx);
                if let Some(project) = &self.project {
                    let doc = &project.planes[active];
                    if let Some(t) = self.tiles.get_mut(active) {
                        t.upload(ctx, &doc.rendered.rgba, &format!("map{}", doc.index), false);
                    }
                    if relief_shown {
                        if let (Some(t), Some(buf)) =
                            (self.relief_tiles.get_mut(active), self.relief.get(active))
                        {
                            if buf.len() == doc.rendered.rgba.len() {
                                t.upload(ctx, buf, &format!("relief{}", doc.index), false);
                            }
                        }
                    }
                    if self.opts.decor && doc.rendered.decor.len() == doc.rendered.rgba.len() {
                        if let Some(t) = self.decor_tiles.get_mut(active) {
                            t.upload(
                                ctx,
                                &doc.rendered.decor,
                                &format!("decor{}", doc.index),
                                true,
                            );
                        }
                    }
                    if let Some(o) = self.overlays.get_mut(active) {
                        let name = format!("sel{}", doc.index);
                        if let Some(t) = &mut o.tiles {
                            t.upload(ctx, &o.rgba, &name, false);
                        }
                    }
                }
                let painter = ui.painter_at(canvas);
                let start_tags: Vec<(u32, String)> = self
                    .project
                    .as_ref()
                    .map(|p| {
                        p.specstarts()
                            .into_iter()
                            .filter(|&(_, pl, _)| pl == self.active)
                            .map(|(n, _, pr)| (pr, format!("{} start", self.gen.nation_name(n))))
                            .collect()
                    })
                    .unwrap_or_default();
                let Some(doc) = self.doc() else {
                    return;
                };
                let w = doc.width();
                let h = doc.height();
                let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                let shifts = wrap_shifts(canvas, self.offset, self.zoom, w, h, hwrap, vwrap);
                {
                    let mut layers = if relief_shown {
                        vec![&self.relief_tiles[self.active]]
                    } else {
                        vec![&self.tiles[self.active]]
                    };
                    if self.opts.decor && !relief_shown {
                        layers.push(&self.decor_tiles[self.active]);
                    }
                    if let Some(t) = &self.overlays[self.active].tiles {
                        layers.push(t);
                    }
                    for grid in layers {
                        for ty in 0..grid.rows {
                            for tx in 0..grid.cols {
                                let Some(hnd) = &grid.handles[ty * grid.cols + tx] else {
                                    continue;
                                };
                                let x0 = (grid.ox + tx * TILE) as f32;
                                let y0 = (grid.oy + ty * TILE) as f32;
                                let tw = (grid.w.saturating_sub(tx * TILE).min(TILE)) as f32;
                                let th = (grid.h.saturating_sub(ty * TILE).min(TILE)) as f32;
                                let mut mesh = egui::Mesh::with_texture(hnd.id());
                                for shift in &shifts {
                                    let r = egui::Rect::from_min_max(
                                        self.map_to_screen_shift(canvas, x0, y0, *shift),
                                        self.map_to_screen_shift(canvas, x0 + tw, y0 + th, *shift),
                                    );
                                    if r.intersects(canvas) {
                                        mesh.add_rect_with_uv(r, uv, Color32::WHITE);
                                    }
                                }
                                if !mesh.is_empty() {
                                    painter.add(egui::Shape::mesh(mesh));
                                }
                            }
                        }
                    }
                }
                for shift in &shifts {
                    let (phw, pvw) = (doc.hwrap(), doc.vwrap());
                    let link_ends = |a: (i16, i16), b: (i16, i16)| -> [(Pos2, Pos2); 2] {
                        let ax = a.0 as f32 + 0.5;
                        let ay = (h - 1 - a.1 as i32) as f32 + 0.5;
                        let bx = b.0 as f32 + 0.5;
                        let by = (h - 1 - b.1 as i32) as f32 + 0.5;
                        let dx = wrap_delta(bx - ax, w as f32, phw);
                        let dy = wrap_delta(by - ay, h as f32, pvw);
                        [
                            (
                                self.map_to_screen_shift(canvas, ax, ay, *shift),
                                self.map_to_screen_shift(canvas, ax + dx, ay + dy, *shift),
                            ),
                            (
                                self.map_to_screen_shift(canvas, bx - dx, by - dy, *shift),
                                self.map_to_screen_shift(canvas, bx, by, *shift),
                            ),
                        ]
                    };
                    if self.show_links && self.zoom >= DOT_MIN_ZOOM {
                        let thin = 2.0_f32 * self.zoom.sqrt().clamp(0.6, 1.5);
                        for p in 1..=doc.province_count() as u32 {
                            let ca = doc.capitals[p as usize - 1];
                            for nb in doc.neighbours(p) {
                                if nb <= p {
                                    continue;
                                }
                                let Some(&cb) = doc.capitals.get(nb as usize - 1) else {
                                    continue;
                                };
                                let ends = link_ends(ca, cb);
                                let both = ends[0].1 != ends[1].1;
                                let col = link_colour(doc.spec(p, nb));
                                for (i, (a, b)) in ends.into_iter().enumerate() {
                                    if i == 1 && !both {
                                        break;
                                    }
                                    if !canvas.intersects(egui::Rect::from_two_pos(a, b)) {
                                        continue;
                                    }
                                    painter.line_segment([a, b], egui::Stroke::new(thin, col));
                                }
                            }
                        }
                    }
                    if self.tool == Tool::Link {
                        if let Some(sel) = self.selected {
                            for nb in doc.neighbours(sel) {
                                let ca = doc.capitals[sel as usize - 1];
                                let Some(&cb) = doc.capitals.get(nb as usize - 1) else {
                                    continue;
                                };
                                let ends = link_ends(ca, cb);
                                let both = ends[0].1 != ends[1].1;
                                let col = link_colour(doc.spec(sel, nb));
                                painter.line_segment(
                                    [ends[0].0, ends[0].1],
                                    egui::Stroke::new(3.0_f32, col),
                                );
                                painter.circle_filled(ends[0].1, 6.0, col);
                                if both {
                                    painter.line_segment(
                                        [ends[1].0, ends[1].1],
                                        egui::Stroke::new(3.0_f32, col),
                                    );
                                    painter.circle_filled(ends[1].1, 6.0, col);
                                }
                            }
                        }
                    }
                    let labels_on = self.zoom >= LABEL_MIN_ZOOM;
                    let dots_on = self.zoom >= DOT_MIN_ZOOM;
                    let dot_r = (3.0 * (self.zoom / LABEL_MIN_ZOOM).sqrt()).clamp(1.5, 3.0);
                    if (self.show_names || self.show_markers || self.show_terrain) && dots_on {
                        let size = (14.0 * self.zoom.sqrt()).clamp(11.0, 26.0);
                        let badge = (12.0 * self.zoom.sqrt()).clamp(10.0, 18.0);
                        let halo = Color32::from_rgba_unmultiplied(255, 244, 214, 150);
                        for (i, &(cx, cy)) in doc.capitals.iter().enumerate() {
                            let id = i as u32 + 1;
                            let centre = self.map_to_screen_shift(
                                canvas,
                                cx as f32 + 0.5,
                                (h - 1 - cy as i32) as f32 + 0.5,
                                *shift,
                            );
                            if !canvas.expand(80.0).contains(centre) {
                                continue;
                            }
                            if self.show_names {
                                let name = doc.name(id);
                                let mut p = centre - Vec2::new(0.0, 8.0);
                                let outlined = |p: Pos2, text: &str, font: FontId, col: Color32| {
                                    for (dx, dy) in
                                        [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
                                    {
                                        painter.text(
                                            p + Vec2::new(dx, dy),
                                            Align2::CENTER_BOTTOM,
                                            text,
                                            font.clone(),
                                            halo,
                                        );
                                    }
                                    painter.text(p, Align2::CENTER_BOTTOM, text, font, col)
                                };
                                if !name.is_empty() {
                                    let r = outlined(
                                        p,
                                        name,
                                        FontId::proportional(size),
                                        theme::NAME_RED,
                                    );
                                    p.y -= r.height() - 2.0;
                                }
                                outlined(
                                    p,
                                    &format!("{id}"),
                                    FontId::proportional(size * 0.85),
                                    theme::NUMBER_BLUE,
                                );
                            }
                            if self.show_markers || self.show_terrain {
                                let f = doc.flags.get(id as usize).copied().unwrap_or(0);
                                let gate = doc.gate(id);
                                let mut tags: Vec<(String, Color32, Vec<u16>)> = Vec::new();
                                if self.show_terrain && labels_on {
                                    let water = f & SEA != 0;
                                    tags.push((
                                        terrain_label(f),
                                        if water {
                                            theme::TAG_WATER
                                        } else {
                                            theme::TAG_LAND
                                        },
                                        Vec::new(),
                                    ));
                                }
                                if self.show_markers && labels_on {
                                    match start_tags.iter().find(|(p, _)| *p == id) {
                                        Some((_, label)) => {
                                            tags.push((label.clone(), theme::TAG_START, Vec::new()))
                                        }
                                        None if f & GOOD_START != 0 => tags.push((
                                            "Start".to_owned(),
                                            theme::TAG_START,
                                            Vec::new(),
                                        )),
                                        None => {}
                                    }
                                }
                                if self.show_markers && labels_on && f & NO_START != 0 {
                                    tags.push(("No start".to_owned(), theme::TAG_NO, Vec::new()));
                                }
                                if self.show_markers && labels_on && f & GOOD_THRONE != 0 {
                                    tags.push(("Throne".to_owned(), theme::TAG_THRONE, Vec::new()));
                                }
                                if self.show_markers && labels_on && f & BAD_THRONE != 0 {
                                    tags.push(("No throne".to_owned(), theme::TAG_NO, Vec::new()));
                                }
                                if self.show_markers && labels_on && f & MANY_SITES != 0 {
                                    tags.push(("Sites".to_owned(), theme::TAG_SITES, Vec::new()));
                                }
                                if self.show_markers && labels_on && gate != 0 {
                                    tags.push((
                                        format!("Gate {gate}"),
                                        theme::TAG_GATE,
                                        Vec::new(),
                                    ));
                                }
                                if self.show_markers {
                                    painter.circle(
                                        centre,
                                        dot_r,
                                        Color32::WHITE,
                                        egui::Stroke::new(1.0_f32, Color32::from_rgb(40, 30, 20)),
                                    );
                                }
                                let mut y = centre.y + 6.0;
                                for (text, col, icons) in tags {
                                    let galley = painter.layout_no_wrap(
                                        text,
                                        FontId::proportional(badge),
                                        Color32::from_rgb(20, 18, 16),
                                    );
                                    let icon = badge * 1.7;
                                    let icons_w = if icons.is_empty() {
                                        0.0
                                    } else {
                                        icons.len() as f32 * (icon + 2.0) + 2.0
                                    };
                                    let text_h = galley.size().y + 2.0;
                                    let h = if icons.is_empty() {
                                        text_h
                                    } else {
                                        text_h.max(icon + 2.0)
                                    };
                                    let sz = Vec2::new(galley.size().x + 8.0 + icons_w, h);
                                    let r = egui::Rect::from_center_size(
                                        Pos2::new(centre.x, y + sz.y * 0.5),
                                        sz,
                                    );
                                    painter.rect_filled(r, 3.0, col);
                                    painter.galley(
                                        r.min + Vec2::new(4.0, (h - galley.size().y) * 0.5),
                                        galley,
                                        Color32::BLACK,
                                    );
                                    let mut x = r.max.x - icons_w + 2.0;
                                    for id in icons {
                                        if let Some((_, tex)) =
                                            self.icons.iter().find(|(k, _)| *k == id)
                                        {
                                            let ir = egui::Rect::from_min_size(
                                                Pos2::new(x, r.min.y + (h - icon) * 0.5),
                                                Vec2::splat(icon),
                                            );
                                            painter.image(
                                                tex.id(),
                                                ir,
                                                egui::Rect::from_min_max(
                                                    Pos2::ZERO,
                                                    Pos2::new(1.0, 1.0),
                                                ),
                                                Color32::WHITE,
                                            );
                                        }
                                        x += icon + 2.0;
                                    }
                                    y += sz.y + 2.0;
                                }
                            }
                        }
                    }
                }
                if paint_tool {
                    if let Some(pos) = resp.hover_pos() {
                        let col = if self.painting == Some(PointerButton::Secondary) {
                            theme::WARN
                        } else {
                            theme::INK_HOT
                        };
                        painter.circle_stroke(
                            pos,
                            self.brush as f32 * self.zoom,
                            egui::Stroke::new(1.5_f32, col),
                        );
                    }
                }
                if let Some(hp) = self.hover {
                    if let Some(pos) = resp.hover_pos() {
                        let name = doc.name(hp);
                        let mut text = if name.is_empty() {
                            format!("{hp}")
                        } else {
                            format!("{hp}  {name}")
                        };
                        if relief_shown {
                            let (mx, my) = self.screen_to_map(canvas, pos);
                            if mx >= 0 && my >= 0 && mx < w && my < h {
                                let hv = doc.rendered.carved[(my * w + mx) as usize];
                                if crate::render::is_channel(hv) {
                                    text.push_str("  river");
                                } else {
                                    text.push_str(&format!("  height {hv:.0}"));
                                }
                            }
                        }
                        let galley =
                            painter.layout_no_wrap(text, FontId::proportional(15.0), theme::INK);
                        let tl = pos + Vec2::new(16.0, 14.0);
                        let r = egui::Rect::from_min_size(tl, galley.size() + Vec2::new(12.0, 6.0));
                        painter.rect_filled(r, 2.0, theme::PANEL_FILL);
                        painter.rect_stroke(
                            r,
                            2.0,
                            egui::Stroke::new(1.0_f32, theme::PANEL_EDGE),
                            StrokeKind::Outside,
                        );
                        painter.galley(tl + Vec2::new(6.0, 3.0), galley, theme::INK);
                    }
                }
            });
    }

    fn draw_side(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("d6sme_side")
            .exact_width(layout::SIDE_W)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDE_FILL)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(self.lay.inner_w());
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                self.mode_section(ui);
                theme::rule(ui);
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(self.lay.inner_w());
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                        if self.mode == Mode::Generator {
                            let blueprints = self.settings.blueprints_dir();
                            let doc_name = self.project.as_ref().map(|p| p.base.clone());
                            self.gen
                                .ui(ui, self.lay.section_w(), &blueprints, doc_name.as_deref());
                            return;
                        }
                        self.tools_section(ui);
                        ui.add_space(8.0);
                        if self.selected.is_some() {
                            self.province_section(ui);
                            ui.add_space(8.0);
                        }
                        if self.project.is_some() {
                            self.plane_section(ui);
                        }
                    });
            });
    }

    fn draw_right_side(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("d6sme_right")
            .exact_width(layout::SIDE_W)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDE_FILL)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("d6sme_right_foot")
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                        left: 0,
                        right: 2,
                        top: 4,
                        bottom: 0,
                    }))
                    .show_separator_line(false)
                    .show_inside(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            github_link(ui, self.github.as_ref());
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_max_width(self.lay.inner_w());
                                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                                self.map_section(ui);
                                ui.add_space(8.0);
                                if self.gen.action(ui, self.lay.section_w(), true) {
                                    self.pending_generate = true;
                                }
                                ui.add_space(8.0);
                                self.settings_section(ui);
                                ui.add_space(8.0);
                                self.view_section(ui);
                            });
                    });
            });
    }

    fn draw_sheet(&mut self, ctx: &egui::Context) {
        let sheet_h = self.lay.sheet_h();
        egui::TopBottomPanel::bottom("d6sme_sheet")
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDE_FILL)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(self.lay.inner_w());
                if let Some(sheet) = self.sheet {
                    egui::ScrollArea::vertical()
                        .id_salt(sheet.label())
                        .max_height(sheet_h)
                        .min_scrolled_height(sheet_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_max_width(self.lay.inner_w());
                            self.sheet_body(ui, sheet);
                        });
                    theme::rule(ui);
                }
                self.sheet_tabs(ui);
            });
    }

    fn sheet_tabs(&mut self, ui: &mut egui::Ui) {
        let n = Sheet::ALL.len();
        let gap = ui.spacing().item_spacing.x;
        let w = (self.lay.inner_w() - gap * (n as f32 - 1.0)) / n as f32;
        let h = 40.0;
        ui.horizontal(|ui| {
            for sheet in Sheet::ALL {
                let open = self.sheet == Some(sheet);
                let text = egui::RichText::new(sheet.label())
                    .size(16.0)
                    .color(if open { theme::INK_ACTIVE } else { theme::INK });
                let btn = egui::Button::new(text)
                    .min_size(egui::vec2(w, h))
                    .fill(if open {
                        theme::PANEL_FILL
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        if open {
                            theme::PANEL_EDGE
                        } else {
                            theme::PANEL_EDGE_DIM
                        },
                    ));
                if ui.add(btn).clicked() {
                    self.sheet = if open { None } else { Some(sheet) };
                }
            }
        });
    }

    fn sheet_body(&mut self, ui: &mut egui::Ui, sheet: Sheet) {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 9.0);
        match sheet {
            Sheet::Map => {
                self.map_section(ui);
                ui.add_space(6.0);
                self.settings_section(ui);
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    github_link(ui, self.github.as_ref());
                });
            }
            Sheet::Tools => {
                self.tools_section(ui);
                ui.add_space(6.0);
                if self.selected.is_some() {
                    self.province_section(ui);
                    ui.add_space(6.0);
                }
                if self.project.is_some() {
                    self.plane_section(ui);
                }
            }
            Sheet::Generate => {
                if self.gen.action(ui, self.lay.section_w(), false) {
                    self.pending_generate = true;
                }
                ui.add_space(6.0);
                let blueprints = self.settings.blueprints_dir();
                let doc_name = self.project.as_ref().map(|p| p.base.clone());
                self.gen
                    .ui(ui, self.lay.section_w(), &blueprints, doc_name.as_deref());
            }
            Sheet::View => self.view_section(ui),
        }
    }

    fn mode_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            ui.horizontal_wrapped(|ui| {
                if theme::tab(ui, self.mode == Mode::Editor, "Editor") {
                    self.mode = Mode::Editor;
                }
                if theme::tab(ui, self.mode == Mode::Generator, "Random Map Generation") {
                    self.mode = Mode::Generator;
                }
            })
            .response
            .on_hover_text("Editor works on the loaded map; Random Map Generation rolls a new one with the game's own generator (Ctrl+N)");
        });
    }

    fn reset_all_settings(&mut self) {
        self.settings.reset();
        self.gen.reset_form();
        let fresh = Options::default();
        let relook = self.opts != fresh;
        self.opts = fresh;
        self.wrap_view = false;
        self.relief_in_height = true;
        self.keep_rivers = true;
        self.terrain_follows_height = true;
        self.brush = 10;
        self.step = 10.0;
        self.paint_empty = false;
        self.flatten = false;
        self.show_markers = true;
        self.show_links = false;
        self.show_terrain = false;
        self.show_names = false;
        if relook {
            self.rerender_all();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn settings_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            theme::section_first(ui, "Folder");
            let folder = crate::io::dir_name();
            if crate::web::can_pick_directory() {
                if let Some(name) = &folder {
                    theme::dim(ui, name);
                }
                ui.horizontal_wrapped(|ui| {
                    if theme::boxed_button_hint(
                        ui,
                        "Choose folder",
                        true,
                        "The game's maps folder: its maps are listed here and Save writes into it",
                    ) {
                        crate::web::pick_directory(self.ctx.clone());
                    }
                    if folder.is_some() && theme::boxed_button(ui, "Forget", true) {
                        crate::web::forget_directory();
                        self.folder_files.clear();
                        self.status = "The folder is no longer used".to_owned();
                    }
                });
            }
            let maps: Vec<String> = self
                .folder_files
                .iter()
                .filter(|n| n.to_ascii_lowercase().ends_with(".d6m"))
                .filter(|n| {
                    let stem = n[..n.len() - 4].to_owned();
                    crate::mapfile::strip_plane_suffix(&stem).1 == 1
                })
                .cloned()
                .collect();
            if !maps.is_empty() {
                theme::section(ui, "Maps in the folder");
                let mut open = None;
                egui::ScrollArea::vertical()
                    .id_salt("folder_maps")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for name in &maps {
                            if theme::text_button(ui, &name[..name.len() - 4], true) {
                                open = Some(name.clone());
                            }
                        }
                    });
                if let Some(name) = open {
                    crate::web::open_from_directory(name, self.ctx.clone());
                }
            }
            if theme::boxed_button_hint(
                ui,
                "Reset all settings",
                true,
                "Puts the generator form, the view toggles and the tool settings back to how the program starts; the open map and its seed and name stay",
            ) {
                self.reset_all_settings();
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn settings_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            theme::section_first(ui, "Settings");
            let root = self.settings.root_dir();
            theme::dim(ui, "Path");
            theme::path_label(ui, &root, 13.0, theme::INK);
            theme::dim(
                ui,
                &format!(
                    "Maps in {}, blueprints in {}",
                    crate::settings::MAPS,
                    crate::settings::BLUEPRINTS
                ),
            );
            let mut changed = false;
            ui.horizontal_wrapped(|ui| {
                if theme::boxed_button_hint(
                    ui,
                    "Browse",
                    true,
                    "Keeps maps and blueprints under a folder of your choice instead of the game's own folder",
                ) {
                    let start = if root.is_dir() {
                        root.clone()
                    } else {
                        self.settings.default_folder()
                    };
                    let picked = rfd::FileDialog::new().set_directory(start).pick_folder();
                    if let Some(dir) = picked {
                        self.settings.set_data_folder(dir);
                        changed = true;
                    }
                }
                if theme::boxed_button_hint(
                    ui,
                    "Reset",
                    !self.settings.is_default(),
                    "Goes back to the game's own folder",
                ) {
                    self.settings.reset();
                    changed = true;
                }
                if theme::boxed_button_hint(ui, "Open folder", true, "Shows the folder on disk") {
                    match crate::settings::ensure(root.clone()).and_then(|d| reveal(&d)) {
                        Ok(()) => {
                            self.status = format!("Opened {}", crate::settings::shown(&root))
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            });
            if theme::boxed_button_hint(
                ui,
                "Reset all settings",
                true,
                "Puts the folder, the generator form, the view toggles and the tool settings back to how the program starts; the open map and its seed and name stay",
            ) {
                self.reset_all_settings();
                changed = true;
            }
            if changed {
                match self.settings.save() {
                    Ok(()) => {
                        self.error = None;
                        self.status = format!(
                            "Maps and blueprints are kept under {}",
                            crate::settings::shown(&self.settings.root_dir())
                        );
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        });
    }

    fn map_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            let title = self.project.as_ref().map(|p| p.base.clone()).unwrap_or_else(|| "Dominions 6 Simple Map Editor".to_owned());
            if self.renaming {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.name_draft)
                        .desired_width(self.lay.section_w() - 20.0)
                        .font(egui::FontId::proportional(20.0)),
                );
                if !field.has_focus() && !field.lost_focus() {
                    field.request_focus();
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.renaming = false;
                } else if field.lost_focus() {
                    self.renaming = false;
                    let wanted = self.name_draft.trim().to_owned();
                    if wanted.is_empty() {
                        self.error = Some("a map needs a name".to_owned());
                    } else if wanted != title {
                        if let Some(p) = &mut self.project {
                            p.rename(&wanted);
                        }
                        let now = self
                            .project
                            .as_ref()
                            .map(|p| p.base.clone())
                            .unwrap_or_default();
                        self.status = format!("Renamed to {now}");
                    }
                }
            } else {
                let label = ui.add(
                    egui::Label::new(egui::RichText::new(&title).size(20.0).color(theme::INK))
                        .sense(Sense::click()),
                );
                if self.project.is_some() {
                    label.clone().on_hover_text(
                        "Click to rename the map: the title inside the .map and the file names Save uses. Enter keeps the new name, Esc leaves it alone",
                    );
                    if label.clicked() {
                        self.renaming = true;
                        self.name_draft = title.clone();
                    }
                }
            }
            if let Some(p) = &self.project {
                let dims = p.planes.get(self.active).map(|d| format!("{} x {} px, {} provinces", d.width(), d.height(), d.province_count())).unwrap_or_default();
                theme::dim(ui, &dims);
                if let Some(Some(tex)) = self.thumbs.get(self.active) {
                    let side = self.lay.section_w();
                    let size = tex.size_vec2();
                    let scale = (side / size.x).min(160.0 / size.y);
                    let src = egui::load::SizedTexture::from_handle(tex);
                    let img = ui
                        .add(egui::Image::new(src).fit_to_exact_size(size * scale).sense(Sense::click()))
                        .on_hover_text("The whole plane; click to look there");
                    if let Some(sel) = self.selected {
                        let doc = p.planes.get(self.active);
                        let (pw, ph) = doc
                            .map(|d| (d.width() as f32, d.height() as f32))
                            .unwrap_or((1.0, 1.0));
                        let k = size * scale / Vec2::new(pw, ph);
                        let painter = ui.painter();
                        if let Some(d) = doc {
                            let cols = (size.x * scale).round().max(1.0) as i32;
                            let rows = (size.y * scale).round().max(1.0) as i32;
                            let fill = theme::INK_ACTIVE.gamma_multiply(0.75);
                            for ty in 0..rows {
                                let my = ph as i32 - 1 - ((ty as f32 + 0.5) / k.y) as i32;
                                let mut run: Option<i32> = None;
                                for tx in 0..=cols {
                                    let inside = tx < cols
                                        && d.owner_at(((tx as f32 + 0.5) / k.x) as i32, my) == sel;
                                    match (run, inside) {
                                        (None, true) => run = Some(tx),
                                        (Some(x0), false) => {
                                            let r = egui::Rect::from_min_max(
                                                img.rect.min + Vec2::new(x0 as f32, ty as f32),
                                                img.rect.min + Vec2::new(tx as f32, ty as f32 + 1.0),
                                            );
                                            painter.rect_filled(r, 0.0, fill);
                                            run = None;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if let Some((cx, cy)) = doc.and_then(|d| d.capital(sel)) {
                            let c = img.rect.min
                                + Vec2::new((cx as f32 + 0.5) * k.x, (ph - 1.0 - cy as f32 + 0.5) * k.y);
                            painter.circle_filled(c, 2.5, theme::INK_ACTIVE);
                        }
                    }
                    if let (Some(pos), Some(canvas)) = (img.interact_pointer_pos(), self.last_canvas) {
                        if img.clicked() {
                            let rel = (pos - img.rect.min) / (size * scale);
                            let (pw, ph) = p
                                .planes
                                .get(self.active)
                                .map(|d| (d.width() as f32, d.height() as f32))
                                .unwrap_or((1.0, 1.0));
                            let target = Vec2::new(rel.x * pw, rel.y * ph) * self.zoom;
                            self.offset = canvas * 0.5 - target;
                        }
                    }
                }
                match p.planes.get(self.active).filter(|_| !p.unsaved) {
                    Some(d) if crate::io::IS_WEB => {
                        let name = d.d6m_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        theme::dim(ui, &name);
                    }
                    Some(d) => theme::path_label(ui, &d.d6m_path, 14.0, theme::INK_DIM),
                    None => {
                        theme::dim(ui, "not saved yet");
                    }
                }
                let labels: Vec<String> = p
                    .planes
                    .iter()
                    .map(|d| plane_label(d.index))
                    .collect();
                let mut switch = None;
                ui.horizontal_wrapped(|ui| {
                    theme::dim(ui, "Plane");
                    for (i, label) in labels.iter().enumerate() {
                        if theme::tab(ui, self.active == i, label) {
                            switch = Some(i);
                        }
                    }
                    if theme::text_button(ui, "+", labels.len() < 9) {
                        self.pending = Pending::AddPlane;
                    }
                    if theme::text_button(ui, "\u{2212}", labels.len() > 1) {
                        self.remove_last_plane();
                    }
                })
                .response
                .on_hover_text("Every plane of the map is loaded from the files beside the one you opened; PageUp and PageDown switch planes. + adds a plane from another .d6m (copied beside this map, with a generated .map when it has none), \u{2212} removes the last plane and keeps its files as .removed");
                if let Some(i) = switch {
                    self.switch_plane(i);
                }
            }
            theme::rule(ui);
            let dirty = self.project.as_ref().map(|p| p.any_dirty()).unwrap_or(false);
            let can_undo = self.doc().map(|d| !d.undo.is_empty()).unwrap_or(false);
            let can_redo = self.doc().map(|d| !d.redo.is_empty()).unwrap_or(false);
            ui.horizontal_wrapped(|ui| {
                if theme::boxed_button(ui, "Open", true) {
                    self.pending = Pending::Open(None);
                }
                if theme::boxed_button(ui, if dirty { "Save *" } else { "Save" }, dirty) {
                    self.save();
                }
                if theme::boxed_button(ui, "Undo", can_undo) {
                    self.undo();
                }
                if theme::boxed_button(ui, "Redo", can_redo) {
                    self.redo();
                }
                if theme::boxed_button_hint(ui, "Undo all", can_undo, "Takes back every edit made to this plane since it was opened; Redo brings them back one by one") {
                    self.undo_all();
                }
                if theme::boxed_button(ui, "Help", true) {
                    self.show_help = !self.show_help;
                }
                if !crate::io::IS_WEB && theme::boxed_button(ui, "Exit", true) {
                    self.pending = Pending::Close;
                }
            });
            theme::rule(ui);
            if let Some(e) = &self.error {
                ui.label(egui::RichText::new(e).color(theme::WARN));
            } else {
                ui.label(egui::RichText::new(&self.status).size(13.0).color(theme::INK_DIM));
            }
        });
    }

    fn province_section(&mut self, ui: &mut egui::Ui) {
        let Some(prov) = self.selected else {
            return;
        };
        let Some(doc) = self.doc() else {
            return;
        };
        let name = doc.name(prov).to_owned();
        let flags = doc.flags.get(prov as usize).copied().unwrap_or(0);
        let st = doc.stats(prov);
        let gate = doc.gate(prov);
        let has_map = doc.has_map();
        let neighbours: Vec<(u32, String, i64)> = doc
            .neighbours(prov)
            .into_iter()
            .map(|n| (n, doc.name(n).to_owned(), doc.spec(prov, n)))
            .collect();
        if self.name_for != Some(prov) {
            self.name_edit = name.clone();
            self.name_for = Some(prov);
            self.gate_edit = gate;
        }
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            ui.horizontal(|ui| {
                theme::title(ui, &format!("{prov}"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.name_edit).hint_text("Province name").desired_width(ui.available_width() - 8.0));
                if r.lost_focus() && self.name_edit != name {
                    let n = self.name_edit.clone();
                    let res = self.with_doc(|d, tex, opts| d.set_name(prov, &n, tex, opts));
                    self.after_edit(res.unwrap_or(false), Some(Rect { x0: 0, y0: 0, x1: -1, y1: -1 }), "Renamed province");
                }
            });
            let icons = terrain_icons(flags);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let cols = icons.len().min(3) as f32;
                    ui.set_width(ui.available_width() - cols * ICON_CELL - 6.0);
                    ui.label(terrain::describe(flags));
                    let look = if st.water_share >= 0.999 {
                "Painted as water".to_owned()
            } else if st.water_share <= 0.001 {
                "Painted as land".to_owned()
            } else {
                format!("Painted as water on {:.0}% of its area", st.water_share * 100.0)
            };
            let flag_water = terrain::is_water(flags);
            let mismatch = (flag_water && st.water_share < 0.5) || (!flag_water && st.water_share > 0.5);
            theme::dim(ui, &format!("Height {:.0} to {:.0}", st.min, st.max));
            if mismatch {
                ui.label(
                    egui::RichText::new(format!(
                        "{look}, but ruled as {}",
                        if flag_water { "sea" } else { "land" }
                    ))
                    .color(theme::WARN),
                );
            } else {
                theme::dim(ui, &look);
            }
                });
                if !icons.is_empty() {
                    let cols = icons.len().min(3);
                    let rows = icons.len().div_ceil(cols);
                    ui.add_space(2.0);
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(cols as f32 * ICON_CELL, rows as f32 * ICON_CELL),
                        Sense::hover(),
                    );
                    for (n, id) in icons.into_iter().enumerate() {
                        if let Some((_, tex)) = self.icons.iter().find(|(k, _)| *k == id) {
                            let cx = rect.min.x + (n % cols) as f32 * ICON_CELL;
                            let cy = rect.min.y + (n / cols) as f32 * ICON_CELL;
                            let ir = egui::Rect::from_min_size(
                                Pos2::new(cx + 2.0, cy + 2.0),
                                Vec2::splat(ICON_CELL - 4.0),
                            );
                            ui.painter().image(
                                tex.id(),
                                ir,
                                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            });
            let inside = self.doc().map(|d| d.capital_inside(prov)).unwrap_or(true);
            ui.horizontal_wrapped(|ui| {
                if !inside {
                    ui.label(egui::RichText::new("Capital outside the province").color(theme::WARN)).on_hover_text("The game places the province's flag, armies and name at the capital pixel, so it should lie inside the province's own area");
                }
                if theme::boxed_button_hint(ui, if self.placing_capital { "Click the map..." } else { "Move capital" }, !self.placing_capital, "Click a pixel inside this province to make it the capital: where the game puts the flag, the armies and the name. Escape cancels") {
                    self.placing_capital = true;
                    self.placing_new = false;
                    if matches!(self.tool, Tool::Paint | Tool::Height) {
                        self.tool = Tool::Select;
                    }
                    self.status = format!("Click inside province {prov} to place its capital");
                }
                if theme::boxed_button_hint(ui, "Centre", true, "Moves the capital to the province's own pixel nearest the middle of its area") {
                    self.centre_capital();
                }
                if theme::boxed_button_hint(ui, "Remove", true, "Removes this province the way the game's editor does: its area goes to the neighbouring provinces pixel by pixel, nearest first, and every later province moves down one number, with names, gates and connections following. One undo step") {
                    self.remove_province();
                }
            });

            theme::section(ui, "Make it look like");
            ui.horizontal(|ui| {
                for p in [Preset::DeepSea, Preset::Sea, Preset::Shallows] {
                    if theme::boxed_button_hint(ui, p.label(), true, p.hint()) {
                        self.preset(p);
                    }
                }
            });
            ui.horizontal(|ui| {
                if theme::boxed_button_hint(ui, Preset::Land.label(), true, Preset::Land.hint()) {
                    self.preset(Preset::Land);
                }
                theme::check(ui, &mut self.flatten, "Flatten").on_hover_text("Give every pixel the same height instead of moving the province as it is");
            });
            ui.horizontal(|ui| {
                theme::dim(ui, "Height");
                ui.add(
                    egui::DragValue::new(&mut self.custom)
                        .range(-2000.0..=2000.0)
                        .speed(1.0),
                );
                if theme::boxed_button_hint(
                    ui,
                    "Apply",
                    true,
                    "Move the province so its top (water) or bottom (land) reaches this height",
                ) {
                    let v = self.custom;
                    let op = if self.flatten {
                        HeightOp::Flat(v)
                    } else if v < 0.0 {
                        HeightOp::Below(v)
                    } else {
                        HeightOp::Above(v)
                    };
                    let f = if v < 0.0 { FlagOp::Sea } else { FlagOp::Land };
                    self.apply(op, f, &format!("Height {v:.0}"));
                }
            });
            ui.horizontal(|ui| {
                theme::dim(ui, "Step");
                ui.add(
                    egui::DragValue::new(&mut self.step)
                        .range(1.0..=500.0)
                        .speed(1.0),
                );
                if theme::boxed_button(ui, "Raise", true) {
                    let s = self.step;
                    self.apply(HeightOp::Offset(s), FlagOp::Keep, &format!("Raise {s:.0}"));
                }
                if theme::boxed_button(ui, "Lower", true) {
                    let s = self.step;
                    self.apply(HeightOp::Offset(-s), FlagOp::Keep, &format!("Lower {s:.0}"));
                }
            });

            theme::section(ui, "Terrain");
            let mut new_flags = flags;
            let mut water_preset: Option<Preset> = None;
            egui::Grid::new("basic_flags").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                for (i, (bit, label)) in BASIC_FLAGS.iter().enumerate() {
                    let mut on = flags & bit != 0;
                    let enabled = *bit != FRESH_WATER || flags & SEA == 0;
                    let r = theme::check_enabled(ui, &mut on, label, enabled);
                    let r = if *bit == SEA || *bit == DEEP_SEA {
                        r.on_hover_text("Also moves the ground to the matching depth, like the buttons above, so the picture and the rules agree")
                    } else {
                        r
                    };
                    if r.clicked() {
                        if *bit == SEA {
                            water_preset = Some(if on { Preset::Sea } else { Preset::Land });
                        } else if *bit == DEEP_SEA {
                            water_preset = Some(if on { Preset::DeepSea } else { Preset::Sea });
                        } else {
                            new_flags = toggle_flag(flags, *bit);
                        }
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            if let Some(p) = water_preset {
                self.preset(p);
            }
            ui.horizontal(|ui| {
                theme::dim(ui, "Gate").on_hover_text("Gateway number. A gateway connects to every other gateway with the same number, also on other planes, so armies can travel between them. 0 means no gateway. Double-click the province on the map to jump to the next gateway with the same number");
                let r = ui.add(egui::DragValue::new(&mut self.gate_edit).range(0..=999).speed(0.1)).on_hover_text("Gateway number. A gateway connects to every other gateway with the same number, also on other planes. 0 means no gateway");
                if r.lost_focus() || (r.changed() && !r.has_focus()) {
                    let g = self.gate_edit;
                    if g != gate {
                        let res = self.with_doc(|d, tex, opts| d.set_gate(prov, g, tex, opts));
                        self.after_edit(res.unwrap_or(false), None, "Gate number");
                    }
                }
            });
            egui::Grid::new("advanced_flags").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                for (i, (bit, label)) in ADVANCED_FLAGS.iter().enumerate() {
                    let mut on = flags & bit != 0;
                    if theme::check(ui, &mut on, label).clicked() {
                        new_flags = toggle_flag(flags, *bit);
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            })
            .response
            .on_hover_text("The terrain kinds behind the game's look buttons: they pick the ground picture and the province's income, sites and movement rules. Highlands, Swamp, Waste, Forest and Farm are the land types; Cave and Cave wall belong to the cave plane; Warmer and Colder shift the climate and pick the winter art");
            if new_flags != flags {
                self.set_flags(prov, new_flags, "Terrain");
            }

            theme::section(ui, "Connections");
            if !has_map {
                theme::dim(ui, "No .map file, connections cannot be edited");
            } else if neighbours.is_empty() {
                theme::dim(ui, "No connections");
            }
            let mut spec_change: Option<(u32, i64)> = None;
            let mut unlink: Option<u32> = None;
            for (nb, nb_name, spec) in &neighbours {
                ui.horizontal_wrapped(|ui| {
                    let label = if nb_name.is_empty() {
                        format!("{nb}")
                    } else {
                        format!("{nb}  {nb_name}")
                    };
                    ui.scope(|ui| {
                        ui.set_width(66.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        if theme::text_button(ui, &label, true) {
                            self.select(Some(*nb));
                        }
                    });
                    let base = *spec & !ROAD;
                    let mut kind = BORDER_KINDS
                        .iter()
                        .position(|(v, _)| *v == base)
                        .unwrap_or(0);
                    egui::ComboBox::from_id_salt(("spec", prov, *nb))
                        .selected_text(BORDER_KINDS[kind].1)
                        .width(100.0)
                        .truncate()
                        .show_ui(ui, |ui| {
                            for (i, (_, name)) in BORDER_KINDS.iter().enumerate() {
                                if ui.selectable_value(&mut kind, i, *name).changed() {
                                    spec_change = Some((*nb, BORDER_KINDS[i].0 | (*spec & ROAD)));
                                }
                            }
                        });
                    let mut road = *spec & ROAD != 0;
                    if theme::check(ui, &mut road, "Road")
                        .on_hover_text("A road across this border. The game lowers the movement cost for it but draws nothing on generated maps")
                        .clicked()
                    {
                        spec_change = Some((*nb, BORDER_KINDS[kind].0 | if road { ROAD } else { 0 }));
                    }
                    if ui
                        .add_enabled(
                            has_map,
                            egui::Button::new(egui::RichText::new("\u{2212}").size(15.0))
                                .frame(false),
                        )
                        .on_hover_text("Remove this connection")
                        .clicked()
                    {
                        unlink = Some(*nb);
                    }
                });
            }
            if let Some((nb, spec)) = spec_change {
                let res = self.with_doc(|d, tex, opts| (d.set_spec(prov, nb, spec, tex, opts), union(d.bbox(prov), d.bbox(nb))));
                if let Some((ok, rect)) = res {
                    self.after_edit(ok, rect, &format!("Border between {prov} and {nb} changed"));
                }
            }
            if let Some(nb) = unlink {
                let res = self.with_doc(|d, tex, opts| (d.set_link(prov, nb, false, tex, opts), union(d.bbox(prov), d.bbox(nb))));
                if let Some((ok, rect)) = res {
                    self.after_edit(ok, rect, &format!("Removed the connection between {prov} and {nb}"));
                }
            }
            if has_map {
                ui.horizontal(|ui| {
                    if theme::boxed_button_hint(ui, if self.tool == Tool::Link { "Linking..." } else { "Link" }, true, "Then click another province to connect it, or click a connected one to disconnect") {
                        self.tool = if self.tool == Tool::Link { Tool::Select } else { Tool::Link };
                    }
                    if self.tool == Tool::Link {
                        theme::dim(ui, "Click a province on the map");
                    }
                });
            }
        });
    }

    fn tools_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            theme::section_first(ui, "Tool");
            let compact = self.lay.compact();
            ui.horizontal_wrapped(|ui| {
                for (tool, text, key) in [
                    (Tool::Select, "Select", "S"),
                    (Tool::Link, "Link", "L"),
                    (Tool::Paint, "Paint area", "P"),
                    (Tool::Height, "Heights", "H"),
                ] {
                    let on = self.tool == tool;
                    let hit = if compact {
                        theme::tab(ui, on, text)
                    } else {
                        keycap::tab(ui, on, text, key, keycap::DEFAULT)
                    };
                    if hit {
                        self.tool = tool;
                    }
                }
            })
            .response
            .on_hover_text("Tab cycles the tools; P or H pressed again goes back to Select");
            match self.tool {
                Tool::Select => {
                    if compact {
                        theme::dim(ui, "Tap a province to select it; drag pans, pinch zooms");
                        theme::dim(ui, "Double-tap a gateway province to jump to the next gateway with the same number.");
                    } else {
                        theme::dim(ui, "Click a province or its capital dot to select it");
                        theme::dim(ui, "Double-click a gateway province to jump to the next gateway with the same number.");
                    }
                    ui.horizontal(|ui| {
                        theme::dim(ui, "Number");
                        let n = self.doc().map(|d| d.province_count() as u32).unwrap_or(1).max(1);
                        ui.add(egui::DragValue::new(&mut self.goto).range(1..=n).speed(0.2));
                        if theme::boxed_button_hint(ui, "Go", self.project.is_some(), "Selects the province with this number even when it has no area left to click") {
                            let g = self.goto.min(n);
                            self.select(Some(g));
                            self.status = format!("Selected province {g}");
                        }
                    });
                }
                Tool::Link => {
                    theme::dim(
                        ui,
                        if compact {
                            "Select a province, then tap others to connect or disconnect"
                        } else {
                            "Select a province, then click others to connect or disconnect"
                        },
                    );
                }
                Tool::Height => {
                    ui.horizontal(|ui| {
                        theme::dim(ui, "Brush");
                        ui.add(egui::Slider::new(&mut self.brush, 1..=60).suffix(" px"));
                    });
                    ui.horizontal(|ui| {
                        theme::dim(ui, "Step");
                        ui.add(egui::DragValue::new(&mut self.step).range(1.0..=500.0).speed(1.0));
                    });
                    if compact {
                        self.stroke_direction(ui, "Raise", "Lower");
                    }
                    theme::check(ui, &mut self.terrain_follows_height, "Height changes terrain").on_hover_text("When a stroke ends, every province it touched gets its Sea and Deep sea marks from where most of its ground now sits: below the waterline is Sea, below -36 is Deep sea, above is Land. Other land types stay as they are");
                    theme::check(ui, &mut self.keep_rivers, "Keep rivers").on_hover_text("Leaves the baked river channels alone so the brush cannot fill them in or lift them out; untick to reshape them like any other ground");
                    theme::check(ui, &mut self.relief_in_height, "Height map").on_hover_text("Shows the height field as a shaded relief while this tool is active: blues below the waterline, sand at the shore, green to brown to white going up. Borders and markers stay");
                    if self.relief_in_height {
                        let (lo, hi) = self
                            .relief_range
                            .get(self.active)
                            .copied()
                            .unwrap_or((-1.0, 1.0));
                        relief_legend(ui, lo, hi);
                    }
                    theme::dim(
                        ui,
                        if compact {
                            "One finger moves the ground under the brush by the step; two fingers pan and zoom. Land turns to water below 0, so a valley can be dug into a lake and a shoal raised into an island"
                        } else {
                            "Left button raises the ground under the brush by the step, right button lowers it. Land turns to water below 0, so a valley can be dug into a lake and a shoal raised into an island"
                        },
                    );
                }
                Tool::Paint => {
                    ui.horizontal(|ui| {
                        theme::dim(ui, "Brush");
                        ui.add(egui::Slider::new(&mut self.brush, 1..=60).suffix(" px"));
                    });
                    if compact {
                        self.stroke_direction(ui, "Add", "Remove");
                    }
                    theme::check(ui, &mut self.paint_empty, "Paint no province").on_hover_text("The left button takes pixels away from every province instead of giving them to the selected one; the right button still restores what was there when the map was opened");
                    if theme::boxed_button_hint(ui, if self.placing_new { "Click the map..." } else { "New province" }, self.project.is_some() && !self.placing_new, "Adds a province numbered after the last one. Click where its capital should be; a disc of the brush size around that point becomes its first area, and it takes the sea or cave marks of the province it was cut from. Then paint the rest of it. Escape cancels") {
                        self.placing_new = true;
                        self.placing_capital = false;
                        self.status = "Click on the map where the new province's capital should be".to_owned();
                    }
                    theme::dim(
                        ui,
                        if compact {
                            "Add gives pixels under the brush to the selected province. Remove undoes the painting there, giving every pixel back to the province that had it when the map was opened. Two fingers pan and zoom"
                        } else {
                            "Left button: give pixels to the selected province. Right button: undo the painting under the brush, giving every pixel back to the province that had it when the map was opened. Middle or Ctrl+left drag pans"
                        },
                    );
                }
            }
        });
    }

    fn stroke_direction(&mut self, ui: &mut egui::Ui, add: &str, remove: &str) {
        ui.horizontal(|ui| {
            if theme::tab(ui, !self.paint_erase, add) {
                self.paint_erase = false;
            }
            if theme::tab(ui, self.paint_erase, remove) {
                self.paint_erase = true;
            }
        });
    }

    fn plane_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            theme::section_first(ui, "Whole plane");
            let hint = "F4 in the game's editor: clears the land types and site marks of every province on this plane and rolls new ones with the game's own odds";
            let hit = if self.lay.compact() {
                theme::boxed_button_hint(ui, "Random terrain", true, hint)
            } else {
                keycap::boxed_button(ui, "Random terrain", &["F4"], true, hint, keycap::DEFAULT)
            };
            if hit {
                self.randomize_terrain();
            }
            ui.horizontal_wrapped(|ui| {
                theme::dim(ui, "No start below");
                ui.add(egui::DragValue::new(&mut self.nostart_min).range(1.0..=12.0).speed(0.1).fixed_decimals(1));
                theme::dim(ui, "links, a river or pass counts");
                ui.add(egui::DragValue::new(&mut self.nostart_crossing).range(0.0..=1.0).speed(0.05).fixed_decimals(2));
            });
            if theme::boxed_button_hint(ui, "Set no start", true, "Marks every province with fewer connections than this as No start. Links between land and sea are not counted; a river without a bridge or a mountain pass counts as the value above; impassable borders count 0") {
                self.set_no_starts();
            }
            if theme::boxed_button_hint(ui, "Clear no start", true, "Removes the No start mark from every province on this plane") {
                self.clear_no_starts();
            }
            let empty = self.doc().map(|d| d.empty_provinces()).unwrap_or_default();
            if !empty.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("No area:").color(theme::WARN)).on_hover_text("These provinces have no pixels left. Paint some, remove them, or undo; the map cannot be saved like this");
                    for p in empty.iter().take(12) {
                        if theme::text_button(ui, &p.to_string(), true) {
                            self.select(Some(*p));
                        }
                    }
                    if empty.len() > 12 {
                        theme::dim(ui, &format!("and {} more", empty.len() - 12));
                    }
                });
            }

        });
    }

    fn view_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(self.lay.section_w());
            theme::section_first(ui, "View");
            let mut changed = false;
            egui::Grid::new("view_flags")
                .num_columns(2)
                .spacing([16.0, 2.0])
                .show(ui, |ui| {
                    changed |= theme::check(ui, &mut self.opts.borders, "Borders").clicked();
                    changed |= theme::check(ui, &mut self.opts.decor, "Trees and rocks").on_hover_text("Forests, mountains, huts, sites and other sprites the game scatters over a map. The game places them at random on every load, so they never match exactly").clicked();
                    ui.end_row();
                    theme::check(ui, &mut self.show_names, "Names").on_hover_text("Province names, or the number where a province has no name");
                    if theme::check(ui, &mut self.show_markers, "Markers").on_hover_text("Capital dot and labels for Start, No start, Throne, No throne, Many sites and Gate").clicked() {
                        self.opts.capitals = self.show_markers;
                        changed = true;
                    }
                    ui.end_row();
                    theme::check(ui, &mut self.show_links, "Connections").on_hover_text("Every connection on the plane as a line between capitals: yellow normal, blue river, orange mountain pass, red impassable. The Link tool always shows the selected province's own");
                    theme::check(ui, &mut self.show_terrain, "Terrain").on_hover_text("The terrain marks of every province in words at its capital, so a province painted as water can be told from one the game treats as sea");
                    let (hwrap, vwrap) = self.plane_wraps();
                    let wraps = hwrap || vwrap;
                    let hint = if wraps {
                        "Repeats the map across the edges it wraps on, the way the game shows it, so panning past an edge continues on the other side"
                    } else {
                        "This plane has no #wraparound in its .map, so there is nothing to repeat"
                    };
                    ui.end_row();
                    theme::check_enabled(ui, &mut self.wrap_view, "Wraparound", wraps).on_hover_text(hint);
                    changed |= theme::check(ui, &mut self.opts.grey_no_start, "Grey no-start").on_hover_text("Shows provinces flagged No start in greyscale, the way the game's layout pictures do. The game itself paints them in full colour on the map").clicked();
                    ui.end_row();
                    changed |= theme::check(ui, &mut self.opts.winter, "Winter").on_hover_text("Draws the map the way the game draws it in a winter month: snow ground on every land province the cold scale reaches, and ice on shallow water and river channels. Warmer provinces, caves and deep sea stay as they are").clicked();
                    changed |= theme::check(ui, &mut self.opts.dirt, "Dirt").on_hover_text("The game's dirtify pass: a few hundred soft earth-coloured blotches over dry land, then a faint dark speckle over every pixel. It is what keeps a game map from looking like flat tiles").clicked();
                    ui.end_row();
                });
            if changed {
                self.view_changed();
            }
            ui.horizontal(|ui| {
                if theme::boxed_button(ui, "Fit", self.project.is_some()) {
                    self.fit_pending = true;
                }
                theme::dim(ui, &format!("{:.0}%", self.zoom * 100.0));
            });
        });
    }

    fn draw_help(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        theme::modal(ctx, "d6sme_help", egui::Order::Foreground, 700.0, |ui| {
            theme::title(ui, "Help");
            theme::section(ui, "Map");
            let compact = self.lay.compact();
            ui.label(if compact {
                "Drag pans, pinch zooms, Fit in the View tab fits the map. Tap a province to select it."
            } else {
                "Wheel zooms, dragging pans, Home fits the map. Click a province to select it."
            });
            theme::section(ui, "Looks");
            ui.label("Water starts below height 0. Shallows reach down to -10, open water to -30, deep sea from -36. The presets move a province to those depths and set its Sea marks so the rules match the picture. Flatten gives every pixel the same height.");
            theme::section(ui, "Rivers");
            ui.label("A river is a connection type: the engine carves the channel along the shared border when the map loads, so rivers can only run between two provinces and are added or removed with the Link tool. Generated maps also carry every river as a trench baked into the height data, because the recipe cannot store the engine's fresh-river marker; on load that trench paints as a sunken river with the border across it, on the cave plane as rock. Adding or removing a river with the Link tool also lifts or carves its channel here.");
            theme::section(ui, "Paint area");
            ui.label(if compact {
                "Add gives pixels to the selected province, Remove takes them back from it, two fingers pan and zoom."
            } else {
                "Left button gives pixels to the selected province, right button takes them back from it, middle button or Ctrl+left drag pans."
            });
            if !compact {
                theme::section(ui, "Keys");
                for (keys, what) in KEY_HELP {
                    keycap::help_row(ui, keys, what, keycap::DEFAULT);
                }
            }
            theme::section(ui, "Files");
            if crate::io::IS_WEB {
                ui.label("Open picks the .d6m and .map files of a map together, or drop them on the window. Choose the game's maps folder once in the Folder box and Save writes back into it; without a folder, Save downloads a zip.");
            } else {
                ui.label("Saving writes the .d6m and the .map beside it, in place.");
            }
            ui.add_space(6.0);
            if theme::boxed_button(ui, "Close", true) {
                self.show_help = false;
            }
        });
    }

    fn draw_generate_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_generate {
            return;
        }
        let name = self
            .project
            .as_ref()
            .map(|p| p.base.clone())
            .unwrap_or_default();
        theme::modal(
            ctx,
            "d6sme_generate_confirm",
            egui::Order::Tooltip,
            460.0,
            |ui| {
                theme::title(ui, "Unsaved changes");
                ui.label(format!("Discard the unsaved changes to {name}?"));
                ui.horizontal(|ui| {
                    if theme::boxed_button(ui, "Save first", true) && self.save() {
                        self.confirm_generate = false;
                        self.gen.begin();
                    }
                    if theme::boxed_button(ui, "Discard", true) {
                        if let Some(p) = &mut self.project {
                            for d in &mut p.planes {
                                d.dirty = false;
                            }
                        }
                        self.confirm_generate = false;
                        self.gen.begin();
                    }
                    if theme::boxed_button(ui, "Cancel", true) {
                        self.confirm_generate = false;
                    }
                });
            },
        );
    }

    fn draw_overwrite_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_overwrite {
            return;
        }
        let Some((dir, base)) = self.default_target() else {
            self.confirm_overwrite = false;
            return;
        };
        let mut action = None;
        let max_h = theme::modal_height(ctx);
        egui::Modal::new(egui::Id::new("d6sme_overwrite"))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                theme::scroll_body(ui, 480.0, max_h, |ui| {
                    theme::title(ui, "File already there");
                    ui.label(format!("Overwrite {base}?"));
                    theme::dim(ui, &crate::settings::shown(&dir));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if theme::boxed_button(ui, "Overwrite", true) {
                            action = Some(0);
                        }
                        if !crate::io::IS_WEB && theme::boxed_button(ui, "Save as\u{2026}", true) {
                            action = Some(1);
                        }
                        if theme::boxed_button(ui, "Cancel", true) {
                            action = Some(2);
                        }
                    });
                });
            });
        match action {
            Some(0) => {
                self.confirm_overwrite = false;
                if self.retarget_to(dir, &base) {
                    self.write_planes();
                }
            }
            Some(1) => {
                self.confirm_overwrite = false;
                if self.pick_save_target() {
                    self.write_planes();
                }
            }
            Some(2) => self.confirm_overwrite = false,
            _ => {}
        }
    }

    fn draw_close_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close {
            return;
        }
        theme::modal(ctx, "d6sme_close", egui::Order::Tooltip, 420.0, |ui| {
            theme::title(ui, "Unsaved changes");
            ui.label("The map has changes that are not saved.");
            ui.horizontal(|ui| {
                if theme::boxed_button(ui, "Save and exit", true) {
                    if self.save() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    self.confirm_close = false;
                }
                if theme::boxed_button(ui, "Discard", true) {
                    if let Some(p) = &mut self.project {
                        for d in &mut p.planes {
                            d.dirty = false;
                        }
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if theme::boxed_button(ui, "Cancel", true) {
                    self.confirm_close = false;
                }
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        self.poll_web();
        self.handle_keys(ctx);
        match std::mem::replace(&mut self.pending, Pending::None) {
            Pending::None => {}
            Pending::Open(Some(p)) => self.open(&p),
            Pending::Open(None) => self.pick_file(),
            Pending::AddPlane => self.add_plane(),
            Pending::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            let dirty = self
                .project
                .as_ref()
                .map(|p| p.any_dirty())
                .unwrap_or(false);
            if dirty && !self.confirm_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
            }
        }
        if let Some(g) = self.gen.poll() {
            self.open_generated(g);
        }
        if self.gen.take_place_request() {
            let wants = self.gen.wants();
            let generic = self.gen.generic_starts();
            self.place_starts(&wants, generic);
        }
        if std::mem::take(&mut self.pending_generate) {
            let edited = self
                .project
                .as_ref()
                .map(|p| p.planes.iter().any(|d| d.edited))
                .unwrap_or(false);
            if edited {
                self.confirm_generate = true;
            } else {
                self.gen.begin();
            }
        }
        let gen_wrap = self.gen.wrap_axes();
        if (gen_wrap.0 && !self.gen_wrap.0) || (gen_wrap.1 && !self.gen_wrap.1) {
            self.wrap_view = true;
        }
        self.gen_wrap = gen_wrap;
        self.lay = Layout::probe(ctx);
        if self.style_kind != Some(self.lay.kind) {
            layout::apply_style(ctx, self.lay.kind);
            self.style_kind = Some(self.lay.kind);
        }
        if self.lay.compact() {
            self.draw_sheet(ctx);
        } else {
            self.draw_side(ctx);
            self.draw_right_side(ctx);
        }
        self.draw_canvas(ctx);
        self.draw_help(ctx);
        self.draw_close_dialog(ctx);
        self.draw_generate_dialog(ctx);
        self.draw_overwrite_dialog(ctx);
        if self.center_pending.is_some() {
            ctx.request_repaint();
        }
    }
}

fn terrain_tally(doc: &PlaneDoc) -> String {
    let mut forests = 0;
    let mut farms = 0;
    let mut swamps = 0;
    let mut wastes = 0;
    let mut highlands = 0;
    let mut kelp = 0;
    let mut gorges = 0;
    for f in doc.flags.iter().skip(1) {
        let sea = f & SEA != 0;
        let deep = f & DEEP_SEA != 0;
        if sea {
            if f & FOREST != 0 && !deep {
                kelp += 1;
            }
            if f & HIGHLAND != 0 && deep {
                gorges += 1;
            }
            continue;
        }
        if f & FOREST != 0 {
            forests += 1;
        }
        if f & FARM != 0 {
            farms += 1;
        }
        if f & SWAMP != 0 {
            swamps += 1;
        }
        if f & WASTE != 0 {
            wastes += 1;
        }
        if f & HIGHLAND != 0 {
            highlands += 1;
        }
    }
    format!(
        "{forests} forests, {farms} farms, {swamps} swamps, {wastes} wastes, {highlands} highlands, {kelp} kelp, {gorges} gorges"
    )
}

pub fn default_maps_dir() -> Option<PathBuf> {
    crate::settings::game_maps_dir()
}

fn load_github_mark(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = decode_png(include_bytes!("../assets/github.png")).ok()?;
    let ci = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
    let opts = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    Some(ctx.load_texture("github_mark", ci, opts))
}

fn github_link(ui: &mut egui::Ui, mark: Option<&egui::TextureHandle>) {
    let font = FontId::proportional(16.0);
    let galley = ui
        .painter()
        .layout_no_wrap("GitHub".to_owned(), font, theme::INK);
    let icon = 20.0;
    let gap = 6.0;
    let size = Vec2::new(icon + gap + galley.size().x, icon.max(galley.size().y));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(REPO_URL);
    let col = if resp.hovered() {
        theme::INK_HOT
    } else {
        theme::BRASS
    };
    let painter = ui.painter();
    if let Some(tex) = mark {
        let ir = egui::Rect::from_min_size(
            Pos2::new(rect.min.x, rect.center().y - icon * 0.5),
            Vec2::splat(icon),
        );
        painter.image(
            tex.id(),
            ir,
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            col,
        );
    }
    let text_pos = Pos2::new(
        rect.min.x + icon + gap,
        rect.center().y - galley.size().y * 0.5,
    );
    painter.galley(text_pos, galley, col);
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(REPO_URL));
    }
}

fn relief_legend(ui: &mut egui::Ui, lo: f32, hi: f32) {
    let width = ui.available_width().min(layout::SIDE_W - 70.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let painter = ui.painter();
    let steps = 64;
    let span = hi - lo;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let hv = lo + span * (t0 + t1) * 0.5;
        let c = crate::render::relief_tint(hv, lo, hi);
        let r = egui::Rect::from_min_max(
            Pos2::new(rect.min.x + width * t0, rect.min.y),
            Pos2::new(rect.min.x + width * t1, rect.max.y),
        );
        painter.rect_filled(
            r,
            0.0,
            Color32::from_rgb(c[0] as u8, c[1] as u8, c[2] as u8),
        );
    }
    let sea_x = rect.min.x + width * ((0.0 - lo) / span).clamp(0.0, 1.0);
    painter.line_segment(
        [Pos2::new(sea_x, rect.min.y), Pos2::new(sea_x, rect.max.y)],
        egui::Stroke::new(1.0_f32, theme::INK),
    );
    ui.horizontal(|ui| {
        theme::dim(ui, &format!("{lo:.0}"));
        ui.add_space((width * ((0.0 - lo) / span).clamp(0.0, 1.0) - 40.0).max(0.0));
        theme::dim(ui, "sea 0");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::dim(ui, &format!("{hi:.0}"));
        });
    });
}

pub fn elide_left(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max < 2 {
        return text.to_owned();
    }
    let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
    format!("\u{2026}{tail}")
}

fn became_status(became: &[(u32, u64)]) -> String {
    let parts: Vec<String> = became
        .iter()
        .map(|&(p, f)| {
            let kind = if f & terrain::DEEP_SEA != 0 {
                "deep sea"
            } else if f & terrain::SEA != 0 {
                "sea"
            } else {
                "land"
            };
            format!("{p} {kind}")
        })
        .collect();
    format!("Now {}", parts.join(", "))
}

#[cfg(target_arch = "wasm32")]
fn primary_map_file(paths: &[PathBuf]) -> Option<PathBuf> {
    let ext_of = |p: &PathBuf| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default()
    };
    let first_plane = |p: &PathBuf| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| crate::mapfile::strip_plane_suffix(s).1 == 1)
            .unwrap_or(false)
    };
    paths
        .iter()
        .find(|p| ext_of(p) == "d6m" && first_plane(p))
        .or_else(|| paths.iter().find(|p| ext_of(p) == "map" && first_plane(p)))
        .or_else(|| {
            paths
                .iter()
                .find(|p| ext_of(p) == "d6m" || ext_of(p) == "map")
        })
        .cloned()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reveal(path: &Path) -> Result<(), String> {
    let cmd = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{cmd}: {e}"))
}
