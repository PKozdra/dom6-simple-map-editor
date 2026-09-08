use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dom6_mapgen::cave::Gate;
use dom6_mapgen::generate::{
    generate_new_game, generate_new_game_per_player, generate_with_terrain, Generated,
};
use dom6_mapgen::layouts::{
    cave_layout_blueprint_variant, cave_layout_variant_for_seed, layout_blue_acc,
    layout_blueprint_variant, layout_variant_for_seed, CaveLayout, LAYOUT_SIZE,
};
use dom6_mapgen::{Blueprint, Control, Layout, Options as GenOptions, Sink, Stage};

use crate::blueprint_editor::{BlueprintEditor, Outcome, Subject};
use crate::keycap;
use crate::theme;

pub const PROVINCES: RangeInclusive<i32> = 10..=1980;
pub const AXIS: RangeInclusive<i32> = 500..=7500;
pub const PERCENT: RangeInclusive<i32> = 0..=100;
pub const WIDE: RangeInclusive<i32> = 0..=1000;
pub const PLAYERS: RangeInclusive<i32> = 2..=95;
pub const PER_PLAYER: [(i32, &str); 3] = [(10, "Small"), (15, "Medium"), (20, "Large")];
pub const PER_PLAYER_RANGE: RangeInclusive<i32> = 1..=100;

pub const LAYOUT_THUMBS: [&[u8]; 8] = [
    include_bytes!("../assets/layouts/standard.png"),
    include_bytes!("../assets/layouts/circle_world.png"),
    include_bytes!("../assets/layouts/small_lakes.png"),
    include_bytes!("../assets/layouts/one_sea.png"),
    include_bytes!("../assets/layouts/two_seas.png"),
    include_bytes!("../assets/layouts/twirling_sea.png"),
    include_bytes!("../assets/layouts/no_mans_land.png"),
    include_bytes!("../assets/layouts/forbidden_center.png"),
];

pub const LAYOUT_HINTS: [&str; 8] = [
    "No blueprint: the shape of the world is entirely up to the seed",
    "A ring of land around an inner sea, with open ocean outside it",
    "Land almost everywhere, broken by a handful of small lakes",
    "One large sea in the middle of a single land mass",
    "Two large seas with land between and around them",
    "A spiral arm of sea winding out from the centre",
    "Land on the west and east edges only; nobody starts in the middle",
    "A reserved middle nobody starts in, ringed by startable land",
];

pub const CAVE_LAYOUT_THUMBS: [&[u8]; 5] = [
    include_bytes!("../assets/layouts/cave_random.png"),
    include_bytes!("../assets/layouts/cave_small_caves.png"),
    include_bytes!("../assets/layouts/cave_one_cave.png"),
    include_bytes!("../assets/layouts/cave_two_caves.png"),
    include_bytes!("../assets/layouts/cave_circle_cave.png"),
];

pub const CAVE_LAYOUT_HINTS: [&str; 5] = [
    "No blueprint: one or more caves of random size, wherever the seed puts them",
    "Four or more small caves scattered through the rock",
    "One large cave with branching arms",
    "Two separate caves winding through the rock",
    "A ring of cave floor around a solid core",
];

pub fn layout_index(kind: Layout) -> usize {
    Layout::ALL.iter().position(|k| *k == kind).unwrap_or(0)
}

pub fn cave_layout_index(kind: CaveLayout) -> usize {
    CaveLayout::ALL.iter().position(|k| *k == kind).unwrap_or(0)
}

pub fn random_seed() -> u32 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0) as u64;
    let mut x = t ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x as u32) & 0x7fff_ffff
}

pub fn default_name(seed: u32) -> String {
    format!("random_{seed}")
}

pub fn name_is_usable(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnImage {
    pub label: String,
    pub image: Blueprint,
}

pub const CUSTOM_PER_PLAYER: i32 = 12;

pub fn per_player_is_preset(v: i32) -> bool {
    PER_PLAYER.iter().any(|(preset, _)| *preset == v)
}

pub struct Form {
    pub opts: GenOptions,
    pub auto_size: bool,
    pub width: i32,
    pub height: i32,
    pub seed: u32,
    pub name: String,
    pub layout: Layout,
    pub own: Option<OwnImage>,
    pub cave_layout: CaveLayout,
    pub cave_own: Option<OwnImage>,
    pub new_game: bool,
    pub players: i32,
    pub per_player_bucket: i32,
    pub custom_count: bool,
    pub manual_seed: bool,
}

impl Default for Form {
    fn default() -> Form {
        let seed = random_seed();
        Form {
            auto_size: true,
            width: 2048,
            height: 1536,
            seed,
            name: default_name(seed),
            layout: Layout::Standard,
            own: None,
            cave_layout: CaveLayout::Random,
            cave_own: None,
            new_game: false,
            players: 4,
            per_player_bucket: 15,
            custom_count: false,
            manual_seed: false,
            opts: GenOptions {
                caves_plane: true,
                ..GenOptions::default()
            },
        }
    }
}

fn out_of_range(errors: &mut Vec<String>, label: &str, v: i32, r: &RangeInclusive<i32>) {
    if !r.contains(&v) {
        errors.push(format!(
            "{label} must be between {} and {}",
            r.start(),
            r.end()
        ));
    }
}

impl Form {
    pub fn percent_fields(&self) -> [(&'static str, i32); 13] {
        let o = &self.opts;
        [
            ("Sea", o.sea_part),
            ("Mountains", o.mount_part),
            ("Forest", o.forest_part),
            ("Farmland", o.farm_part),
            ("Swamp", o.swamp_part),
            ("Waste", o.waste_part),
            ("Highland", o.highland_part),
            ("Kelp", o.kelp_part),
            ("Gorge", o.gorge_part),
            ("Ruggedness", o.rugedness),
            ("Bridges", o.bridges),
            ("Blueprint accuracy", o.blue_acc),
            ("Cave part", o.cave_part),
        ]
    }

    pub fn wide_fields(&self) -> [(&'static str, i32); 4] {
        let o = &self.opts;
        [
            ("Rivers", o.river_part),
            ("Hills", o.hills),
            ("Sea size", o.sea_size),
            ("Extra islands", o.extra_islands),
        ]
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.new_game {
            out_of_range(&mut errors, "Players", self.players, &PLAYERS);
            out_of_range(
                &mut errors,
                "Provinces per player",
                self.per_player_bucket,
                &PER_PLAYER_RANGE,
            );
        } else {
            out_of_range(&mut errors, "Provinces", self.opts.provinces, &PROVINCES);
        }
        if !self.auto_size {
            out_of_range(&mut errors, "Width", self.width, &AXIS);
            out_of_range(&mut errors, "Height", self.height, &AXIS);
        }
        for (label, v) in self.percent_fields() {
            out_of_range(&mut errors, label, v, &PERCENT);
        }
        for (label, v) in self.wide_fields() {
            out_of_range(&mut errors, label, v, &WIDE);
        }
        if !name_is_usable(self.name.trim()) {
            errors.push("Name may only hold letters, digits, - and _".to_string());
        }
        errors
    }

    pub fn blueprint(&self) -> Option<Blueprint> {
        match &self.own {
            Some(o) => Some(o.image.clone()),
            None => layout_blueprint_variant(
                self.layout,
                layout_variant_for_seed(self.layout, self.seed),
                LAYOUT_SIZE,
                LAYOUT_SIZE,
            ),
        }
    }

    pub fn cave_blueprint(&self) -> Result<Option<Blueprint>, String> {
        Ok(match &self.cave_own {
            Some(o) => Some(o.image.clone()),
            None => cave_layout_blueprint_variant(
                self.cave_layout,
                cave_layout_variant_for_seed(self.cave_layout, self.seed),
                LAYOUT_SIZE,
                LAYOUT_SIZE,
            ),
        })
    }

    pub fn options(&self, blueprint: Option<Blueprint>, cave: Option<Blueprint>) -> GenOptions {
        let mut o = self.opts.clone();
        if self.auto_size {
            o.width = -1;
            o.height = -1;
        } else {
            o.width = self.width;
            o.height = self.height;
        }
        o.blueprint = blueprint;
        o.cave_blueprint = cave;
        o
    }
}

pub fn load_blueprint(path: &Path) -> Result<Blueprint, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let img = if ext == "tga" {
        let mut i = crate::tga::decode(&bytes)?;
        i.flip_rows();
        i
    } else {
        crate::textures::decode_png(&bytes)?
    };
    if img.w == 0 || img.h == 0 {
        return Err("blueprint image is empty".to_string());
    }
    let mut bgra = Vec::with_capacity(img.w * img.h * 4);
    for px in img.rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    Ok(Blueprint {
        w: img.w as i32,
        h: img.h as i32,
        bgra,
    })
}

pub struct GeneratedPlane {
    pub d6m: Vec<u8>,
    pub map_text: String,
    pub width: i32,
    pub height: i32,
    pub provinces: usize,
}

pub struct GeneratedMap {
    pub name: String,
    pub seed: u32,
    pub planes: Vec<GeneratedPlane>,
    pub gates: Vec<Gate>,
    pub elapsed: Duration,
}

enum Msg {
    Progress(Stage, u32, u32),
    Done(Box<Generated>),
    Cancelled,
}

struct ChannelSink {
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
}

impl ChannelSink {
    fn control(&self) -> Control {
        if self.cancel.load(Ordering::Relaxed) {
            Control::Cancel
        } else {
            Control::Continue
        }
    }
}

impl Sink for ChannelSink {
    fn wants_hash(&self) -> bool {
        false
    }

    fn stage(&mut self, stage: Stage, _call: u32, _hash: u64) -> Control {
        if self.tx.send(Msg::Progress(stage, 0, 0)).is_err() {
            return Control::Cancel;
        }
        self.control()
    }

    fn progress(&mut self, stage: Stage, done: u32, total: u32) -> Control {
        if self.tx.send(Msg::Progress(stage, done, total)).is_err() {
            return Control::Cancel;
        }
        self.control()
    }
}

struct Run {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Msg>,
    stage: Stage,
    done: u32,
    total: u32,
    started: Instant,
    name: String,
    seed: u32,
}

impl Run {
    fn fraction(&self) -> f32 {
        let index = Stage::ALL
            .iter()
            .position(|s| *s == self.stage)
            .unwrap_or(0) as f32;
        let within = if self.total > 0 {
            (self.done as f32 / self.total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ((index + within) / Stage::ALL.len() as f32).clamp(0.0, 1.0)
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct GeneratorPanel {
    form: Form,
    errors: Vec<String>,
    note: Option<String>,
    run: Option<Run>,
    thumbs: [Option<egui::TextureHandle>; 8],
    cave_thumbs: [Option<egui::TextureHandle>; 5],
    own_tex: Option<egui::TextureHandle>,
    cave_own_tex: Option<egui::TextureHandle>,
    cave_editor: Option<BlueprintEditor>,
    cave_guide: Option<Blueprint>,
    sprite: Option<egui::TextureHandle>,
    editor: Option<BlueprintEditor>,
}

const ACTION_ICON: &[u8] = include_bytes!("../assets/icons/156.png");
const ACTION_SPRITE_SIZE: f32 = 26.0;
const VALUE_BOX_W: f32 = 52.0;
const ROW_LABEL_W: f32 = 80.0;

fn action_sprite<'a>(
    ctx: &egui::Context,
    slot: &'a mut Option<egui::TextureHandle>,
) -> Option<&'a egui::TextureHandle> {
    if slot.is_none() {
        let img = crate::textures::decode_png(ACTION_ICON).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
        *slot = Some(ctx.load_texture("generate_sprite", image, egui::TextureOptions::LINEAR));
    }
    slot.as_ref()
}

impl GeneratorPanel {
    pub fn map_name(&self) -> String {
        if name_is_usable(&self.form.name) {
            self.form.name.clone()
        } else {
            default_name(self.form.seed)
        }
    }

    pub fn wrap_axes(&self) -> (bool, bool) {
        (self.form.opts.hwrap, self.form.opts.vwrap)
    }

    pub fn reset_form(&mut self) {
        let seed = self.form.seed;
        let name = self.form.name.clone();
        self.form = Form {
            seed,
            name,
            ..Form::default()
        };
        self.own_tex = None;
        self.cave_own_tex = None;
    }

    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    pub fn begin(&mut self) {
        if !self.form.manual_seed {
            let s = random_seed();
            set_seed(&mut self.form, s);
        }
        self.start();
    }

    fn start(&mut self) {
        let blueprint = self.form.blueprint();
        let cave_blueprint = match self.form.cave_blueprint() {
            Ok(b) => b,
            Err(e) => {
                self.errors = vec![e];
                return;
            }
        };
        let errors = self.form.validate();
        if !errors.is_empty() {
            self.errors = errors;
            return;
        }
        self.errors.clear();
        self.note = None;
        let opts = self.form.options(blueprint, cave_blueprint);
        let new_game = self.form.new_game;
        let players = self.form.players;
        let per_player_bucket = self.form.per_player_bucket;
        let custom_count = self.form.custom_count;
        let seed = self.form.seed;
        let name = self.form.name.trim().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let worker_cancel = Arc::clone(&cancel);
        let worker_name = name.clone();
        std::thread::spawn(move || {
            let mut sink = ChannelSink {
                tx: tx.clone(),
                cancel: worker_cancel,
            };
            let made = if new_game {
                if !custom_count && per_player_is_preset(per_player_bucket) {
                    generate_new_game(
                        &opts,
                        seed,
                        players,
                        per_player_bucket,
                        &worker_name,
                        &mut sink,
                    )
                } else {
                    generate_new_game_per_player(
                        &opts,
                        seed,
                        players,
                        per_player_bucket,
                        &worker_name,
                        &mut sink,
                    )
                }
            } else {
                generate_with_terrain(&opts, seed, &worker_name, &mut sink)
            };
            let msg = match made {
                Ok(g) => Msg::Done(Box::new(g)),
                Err(_) => Msg::Cancelled,
            };
            let _ = tx.send(msg);
        });
        self.run = Some(Run {
            cancel,
            rx,
            stage: Stage::NoiseTable,
            done: 0,
            total: 0,
            started: Instant::now(),
            name,
            seed,
        });
    }

    pub fn poll(&mut self) -> Option<GeneratedMap> {
        let run = self.run.as_mut()?;
        loop {
            match run.rx.try_recv() {
                Ok(Msg::Progress(stage, done, total)) => {
                    run.stage = stage;
                    run.done = done;
                    run.total = total;
                }
                Ok(Msg::Done(g)) => {
                    let elapsed = run.started.elapsed();
                    let name = run.name.clone();
                    let seed = run.seed;
                    self.run = None;
                    let g = *g;
                    let planes = g
                        .planes
                        .into_iter()
                        .map(|p| GeneratedPlane {
                            d6m: p.d6m,
                            map_text: p.map_text,
                            width: p.width,
                            height: p.height,
                            provinces: p.provinces.len().saturating_sub(1),
                        })
                        .collect();
                    return Some(GeneratedMap {
                        name,
                        seed,
                        planes,
                        gates: g.gates,
                        elapsed,
                    });
                }
                Ok(Msg::Cancelled) => {
                    self.run = None;
                    self.note = Some("Generation cancelled".to_string());
                    return None;
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.run = None;
                    self.note = Some("Generation stopped".to_string());
                    return None;
                }
            }
        }
    }

    pub fn action(&mut self, ui: &mut egui::Ui, width: f32) -> bool {
        if let Some(run) = &self.run {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            let mut cancel = false;
            theme::panel_frame().show(ui, |ui| {
                ui.set_width(width);
                theme::section_first(ui, "Generating");
                ui.label(run.stage.label());
                ui.add(egui::ProgressBar::new(run.fraction()).desired_height(14.0));
                theme::dim(ui, &format!("{:.1} s", run.started.elapsed().as_secs_f32()));
                ui.add_space(4.0);
                if theme::boxed_button(ui, "Cancel", true) {
                    cancel = true;
                }
            });
            if cancel {
                self.run = None;
                self.note = Some("Generation cancelled".to_string());
            }
            return false;
        }
        let sprite = action_sprite(ui.ctx(), &mut self.sprite).cloned();
        keycap::primary_action(
            ui,
            "Generate",
            Some("G"),
            sprite.as_ref(),
            ACTION_SPRITE_SIZE,
            keycap::DEFAULT,
        )
        .on_hover_text("Rolls a whole new map into a new unsaved document")
        .clicked()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, width: f32, blueprints: &Path, doc_name: Option<&str>) {
        let map_name = match doc_name {
            Some(n) if name_is_usable(n) => n.to_owned(),
            _ => self.map_name(),
        };
        let drawn = match &mut self.editor {
            Some(ed) => Some(ed.show(
                ui.ctx(),
                Subject {
                    layout: self.form.layout,
                    seed: self.form.seed,
                    current: self.form.own.as_ref().map(|o| &o.image),
                    blueprints,
                    name: &map_name,
                    caves: false,
                    guide: None,
                },
            )),
            None => None,
        };
        match drawn {
            Some(Outcome::Applied(image)) => {
                self.form.own = Some(OwnImage {
                    label: "drawing".to_string(),
                    image,
                });
                self.own_tex = None;
                self.editor = None;
            }
            Some(Outcome::Cancelled) => self.editor = None,
            Some(Outcome::Open) | None => {}
        }
        let cave_name = format!("{map_name}_caves");
        let drawn = match &mut self.cave_editor {
            Some(ed) => Some(ed.show(
                ui.ctx(),
                Subject {
                    layout: Layout::Standard,
                    seed: self.form.seed,
                    current: self.form.cave_own.as_ref().map(|o| &o.image),
                    blueprints,
                    name: &cave_name,
                    caves: true,
                    guide: self.cave_guide.as_ref(),
                },
            )),
            None => None,
        };
        match drawn {
            Some(Outcome::Applied(image)) => {
                self.form.cave_own = Some(OwnImage {
                    label: "drawing".to_string(),
                    image,
                });
                self.cave_own_tex = None;
                self.cave_editor = None;
            }
            Some(Outcome::Cancelled) => self.cave_editor = None,
            Some(Outcome::Open) | None => {}
        }
        let mut pick_error: Option<String> = None;
        let f = &mut self.form;
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Seed");
            theme::check(ui, &mut f.manual_seed, "Set seed manually").on_hover_text(
                "Off: every Generate rolls a fresh seed and the status line names it",
            );
            if f.manual_seed {
                ui.horizontal(|ui| {
                    ui.add_sized([80.0, 20.0], egui::Label::new("Seed"));
                    let mut seed = f.seed as i64;
                    if ui
                        .add(
                            egui::DragValue::new(&mut seed)
                                .range(0..=2_147_483_647)
                                .speed(1.0),
                        )
                        .changed()
                    {
                        set_seed(f, seed as u32);
                    }
                    if theme::boxed_button_hint(ui, "Roll", true, "Picks a fresh seed") {
                        let s = random_seed();
                        set_seed(f, s);
                    }
                });
            }
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "New-game mode");
            theme::check(ui, &mut f.new_game, "Roll it the way a new game does")
                .on_hover_text("Rolls the map the way starting a new game does: the province count comes from the players and the size below, and the caves plane comes with it");
            if f.new_game {
                row(ui, "Players", |ui| {
                    ui.add(egui::DragValue::new(&mut f.players).range(PLAYERS).speed(0.2));
                });
                ui.horizontal(|ui| {
                    for (v, label) in PER_PLAYER {
                        let on = !f.custom_count && f.per_player_bucket == v;
                        let r = ui.scope(|ui| theme::tab(ui, on, label));
                        r.response
                            .on_hover_text(format!("{v} provinces per player"));
                        if r.inner {
                            f.per_player_bucket = v;
                            f.custom_count = false;
                        }
                    }
                });
                if theme::check(ui, &mut f.custom_count, "Custom count")
                    .on_hover_text("Any number of provinces per player, typed or dragged; the presets are 10, 15 and 20")
                    .clicked()
                    && f.custom_count
                    && per_player_is_preset(f.per_player_bucket)
                {
                    f.per_player_bucket = CUSTOM_PER_PLAYER;
                }
                if !f.custom_count {
                    theme::dim(ui, &format!("Provinces per player: {}", f.per_player_bucket));
                } else {
                    ui.horizontal(|ui| {
                        ui.add_sized([140.0, 20.0], egui::Label::new("Provinces per player"));
                        ui.add(
                            egui::DragValue::new(&mut f.per_player_bucket)
                                .range(PER_PLAYER_RANGE)
                                .speed(0.2),
                        );
                    });
                }
            }
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Size and provinces");
            let free = !f.new_game;
            ui.horizontal(|ui| {
                ui.add_sized([80.0, 20.0], egui::Label::new("Provinces"));
                ui.add_enabled(
                    free,
                    egui::DragValue::new(&mut f.opts.provinces)
                        .range(PROVINCES)
                        .speed(1.0),
                );
            });
            if !free {
                theme::dim(ui, "The new-game path picks the province count");
            }
            theme::check(ui, &mut f.auto_size, "Size fits the province count")
                .on_hover_text("Wrapped axes round up to a multiple of 512; unwrapped axes gain a 192 pixel margin on each side");
            if !f.auto_size {
                row(ui, "Width", |ui| {
                    ui.add(egui::DragValue::new(&mut f.width).range(AXIS).speed(8.0));
                });
                row(ui, "Height", |ui| {
                    ui.add(egui::DragValue::new(&mut f.height).range(AXIS).speed(8.0));
                });
            }
        });
        ui.add_space(8.0);
        let thumbs = &mut self.thumbs;
        let own_tex = &mut self.own_tex;
        let mut open_editor = false;
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Blueprint");
            let mut chosen = layout_index(f.layout);
            let picked = f.own.is_some();
            egui::ComboBox::from_id_salt("layout")
                .selected_text(f.layout.label())
                .width(160.0)
                .truncate()
                .show_ui(ui, |ui| {
                    for (i, kind) in Layout::ALL.iter().enumerate() {
                        if ui
                            .selectable_value(&mut chosen, i, kind.label())
                            .on_hover_text(LAYOUT_HINTS[i])
                            .changed()
                        {
                            f.layout = *kind;
                            f.opts.blue_acc = layout_blue_acc(*kind);
                        }
                    }
                });
            let index = layout_index(f.layout);
            theme::dim(ui, LAYOUT_HINTS[index]);
            if let Some(tex) = layout_thumb(ui.ctx(), thumbs, &LAYOUT_THUMBS, "layout", index) {
                show_thumb(ui, &tex, width);
            }
            ui.add_space(4.0);
            let label = match &f.own {
                Some(o) => o.label.clone(),
                None => "none".to_string(),
            };
            theme::dim(ui, &format!("Own image: {label}")).on_hover_text(
                "An image whose colours say where land and sea go; it replaces the layout above",
            );
            if let Some(o) = &f.own {
                if let Some(tex) = own_preview(ui.ctx(), own_tex, o, "own_blueprint") {
                    let side = (width - 24.0).clamp(64.0, 240.0);
                    let size = tex.size_vec2();
                    let scale = (side / size.x).min(side / size.y);
                    let src = egui::load::SizedTexture::from_handle(&tex);
                    ui.add(egui::Image::new(src).fit_to_exact_size(size * scale));
                }
            }
            ui.horizontal(|ui| {
                if theme::boxed_button_hint(ui, "Draw", true, "Paints a blueprint by hand") {
                    open_editor = true;
                }
                if theme::boxed_button(ui, "Pick", true) {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Blueprint image", &["png", "tga"])
                        .set_directory(blueprints)
                        .pick_file()
                    {
                        match load_blueprint(&p) {
                            Ok(image) => {
                                f.own = Some(OwnImage {
                                    label: file_label(&p),
                                    image,
                                });
                                *own_tex = None;
                            }
                            Err(e) => pick_error = Some(e),
                        }
                    }
                }
                if theme::boxed_button(ui, "Clear", picked) {
                    f.own = None;
                    *own_tex = None;
                }
            });
            percent_row(ui, "Accuracy", &mut f.opts.blue_acc);
        });
        if open_editor && self.editor.is_none() {
            self.editor = Some(BlueprintEditor::new(
                self.form.own.as_ref().map(|o| &o.image),
            ));
        }
        let f = &mut self.form;
        let cave_own_tex = &mut self.cave_own_tex;
        let cave_thumbs = &mut self.cave_thumbs;
        let mut open_cave_editor = false;
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Sea and mountains");
            percent_row(ui, "Sea", &mut f.opts.sea_part);
            wide_row(ui, "Sea size", &mut f.opts.sea_size);
            percent_row(ui, "Mountains", &mut f.opts.mount_part);
            percent_row(ui, "Gorge", &mut f.opts.gorge_part);
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Land types");
            percent_row(ui, "Forest", &mut f.opts.forest_part);
            percent_row(ui, "Farmland", &mut f.opts.farm_part);
            percent_row(ui, "Swamp", &mut f.opts.swamp_part);
            percent_row(ui, "Waste", &mut f.opts.waste_part);
            percent_row(ui, "Highland", &mut f.opts.highland_part);
            percent_row(ui, "Kelp", &mut f.opts.kelp_part);
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Rivers, hills, ruggedness");
            wide_row(ui, "Rivers", &mut f.opts.river_part);
            wide_row(ui, "Hills", &mut f.opts.hills);
            percent_row(ui, "Ruggedness", &mut f.opts.rugedness);
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Islands and bridges");
            wide_row(ui, "Extra islands", &mut f.opts.extra_islands);
            percent_row(ui, "Bridges", &mut f.opts.bridges);
            theme::check(ui, &mut f.opts.no_water_prov, "No water provinces");
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Wraparound");
            theme::check(ui, &mut f.opts.hwrap, "Wrap left to right");
            theme::check(ui, &mut f.opts.vwrap, "Wrap top to bottom");
            theme::check(ui, &mut f.opts.cave_world, "Cave world");
        });
        ui.add_space(8.0);
        theme::panel_frame().show(ui, |ui| {
            ui.set_width(width);
            theme::section_first(ui, "Caves plane");
            theme::check(ui, &mut f.opts.caves_plane, "The Realm Beneath").on_hover_text(
                "A second plane under the surface, connected by gateways, as a new game makes it",
            );
            if f.opts.caves_plane {
                percent_row(ui, "Cave part", &mut f.opts.cave_part);
                let mut chosen = cave_layout_index(f.cave_layout);
                egui::ComboBox::from_id_salt("cave_layout")
                    .selected_text(f.cave_layout.label())
                    .width(160.0)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for (i, kind) in CaveLayout::ALL.iter().enumerate() {
                            if ui
                                .selectable_value(&mut chosen, i, kind.label())
                                .on_hover_text(CAVE_LAYOUT_HINTS[i])
                                .changed()
                            {
                                f.cave_layout = *kind;
                            }
                        }
                    });
                let index = cave_layout_index(f.cave_layout);
                theme::dim(ui, CAVE_LAYOUT_HINTS[index]);
                if let Some(tex) =
                    layout_thumb(ui.ctx(), cave_thumbs, &CAVE_LAYOUT_THUMBS, "cave_layout", index)
                {
                    show_thumb(ui, &tex, width);
                }
                ui.add_space(4.0);
                let label = match &f.cave_own {
                    Some(o) => o.label.clone(),
                    None => "none".to_string(),
                };
                theme::dim(ui, &format!("Own image: {label}")).on_hover_text(
                    "An image whose colours say where the cave floor and its rock walls go: land is floor, sea is rock",
                );
                if let Some(o) = &f.cave_own {
                    if let Some(tex) = own_preview(ui.ctx(), cave_own_tex, o, "cave_blueprint") {
                        let side = (width - 24.0).clamp(64.0, 240.0);
                        let size = tex.size_vec2();
                        let scale = (side / size.x).min(side / size.y);
                        let src = egui::load::SizedTexture::from_handle(&tex);
                        ui.add(egui::Image::new(src).fit_to_exact_size(size * scale));
                    }
                }
                ui.horizontal(|ui| {
                    if theme::boxed_button_hint(ui, "Draw", true, "Paints the cave floor by hand, with the surface shown underneath") {
                        open_cave_editor = true;
                    }
                    if theme::boxed_button(ui, "Pick", true) {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Blueprint image", &["png", "tga"])
                            .set_directory(blueprints)
                            .pick_file()
                        {
                            match load_blueprint(&p) {
                                Ok(image) => {
                                    f.cave_own = Some(OwnImage {
                                        label: file_label(&p),
                                        image,
                                    });
                                    *cave_own_tex = None;
                                }
                                Err(e) => pick_error = Some(e),
                            }
                        }
                    }
                    if theme::boxed_button(ui, "Clear", f.cave_own.is_some()) {
                        f.cave_own = None;
                        *cave_own_tex = None;
                    }
                });
            }
        });
        if open_cave_editor && self.cave_editor.is_none() {
            self.cave_guide = self.form.blueprint();
            self.cave_editor = Some(BlueprintEditor::new(
                self.form.cave_own.as_ref().map(|o| &o.image),
            ));
        }
        if let Some(e) = pick_error {
            self.note = Some(e);
        }
        if !self.errors.is_empty() || self.note.is_some() {
            ui.add_space(8.0);
            theme::panel_frame().show(ui, |ui| {
                ui.set_width(width);
                for e in &self.errors {
                    ui.label(egui::RichText::new(e).color(theme::WARN));
                }
                if let Some(n) = &self.note {
                    theme::dim(ui, n);
                }
            });
        }
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn preview_image(image: &Blueprint, max_side: usize) -> (usize, usize, Vec<u8>) {
    let sw = image.w.max(1) as usize;
    let sh = image.h.max(1) as usize;
    let step = (sw.max(sh)).div_ceil(max_side.max(1)).max(1);
    let w = sw.div_ceil(step);
    let h = sh.div_ceil(step);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = (y * step).min(sh - 1);
        for x in 0..w {
            let sx = (x * step).min(sw - 1);
            let p = (sy * sw + sx) * 4;
            if p + 3 < image.bgra.len() {
                rgba.extend_from_slice(&[
                    image.bgra[p + 2],
                    image.bgra[p + 1],
                    image.bgra[p],
                    image.bgra[p + 3],
                ]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    (w, h, rgba)
}

fn own_preview(
    ctx: &egui::Context,
    slot: &mut Option<egui::TextureHandle>,
    own: &OwnImage,
    name: &str,
) -> Option<egui::TextureHandle> {
    if slot.is_none() {
        let (w, h, rgba) = preview_image(&own.image, 256);
        if w == 0 || h == 0 {
            return None;
        }
        let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        *slot = Some(ctx.load_texture(name, ci, egui::TextureOptions::NEAREST));
    }
    slot.clone()
}

fn layout_thumb(
    ctx: &egui::Context,
    thumbs: &mut [Option<egui::TextureHandle>],
    bytes: &[&[u8]],
    prefix: &str,
    index: usize,
) -> Option<egui::TextureHandle> {
    if thumbs[index].is_none() {
        let img = crate::textures::decode_png(bytes[index]).ok()?;
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &img.rgba);
        thumbs[index] = Some(ctx.load_texture(
            format!("{prefix}_{index}"),
            ci,
            egui::TextureOptions::LINEAR,
        ));
    }
    thumbs[index].clone()
}

fn show_thumb(ui: &mut egui::Ui, tex: &egui::TextureHandle, width: f32) {
    let side = (width - 24.0).clamp(64.0, 240.0);
    let size = tex.size_vec2();
    let scale = side / size.x;
    let src = egui::load::SizedTexture::from_handle(tex);
    ui.add(egui::Image::new(src).fit_to_exact_size(size * scale));
}

fn set_seed(f: &mut Form, seed: u32) {
    let old = f.seed;
    f.seed = seed;
    if f.name == default_name(old) {
        f.name = default_name(seed);
    }
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([ROW_LABEL_W, 20.0], egui::Label::new(label));
        add(ui);
    });
}

fn slider_row(ui: &mut egui::Ui, label: &str, value: &mut i32, range: RangeInclusive<i32>) {
    ui.horizontal(|ui| {
        ui.add_sized([ROW_LABEL_W, 20.0], egui::Label::new(label));
        let gap = ui.spacing().item_spacing.x;
        let track = (ui.available_width() - VALUE_BOX_W - gap).max(40.0);
        ui.spacing_mut().slider_width = track;
        ui.spacing_mut().interact_size.x = VALUE_BOX_W;
        ui.add(egui::Slider::new(value, range));
    });
}

fn percent_row(ui: &mut egui::Ui, label: &str, value: &mut i32) {
    slider_row(ui, label, value, PERCENT);
}

fn wide_row(ui: &mut egui::Ui, label: &str, value: &mut i32) {
    slider_row(ui, label, value, WIDE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        assert!(Form::default().validate().is_empty());
    }

    #[test]
    fn province_count_is_bounded() {
        let mut f = Form::default();
        f.opts.provinces = 9;
        assert_eq!(f.validate().len(), 1);
        f.opts.provinces = 1981;
        assert_eq!(f.validate().len(), 1);
        f.opts.provinces = 1980;
        assert!(f.validate().is_empty());
        f.opts.provinces = 10;
        assert!(f.validate().is_empty());
    }

    #[test]
    fn axis_bounds_only_apply_when_size_is_not_auto() {
        let mut f = Form {
            width: 100,
            height: 9000,
            ..Form::default()
        };
        assert!(f.validate().is_empty());
        f.auto_size = false;
        assert_eq!(f.validate().len(), 2);
        f.width = 500;
        f.height = 7500;
        assert!(f.validate().is_empty());
    }

    #[test]
    fn percent_fields_reject_over_a_hundred() {
        let mut f = Form::default();
        f.opts.sea_part = 101;
        f.opts.gorge_part = -1;
        f.opts.rugedness = 100;
        let e = f.validate();
        assert_eq!(e.len(), 2);
        assert!(e.iter().any(|m| m.starts_with("Sea")));
        assert!(e.iter().any(|m| m.starts_with("Gorge")));
    }

    #[test]
    fn wide_fields_reach_a_thousand() {
        let mut f = Form::default();
        f.opts.hills = 1000;
        f.opts.sea_size = 1000;
        f.opts.river_part = 1000;
        f.opts.extra_islands = 1000;
        assert!(f.validate().is_empty());
        f.opts.hills = 1001;
        assert_eq!(f.validate().len(), 1);
    }

    #[test]
    fn name_must_be_a_plain_file_stem() {
        let mut f = Form {
            name: String::new(),
            ..Form::default()
        };
        assert_eq!(f.validate().len(), 1);
        f.name = "my map".to_string();
        assert_eq!(f.validate().len(), 1);
        f.name = "my_map-2".to_string();
        assert!(f.validate().is_empty());
    }

    #[test]
    fn auto_size_clears_the_axes_in_the_options() {
        let mut f = Form {
            width: 1024,
            height: 768,
            ..Form::default()
        };
        assert_eq!(f.options(None, None).width, -1);
        f.auto_size = false;
        let o = f.options(None, None);
        assert_eq!((o.width, o.height), (1024, 768));
    }

    #[test]
    fn a_raised_cancel_flag_stops_the_sink() {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut sink = ChannelSink {
            tx,
            cancel: Arc::clone(&cancel),
        };
        assert_eq!(sink.stage(Stage::Height, 0, 0), Control::Continue);
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(sink.stage(Stage::Height, 1, 0), Control::Cancel);
        cancel.store(false, Ordering::Relaxed);
        drop(rx);
        assert_eq!(sink.progress(Stage::Height, 1, 2), Control::Cancel);
    }

    #[test]
    fn the_worker_thread_reports_stages_and_hands_back_a_map() {
        let form = Form {
            auto_size: false,
            width: 512,
            height: 512,
            seed: 5,
            name: default_name(5),
            opts: GenOptions {
                provinces: 20,
                caves_plane: false,
                ..GenOptions::default()
            },
            layout: Layout::Standard,
            own: None,
            cave_layout: CaveLayout::Random,
            cave_own: None,
            new_game: false,
            players: 4,
            per_player_bucket: 15,
            custom_count: false,
            manual_seed: true,
        };
        let mut d = GeneratorPanel {
            form,
            ..Default::default()
        };
        d.begin();
        assert!(d.errors.is_empty());
        assert!(d.is_running());
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut seen = 0usize;
        loop {
            if let Some(g) = d.poll() {
                assert_eq!(g.name, "random_5");
                assert_eq!(g.seed, 5);
                assert_eq!(g.planes.len(), 1);
                assert!(g.planes[0].provinces >= 20);
                assert!(!g.planes[0].d6m.is_empty());
                assert!(g.planes[0].map_text.contains("#imagefile random_5.d6m"));
                assert!(!d.is_running());
                break;
            }
            if let Some(run) = &d.run {
                if run.fraction() > 0.0 {
                    seen += 1;
                }
            }
            assert!(Instant::now() < deadline, "generation did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(seen > 0);
    }

    #[test]
    fn the_standard_layout_asks_for_no_blueprint() {
        let f = Form::default();
        assert_eq!(f.layout, Layout::Standard);
        assert!(f.blueprint().is_none());
    }

    #[test]
    fn a_layout_supplies_a_blueprint_that_the_seed_varies() {
        let mut f = Form {
            layout: Layout::SmallLakes,
            seed: 0,
            ..Form::default()
        };
        let a = f.blueprint().unwrap();
        assert_eq!((a.w, a.h), (LAYOUT_SIZE, LAYOUT_SIZE));
        f.seed = 1;
        let b = f.blueprint().unwrap();
        assert_ne!(a.bgra, b.bgra);
    }

    #[test]
    fn an_own_image_replaces_the_layout_blueprint() {
        use crate::blueprint_editor::{Canvas, Paint};
        let mut f = Form {
            layout: Layout::SmallLakes,
            ..Form::default()
        };
        let from_layout = f.blueprint().unwrap();
        let mut c = Canvas::new(64, 40, Paint::Sea);
        for y in 0..40 {
            for x in 0..32 {
                c.set(x, y, Paint::Land);
            }
        }
        let image = c.to_blueprint();
        f.own = Some(OwnImage {
            label: "drawing".to_string(),
            image: image.clone(),
        });
        let own = f.blueprint().unwrap();
        assert_eq!((own.w, own.h), (64, 40));
        assert_eq!(own, image);
        assert_ne!(own.bgra, from_layout.bgra);
        assert_eq!(f.options(f.blueprint(), None).blueprint, Some(image));
        f.own = None;
        assert_eq!(f.blueprint().unwrap().bgra, from_layout.bgra);
    }

    #[test]
    fn a_drawn_sea_half_stays_sea_through_the_generator() {
        use crate::blueprint_editor::{Canvas, Paint};
        use dom6_mapgen::height::build_randboard_rgb_classification_mask;
        let mut c = Canvas::new(64, 64, Paint::Land);
        for y in 0..64 {
            for x in 0..32 {
                c.set(x, y, Paint::Sea);
            }
        }
        let f = Form {
            own: Some(OwnImage {
                label: "drawing".to_string(),
                image: c.to_blueprint(),
            }),
            ..Form::default()
        };
        let o = f.options(f.blueprint(), None);
        let bp = o.blueprint.unwrap();
        let mask = build_randboard_rgb_classification_mask(&bp.bgra, bp.w, bp.h);
        assert!((mask.sea_frac - 0.5).abs() < 1e-6);
        for y in 0..64 {
            assert_eq!(mask.cells[y * 64], -1);
            assert_eq!(mask.cells[y * 64 + 63], 1);
        }
    }

    #[test]
    fn the_preview_shrinks_a_big_image_and_swaps_to_rgba() {
        use crate::blueprint_editor::{Canvas, Paint};
        let c = Canvas::new(512, 256, Paint::Sea);
        let (w, h, rgba) = preview_image(&c.to_blueprint(), 256);
        assert_eq!((w, h), (256, 128));
        assert_eq!(rgba.len(), 256 * 128 * 4);
        assert_eq!(&rgba[0..4], &[33, 5, 144, 255]);
        let small = Canvas::new(4, 4, Paint::Land).to_blueprint();
        let (w, h, _) = preview_image(&small, 256);
        assert_eq!((w, h), (4, 4));
    }

    #[test]
    fn the_per_player_presets_are_the_size_buckets() {
        assert!(per_player_is_preset(10));
        assert!(per_player_is_preset(15));
        assert!(per_player_is_preset(20));
        assert!(!per_player_is_preset(CUSTOM_PER_PLAYER));
        for (v, label) in PER_PLAYER {
            assert!(!label.contains(char::is_numeric), "{v}");
        }
    }

    #[test]
    fn every_layout_ships_a_thumbnail() {
        for (i, bytes) in LAYOUT_THUMBS.iter().enumerate() {
            let img = crate::textures::decode_png(bytes).unwrap();
            assert!(img.w > 32 && img.h > 32, "{}", Layout::ALL[i].label());
            assert!(bytes.len() < 60 * 1024, "{}", Layout::ALL[i].label());
        }
        assert_eq!(layout_index(Layout::ForbiddenCenter), 7);
    }

    #[test]
    fn every_cave_layout_ships_a_thumbnail() {
        for (i, bytes) in CAVE_LAYOUT_THUMBS.iter().enumerate() {
            let img = crate::textures::decode_png(bytes).unwrap();
            assert!(img.w > 32 && img.h > 32, "{}", CaveLayout::ALL[i].label());
            assert!(bytes.len() < 60 * 1024, "{}", CaveLayout::ALL[i].label());
        }
        assert_eq!(cave_layout_index(CaveLayout::CircleCave), 4);
        assert_eq!(CAVE_LAYOUT_HINTS.len(), CaveLayout::ALL.len());
    }

    #[test]
    fn the_random_cave_layout_asks_for_no_blueprint() {
        let f = Form::default();
        assert_eq!(f.cave_layout, CaveLayout::Random);
        assert!(f.cave_blueprint().unwrap().is_none());
    }

    #[test]
    fn a_cave_layout_supplies_a_blueprint_that_the_seed_varies() {
        let mut f = Form {
            cave_layout: CaveLayout::SmallCaves,
            seed: 0,
            ..Form::default()
        };
        let a = f.cave_blueprint().unwrap().unwrap();
        assert_eq!((a.w, a.h), (LAYOUT_SIZE, LAYOUT_SIZE));
        f.seed = 1;
        let b = f.cave_blueprint().unwrap().unwrap();
        assert_ne!(a.bgra, b.bgra);
        let o = f.options(f.blueprint(), f.cave_blueprint().unwrap());
        assert_eq!(o.cave_blueprint, Some(b));
        assert!(o.blueprint.is_none());
    }

    #[test]
    fn an_own_cave_image_replaces_the_cave_layout() {
        use crate::blueprint_editor::{Canvas, Paint};
        let mut f = Form {
            cave_layout: CaveLayout::OneCave,
            ..Form::default()
        };
        let from_layout = f.cave_blueprint().unwrap().unwrap();
        let image = Canvas::new(48, 32, Paint::Land).to_blueprint();
        f.cave_own = Some(OwnImage {
            label: "drawing".to_string(),
            image: image.clone(),
        });
        assert_eq!(f.cave_blueprint().unwrap(), Some(image));
        f.cave_own = None;
        assert_eq!(f.cave_blueprint().unwrap().unwrap().bgra, from_layout.bgra);
    }

    #[test]
    fn new_game_mode_validates_players_and_size_instead_of_provinces() {
        let mut f = Form {
            new_game: true,
            players: 1,
            ..Form::default()
        };
        f.opts.provinces = 5;
        let e = f.validate();
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("Players"));
        f.players = 6;
        assert!(f.validate().is_empty());
        f.per_player_bucket = 0;
        assert_eq!(f.validate().len(), 1);
        f.per_player_bucket = 30;
        assert!(f.validate().is_empty());
    }

    #[test]
    fn the_caves_plane_is_on_by_default_and_its_part_is_a_percentage() {
        let mut f = Form::default();
        assert!(f.opts.caves_plane);
        f.opts.cave_part = 101;
        let e = f.validate();
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("Cave part"));
    }

    #[test]
    fn an_unset_manual_seed_makes_every_run_pick_a_new_one() {
        let mut d = GeneratorPanel::default();
        assert!(!d.form.manual_seed);
        let before = d.form.seed;
        d.form.opts.provinces = 5;
        d.begin();
        assert_ne!(d.form.seed, before);
        assert!(!d.errors.is_empty());
        assert!(!d.is_running());
        assert_eq!(d.form.name, default_name(d.form.seed));
    }

    #[test]
    fn default_name_follows_the_seed() {
        let f = Form::default();
        assert_eq!(f.name, default_name(f.seed));
    }
}
