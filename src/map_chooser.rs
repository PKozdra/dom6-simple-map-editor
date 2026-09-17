use crate::d6m::{units_from_stored, D6m};
#[cfg(not(target_arch = "wasm32"))]
use crate::mapfile::plane_file_name;
use crate::mapfile::{strip_plane_suffix, MapFile};
use crate::render::{is_channel, relief_tint, Options, Plane, Rendered};
use crate::textures::{Image, TexSet};
use crate::theme;
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

pub const PREVIEW_W: usize = 512;
const CARD_W: f32 = 268.0;
const CARD_GAP: f32 = 12.0;
const THUMB_H: f32 = 160.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Recipe,
    Picture,
    Unknown,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Recipe => "recipe (.d6m)",
            Kind::Picture => "picture (.tga)",
            Kind::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub kind: Kind,
    pub size: Option<(i32, i32)>,
    pub provinces: usize,
    pub sea: usize,
    pub land: usize,
    pub planes: usize,
}

pub fn name_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn folder_of(root: &Path, path: &Path) -> String {
    let rel = rel_of(root, path);
    match rel.rfind('/') {
        Some(i) => rel[..i].to_string(),
        None => String::new(),
    }
}

pub fn order(root: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut keyed: Vec<(String, PathBuf)> = paths
        .iter()
        .map(|p| (rel_of(root, p).to_lowercase(), p.clone()))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    keyed.dedup_by(|a, b| a.1 == b.1);
    keyed.into_iter().map(|(_, p)| p).collect()
}

pub struct Slots<T> {
    items: Vec<Option<T>>,
    ready: usize,
    filled: usize,
}

impl<T> Slots<T> {
    pub fn new(n: usize) -> Slots<T> {
        Slots {
            items: (0..n).map(|_| None).collect(),
            ready: 0,
            filled: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn filled(&self) -> usize {
        self.filled
    }

    pub fn ready(&self) -> usize {
        self.ready
    }

    pub fn put(&mut self, i: usize, value: T) {
        if i >= self.items.len() || self.items[i].is_some() {
            return;
        }
        self.items[i] = Some(value);
        self.filled += 1;
        while self.ready < self.items.len() && self.items[self.ready].is_some() {
            self.ready += 1;
        }
    }

    pub fn visible(&self) -> Vec<&T> {
        self.items[..self.ready]
            .iter()
            .map(|s| s.as_ref().expect("prefix is filled"))
            .collect()
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.items.get(i).and_then(|s| s.as_ref())
    }
}

fn kind_of(imagefile: Option<&str>) -> Kind {
    match imagefile {
        Some(name) => {
            let lower = name.trim().to_lowercase();
            if lower.ends_with(".d6m") {
                Kind::Recipe
            } else if lower.ends_with(".tga") {
                Kind::Picture
            } else {
                Kind::Unknown
            }
        }
        None => Kind::Unknown,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn plane_count(path: &Path) -> usize {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = strip_plane_suffix(&name_of(path)).0;
    let mut n = 1;
    for p in 2..=9 {
        if crate::io::exists(&dir.join(plane_file_name(&base, p, "map"))) {
            n = p as usize;
        }
    }
    n
}

pub fn stats_from_text(text: &str, path: &Path, planes: usize) -> (Stats, Option<String>) {
    let mf = MapFile::parse(text, path);
    let kind = kind_of(mf.imagefile.as_deref());
    let image = mf.imagefile.as_ref().map(|f| f.trim().to_owned());
    let sea = mf
        .terrain
        .values()
        .filter(|&&f| (f as u64) & crate::terrain::SEA != 0)
        .count();
    let listed = mf.terrain.len();
    let provinces = mf
        .terrain
        .keys()
        .copied()
        .max()
        .unwrap_or(0)
        .max(listed as u32) as usize;
    (
        Stats {
            kind,
            size: mf.mapsize,
            provinces,
            sea,
            land: listed.saturating_sub(sea),
            planes,
        },
        image,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stats_of(path: &Path) -> Result<(Stats, Option<PathBuf>), String> {
    let bytes = crate::io::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let (stats, image) = stats_from_text(&text, path, plane_count(path));
    let image = image.map(|f| {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .join(f)
    });
    Ok((stats, image))
}

pub fn downsample(src: &Image, target: usize) -> Image {
    let step = src.w.div_ceil(target.max(1)).max(1);
    if step == 1 {
        return src.clone();
    }
    let ow = src.w.div_ceil(step);
    let oh = src.h.div_ceil(step);
    let mut rgba = vec![0u8; ow * oh * 4];
    for oy in 0..oh {
        for ox in 0..ow {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for y in oy * step..((oy + 1) * step).min(src.h) {
                for x in ox * step..((ox + 1) * step).min(src.w) {
                    let i = (y * src.w + x) * 4;
                    for (a, &v) in acc.iter_mut().zip(&src.rgba[i..i + 4]) {
                        *a += v as u32;
                    }
                    n += 1;
                }
            }
            let o = (oy * ow + ox) * 4;
            for (out, &a) in rgba[o..o + 4].iter_mut().zip(&acc) {
                *out = if n == 0 { 0 } else { (a / n) as u8 };
            }
        }
    }
    Image { w: ow, h: oh, rgba }
}

pub fn picture_preview(bytes: &[u8]) -> Result<Image, String> {
    let img = crate::tga::decode(bytes)?;
    if img.w == 0 || img.h == 0 {
        return Err("empty picture".into());
    }
    let mut small = downsample(&img, PREVIEW_W);
    small.flip_rows();
    Ok(small)
}

pub fn recipe_preview(d: &D6m) -> Result<Image, String> {
    let w = d.width.max(0) as usize;
    let h = d.height.max(0) as usize;
    if w == 0 || h == 0 || d.heights.len() < w * h {
        return Err("no height field".into());
    }
    let step = w.div_ceil(PREVIEW_W).max(1);
    let ow = w.div_ceil(step);
    let oh = h.div_ceil(step);
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for oy in 0..oh {
        for ox in 0..ow {
            let i = (oy * step) * w + ox * step;
            let v = units_from_stored(d.heights[i]);
            if is_channel(v) || d.owners.get(i).copied().unwrap_or(0) <= 0 {
                continue;
            }
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let (lo, hi) = if lo == f32::MAX {
        (-1.0, 1.0)
    } else {
        (lo.min(-1.0), hi.max(1.0))
    };
    let mut rgba = vec![255u8; ow * oh * 4];
    for oy in 0..oh {
        let sy = h - 1 - (oy * step).min(h - 1);
        for ox in 0..ow {
            let i = sy * w + (ox * step).min(w - 1);
            let o = (oy * ow + ox) * 4;
            if d.owners.get(i).copied().unwrap_or(0) <= 0
                && !is_channel(units_from_stored(d.heights[i]))
            {
                rgba[o..o + 3].copy_from_slice(&[18, 18, 20]);
                continue;
            }
            let c = relief_tint(units_from_stored(d.heights[i]), lo, hi);
            for k in 0..3 {
                rgba[o + k] = c[k].clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(Image { w: ow, h: oh, rgba })
}

pub struct Card {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    pub stats: Option<Stats>,
    pub error: Option<String>,
    pub image: Option<egui::ColorImage>,
}

#[cfg(not(target_arch = "wasm32"))]
struct Ready {
    index: usize,
    card: Card,
}

pub fn rendered_preview(d: &D6m, map: Option<&MapFile>, tex: &TexSet) -> Result<Image, String> {
    let w = d.width.max(0) as usize;
    let h = d.height.max(0) as usize;
    if w == 0 || h == 0 || d.heights.len() < w * h || d.owners.len() < w * h {
        return Err("no height field".into());
    }
    let step = w.div_ceil(PREVIEW_W * 2).max(1);
    let ow = w.div_ceil(step);
    let oh = h.div_ceil(step);
    let mut heights = Vec::with_capacity(ow * oh);
    let mut owners = Vec::with_capacity(ow * oh);
    for oy in 0..oh {
        let row = (oy * step).min(h - 1) * w;
        for ox in 0..ow {
            let i = row + (ox * step).min(w - 1);
            heights.push(d.heights[i]);
            owners.push(d.owners[i]);
        }
    }
    let n = d.provinces.len();
    let mut flags = vec![0u64; n + 1];
    for (i, p) in d.provinces.iter().enumerate() {
        let id = i as u32 + 1;
        let from_map = map.and_then(|m| m.terrain.get(&id).copied());
        flags[i + 1] = from_map.unwrap_or(p.terrain) as u64;
    }
    let capitals: Vec<(i16, i16)> = d
        .provinces
        .iter()
        .map(|p| (p.x / step as i16, p.y / step as i16))
        .collect();
    let index = map
        .and_then(|m| {
            m.path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .map(|s| strip_plane_suffix(&s).1)
        .unwrap_or(1);
    let mut rivers = Vec::new();
    let mut lines = Vec::new();
    let mut bridges = Vec::new();
    if let Some(m) = map {
        for &(a, b) in &m.neighbours {
            if a == 0 || b == 0 || a as usize > n || b as usize > n || a >= b {
                continue;
            }
            if (flags[a as usize] | flags[b as usize]) & crate::terrain::UNKNOWN != 0 {
                continue;
            }
            let spec = m.spec_between(a, b);
            if spec as u64 & crate::terrain::BORDER_CARVED != 0 {
                rivers.push((a, b));
            }
            if crate::decor::is_mountain_line(spec) {
                lines.push((a, b));
            }
            if spec as u64 & crate::terrain::BORDER_BRIDGE != 0 {
                bridges.push((a, b));
            }
        }
    }
    let plane = Plane {
        w: ow as i32,
        h: oh as i32,
        heights: &heights,
        owners: &owners,
        flags: &flags,
        scale: d.map_scale() / step as f32,
        hwrap: map.map(|m| m.hwrap).unwrap_or(false),
        vwrap: map.map(|m| m.vwrap).unwrap_or(false),
        capitals: &capitals,
        rivers: &rivers,
        mountain_lines: &lines,
        bridges: &bridges,
        cave_plane: index == 2,
        image: None,
    };
    let opts = Options {
        capitals: false,
        ..Options::default()
    };
    let rendered = Rendered::new(&plane, tex, &opts);
    let rgba = crate::render::flip_to_top_down(plane.w, plane.h, &rendered.composed(&[]));
    let (mut x0, mut y0, mut x1, mut y1) = (ow, oh, 0usize, 0usize);
    for (i, &o) in owners.iter().enumerate() {
        if o > 0 {
            let (x, y) = (i % ow, oh - 1 - i / ow);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 || y0 > y1 {
        return Ok(downsample(&Image { w: ow, h: oh, rgba }, PREVIEW_W));
    }
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut cut = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        cut.extend_from_slice(&rgba[(y * ow + x0) * 4..(y * ow + x1 + 1) * 4]);
    }
    let cropped = Image {
        w: cw,
        h: ch,
        rgba: cut,
    };
    Ok(downsample(&cropped, PREVIEW_W))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_card(root: &Path, path: &Path, tex: &TexSet) -> Card {
    let mut card = Card {
        path: path.to_path_buf(),
        name: name_of(path),
        folder: folder_of(root, path),
        stats: None,
        error: None,
        image: None,
    };
    match stats_of(path) {
        Ok((stats, image)) => {
            let kind = stats.kind;
            let mut size = stats.size;
            card.stats = Some(stats);
            let preview = match (kind, image.as_ref()) {
                (Kind::Picture, Some(p)) => crate::io::read(p)
                    .map_err(|e| e.to_string())
                    .and_then(|b| picture_preview(&b)),
                (Kind::Recipe, Some(p)) => D6m::load(p).map_err(|e| e.to_string()).and_then(|d| {
                    if size.is_none() {
                        size = Some((d.width, d.height));
                    }
                    let map = MapFile::load(path).ok();
                    rendered_preview(&d, map.as_ref(), tex).or_else(|_| recipe_preview(&d))
                }),
                _ => Err(String::new()),
            };
            if let Some(s) = card.stats.as_mut() {
                s.size = size;
            }
            match preview {
                Ok(img) => {
                    card.image = Some(egui::ColorImage::from_rgba_unmultiplied(
                        [img.w, img.h],
                        &img.rgba,
                    ));
                }
                Err(e) if !e.is_empty() => card.error = Some(e),
                Err(_) => {}
            }
        }
        Err(e) => card.error = Some(e),
    }
    card
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(
    ctx: egui::Context,
    root: PathBuf,
    paths: Vec<PathBuf>,
    tx: Sender<Ready>,
    cancel: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let tex = TexSet::embedded();
        for (index, path) in paths.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let card = load_card(&root, &path, &tex);
            if tx.send(Ready { index, card }).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
}

pub enum Outcome {
    Stay,
    Close,
    Open(PathBuf),
}

#[cfg(not(target_arch = "wasm32"))]
struct Feed {
    rx: Receiver<Ready>,
    cancel: Arc<AtomicBool>,
}

#[cfg(target_arch = "wasm32")]
struct Feed {
    cancel: std::rc::Rc<std::cell::Cell<bool>>,
}

pub struct MapChooser {
    label: String,
    slots: Slots<Card>,
    tex: Vec<Option<egui::TextureHandle>>,
    feed: Feed,
}

impl Drop for MapChooser {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.feed.cancel.store(true, Ordering::Relaxed);
        #[cfg(target_arch = "wasm32")]
        self.feed.cancel.set(true);
    }
}

impl MapChooser {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(ctx: &egui::Context, root: &Path, paths: Vec<PathBuf>) -> MapChooser {
        let paths = order(root, &paths);
        let n = paths.len();
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        spawn(
            ctx.clone(),
            root.to_path_buf(),
            paths,
            tx,
            Arc::clone(&cancel),
        );
        MapChooser {
            label: crate::settings::shown(root),
            slots: Slots::new(n),
            tex: (0..n).map(|_| None).collect(),
            feed: Feed { rx, cancel },
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn web(ctx: &egui::Context, folder: &str, rels: Vec<String>) -> MapChooser {
        let paths: Vec<PathBuf> = rels
            .iter()
            .map(|r| PathBuf::from(format!("/{r}")))
            .collect();
        let ordered = order(Path::new("/"), &paths);
        let rels: Vec<String> = ordered.iter().map(|p| rel_of(Path::new("/"), p)).collect();
        let n = rels.len();
        let cancel = std::rc::Rc::new(std::cell::Cell::new(false));
        crate::web::spawn_cards(ctx.clone(), rels, std::rc::Rc::clone(&cancel));
        MapChooser {
            label: folder.to_owned(),
            slots: Slots::new(n),
            tex: (0..n).map(|_| None).collect(),
            feed: Feed { cancel },
        }
    }

    pub fn count(&self) -> usize {
        self.slots.len()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll(&mut self) {
        while let Ok(ready) = self.feed.rx.try_recv() {
            self.slots.put(ready.index, ready.card);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll(&mut self) {
        for (index, card) in crate::web::take_cards() {
            self.slots.put(index, card);
        }
    }

    fn texture(&mut self, ctx: &egui::Context, i: usize) -> Option<egui::TextureHandle> {
        if self.tex[i].is_none() {
            let image = self.slots.get(i).and_then(|c| c.image.clone())?;
            self.tex[i] = Some(ctx.load_texture(
                format!("map_chooser_{i}"),
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
        self.tex[i].clone()
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        self.poll();
        let total = self.slots.len();
        let filled = self.slots.filled();
        let shown = self.slots.ready();
        let screen = ctx.screen_rect();
        let cols = (((screen.width() - 80.0 + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize)
            .clamp(1, 4)
            .min(total.max(1));
        let width = cols as f32 * CARD_W + (cols - 1) as f32 * CARD_GAP;
        let max_h = theme::modal_height(ctx);
        let root = self.label.clone();
        let mut picked = None;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("map_chooser"))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                theme::scroll_body(ui, width, max_h, |ui| {
                    ui.horizontal(|ui| {
                        theme::title(ui, "Choose map");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if filled < total {
                                ui.label(
                                    egui::RichText::new(format!("{filled} / {total}"))
                                        .color(theme::INK_DIM),
                                );
                                ui.add(egui::Spinner::new().size(14.0));
                            }
                        });
                    });
                    theme::dim(ui, &format!("{total} maps in {root}"));
                    ui.add_space(8.0);
                    let textures: Vec<Option<egui::TextureHandle>> =
                        (0..shown).map(|i| self.texture(ctx, i)).collect();
                    egui::Grid::new("map_chooser_grid")
                        .num_columns(cols)
                        .spacing(egui::vec2(CARD_GAP, CARD_GAP))
                        .show(ui, |ui| {
                            for (i, tex) in textures.iter().enumerate() {
                                let card = match self.slots.get(i) {
                                    Some(c) => c,
                                    None => continue,
                                };
                                if map_card(ui, card, tex.as_ref()) {
                                    picked = Some(card.path.clone());
                                }
                                if (i + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    ui.add_space(8.0);
                    theme::rule(ui);
                    if theme::boxed_button(ui, "Cancel", true) {
                        cancel = true;
                    }
                });
            });
        if let Some(path) = picked {
            return Outcome::Open(path);
        }
        if cancel || response.should_close() {
            return Outcome::Close;
        }
        Outcome::Stay
    }
}

fn stat_lines(card: &Card) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(s) = &card.stats {
        let size = match s.size {
            Some((w, h)) => format!("{w} x {h}"),
            None => "size unknown".to_string(),
        };
        out.push(format!("{size}  {}", s.kind.label()));
        let planes = if s.planes > 1 {
            format!("  {} planes", s.planes)
        } else {
            String::new()
        };
        out.push(format!(
            "{} provinces  {} sea / {} land{planes}",
            s.provinces, s.sea, s.land
        ));
    }
    out
}

fn map_card(ui: &mut egui::Ui, card: &Card, tex: Option<&egui::TextureHandle>) -> bool {
    let inner_w = CARD_W - 16.0;
    let thumb_h = THUMB_H;
    let lines = stat_lines(card);
    let text_h = 22.0 + 16.0 * 4.0;
    let size = egui::vec2(CARD_W, thumb_h + 8.0 + text_h + 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let hot = resp.hovered();
    let stroke = if hot {
        egui::Stroke::new(1.0, theme::INK_HOT)
    } else {
        egui::Stroke::new(1.0, theme::PANEL_EDGE_DIM)
    };
    let p = ui.painter();
    p.rect(
        rect,
        3.0,
        if hot {
            egui::Color32::from_rgb(40, 36, 26)
        } else {
            egui::Color32::from_rgb(26, 24, 18)
        },
        stroke,
        egui::StrokeKind::Inside,
    );
    let img = egui::Rect::from_min_size(
        rect.min + egui::vec2(8.0, 8.0),
        egui::vec2(inner_w, thumb_h),
    );
    match tex {
        Some(t) => {
            p.rect_filled(img, 2.0, egui::Color32::from_rgb(18, 18, 20));
            let s = t.size_vec2();
            let k = (img.width() / s.x).min(img.height() / s.y);
            let fitted = egui::Rect::from_center_size(img.center(), s * k);
            p.image(
                t.id(),
                fitted,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            p.rect_filled(img, 2.0, egui::Color32::from_rgb(18, 18, 20));
        }
    }
    let mut y = img.max.y + 6.0;
    let x = rect.min.x + 8.0;
    p.text(
        egui::pos2(x, y),
        egui::Align2::LEFT_TOP,
        &card.name,
        egui::FontId::proportional(15.0),
        if hot { theme::INK_HOT } else { theme::INK },
    );
    y += 20.0;
    if !card.folder.is_empty() {
        p.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_TOP,
            &card.folder,
            egui::FontId::proportional(12.5),
            theme::BRASS,
        );
        y += 16.0;
    }
    for line in &lines {
        p.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::proportional(12.5),
            theme::INK_DIM,
        );
        y += 16.0;
    }
    if let Some(e) = &card.error {
        p.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_TOP,
            e,
            egui::FontId::proportional(12.5),
            theme::WARN,
        );
    }
    resp.clicked()
}
