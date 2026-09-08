use crate::theme::{INK, INK_ACTIVE, INK_DIM, INK_HOT, PANEL_EDGE, PANEL_EDGE_DIM};
use egui::{Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Paren,
    Outline,
    Raised,
    Brass,
    Inverse,
    Ghost,
}

impl Style {
    pub const ALL: [Style; 6] = [
        Style::Paren,
        Style::Outline,
        Style::Raised,
        Style::Brass,
        Style::Inverse,
        Style::Ghost,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Style::Paren => "Paren",
            Style::Outline => "Outline",
            Style::Raised => "Raised",
            Style::Brass => "Brass",
            Style::Inverse => "Inverse",
            Style::Ghost => "Ghost",
        }
    }
}

pub const DEFAULT: Style = Style::Brass;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tone {
    #[default]
    Plain,
    Hot,
    Active,
}

struct Look {
    fill: Color32,
    edge: Color32,
    ink: Color32,
    lip: Option<Color32>,
}

fn look(style: Style, tone: Tone) -> Look {
    let ink = match tone {
        Tone::Plain => INK,
        Tone::Hot => INK_HOT,
        Tone::Active => INK_ACTIVE,
    };
    let edge = match tone {
        Tone::Plain => PANEL_EDGE_DIM,
        Tone::Hot => PANEL_EDGE,
        Tone::Active => INK_ACTIVE,
    };
    match style {
        Style::Paren => Look {
            fill: Color32::TRANSPARENT,
            edge: Color32::TRANSPARENT,
            ink,
            lip: None,
        },
        Style::Outline => Look {
            fill: Color32::TRANSPARENT,
            edge,
            ink,
            lip: None,
        },
        Style::Raised => Look {
            fill: Color32::from_rgb(44, 42, 38),
            edge,
            ink,
            lip: Some(Color32::from_rgb(14, 13, 12)),
        },
        Style::Brass => Look {
            fill: Color32::from_rgb(72, 62, 40),
            edge: match tone {
                Tone::Plain => PANEL_EDGE,
                _ => INK_ACTIVE,
            },
            ink: match tone {
                Tone::Plain => INK_ACTIVE,
                Tone::Hot => INK_HOT,
                Tone::Active => INK_HOT,
            },
            lip: Some(Color32::from_rgb(30, 25, 16)),
        },
        Style::Inverse => Look {
            fill: match tone {
                Tone::Plain => Color32::from_rgb(200, 192, 172),
                Tone::Hot => INK_HOT,
                Tone::Active => INK_ACTIVE,
            },
            edge: Color32::TRANSPARENT,
            ink: Color32::from_rgb(20, 18, 14),
            lip: None,
        },
        Style::Ghost => Look {
            fill: Color32::from_rgba_premultiplied(255, 255, 255, 10),
            edge: Color32::from_rgba_premultiplied(255, 255, 255, 40),
            ink: match tone {
                Tone::Plain => INK_DIM,
                Tone::Hot => INK,
                Tone::Active => INK_ACTIVE,
            },
            lip: None,
        },
    }
}

fn glyph_font(label_size: f32) -> FontId {
    FontId::proportional((label_size - 3.0).max(10.0))
}

fn shown(style: Style, key: &str) -> String {
    match style {
        Style::Paren => format!("({key})"),
        _ => key.to_owned(),
    }
}

pub fn size(ctx: &egui::Context, key: &str, label_size: f32, style: Style) -> Vec2 {
    let text = shown(style, key);
    let font = glyph_font(label_size);
    let t = ctx.fonts(|f| f.layout_no_wrap(text, font, Color32::WHITE).size());
    if style == Style::Paren {
        return t;
    }
    let h = label_size + 6.0;
    let w = (t.x + 10.0).max(h);
    Vec2::new(w.round(), h.round())
}

pub fn paint(
    painter: &Painter,
    min: Pos2,
    key: &str,
    label_size: f32,
    style: Style,
    tone: Tone,
) -> Rect {
    let ctx = painter.ctx();
    let sz = size(ctx, key, label_size, style);
    let rect = Rect::from_min_size(min, sz);
    let lk = look(style, tone);
    let text = shown(style, key);
    let font = glyph_font(label_size);
    if style != Style::Paren {
        let body = if lk.lip.is_some() {
            Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - 2.0))
        } else {
            rect
        };
        if let Some(lip) = lk.lip {
            painter.rect_filled(rect, CornerRadius::same(3), lip);
        }
        painter.rect_filled(body, CornerRadius::same(3), lk.fill);
        if lk.edge != Color32::TRANSPARENT {
            painter.rect_stroke(
                body,
                CornerRadius::same(3),
                Stroke::new(1.0_f32, lk.edge),
                egui::StrokeKind::Inside,
            );
        }
        let galley = painter.layout_no_wrap(text, font, lk.ink);
        painter.galley(
            egui::pos2(
                body.center().x - galley.size().x * 0.5,
                body.center().y - galley.size().y * 0.5,
            ),
            galley,
            lk.ink,
        );
    } else {
        let galley = painter.layout_no_wrap(text, font, lk.ink);
        painter.galley(
            egui::pos2(rect.min.x, rect.center().y - galley.size().y * 0.5),
            galley,
            lk.ink,
        );
    }
    rect
}

pub fn chord_size(ctx: &egui::Context, keys: &[&str], label_size: f32, style: Style) -> Vec2 {
    if style == Style::Paren {
        return size(ctx, &keys.join("+"), label_size, style);
    }
    let mut w = 0.0_f32;
    let mut h = 0.0_f32;
    for (i, k) in keys.iter().enumerate() {
        let s = size(ctx, k, label_size, style);
        if i > 0 {
            w += joiner_width(ctx, label_size);
        }
        w += s.x;
        h = h.max(s.y);
    }
    Vec2::new(w, h)
}

fn joiner_width(ctx: &egui::Context, label_size: f32) -> f32 {
    ctx.fonts(|f| {
        f.layout_no_wrap("+".to_owned(), glyph_font(label_size), Color32::WHITE)
            .size()
            .x
    }) + 6.0
}

pub fn paint_chord(
    painter: &Painter,
    min: Pos2,
    keys: &[&str],
    label_size: f32,
    style: Style,
    tone: Tone,
) -> Rect {
    let ctx = painter.ctx();
    if style == Style::Paren {
        return paint(painter, min, &keys.join("+"), label_size, style, tone);
    }
    let total = chord_size(ctx, keys, label_size, style);
    let mut x = min.x;
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            let galley = painter.layout_no_wrap("+".to_owned(), glyph_font(label_size), INK_DIM);
            painter.galley(
                egui::pos2(x + 3.0, min.y + total.y * 0.5 - galley.size().y * 0.5),
                galley,
                INK_DIM,
            );
            x += joiner_width(ctx, label_size);
        }
        let s = size(ctx, k, label_size, style);
        let r = paint(
            painter,
            egui::pos2(x, min.y + (total.y - s.y) * 0.5),
            k,
            label_size,
            style,
            tone,
        );
        x = r.max.x;
    }
    Rect::from_min_size(min, total)
}

const GAP: f32 = 7.0;

fn button_fill(selected: bool, hot: bool) -> (Color32, Color32, Tone) {
    if selected {
        (Color32::from_rgb(72, 62, 40), INK_ACTIVE, Tone::Active)
    } else if hot {
        (Color32::from_rgb(56, 51, 40), PANEL_EDGE, Tone::Hot)
    } else {
        (Color32::from_rgb(34, 33, 31), PANEL_EDGE_DIM, Tone::Plain)
    }
}

struct Labelled<'a> {
    text: &'a str,
    keys: &'a [&'a str],
    style: Style,
    selected: bool,
    enabled: bool,
}

fn labelled(ui: &mut egui::Ui, l: Labelled<'_>) -> Response {
    let Labelled {
        text,
        keys,
        style,
        selected,
        enabled,
    } = l;
    let label_size = 15.0;
    let min = egui::vec2(64.0, 26.0);
    let font = FontId::proportional(label_size);
    let ctx = ui.ctx().clone();
    let text_size = ctx.fonts(|f| f.layout_no_wrap(text.to_owned(), font.clone(), INK).size());
    let cap = chord_size(&ctx, keys, label_size, style);
    let pad = ui.spacing().button_padding;
    let inner = Vec2::new(text_size.x + GAP + cap.x, text_size.y.max(cap.y));
    let want = (inner + pad * 2.0).max(min);
    let (rect, r) = ui.allocate_exact_size(
        want,
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let hot = enabled && r.hovered();
        let (fill, edge, tone) = button_fill(selected, hot);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(2), fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, edge),
            egui::StrokeKind::Inside,
        );
        let ink = match (enabled, tone) {
            (false, _) => INK_DIM,
            (_, Tone::Plain) => INK,
            (_, Tone::Hot) => INK_HOT,
            (_, Tone::Active) => INK_ACTIVE,
        };
        let left = rect.center().x - inner.x * 0.5;
        let galley = painter.layout_no_wrap(text.to_owned(), font, ink);
        painter.galley(
            egui::pos2(left, rect.center().y - galley.size().y * 0.5),
            galley,
            ink,
        );
        paint_chord(
            painter,
            egui::pos2(left + text_size.x + GAP, rect.center().y - cap.y * 0.5),
            keys,
            label_size,
            style,
            if enabled { tone } else { Tone::Plain },
        );
    }
    r
}

pub fn tab(ui: &mut egui::Ui, selected: bool, text: &str, key: &str, style: Style) -> bool {
    labelled(
        ui,
        Labelled {
            text,
            keys: &[key],
            style,
            selected,
            enabled: true,
        },
    )
    .clicked()
}

pub fn boxed_button(
    ui: &mut egui::Ui,
    text: &str,
    keys: &[&str],
    enabled: bool,
    hint: &str,
    style: Style,
) -> bool {
    labelled(
        ui,
        Labelled {
            text,
            keys,
            style,
            selected: false,
            enabled,
        },
    )
    .on_hover_text(hint)
    .clicked()
}

pub fn primary_action(
    ui: &mut egui::Ui,
    text: &str,
    key: Option<&str>,
    sprite: Option<&egui::TextureHandle>,
    sprite_size: f32,
    style: Style,
) -> Response {
    let width = ui.available_width();
    let height = 44.0;
    let label_size = 19.0;
    let (rect, r) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let hot = r.hovered();
        let fill = if hot {
            Color32::from_rgb(92, 79, 50)
        } else {
            Color32::from_rgb(72, 62, 40)
        };
        let ctx = ui.ctx().clone();
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, INK_ACTIVE),
            egui::StrokeKind::Inside,
        );
        let ink = if hot { INK_HOT } else { INK_ACTIVE };
        let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(label_size), ink);
        let cap = key.map(|k| size(&ctx, k, label_size, style));
        let cap_extra = cap.map(|c| GAP + c.x).unwrap_or(0.0);
        let sprite_extra = sprite.map(|_| 10.0 + sprite_size).unwrap_or(0.0);
        let left = rect.center().x - (galley.size().x + cap_extra + sprite_extra) * 0.5;
        let mut x = left + galley.size().x;
        painter.galley(
            egui::pos2(left, rect.center().y - galley.size().y * 0.5),
            galley,
            ink,
        );
        if let (Some(k), Some(c)) = (key, cap) {
            let placed = paint(
                painter,
                egui::pos2(x + GAP, rect.center().y - c.y * 0.5),
                k,
                label_size,
                style,
                if hot { Tone::Hot } else { Tone::Active },
            );
            x = placed.max.x;
        }
        if let Some(tex) = sprite {
            let image = egui::Rect::from_min_size(
                egui::pos2(x + 10.0, rect.center().y - sprite_size * 0.5),
                egui::vec2(sprite_size, sprite_size),
            );
            painter.image(
                tex.id(),
                image,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    r
}

pub fn help_row(ui: &mut egui::Ui, keys: &[&str], what: &str, style: Style) {
    let label_size = 15.0;
    let ctx = ui.ctx().clone();
    let cap = chord_size(&ctx, keys, label_size, style);
    let col = 150.0_f32;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(col.max(cap.x), cap.y.max(22.0)),
            egui::Sense::hover(),
        );
        paint_chord(
            ui.painter(),
            egui::pos2(rect.min.x, rect.center().y - cap.y * 0.5),
            keys,
            label_size,
            style,
            Tone::Plain,
        );
        ui.label(what);
    });
}
