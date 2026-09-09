use egui::Context;

pub const SIDE_W: f32 = 340.0;
const COMPACT_BELOW_W: f32 = SIDE_W * 2.0 + 260.0;
const COMPACT_BELOW_H: f32 = 480.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Wide,
    Compact,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Map,
    Tools,
    Generate,
    View,
}

impl Sheet {
    pub const ALL: [Sheet; 4] = [Sheet::Map, Sheet::Tools, Sheet::Generate, Sheet::View];

    pub fn label(self) -> &'static str {
        match self {
            Sheet::Map => "Map",
            Sheet::Tools => "Tools",
            Sheet::Generate => "Generate",
            Sheet::View => "View",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub kind: Kind,
    pub panel_w: f32,
    pub screen: egui::Rect,
}

impl Layout {
    pub fn probe(ctx: &Context) -> Layout {
        let screen = ctx.screen_rect();
        let compact = screen.width() < COMPACT_BELOW_W || screen.height() < COMPACT_BELOW_H;
        let kind = if compact { Kind::Compact } else { Kind::Wide };
        let panel_w = match kind {
            Kind::Wide => SIDE_W,
            Kind::Compact => screen.width(),
        };
        Layout {
            kind,
            panel_w,
            screen,
        }
    }

    pub fn compact(&self) -> bool {
        self.kind == Kind::Compact
    }

    pub fn inner_w(&self) -> f32 {
        self.panel_w - 22.0
    }

    pub fn section_w(&self) -> f32 {
        self.panel_w - 50.0
    }

    pub fn sheet_h(&self) -> f32 {
        let h = self.screen.height();
        if self.screen.width() > h {
            (h * 0.62).max(160.0)
        } else {
            (h * 0.48).max(200.0)
        }
    }
}

pub fn apply_style(ctx: &Context, kind: Kind) {
    ctx.all_styles_mut(|style| {
        let s = &mut style.spacing;
        match kind {
            Kind::Wide => {
                s.item_spacing = egui::vec2(8.0, 6.0);
                s.button_padding = egui::vec2(9.0, 3.0);
                s.interact_size.y = 22.0;
                s.icon_width = 14.0;
                s.icon_width_inner = 8.0;
                s.slider_width = 150.0;
            }
            Kind::Compact => {
                s.item_spacing = egui::vec2(10.0, 9.0);
                s.button_padding = egui::vec2(13.0, 7.0);
                s.interact_size.y = 34.0;
                s.icon_width = 22.0;
                s.icon_width_inner = 12.0;
                s.slider_width = 180.0;
            }
        }
    });
}
