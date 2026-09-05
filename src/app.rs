use crate::project::{union, FlagOp, HeightOp, PlaneDoc, Project};
use crate::render::{Options, Rect};
use crate::terrain::{self, *};
use crate::textures::{decode_png, TexSet};
use crate::theme;
use egui::{Align2, Color32, FontId, Key, PointerButton, Pos2, Sense, StrokeKind, Vec2};
use std::path::{Path, PathBuf};

const TILE: usize = 1024;
const PANEL_W: f32 = 340.0;
const ICON_CELL: f32 = 48.0;

struct TileGrid {
    w: usize,
    h: usize,
    cols: usize,
    rows: usize,
    handles: Vec<Option<egui::TextureHandle>>,
    dirty: Vec<bool>,
}

impl TileGrid {
    fn new(w: i32, h: i32) -> TileGrid {
        let cols = (w as usize).div_ceil(TILE);
        let rows = (h as usize).div_ceil(TILE);
        TileGrid {
            w: w as usize,
            h: h as usize,
            cols,
            rows,
            handles: (0..cols * rows).map(|_| None).collect(),
            dirty: vec![true; cols * rows],
        }
    }

    fn mark(&mut self, r: Rect) {
        let h = self.h as i32;
        let top = (h - 1 - r.y1).max(0) as usize / TILE;
        let bot = (h - 1 - r.y0).max(0) as usize / TILE;
        let left = r.x0.max(0) as usize / TILE;
        let right = r.x1.max(0) as usize / TILE;
        for ty in top..=bot.min(self.rows.saturating_sub(1)) {
            for tx in left..=right.min(self.cols.saturating_sub(1)) {
                self.dirty[ty * self.cols + tx] = true;
            }
        }
    }

    fn mark_all(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = true);
    }

    fn upload(&mut self, ctx: &egui::Context, rgba: &[u8], name: &str, premultiplied: bool) {
        let w = self.w;
        let h = self.h;
        for ty in 0..self.rows {
            for tx in 0..self.cols {
                let i = ty * self.cols + tx;
                if !self.dirty[i] {
                    continue;
                }
                self.dirty[i] = false;
                let x0 = tx * TILE;
                let y0 = ty * TILE;
                let tw = (w - x0).min(TILE);
                let th = (h - y0).min(TILE);
                let mut buf = vec![0u8; tw * th * 4];
                for row in 0..th {
                    let eng_y = h - 1 - (y0 + row);
                    let src = (eng_y * w + x0) * 4;
                    buf[row * tw * 4..(row + 1) * tw * 4].copy_from_slice(&rgba[src..src + tw * 4]);
                }
                let image = if premultiplied {
                    egui::ColorImage::from_rgba_premultiplied([tw, th], &buf)
                } else {
                    egui::ColorImage::from_rgba_unmultiplied([tw, th], &buf)
                };
                let opts = egui::TextureOptions::LINEAR;
                match &mut self.handles[i] {
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
    rgba: Vec<u8>,
    tiles: TileGrid,
    painted: Option<Rect>,
}

impl Overlay {
    fn new(w: i32, h: i32) -> Overlay {
        Overlay {
            rgba: vec![0u8; (w * h * 4) as usize],
            tiles: TileGrid::new(w, h),
            painted: None,
        }
    }

    fn clear(&mut self) {
        if let Some(r) = self.painted.take() {
            let w = self.tiles.w;
            for y in r.y0..=r.y1 {
                let row = y as usize * w;
                self.rgba[(row + r.x0 as usize) * 4..(row + r.x1 as usize + 1) * 4].fill(0);
            }
            self.tiles.mark(r);
        }
    }

    fn paint_selection(&mut self, doc: &PlaneDoc, prov: u32) {
        self.clear();
        let Some(r) = doc.bbox(prov) else {
            return;
        };
        let w = doc.width();
        let h = doc.height();
        let owners = &doc.d6m.owners;
        let inside = |x: i32, y: i32| {
            x >= 0 && y >= 0 && x < w && y < h && owners[(y * w + x) as usize] as u32 == prov
        };
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                if !inside(x, y) {
                    continue;
                }
                let mut edge = 0;
                for (dx, dy) in [
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-2, 0),
                    (2, 0),
                    (0, -2),
                    (0, 2),
                    (-1, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                ] {
                    if !inside(x + dx, y + dy) {
                        edge += 1;
                    }
                }
                let i = ((y * w + x) * 4) as usize;
                let (c, a) = if edge > 0 {
                    (theme::SELECT_EDGE, 235u8)
                } else {
                    (theme::SELECT_FILL, 46u8)
                };
                self.rgba[i] = c[0];
                self.rgba[i + 1] = c[1];
                self.rgba[i + 2] = c[2];
                self.rgba[i + 3] = a;
            }
        }
        self.painted = Some(r);
        self.tiles.mark(r);
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

#[derive(Clone, PartialEq)]
enum Pending {
    None,
    Open(Option<PathBuf>),
    AddPlane,
    Close,
}

const BORDER_KINDS: [(i64, &str); 6] = [
    (0, "Normal"),
    (BORDER_RIVER as i64, "River"),
    (BORDER_BRIDGE as i64, "Bridge"),
    (
        (BORDER_MOUNTAIN_LINE | BORDER_IMPASSABLE) as i64,
        "Mountains",
    ),
    (
        (BORDER_MOUNTAIN_LINE | BORDER_MOUNTAIN_PASS) as i64,
        "Mountain pass",
    ),
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
    selected: Option<u32>,
    hover: Option<u32>,
    decor_tiles: Vec<TileGrid>,
    tool: Tool,
    brush: i32,
    paint_empty: bool,
    painting: Option<PointerButton>,
    show_markers: bool,
    show_links: bool,
    show_terrain: bool,
    placing_capital: bool,
    goto: u32,
    placing_new: bool,
    icons: Vec<(u16, egui::TextureHandle)>,
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
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        preselect: Option<u32>,
    ) -> App {
        theme::install(&cc.egui_ctx);
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
            selected: None,
            hover: None,
            tool: Tool::Select,
            decor_tiles: Vec::new(),
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
            flatten: false,
            show_names: false,
            custom: -20.0,
            step: 10.0,
            nostart_min: 4.0,
            nostart_crossing: 0.5,
            name_edit: String::new(),
            name_for: None,
            gate_edit: 0,
            status: "Open a map, or drop a .d6m or .map file on the window".to_owned(),
            error: None,
            pending: Pending::None,
            confirm_close: false,
            show_help: false,
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

    fn open(&mut self, path: &Path) {
        match Project::open(path, &self.tex, &self.opts) {
            Ok(p) => {
                let planes = p.planes.len();
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
                let notes = p.notes.clone();
                let scars: usize = p
                    .planes
                    .iter()
                    .filter(|d| d.index == 2)
                    .map(|d| d.scar_count())
                    .sum();
                self.status = if planes > 1 {
                    format!("Loaded {} with {} planes", p.base, planes)
                } else {
                    format!("Loaded {}", p.base)
                };
                if scars > 0 {
                    self.status = format!(
                        "{}. {} grey river pixels on the cave plane, see Repair there",
                        self.status, scars
                    );
                }
                if !notes.is_empty() {
                    self.status = format!("{} ({})", self.status, notes.join("; "));
                }
                self.project = Some(p);
                self.active = 0;
                self.selected = None;
                self.name_for = None;
                self.error = None;
                self.fit_pending = true;
                self.tool = Tool::Select;
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

    fn map_to_screen(&self, canvas: egui::Rect, x: f32, y_img: f32) -> Pos2 {
        canvas.min + self.offset + Vec2::new(x, y_img) * self.zoom
    }

    fn screen_to_map(&self, canvas: egui::Rect, p: Pos2) -> (i32, i32) {
        let v = (p - canvas.min - self.offset) / self.zoom;
        let h = self.doc().map(|d| d.height()).unwrap_or(0);
        (v.x.floor() as i32, h - 1 - v.y.floor() as i32)
    }

    fn fit(&mut self, canvas: egui::Rect) {
        let Some(doc) = self.doc() else {
            return;
        };
        let w = doc.width() as f32;
        let h = doc.height() as f32;
        let z = (canvas.width() / w).min(canvas.height() / h) * 0.985;
        self.zoom = z.max(0.02);
        self.offset = Vec2::new(
            (canvas.width() - w * self.zoom) * 0.5,
            (canvas.height() - h * self.zoom) * 0.5,
        );
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
    }

    fn repair_scars(&mut self) {
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let mut total = 0;
        let mut cave = false;
        if let Some(p) = &mut self.project {
            if let Some(d) = p.planes.get_mut(self.active) {
                cave = d.index == 2;
                total = d.repair_scars(&tex, &opts);
            }
        }
        self.tex = tex;
        for t in self.tiles.iter_mut().chain(self.decor_tiles.iter_mut()) {
            t.mark_all();
        }
        self.refresh_selection();
        self.status = if total > 0 && cave {
            format!("Raised {total} grey river pixels to the cave floor; the game carves the rivers there on load")
        } else {
            "No grey river pixels on this plane".to_owned()
        };
    }

    fn save(&mut self) -> bool {
        let Some(p) = &mut self.project else {
            return false;
        };
        let mut written = Vec::new();
        for d in &mut p.planes {
            if !d.dirty {
                continue;
            }
            match d.save() {
                Ok(files) => written.extend(files),
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
            format!(
                "Saved {} (the untouched originals stay beside them as .bak)",
                names.join(", ")
            )
        };
        true
    }

    fn pick_file(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Dominions 6 map", &["d6m", "map"]);
        if let Some(p) = &self.project {
            dlg = dlg.set_directory(&p.dir);
        } else if let Some(d) = default_maps_dir() {
            dlg = dlg.set_directory(d);
        }
        if let Some(path) = dlg.pick_file() {
            self.open(&path);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (undo, redo, save, open, fit, rerender, esc, help, tab) = ctx.input(|i| {
            let c = i.modifiers.command;
            (
                c && i.key_pressed(Key::Z) && !i.modifiers.shift,
                c && (i.key_pressed(Key::Y) || (i.key_pressed(Key::Z) && i.modifiers.shift)),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::O),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::F5),
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
        if rerender {
            self.rerender_all();
            self.status = "Rerendered".to_owned();
        }
        if esc {
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
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            self.pending = Pending::Open(Some(p));
        }
    }

    fn switch_plane(&mut self, i: usize) {
        if self.active == i {
            return;
        }
        self.active = i;
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
        let doc = self.doc()?;
        let h = doc.height();
        let mut best = None;
        let mut best_d = 8.0_f32 * 8.0;
        for (i, &(cx, cy)) in doc.capitals.iter().enumerate() {
            let c = self.map_to_screen(canvas, cx as f32 + 0.5, (h - 1 - cy as i32) as f32 + 0.5);
            let d = c.distance_sq(pos);
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

    fn add_plane(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Dominions 6 map", &["d6m", "map"]);
        if let Some(p) = &self.project {
            dlg = dlg.set_directory(&p.dir);
        }
        let Some(path) = dlg.pick_file() else {
            return;
        };
        let tex = std::mem::replace(&mut self.tex, TexSet::from_images(Vec::new()));
        let opts = self.opts;
        let res = self
            .project
            .as_mut()
            .map(|p| p.add_plane(&path, &tex, &opts));
        self.tex = tex;
        match res {
            Some(Ok(n)) => {
                if let Some(p) = &self.project {
                    let d = p.planes.last().unwrap();
                    self.tiles.push(TileGrid::new(d.width(), d.height()));
                    self.overlays.push(Overlay::new(d.width(), d.height()));
                    self.decor_tiles.push(TileGrid::new(d.width(), d.height()));
                    let last = p.planes.len() - 1;
                    self.switch_plane(last);
                }
                self.status = format!("Added plane {n} from {}", path.display());
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
                self.status = format!(
                    "Removed the last plane; its files were kept as {}",
                    names.join(", ")
                );
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(e),
            None => {}
        }
    }

    fn paint_at(&mut self, x: i32, y: i32, remove: bool) {
        let r = self.brush;
        let res = if self.tool == Tool::Height {
            let delta = if remove { -self.step } else { self.step };
            self.with_doc(|d, tex, opts| d.paint_height(x, y, r, delta, tex, opts))
                .flatten()
        } else if remove {
            self.with_doc(|d, tex, opts| d.paint_restore(x, y, r, tex, opts))
                .flatten()
        } else {
            let prov = if self.paint_empty {
                0
            } else {
                self.selected.unwrap_or(0)
            };
            if !self.paint_empty && prov == 0 {
                return;
            }
            self.with_doc(|d, tex, opts| d.paint(prov, x, y, r, tex, opts))
                .flatten()
        };
        if let Some(rect) = res {
            self.mark_tiles(Some(rect));
        }
    }

    fn draw_canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::CANVAS))
            .show(ctx, |ui| {
                let (canvas, resp) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if self.project.is_none() {
                    let p = canvas.center();
                    ui.painter().text(
                        p,
                        Align2::CENTER_CENTER,
                        "Drop a .d6m or .map here, or use Open map",
                        FontId::proportional(20.0),
                        theme::INK_DIM,
                    );
                    return;
                }
                if self.fit_pending {
                    self.fit(canvas);
                    self.fit_pending = false;
                }
                let paint_tool = matches!(self.tool, Tool::Paint | Tool::Height);
                let ctrl = ctx.input(|i| i.modifiers.command);
                let pan = resp.dragged_by(PointerButton::Middle)
                    || (!paint_tool && resp.dragged_by(PointerButton::Secondary))
                    || ((!paint_tool || ctrl) && resp.dragged_by(PointerButton::Primary));
                if pan {
                    self.offset += resp.drag_delta();
                }
                if let Some(pos) = resp.hover_pos() {
                    let scroll = ctx.input(|i| i.raw_scroll_delta.y);
                    let zd = ctx.input(|i| i.zoom_delta());
                    let factor = if scroll != 0.0 {
                        (scroll / 120.0 * 0.25).exp()
                    } else {
                        zd
                    };
                    if factor != 1.0 {
                        let new_zoom = (self.zoom * factor).clamp(0.02, 16.0);
                        let k = new_zoom / self.zoom;
                        let rel = pos - canvas.min;
                        self.offset = rel - (rel - self.offset) * k;
                        self.zoom = new_zoom;
                    }
                    let (x, y) = self.screen_to_map(canvas, pos);
                    self.hover = self.doc().map(|d| d.owner_at(x, y)).filter(|&p| p > 0);
                    let placing = self.placing_capital || self.placing_new;
                    if placing && resp.clicked_by(PointerButton::Primary) {
                        if self.placing_new {
                            self.place_new_province(x, y);
                        } else {
                            self.place_capital(x, y);
                        }
                    } else if !paint_tool && resp.clicked_by(PointerButton::Primary) {
                        let dot = if self.tool == Tool::Select {
                            self.capital_near(canvas, pos)
                        } else {
                            None
                        };
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
                        let button = if primary {
                            Some(PointerButton::Primary)
                        } else if secondary {
                            Some(PointerButton::Secondary)
                        } else {
                            None
                        };
                        if let Some(b) = button {
                            let started = resp.contains_pointer() || self.painting == Some(b);
                            if started {
                                if self.painting != Some(b) {
                                    self.painting = Some(b);
                                    let label = match (self.tool, b) {
                                        (Tool::Height, _) => "Height brush",
                                        (_, PointerButton::Primary) => "Paint area",
                                        _ => "Remove area",
                                    };
                                    self.with_doc(|d, _, _| d.paint_begin(label));
                                }
                                self.paint_at(x, y, b == PointerButton::Secondary);
                            }
                        }
                    }
                } else {
                    self.hover = None;
                }
                if let Some(b) = self.painting {
                    let still = ctx.input(|i| i.pointer.button_down(b));
                    if !still {
                        self.painting = None;
                        let done = self
                            .with_doc(|d, tex, opts| d.paint_end(tex, opts))
                            .flatten();
                        if let Some(rect) = done {
                            self.mark_tiles(Some(rect));
                        }
                        self.refresh_selection();
                    }
                }
                let active = self.active;
                if let Some(project) = &self.project {
                    let doc = &project.planes[active];
                    if let Some(t) = self.tiles.get_mut(active) {
                        t.upload(ctx, &doc.rendered.rgba, &format!("map{}", doc.index), false);
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
                        o.tiles.upload(ctx, &o.rgba, &name, false);
                    }
                }
                let painter = ui.painter_at(canvas);
                let Some(doc) = self.doc() else {
                    return;
                };
                let w = doc.width();
                let h = doc.height();
                let uv = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                let mut layers = vec![&self.tiles[self.active]];
                if self.opts.decor {
                    layers.push(&self.decor_tiles[self.active]);
                }
                layers.push(&self.overlays[self.active].tiles);
                for grid in layers {
                    for ty in 0..grid.rows {
                        for tx in 0..grid.cols {
                            let Some(hnd) = &grid.handles[ty * grid.cols + tx] else {
                                continue;
                            };
                            let x0 = (tx * TILE) as f32;
                            let y0 = (ty * TILE) as f32;
                            let tw = ((w as usize - tx * TILE).min(TILE)) as f32;
                            let th = ((h as usize - ty * TILE).min(TILE)) as f32;
                            let r = egui::Rect::from_min_max(
                                self.map_to_screen(canvas, x0, y0),
                                self.map_to_screen(canvas, x0 + tw, y0 + th),
                            );
                            if r.intersects(canvas) {
                                painter.image(hnd.id(), r, uv, Color32::WHITE);
                            }
                        }
                    }
                }
                if self.show_links {
                    let thin = 2.0_f32 * self.zoom.sqrt().clamp(0.6, 1.5);
                    for p in 1..=doc.province_count() as u32 {
                        let (ax, ay) = doc.capitals[p as usize - 1];
                        for nb in doc.neighbours(p) {
                            if nb <= p {
                                continue;
                            }
                            let Some(&(bx, by)) = doc.capitals.get(nb as usize - 1) else {
                                continue;
                            };
                            let a = self.map_to_screen(
                                canvas,
                                ax as f32 + 0.5,
                                (h - 1 - ay as i32) as f32 + 0.5,
                            );
                            let b = self.map_to_screen(
                                canvas,
                                bx as f32 + 0.5,
                                (h - 1 - by as i32) as f32 + 0.5,
                            );
                            if !canvas.intersects(egui::Rect::from_two_pos(a, b)) {
                                continue;
                            }
                            painter.line_segment(
                                [a, b],
                                egui::Stroke::new(thin, link_colour(doc.spec(p, nb))),
                            );
                        }
                    }
                }
                if self.tool == Tool::Link {
                    if let Some(sel) = self.selected {
                        for nb in doc.neighbours(sel) {
                            let (ax, ay) = doc.capitals[sel as usize - 1];
                            let Some(&(bx, by)) = doc.capitals.get(nb as usize - 1) else {
                                continue;
                            };
                            let a = self.map_to_screen(
                                canvas,
                                ax as f32 + 0.5,
                                (h - 1 - ay as i32) as f32 + 0.5,
                            );
                            let b = self.map_to_screen(
                                canvas,
                                bx as f32 + 0.5,
                                (h - 1 - by as i32) as f32 + 0.5,
                            );
                            let col = link_colour(doc.spec(sel, nb));
                            painter.line_segment([a, b], egui::Stroke::new(3.0_f32, col));
                            painter.circle_filled(b, 6.0, col);
                        }
                    }
                }
                if self.show_names || self.show_markers || self.show_terrain {
                    let size = (14.0 * self.zoom.sqrt()).clamp(11.0, 26.0);
                    let badge = (12.0 * self.zoom.sqrt()).clamp(10.0, 18.0);
                    let halo = Color32::from_rgba_unmultiplied(255, 244, 214, 150);
                    for (i, &(cx, cy)) in doc.capitals.iter().enumerate() {
                        let id = i as u32 + 1;
                        let centre = self.map_to_screen(
                            canvas,
                            cx as f32 + 0.5,
                            (h - 1 - cy as i32) as f32 + 0.5,
                        );
                        if !canvas.expand(80.0).contains(centre) {
                            continue;
                        }
                        if self.show_names {
                            let name = doc.name(id);
                            let mut p = centre - Vec2::new(0.0, 8.0);
                            let outlined = |p: Pos2, text: &str, font: FontId, col: Color32| {
                                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
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
                                let r =
                                    outlined(p, name, FontId::proportional(size), theme::NAME_RED);
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
                            if self.show_terrain {
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
                            if self.show_markers && f & GOOD_START != 0 {
                                tags.push(("Start".to_owned(), theme::TAG_START, Vec::new()));
                            }
                            if self.show_markers && f & NO_START != 0 {
                                tags.push(("No start".to_owned(), theme::TAG_NO, Vec::new()));
                            }
                            if self.show_markers && f & GOOD_THRONE != 0 {
                                tags.push(("Throne".to_owned(), theme::TAG_THRONE, Vec::new()));
                            }
                            if self.show_markers && f & BAD_THRONE != 0 {
                                tags.push(("No throne".to_owned(), theme::TAG_NO, Vec::new()));
                            }
                            if self.show_markers && f & MANY_SITES != 0 {
                                tags.push(("Sites".to_owned(), theme::TAG_SITES, Vec::new()));
                            }
                            if self.show_markers && gate != 0 {
                                tags.push((format!("Gate {gate}"), theme::TAG_GATE, Vec::new()));
                            }
                            if self.show_markers {
                                painter.circle(
                                    centre,
                                    3.0,
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
                        let text = if name.is_empty() {
                            format!("{hp}")
                        } else {
                            format!("{hp}  {name}")
                        };
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
            .exact_width(PANEL_W)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDE_FILL)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(PANEL_W - 22.0);
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                        self.map_section(ui);
                        ui.add_space(8.0);
                        if self.selected.is_some() {
                            self.province_section(ui);
                            ui.add_space(8.0);
                        }
                        self.tools_section(ui);
                        ui.add_space(8.0);
                        if self.project.is_some() {
                            self.plane_section(ui);
                            ui.add_space(8.0);
                        }
                        self.view_section(ui);
                    });
            });
    }

    fn map_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(PANEL_W - 50.0);
            let title = self.project.as_ref().map(|p| p.base.clone()).unwrap_or_else(|| "Dominions 6 Simple Map Editor".to_owned());
            theme::title(ui, &title);
            if let Some(p) = &self.project {
                let dims = p.planes.get(self.active).map(|d| format!("{} x {} px, {} provinces", d.width(), d.height(), d.province_count())).unwrap_or_default();
                theme::dim(ui, &dims);
                let labels: Vec<String> = p
                    .planes
                    .iter()
                    .map(|d| if d.index == 1 { "Surface".to_owned() } else if d.index == 2 { "Caves".to_owned() } else { format!("Plane {}", d.index) })
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
                if theme::boxed_button(ui, "Exit", true) {
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
            ui.set_width(PANEL_W - 50.0);
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
                theme::dim(ui, "Gate").on_hover_text("Gateway number. A gateway connects to every other gateway with the same number, also on other planes, so armies can travel between them. 0 means no gateway");
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
            ui.set_width(PANEL_W - 50.0);
            theme::section_first(ui, "Tool");
            ui.horizontal_wrapped(|ui| {
                if theme::tab(ui, self.tool == Tool::Select, "Select (S)") {
                    self.tool = Tool::Select;
                }
                if theme::tab(ui, self.tool == Tool::Link, "Link (L)") {
                    self.tool = Tool::Link;
                }
                if theme::tab(ui, self.tool == Tool::Paint, "Paint area (P)") {
                    self.tool = Tool::Paint;
                }
                if theme::tab(ui, self.tool == Tool::Height, "Heights (H)") {
                    self.tool = Tool::Height;
                }
            })
            .response
            .on_hover_text("Tab cycles the tools; P or H pressed again goes back to Select");
            match self.tool {
                Tool::Select => {
                    theme::dim(ui, "Click a province or its capital dot to select it (S)");
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
                        "Select a province, then click others to connect or disconnect (L)",
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
                    theme::dim(
                        ui,
                        "Left button raises the ground under the brush by the step, right button lowers it. Land turns to water below 0, so a valley can be dug into a lake and a shoal raised into an island (H)",
                    );
                }
                Tool::Paint => {
                    ui.horizontal(|ui| {
                        theme::dim(ui, "Brush");
                        ui.add(egui::Slider::new(&mut self.brush, 1..=60).suffix(" px"));
                    });
                    theme::check(ui, &mut self.paint_empty, "Paint no province").on_hover_text("The left button takes pixels away from every province instead of giving them to the selected one; the right button still restores what was there when the map was opened");
                    if theme::boxed_button_hint(ui, if self.placing_new { "Click the map..." } else { "New province" }, self.project.is_some() && !self.placing_new, "Adds a province numbered after the last one. Click where its capital should be; a disc of the brush size around that point becomes its first area, and it takes the sea or cave marks of the province it was cut from. Then paint the rest of it. Escape cancels") {
                        self.placing_new = true;
                        self.placing_capital = false;
                        self.status = "Click on the map where the new province's capital should be".to_owned();
                    }
                    theme::dim(
                        ui,
                        "Left button: give pixels to the selected province. Right button: undo the painting under the brush, giving every pixel back to the province that had it when the map was opened. Middle or Ctrl+left drag pans (P)",
                    );
                }
            }
        });
    }

    fn plane_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(PANEL_W - 50.0);
            theme::section_first(ui, "Whole plane");
            if theme::boxed_button_hint(ui, "Random terrain", true, "F4 in the game's editor: clears the land types and site marks of every province on this plane and rolls new ones with the game's own odds") {
                self.randomize_terrain();
            }
            ui.horizontal_wrapped(|ui| {
                theme::dim(ui, "No start below");
                ui.add(egui::DragValue::new(&mut self.nostart_min).range(1.0..=12.0).speed(0.1).fixed_decimals(1));
                theme::dim(ui, "links, a river or pass counts");
                ui.add(egui::DragValue::new(&mut self.nostart_crossing).range(0.0..=1.0).speed(0.05).fixed_decimals(2));
            });
            if theme::boxed_button_hint(ui, "Set no start", true, "Marks every province on this plane with fewer connections than this as No start, the way the Random Map NoStart setter does. Links between land and sea are not counted; a river without a bridge or a mountain pass counts as the value above instead of 1; impassable borders count 0") {
                self.set_no_starts();
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
            let (scars, cave) = self
                .doc()
                .map(|d| (d.scar_count(), d.index == 2))
                .unwrap_or((0, false));
            if cave {
                ui.horizontal_wrapped(|ui| {
                    if scars > 0 {
                        ui.label(
                            egui::RichText::new(format!("{scars} grey river pixels"))
                                .color(theme::WARN),
                        );
                        if theme::boxed_button(ui, "Repair", true) {
                            self.repair_scars();
                        }
                    } else {
                        theme::dim(ui, "No grey river pixels");
                    }
                })
                .response
                .on_hover_text("Rivers saved by the game's editor keep their channel in the height data, and on the cave plane that channel paints as plain rock with no river. Repair raises it to the cave floor; the game then carves a proper river there from the connection when the map loads. The surface needs nothing: the game paints an old channel as water anyway");
            }
        });
    }

    fn view_section(&mut self, ui: &mut egui::Ui) {
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(PANEL_W - 50.0);
            theme::section_first(ui, "View");
            let mut changed = false;
            egui::Grid::new("view_flags")
                .num_columns(2)
                .spacing([16.0, 2.0])
                .show(ui, |ui| {
                    changed |= theme::check(ui, &mut self.opts.borders, "Borders").clicked();
                    changed |= theme::check(ui, &mut self.opts.edge_fade, "Dark edges").clicked();
                    ui.end_row();
                    changed |= theme::check(ui, &mut self.opts.decor, "Trees and rocks").on_hover_text("Forests, mountains, huts, sites and other sprites the game scatters over a map. The game places them at random on every load, so they never match exactly").clicked();
                    theme::check(ui, &mut self.show_names, "Names").on_hover_text("Province names, or the number where a province has no name");
                    ui.end_row();
                    theme::check(ui, &mut self.show_markers, "Markers").on_hover_text("Capital dot and labels for Start, No start, Throne, No throne, Many sites and Gate");
                    theme::check(ui, &mut self.show_links, "Connections").on_hover_text("Every connection on the plane as a line between capitals: yellow normal, blue river, orange mountain pass, red impassable. The Link tool always shows the selected province's own");
                    ui.end_row();
                    theme::check(ui, &mut self.show_terrain, "Terrain").on_hover_text("The terrain marks of every province in words at its capital, so a province painted as water can be told from one the game treats as sea");
                    ui.end_row();
                });
            if changed {
                self.rerender_all();
            }
            ui.horizontal(|ui| {
                if theme::boxed_button(ui, "Fit", self.project.is_some()) {
                    self.fit_pending = true;
                }
                if theme::boxed_button(ui, "Rerender", self.project.is_some()) {
                    self.rerender_all();
                }
                theme::dim(ui, &format!("{:.0}%", self.zoom * 100.0));
            });
        });
    }

    fn draw_help(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        egui::Area::new(egui::Id::new("d6sme_help"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                theme::panel_frame().show(ui, |ui| {
                    ui.set_width(520.0);
                    theme::title(ui, "Help");
                    theme::section(ui, "Map");
                    ui.label("Wheel zooms, dragging pans, Home fits the map. Click a province to select it.");
                    theme::section(ui, "Looks");
                    ui.label("Water starts below height 0. Shallows reach down to -10, open water to -30, deep sea from -36. The presets move a province to those depths and set its Sea marks so the rules match the picture. Flatten gives every pixel the same height.");
                    theme::section(ui, "Rivers");
                    ui.label("A river is a connection type and a channel in the height data. Removing a river here also lifts its channel. Maps saved by the game's editor keep old channels in the height data: on the surface they still show as a sunken river with the border drawn across it, on the cave plane as plain rock. Repair on the cave plane raises them so the game carves real rivers there on load.");
                    theme::section(ui, "Paint area");
                    ui.label("Left button gives pixels to the selected province, right button takes them back from it, middle button or Ctrl+left drag pans.");
                    theme::section(ui, "Keys");
                    ui.label("Ctrl+Z undo, Ctrl+Y redo, Ctrl+S save, Ctrl+O open, F4 random terrain, F5 rerender, S select, L link, P paint, H heights, Tab next tool, PageUp and PageDown switch planes, Esc back, F1 help.");
                    theme::section(ui, "Files");
                    ui.label("Saving writes the .d6m and the .map beside it. The first save keeps the untouched originals as .bak copies; later saves leave those copies alone.");
                    ui.add_space(6.0);
                    if theme::boxed_button(ui, "Close", true) {
                        self.show_help = false;
                    }
                });
            });
    }

    fn draw_close_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_close {
            return;
        }
        egui::Area::new(egui::Id::new("d6sme_close"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                theme::panel_frame().show(ui, |ui| {
                    ui.set_width(320.0);
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
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        self.draw_side(ctx);
        self.draw_canvas(ctx);
        self.draw_help(ctx);
        self.draw_close_dialog(ctx);
    }
}

pub fn default_maps_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let p = PathBuf::from(appdata).join("Dominions6").join("maps");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}
