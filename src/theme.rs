use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response, Stroke,
    TextStyle,
};
use std::sync::Arc;

pub const SIDE_FILL: Color32 = Color32::from_rgb(9, 9, 10);
pub const PANEL_FILL: Color32 = Color32::from_rgba_premultiplied(18, 18, 19, 240);
pub const PANEL_EDGE: Color32 = Color32::from_rgb(150, 132, 90);
pub const PANEL_EDGE_DIM: Color32 = Color32::from_rgb(78, 70, 52);
pub const INK: Color32 = Color32::from_rgb(232, 226, 208);
pub const INK_DIM: Color32 = Color32::from_rgb(158, 152, 138);
pub const INK_HOT: Color32 = Color32::from_rgb(255, 246, 210);
pub const INK_ACTIVE: Color32 = Color32::from_rgb(255, 214, 120);
pub const BRASS: Color32 = Color32::from_rgb(205, 178, 112);
pub const NAME_RED: Color32 = Color32::from_rgb(112, 22, 12);
pub const SELECT_EDGE: [u8; 3] = [255, 226, 70];
pub const SELECT_FILL: [u8; 3] = [255, 220, 90];
pub const WARN: Color32 = Color32::from_rgb(236, 160, 96);
pub const CANVAS: Color32 = Color32::from_rgb(0, 0, 0);
pub const TAG_START: Color32 = Color32::from_rgb(150, 220, 120);
pub const TAG_NO: Color32 = Color32::from_rgb(190, 180, 170);
pub const TAG_THRONE: Color32 = Color32::from_rgb(255, 214, 120);
pub const TAG_SITES: Color32 = Color32::from_rgb(200, 160, 240);
pub const TAG_GATE: Color32 = Color32::from_rgb(120, 210, 230);

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "dom6".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/guifont.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "dom6".to_owned());
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(15.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(15.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(20.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.5));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(14.0));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.combo_width = 120.0;
        style.spacing.slider_width = 150.0;
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = SIDE_FILL;
        v.window_fill = PANEL_FILL;
        v.extreme_bg_color = Color32::from_rgb(28, 28, 30);
        v.faint_bg_color = Color32::from_rgba_premultiplied(255, 255, 255, 8);
        v.window_stroke = Stroke::new(1.0_f32, PANEL_EDGE);
        v.selection.bg_fill = Color32::from_rgb(96, 82, 46);
        v.selection.stroke = Stroke::new(1.0_f32, INK_ACTIVE);
        v.hyperlink_color = INK_HOT;
        v.warn_fg_color = WARN;
        v.error_fg_color = Color32::from_rgb(240, 110, 90);
        v.popup_shadow = egui::Shadow::NONE;
        v.window_shadow = egui::Shadow::NONE;
        let w = &mut v.widgets;
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, PANEL_EDGE_DIM);
        w.noninteractive.bg_fill = Color32::TRANSPARENT;
        w.inactive.fg_stroke = Stroke::new(1.0_f32, INK);
        w.inactive.bg_fill = Color32::from_rgb(34, 33, 31);
        w.inactive.weak_bg_fill = Color32::from_rgb(34, 33, 31);
        w.inactive.bg_stroke = Stroke::new(1.0_f32, PANEL_EDGE_DIM);
        w.hovered.fg_stroke = Stroke::new(1.0_f32, INK_HOT);
        w.hovered.bg_fill = Color32::from_rgb(56, 51, 40);
        w.hovered.weak_bg_fill = Color32::from_rgb(56, 51, 40);
        w.hovered.bg_stroke = Stroke::new(1.0_f32, PANEL_EDGE);
        w.active.fg_stroke = Stroke::new(1.0_f32, INK_ACTIVE);
        w.active.bg_fill = Color32::from_rgb(72, 62, 40);
        w.active.weak_bg_fill = Color32::from_rgb(72, 62, 40);
        w.active.bg_stroke = Stroke::new(1.0_f32, INK_ACTIVE);
        w.open.fg_stroke = Stroke::new(1.0_f32, INK);
        w.open.bg_fill = Color32::from_rgb(40, 38, 34);
        w.open.weak_bg_fill = Color32::from_rgb(40, 38, 34);
        w.open.bg_stroke = Stroke::new(1.0_f32, PANEL_EDGE);
        for s in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            s.corner_radius = CornerRadius::same(2);
            s.expansion = 0.0;
        }
        v.window_corner_radius = CornerRadius::same(3);
        v.menu_corner_radius = CornerRadius::same(3);
    });
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_FILL)
        .stroke(Stroke::new(1.0_f32, PANEL_EDGE))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(14, 10))
}

pub fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(20.0).color(INK));
}

pub fn section_first(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text.to_uppercase())
                .size(12.0)
                .color(BRASS),
        );
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        let y = rect.center().y;
        ui.painter().line_segment(
            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
            Stroke::new(1.0_f32, PANEL_EDGE_DIM),
        );
    });
}

pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    section_first(ui, text);
}

pub fn dim(ui: &mut egui::Ui, text: &str) -> Response {
    ui.label(egui::RichText::new(text).color(INK_DIM))
}

pub fn text_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).size(15.0)).frame(false),
    )
    .clicked()
}

pub fn boxed_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).size(15.0)).min_size(egui::vec2(64.0, 26.0)),
    )
    .clicked()
}

pub fn boxed_button_hint(ui: &mut egui::Ui, text: &str, enabled: bool, hint: &str) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).size(15.0)).min_size(egui::vec2(64.0, 26.0)),
    )
    .on_hover_text(hint)
    .clicked()
}

pub fn tab(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
    let rich = egui::RichText::new(text)
        .size(15.0)
        .color(if selected { INK_ACTIVE } else { INK });
    let mut b = egui::Button::new(rich).min_size(egui::vec2(64.0, 26.0));
    if selected {
        b = b
            .fill(Color32::from_rgb(72, 62, 40))
            .stroke(Stroke::new(1.0_f32, INK_ACTIVE));
    }
    ui.add(b).clicked()
}

pub fn check(ui: &mut egui::Ui, value: &mut bool, text: &str) -> Response {
    check_enabled(ui, value, text, true)
}

pub fn check_enabled(ui: &mut egui::Ui, value: &mut bool, text: &str, enabled: bool) -> Response {
    let colour = if !enabled {
        INK_DIM
    } else if *value {
        INK_HOT
    } else {
        INK
    };
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(15.0), colour);
    let box_w = 22.0;
    let size = egui::vec2(box_w + galley.size().x + 2.0, galley.size().y.max(20.0));
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    if enabled && r.clicked() {
        *value = !*value;
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let b = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 8.0, rect.center().y),
            egui::vec2(14.0, 14.0),
        );
        let edge = if !enabled {
            PANEL_EDGE_DIM
        } else if r.hovered() {
            INK_ACTIVE
        } else {
            PANEL_EDGE
        };
        painter.rect_filled(b, 2.0, Color32::from_rgb(28, 27, 26));
        painter.rect_stroke(b, 2.0, Stroke::new(1.0_f32, edge), egui::StrokeKind::Inside);
        if *value {
            let c = if enabled { INK_ACTIVE } else { INK_DIM };
            painter.line_segment(
                [
                    egui::pos2(b.min.x + 3.0, b.center().y),
                    egui::pos2(b.center().x - 0.5, b.max.y - 3.5),
                ],
                Stroke::new(2.0_f32, c),
            );
            painter.line_segment(
                [
                    egui::pos2(b.center().x - 0.5, b.max.y - 3.5),
                    egui::pos2(b.max.x - 2.5, b.min.y + 3.0),
                ],
                Stroke::new(2.0_f32, c),
            );
        }
        let text_colour = if enabled && r.hovered() {
            INK_HOT
        } else {
            colour
        };
        let pos = egui::pos2(rect.min.x + box_w, rect.center().y - galley.size().y * 0.5);
        painter.galley(pos, galley, text_colour);
    }
    r
}

pub fn rule(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, PANEL_EDGE_DIM);
    ui.add_space(3.0);
}
