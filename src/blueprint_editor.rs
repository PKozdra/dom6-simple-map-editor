use dom6_mapgen::height::build_randboard_rgb_classification_mask;
use dom6_mapgen::layouts::{layout_blueprint_variant, layout_variant_for_seed};
use dom6_mapgen::{Blueprint, Layout};

use crate::theme;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Paint {
    Land,
    Sea,
    NoStartLand,
    NoStartSea,
}

impl Paint {
    pub const ALL: [Paint; 4] = [
        Paint::Land,
        Paint::Sea,
        Paint::NoStartLand,
        Paint::NoStartSea,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Paint::Land => "Land",
            Paint::Sea => "Sea",
            Paint::NoStartLand => "Land, no start",
            Paint::NoStartSea => "Sea, no start",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Paint::Land => "Dry land that players may start on",
            Paint::Sea => "Water that players may start in",
            Paint::NoStartLand => "Dry land nobody starts on",
            Paint::NoStartSea => "Water nobody starts in",
        }
    }

    pub fn rgba(self) -> [u8; 4] {
        match self {
            Paint::Land => [5, 78, 32, 255],
            Paint::Sea => [33, 5, 144, 255],
            Paint::NoStartLand => [67, 67, 67, 255],
            Paint::NoStartSea => [40, 40, 40, 255],
        }
    }

    pub fn index(self) -> u8 {
        match self {
            Paint::Land => 0,
            Paint::Sea => 1,
            Paint::NoStartLand => 2,
            Paint::NoStartSea => 3,
        }
    }

    pub fn from_index(i: u8) -> Paint {
        Paint::ALL[(i as usize).min(3)]
    }

    pub fn code(self) -> i8 {
        match self {
            Paint::Land => 1,
            Paint::Sea => -1,
            Paint::NoStartLand => 2,
            Paint::NoStartSea => -2,
        }
    }

    pub fn from_code(code: i8) -> Paint {
        match code {
            1 => Paint::Land,
            2 => Paint::NoStartLand,
            -2 => Paint::NoStartSea,
            _ => Paint::Sea,
        }
    }

    pub fn colour(self) -> egui::Color32 {
        let c = self.rgba();
        egui::Color32::from_rgb(c[0], c[1], c[2])
    }
}

pub const CANVAS_SHAPES: [(&str, usize, usize); 4] = [
    ("1:1", 256, 256),
    ("3:2", 264, 176),
    ("2:1", 256, 128),
    ("16:9", 256, 144),
];

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<u8>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, fill: Paint) -> Canvas {
        Canvas {
            w: w.max(1),
            h: h.max(1),
            cells: vec![fill.index(); w.max(1) * h.max(1)],
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Paint> {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return None;
        }
        Some(Paint::from_index(
            self.cells[y as usize * self.w + x as usize],
        ))
    }

    pub fn set(&mut self, x: i32, y: i32, paint: Paint) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        self.cells[y as usize * self.w + x as usize] = paint.index();
    }

    pub fn fill_all(&mut self, paint: Paint) {
        self.cells.fill(paint.index());
    }

    pub fn disc(&mut self, cx: i32, cy: i32, size: i32, paint: Paint) {
        let r = size.max(1) as f32 * 0.5;
        let rr = r * r;
        let reach = size.max(1) / 2;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                if (dx * dx + dy * dy) as f32 <= rr {
                    self.set(cx + dx, cy + dy, paint);
                }
            }
        }
    }

    pub fn stroke(&mut self, from: (i32, i32), to: (i32, i32), size: i32, paint: Paint) {
        let (mut x, mut y) = from;
        let (tx, ty) = to;
        let dx = (tx - x).abs();
        let dy = -(ty - y).abs();
        let sx = if x < tx { 1 } else { -1 };
        let sy = if y < ty { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.disc(x, y, size, paint);
            if x == tx && y == ty {
                break;
            }
            let e2 = err * 2;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn flood_fill(&mut self, x: i32, y: i32, paint: Paint) -> bool {
        let Some(from) = self.get(x, y) else {
            return false;
        };
        if from == paint {
            return false;
        }
        let target = from.index();
        let value = paint.index();
        let mut stack = vec![(x as usize, y as usize)];
        while let Some((px, py)) = stack.pop() {
            let i = py * self.w + px;
            if self.cells[i] != target {
                continue;
            }
            self.cells[i] = value;
            if px > 0 {
                stack.push((px - 1, py));
            }
            if px + 1 < self.w {
                stack.push((px + 1, py));
            }
            if py > 0 {
                stack.push((px, py - 1));
            }
            if py + 1 < self.h {
                stack.push((px, py + 1));
            }
        }
        true
    }

    pub fn resampled(&self, w: usize, h: usize) -> Canvas {
        let w = w.max(1);
        let h = h.max(1);
        let mut cells = vec![0u8; w * h];
        for y in 0..h {
            let sy = (y * self.h) / h;
            for x in 0..w {
                let sx = (x * self.w) / w;
                cells[y * w + x] = self.cells[sy * self.w + sx];
            }
        }
        Canvas { w, h, cells }
    }

    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for c in &self.cells {
            out.extend_from_slice(&Paint::from_index(*c).rgba());
        }
        out
    }

    pub fn to_blueprint(&self) -> Blueprint {
        let mut bgra = Vec::with_capacity(self.w * self.h * 4);
        for c in &self.cells {
            let p = Paint::from_index(*c).rgba();
            bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        Blueprint {
            w: self.w as i32,
            h: self.h as i32,
            bgra,
        }
    }

    pub fn from_blueprint(bp: &Blueprint, w: usize, h: usize) -> Canvas {
        let w = w.max(1);
        let h = h.max(1);
        let sw = bp.w.max(1) as usize;
        let sh = bp.h.max(1) as usize;
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        let mut cells = vec![Paint::Sea.index(); w * h];
        if mask.cells.len() < sw * sh {
            return Canvas { w, h, cells };
        }
        for y in 0..h {
            let sy = (y * sh) / h;
            for x in 0..w {
                let sx = (x * sw) / w;
                cells[y * w + x] = Paint::from_code(mask.cells[sy * sw + sx]).index();
            }
        }
        Canvas { w, h, cells }
    }

    pub fn classification_rgba(&self) -> Vec<u8> {
        let bp = self.to_blueprint();
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for code in &mask.cells {
            let c = match code {
                1 => [96, 176, 96, 255],
                2 => [150, 150, 150, 255],
                -2 => [80, 80, 80, 255],
                _ => [70, 110, 200, 255],
            };
            out.extend_from_slice(&c);
        }
        out
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Fill,
}

pub enum Outcome {
    Open,
    Cancelled,
    Applied(Blueprint),
}

#[derive(Clone, Copy)]
pub struct Subject<'a> {
    pub layout: Layout,
    pub seed: u32,
    pub current: Option<&'a Blueprint>,
    pub blueprints: &'a Path,
    pub name: &'a str,
    pub caves: bool,
    pub guide: Option<&'a Blueprint>,
}

pub struct BlueprintEditor {
    pub canvas: Canvas,
    pub paint: Paint,
    pub tool: Tool,
    pub size: i32,
    pub overlay: bool,
    pub guide_on: bool,
    shown_guide: bool,
    guide_rgba: Option<(usize, usize, Vec<u8>)>,
    pub shape: usize,
    undo: Vec<Canvas>,
    tex: Option<egui::TextureHandle>,
    stale: bool,
    shown_overlay: bool,
    last: Option<(i32, i32)>,
    note: Option<String>,
}

const UNDO_DEPTH: usize = 48;

impl Default for BlueprintEditor {
    fn default() -> BlueprintEditor {
        BlueprintEditor::new(None)
    }
}

impl BlueprintEditor {
    pub fn new(start: Option<&Blueprint>) -> BlueprintEditor {
        let (_, w, h) = CANVAS_SHAPES[0];
        let canvas = match start {
            Some(bp) => Canvas::from_blueprint(bp, w, h),
            None => Canvas::new(w, h, Paint::Sea),
        };
        BlueprintEditor {
            canvas,
            paint: Paint::Land,
            tool: Tool::Brush,
            size: 12,
            overlay: false,
            guide_on: true,
            shown_guide: false,
            guide_rgba: None,
            shape: 0,
            undo: Vec::new(),
            tex: None,
            stale: true,
            shown_overlay: false,
            last: None,
            note: None,
        }
    }

    pub fn push_undo(&mut self) {
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(self.canvas.clone());
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(c) => {
                self.canvas = c;
                self.stale = true;
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn replace(&mut self, canvas: Canvas) {
        self.push_undo();
        self.canvas = canvas;
        self.stale = true;
    }

    pub fn set_shape(&mut self, shape: usize) {
        let Some((_, w, h)) = CANVAS_SHAPES.get(shape).copied() else {
            return;
        };
        if self.canvas.w == w && self.canvas.h == h {
            self.shape = shape;
            return;
        }
        self.push_undo();
        self.canvas = self.canvas.resampled(w, h);
        self.shape = shape;
        self.stale = true;
    }

    fn texture(&mut self, ctx: &egui::Context, guide: Option<&Blueprint>) -> egui::TextureHandle {
        let want_guide = self.guide_on && guide.is_some();
        if self.stale
            || self.shown_overlay != self.overlay
            || self.shown_guide != want_guide
            || self.tex.is_none()
        {
            let mut pixels = if self.overlay {
                self.canvas.classification_rgba()
            } else {
                self.canvas.rgba()
            };
            if let (true, Some(g)) = (want_guide, guide) {
                let (w, h) = (self.canvas.w, self.canvas.h);
                let fresh = match &self.guide_rgba {
                    Some((gw, gh, _)) => *gw != w || *gh != h,
                    None => true,
                };
                if fresh {
                    self.guide_rgba = Some((w, h, Canvas::from_blueprint(g, w, h).rgba()));
                }
                if let Some((_, _, g)) = &self.guide_rgba {
                    for (px, gp) in pixels.chunks_exact_mut(4).zip(g.chunks_exact(4)) {
                        let land = gp[1] > gp[2];
                        let tint: [u8; 3] = if land { [190, 170, 90] } else { [60, 110, 200] };
                        for c in 0..3 {
                            px[c] = ((px[c] as u32 * 11 + tint[c] as u32 * 5) / 16) as u8;
                        }
                    }
                }
            }
            self.shown_guide = want_guide;
            let image =
                egui::ColorImage::from_rgba_unmultiplied([self.canvas.w, self.canvas.h], &pixels);
            self.tex =
                Some(ctx.load_texture("blueprint_editor", image, egui::TextureOptions::NEAREST));
            self.stale = false;
            self.shown_overlay = self.overlay;
        }
        self.tex.clone().expect("texture was just uploaded")
    }

    pub fn show(&mut self, ctx: &egui::Context, at: Subject<'_>) -> Outcome {
        let mut outcome = Outcome::Open;
        let max_h = theme::modal_height(ctx);
        let response = egui::Modal::new(egui::Id::new("blueprint_editor"))
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                theme::panel_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(max_h - 24.0)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            self.body(ui, at, &mut outcome);
                        });
                });
            });
        if matches!(outcome, Outcome::Open) && response.should_close() {
            outcome = Outcome::Cancelled;
        }
        outcome
    }

    fn body(&mut self, ui: &mut egui::Ui, at: Subject<'_>, outcome: &mut Outcome) {
        if at.caves {
            theme::title(ui, "Draw a cave blueprint");
            theme::dim(
                ui,
                "Land is cave floor, sea is solid rock; the picture is stretched over the whole cave plane, which lies under the surface",
            );
        } else {
            theme::title(ui, "Draw a blueprint");
            theme::dim(
                ui,
                "Colours say where land and sea go; the picture is stretched over the whole map",
            );
        }
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            self.canvas_ui(ui, at.guide);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.set_width(220.0);
                self.tools_ui(ui, at);
            });
        });
        ui.add_space(8.0);
        theme::rule(ui);
        ui.horizontal(|ui| {
            if theme::boxed_button(ui, "OK", true) {
                *outcome = Outcome::Applied(self.canvas.to_blueprint());
            }
            if theme::boxed_button(ui, "Cancel", true) {
                *outcome = Outcome::Cancelled;
            }
            theme::dim(ui, &format!("{} x {}", self.canvas.w, self.canvas.h));
        });
    }

    fn canvas_ui(&mut self, ui: &mut egui::Ui, guide: Option<&Blueprint>) {
        let screen = ui.ctx().screen_rect();
        let max_w = (screen.width() - 320.0).max(200.0);
        let max_h = (screen.height() - 220.0).max(200.0);
        let scale = (max_w / self.canvas.w as f32)
            .min(max_h / self.canvas.h as f32)
            .clamp(1.0, 3.0)
            .floor();
        let size = egui::vec2(self.canvas.w as f32 * scale, self.canvas.h as f32 * scale);
        let tex = self.texture(ui.ctx(), guide);
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let rect = response.rect;
        painter.image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, theme::PANEL_EDGE),
            egui::StrokeKind::Outside,
        );
        let at = response.interact_pointer_pos().map(|p| {
            (
                ((p.x - rect.min.x) / scale).floor() as i32,
                ((p.y - rect.min.y) / scale).floor() as i32,
            )
        });
        if response.drag_started() || (response.clicked() && self.last.is_none()) {
            self.push_undo();
            self.last = None;
        }
        if let Some(pos) = at {
            if response.dragged() || response.clicked() {
                match self.tool {
                    Tool::Brush => {
                        match self.last {
                            Some(prev) => self.canvas.stroke(prev, pos, self.size, self.paint),
                            None => self.canvas.disc(pos.0, pos.1, self.size, self.paint),
                        }
                        self.last = Some(pos);
                    }
                    Tool::Fill => {
                        self.canvas.flood_fill(pos.0, pos.1, self.paint);
                    }
                }
                self.stale = true;
            }
        }
        if response.drag_stopped() || response.clicked() {
            self.last = None;
        }
    }

    fn tools_ui(&mut self, ui: &mut egui::Ui, at: Subject<'_>) {
        let Subject {
            layout,
            seed,
            current,
            blueprints,
            name,
            caves,
            guide,
        } = at;
        theme::section_first(ui, "Colour");
        for chunk in Paint::ALL.chunks(2) {
            ui.horizontal(|ui| {
                for p in chunk {
                    if swatch(ui, self.paint == *p, *p) {
                        self.paint = *p;
                    }
                }
            });
        }
        theme::section(ui, "Tool");
        ui.horizontal(|ui| {
            if theme::tab(ui, self.tool == Tool::Brush, "Brush") {
                self.tool = Tool::Brush;
            }
            if theme::tab(ui, self.tool == Tool::Fill, "Fill") {
                self.tool = Tool::Fill;
            }
        });
        ui.horizontal(|ui| {
            ui.add_sized([44.0, 20.0], egui::Label::new("Size"));
            ui.add_enabled(
                self.tool == Tool::Brush,
                egui::Slider::new(&mut self.size, 1..=64),
            );
        });
        theme::section(ui, "Canvas");
        ui.horizontal_wrapped(|ui| {
            for (i, (label, _, _)) in CANVAS_SHAPES.iter().enumerate() {
                if theme::tab(ui, self.shape == i, label) {
                    self.set_shape(i);
                }
            }
        });
        ui.horizontal(|ui| {
            if theme::boxed_button(ui, "All sea", true) {
                self.push_undo();
                self.canvas.fill_all(Paint::Sea);
                self.stale = true;
            }
            if theme::boxed_button(ui, "All land", true) {
                self.push_undo();
                self.canvas.fill_all(Paint::Land);
                self.stale = true;
            }
        });
        ui.horizontal(|ui| {
            if theme::boxed_button(ui, "Undo", self.can_undo()) {
                self.undo();
            }
        });
        ui.horizontal(|ui| {
            if theme::boxed_button_hint(
                ui,
                "Save image",
                true,
                "Writes the canvas as a PNG in the blueprints folder",
            ) {
                self.note = match save_canvas_into(&self.canvas, blueprints, name) {
                    Ok(path) => Some(format!("Saved {path}")),
                    Err(e) => Some(e),
                };
            }
            if theme::boxed_button_hint(ui, "Save as\u{2026}", true, "Chooses where the PNG goes") {
                self.note = match save_canvas_as(&self.canvas, blueprints, name) {
                    Ok(Some(path)) => Some(format!("Saved {path}")),
                    Ok(None) => None,
                    Err(e) => Some(e),
                };
            }
        });
        if let Some(n) = &self.note {
            ui.label(
                egui::RichText::new(crate::app::elide_left(n, 30))
                    .size(12.0)
                    .color(theme::INK_DIM),
            )
            .on_hover_text(n);
        }
        theme::section(ui, "Start from");
        ui.horizontal(|ui| {
            if theme::boxed_button(ui, "Own image", current.is_some()) {
                if let Some(bp) = current {
                    let c = Canvas::from_blueprint(bp, self.canvas.w, self.canvas.h);
                    self.replace(c);
                }
            }
            if theme::boxed_button(ui, "Surface", guide.is_some()) {
                if let Some(bp) = guide {
                    let c = Canvas::from_blueprint(bp, self.canvas.w, self.canvas.h);
                    self.replace(c);
                }
            }
            if theme::boxed_button(ui, "Layout", layout != Layout::Standard) {
                let variant = layout_variant_for_seed(layout, seed);
                if let Some(bp) = layout_blueprint_variant(
                    layout,
                    variant,
                    self.canvas.w as i32,
                    self.canvas.h as i32,
                ) {
                    let c = Canvas::from_blueprint(&bp, self.canvas.w, self.canvas.h);
                    self.replace(c);
                }
            }
        });
        if caves && guide.is_some() {
            theme::section(ui, "View");
            theme::check(ui, &mut self.guide_on, "Show the surface above")
                .on_hover_text("Tints the canvas with the surface blueprint so cave floor can be put under the land where gateways should come out");
        }
    }
}

fn swatch(ui: &mut egui::Ui, selected: bool, paint: Paint) -> bool {
    let galley = ui.painter().layout_no_wrap(
        paint.label().to_owned(),
        egui::FontId::proportional(13.0),
        if selected { theme::INK_HOT } else { theme::INK },
    );
    let box_w = 22.0;
    let size = egui::vec2(box_w + galley.size().x + 6.0, galley.size().y.max(20.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let b = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 9.0, rect.center().y),
            egui::vec2(16.0, 16.0),
        );
        painter.rect_filled(b, 2.0, paint.colour());
        let edge = if selected {
            theme::INK_ACTIVE
        } else if response.hovered() {
            theme::PANEL_EDGE
        } else {
            theme::PANEL_EDGE_DIM
        };
        painter.rect_stroke(
            b,
            2.0,
            egui::Stroke::new(if selected { 2.0 } else { 1.0 }, edge),
            egui::StrokeKind::Inside,
        );
        let pos = egui::pos2(rect.min.x + box_w, rect.center().y - galley.size().y * 0.5);
        painter.galley(pos, galley, theme::INK);
    }
    response.on_hover_text(paint.hint()).clicked()
}

pub fn encode_png(w: usize, h: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_canvas(canvas: &Canvas, path: &Path) -> Result<String, String> {
    let bytes = encode_png(canvas.w, canvas.h, &canvas.rgba())?;
    crate::io::write_plain(path, &bytes)?;
    Ok(crate::settings::shown(path))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_canvas_into(canvas: &Canvas, blueprints: &Path, name: &str) -> Result<String, String> {
    let dir = crate::settings::ensure(blueprints.to_path_buf())?;
    let path = crate::settings::unique_path(&dir, name, "png");
    write_canvas(canvas, &path)
}

#[cfg(not(target_arch = "wasm32"))]
fn save_canvas_as(
    canvas: &Canvas,
    blueprints: &Path,
    name: &str,
) -> Result<Option<String>, String> {
    let dir = crate::settings::ensure(blueprints.to_path_buf())?;
    let Some(path) = rfd::FileDialog::new()
        .add_filter("PNG image", &["png"])
        .set_directory(&dir)
        .set_file_name(format!("{name}.png"))
        .save_file()
    else {
        return Ok(None);
    };
    write_canvas(canvas, &path).map(Some)
}

#[cfg(target_arch = "wasm32")]
fn save_canvas_into(canvas: &Canvas, _blueprints: &Path, name: &str) -> Result<String, String> {
    let bytes = encode_png(canvas.w, canvas.h, &canvas.rgba())?;
    let file = format!("{name}.png");
    crate::web::download(&file, &bytes);
    Ok(format!("{file} as a download"))
}

#[cfg(target_arch = "wasm32")]
fn save_canvas_as(
    canvas: &Canvas,
    blueprints: &Path,
    name: &str,
) -> Result<Option<String>, String> {
    save_canvas_into(canvas, blueprints, name).map(Some)
}

pub fn blueprint_rgba(bp: &Blueprint) -> Vec<u8> {
    let mut out = Vec::with_capacity(bp.bgra.len());
    for px in bp.bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(c: &Canvas, paint: Paint) -> usize {
        c.cells.iter().filter(|v| **v == paint.index()).count()
    }

    #[test]
    fn a_stroke_paints_a_line_of_discs() {
        let mut c = Canvas::new(32, 32, Paint::Sea);
        c.stroke((4, 16), (27, 16), 3, Paint::Land);
        for x in 4..=27 {
            assert_eq!(c.get(x, 16), Some(Paint::Land), "x={x}");
            assert_eq!(c.get(x, 15), Some(Paint::Land), "x={x}");
        }
        assert_eq!(c.get(2, 16), Some(Paint::Sea));
        assert_eq!(c.get(29, 16), Some(Paint::Sea));
        assert_eq!(c.get(0, 0), Some(Paint::Sea));
        assert_eq!(c.get(16, 13), Some(Paint::Sea));
    }

    #[test]
    fn a_size_one_brush_touches_one_pixel() {
        let mut c = Canvas::new(8, 8, Paint::Sea);
        c.disc(3, 3, 1, Paint::Land);
        assert_eq!(count(&c, Paint::Land), 1);
        assert_eq!(c.get(3, 3), Some(Paint::Land));
    }

    #[test]
    fn discs_are_clipped_at_the_edges() {
        let mut c = Canvas::new(16, 16, Paint::Sea);
        c.disc(0, 0, 9, Paint::Land);
        assert_eq!(c.get(0, 0), Some(Paint::Land));
        assert_eq!(c.get(4, 0), Some(Paint::Land));
        assert_eq!(c.get(5, 0), Some(Paint::Sea));
        let mut whole = Canvas::new(16, 16, Paint::Sea);
        whole.disc(8, 8, 9, Paint::Land);
        assert!(count(&c, Paint::Land) < count(&whole, Paint::Land));
        c.stroke((-40, -40), (60, 60), 7, Paint::NoStartLand);
        assert_eq!(c.get(15, 15), Some(Paint::NoStartLand));
    }

    #[test]
    fn flood_fill_stops_at_a_painted_border() {
        let mut c = Canvas::new(16, 16, Paint::Sea);
        c.stroke((8, 0), (8, 15), 1, Paint::Land);
        assert!(c.flood_fill(0, 0, Paint::NoStartSea));
        assert_eq!(c.get(7, 9), Some(Paint::NoStartSea));
        assert_eq!(c.get(8, 9), Some(Paint::Land));
        assert_eq!(c.get(9, 9), Some(Paint::Sea));
        assert!(!c.flood_fill(0, 0, Paint::NoStartSea));
        assert!(!c.flood_fill(-1, 0, Paint::Land));
    }

    #[test]
    fn undo_restores_the_previous_canvas() {
        let mut e = BlueprintEditor::new(None);
        assert!(!e.can_undo());
        let before = e.canvas.clone();
        e.push_undo();
        e.canvas.disc(10, 10, 9, Paint::Land);
        assert_ne!(e.canvas, before);
        assert!(e.undo());
        assert_eq!(e.canvas, before);
        assert!(!e.undo());
    }

    #[test]
    fn undo_keeps_a_bounded_stack() {
        let mut e = BlueprintEditor::new(None);
        for i in 0..UNDO_DEPTH + 10 {
            e.push_undo();
            e.canvas.set(i as i32 % 40, 0, Paint::Land);
        }
        assert_eq!(e.undo.len(), UNDO_DEPTH);
    }

    #[test]
    fn export_uses_bgra_and_the_canvas_size() {
        let mut c = Canvas::new(4, 3, Paint::Sea);
        c.set(0, 0, Paint::Land);
        let bp = c.to_blueprint();
        assert_eq!((bp.w, bp.h), (4, 3));
        assert_eq!(bp.bgra.len(), 4 * 3 * 4);
        assert_eq!(&bp.bgra[0..4], &[32, 78, 5, 255]);
        assert_eq!(&bp.bgra[4..8], &[144, 5, 33, 255]);
    }

    #[test]
    fn the_generator_reads_back_the_four_colours() {
        let mut c = Canvas::new(2, 2, Paint::Sea);
        c.set(0, 0, Paint::Land);
        c.set(1, 0, Paint::Sea);
        c.set(0, 1, Paint::NoStartLand);
        c.set(1, 1, Paint::NoStartSea);
        let bp = c.to_blueprint();
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        assert_eq!(mask.cells, vec![1, -1, 2, -2]);
        let back = Canvas::from_blueprint(&bp, 2, 2);
        assert_eq!(back, c);
    }

    #[test]
    fn a_half_land_canvas_reports_half_sea() {
        let mut c = Canvas::new(64, 64, Paint::Sea);
        for y in 0..64 {
            for x in 0..32 {
                c.set(x, y, Paint::Land);
            }
        }
        let bp = c.to_blueprint();
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        assert!((mask.sea_frac - 0.5).abs() < 1e-6);
        let all_sea = Canvas::new(64, 64, Paint::Sea).to_blueprint();
        let mask = build_randboard_rgb_classification_mask(&all_sea.bgra, all_sea.w, all_sea.h);
        assert!((mask.sea_frac - 1.0).abs() < 1e-6);
        assert!(mask.cells.iter().all(|c| *c == -1));
    }

    #[test]
    fn no_start_paints_count_as_sea_and_land_for_the_sea_fraction() {
        let mut c = Canvas::new(4, 1, Paint::NoStartSea);
        c.set(0, 0, Paint::NoStartLand);
        let bp = c.to_blueprint();
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        assert!((mask.sea_frac - 0.75).abs() < 1e-6);
    }

    #[test]
    fn changing_the_canvas_shape_keeps_the_drawing() {
        let mut e = BlueprintEditor::new(None);
        e.canvas.fill_all(Paint::Land);
        e.set_shape(2);
        assert_eq!((e.canvas.w, e.canvas.h), (256, 128));
        assert_eq!(count(&e.canvas, Paint::Land), 256 * 128);
        assert!(e.can_undo());
        e.set_shape(2);
        assert_eq!((e.canvas.w, e.canvas.h), (256, 128));
    }

    #[test]
    fn starting_from_a_layout_gives_both_land_and_sea() {
        let bp = layout_blueprint_variant(Layout::OneSea, 0, 128, 128).unwrap();
        let c = Canvas::from_blueprint(&bp, 128, 128);
        assert!(count(&c, Paint::Land) > 0);
        assert!(count(&c, Paint::Sea) > 0);
    }

    #[test]
    fn a_new_editor_starts_from_the_image_it_is_given() {
        let mut src = Canvas::new(16, 16, Paint::Land);
        src.set(0, 0, Paint::Sea);
        let e = BlueprintEditor::new(Some(&src.to_blueprint()));
        assert_eq!((e.canvas.w, e.canvas.h), (256, 256));
        assert_eq!(e.canvas.get(0, 0), Some(Paint::Sea));
        assert_eq!(e.canvas.get(255, 255), Some(Paint::Land));
    }

    #[test]
    fn a_png_of_the_canvas_decodes_back_to_the_same_pixels() {
        let mut c = Canvas::new(8, 5, Paint::Sea);
        c.disc(4, 2, 3, Paint::Land);
        let rgba = c.rgba();
        let bytes = encode_png(c.w, c.h, &rgba).unwrap();
        let img = crate::textures::decode_png(&bytes).unwrap();
        assert_eq!((img.w, img.h), (8, 5));
        assert_eq!(img.rgba, rgba);
        assert_eq!(blueprint_rgba(&c.to_blueprint()), rgba);
    }

    #[test]
    fn the_overlay_paints_one_colour_per_class() {
        let mut c = Canvas::new(2, 2, Paint::Sea);
        c.set(0, 0, Paint::Land);
        c.set(0, 1, Paint::NoStartLand);
        c.set(1, 1, Paint::NoStartSea);
        let o = c.classification_rgba();
        assert_eq!(&o[0..4], &[96, 176, 96, 255]);
        assert_eq!(&o[4..8], &[70, 110, 200, 255]);
        assert_eq!(&o[8..12], &[150, 150, 150, 255]);
        assert_eq!(&o[12..16], &[80, 80, 80, 255]);
    }
}
