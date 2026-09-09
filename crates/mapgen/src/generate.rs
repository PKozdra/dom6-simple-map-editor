use crate::cave::{generate_cave_gates, make_cave_world, roll_undercave_terrain, Gate, PlaneRange};
use crate::features::{apply_margin, build_graph_and_features, finish_edges};
use crate::height::build_height_field;
use crate::options::Options;
use crate::options::{newgame_per_player, NEWGAME_CAVE_MAP_H, NEWGAME_CAVE_MAP_W};
use crate::paint_writes::{apply_paint_writes, illuminate_map_edges};
use crate::provinces::place_and_grow;
use crate::rng::{CrtRng, PoolRng};
use crate::stage::{Control, Sink, Stage};
use crate::terrain::roll_terrain;
use crate::world::{Province, World};
use crate::writers::{wrap_code, write_d6m, write_map_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

#[derive(Clone, Debug)]
pub struct PlaneOut {
    pub width: i32,
    pub height: i32,
    pub wrap: u8,
    pub provinces: Vec<Province>,
    pub sea_level: f32,
    pub map_scale: f32,
    pub d6m: Vec<u8>,
    pub map_text: String,
    pub pool: PoolRng,
    pub crt: CrtRng,
}

#[derive(Clone, Debug)]
pub struct Generated {
    pub planes: Vec<PlaneOut>,
    pub gates: Vec<Gate>,
}

pub const PROVINCE_BUDGET: usize = 0x7c6;
pub const CAVE_BUDGET_MARGIN: usize = 5;
pub const NEWGAME_COUNT_SLOPE: f64 = 0.3;
pub const NEWGAME_COUNT_BASE: f64 = 0.85;
pub const NEWGAME_COUNT_PAD: i32 = 5;
pub const NEWGAME_MIN_PROV: i32 = 10;
pub const NEWGAME_MAX_PROV: i32 = 1985;
pub const NEWGAME_RUGEDNESS_SCALE: f64 = 0.4;
pub const NEWGAME_RUGEDNESS_BASE: f64 = 0.05;

impl PlaneOut {
    pub fn neighbours(&self) -> Vec<(u16, u16, i64)> {
        let mut out = Vec::new();
        for (a, p) in self.provinces.iter().enumerate().skip(1) {
            for (n, b) in p.nbors.iter().zip(&p.border) {
                if (a as u16) < *n {
                    out.push((a as u16, *n, *b));
                }
            }
        }
        out
    }
}

pub fn generate_plane(
    opts: &Options,
    seed: u32,
    name: &str,
    sink: &mut dyn Sink,
) -> Result<PlaneOut, Cancelled> {
    let mut w = World::new(seed);
    if w.emit(Stage::NoiseTable, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if build_height_field(&mut w, opts, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if place_and_grow(&mut w, opts, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if build_graph_and_features(&mut w, opts, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if opts.cave_world && make_cave_world(&mut w, opts, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if apply_paint_writes(&mut w, opts, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    let board_w = w.w;
    let board_h = w.h;
    if apply_margin(&mut w, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    let alpha = illuminate_map_edges(&mut w, board_w, board_h);
    if finish_edges(&mut w, &alpha, board_w, board_h, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    let d6m = write_d6m(&w);
    if w.emit_bytes(Stage::Recipe, &d6m, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    let map_text = write_map_text(&mut w, name);
    if w.emit_bytes(Stage::MapText, map_text.as_bytes(), sink) == Control::Cancel {
        return Err(Cancelled);
    }
    Ok(PlaneOut {
        width: w.w,
        height: w.h,
        wrap: wrap_code(&w),
        provinces: w.provinces,
        sea_level: w.sea_level,
        map_scale: w.spacing,
        d6m,
        map_text,
        pool: w.pool,
        crt: w.crt,
    })
}

pub fn caves_plane_fits(surface: &PlaneOut) -> bool {
    caves_plane_fits_with_requested_count(surface, 0)
}

pub fn caves_plane_fits_with_requested_count(surface: &PlaneOut, requested: i32) -> bool {
    surface.provinces.len() - 1 + CAVE_BUDGET_MARGIN + (requested.max(0) as usize) < PROVINCE_BUDGET
}

pub fn generate(
    opts: &Options,
    seed: u32,
    name: &str,
    sink: &mut dyn Sink,
) -> Result<Generated, Cancelled> {
    let surface = generate_plane(opts, seed, name, sink)?;
    let mut planes = vec![surface];
    if opts.caves_plane && caves_plane_fits(&planes[0]) {
        let cave_opts = opts.caves_plane_options(planes[0].width, planes[0].height);
        let cave = generate_plane(&cave_opts, seed, &format!("__under_{name}"), sink)?;
        planes.push(cave);
    }
    Ok(Generated {
        planes,
        gates: Vec::new(),
    })
}

pub fn newgame_setup(crt: &mut CrtRng, players: i32, per_player_bucket: i32) -> (i32, f32) {
    newgame_setup_per_player(crt, players, newgame_per_player(per_player_bucket))
}

pub fn newgame_setup_per_player(crt: &mut CrtRng, players: i32, per: i32) -> (i32, f32) {
    let _ = crt.float();
    let roll = crt.float();
    let n = ((f64::from(roll) * NEWGAME_COUNT_SLOPE + NEWGAME_COUNT_BASE)
        * f64::from(per * players + NEWGAME_COUNT_PAD)) as i32;
    let rug = crt.float();
    (
        n.clamp(NEWGAME_MIN_PROV, NEWGAME_MAX_PROV),
        (f64::from(rug) * NEWGAME_RUGEDNESS_SCALE + NEWGAME_RUGEDNESS_BASE) as f32,
    )
}

fn replay_map_text(world: &mut World, text: &str, offset: usize) {
    for line in text.lines() {
        let mut t = line.split_whitespace();
        match t.next() {
            Some("#neighbour") => {
                let a: usize = t.next().unwrap().parse().unwrap();
                let b: usize = t.next().unwrap().parse().unwrap();
                crate::graph::addnbor(world, (a + offset) as i32, (b + offset) as i32);
            }
            Some("#neighbourspec") => {
                let a: usize = t.next().unwrap().parse().unwrap();
                let b: usize = t.next().unwrap().parse().unwrap();
                let v: i64 = t.next().unwrap().parse().unwrap();
                crate::graph::set_border_flags(world, (a + offset) as i32, (b + offset) as i32, v);
            }
            _ => {}
        }
    }
}

fn append_plane(world: &mut World, plane: &PlaneOut) -> PlaneRange {
    let first = world.provinces.len();
    for p in plane.provinces.iter().skip(1) {
        world.provinces.push(Province {
            x: p.x,
            y: p.y,
            terrain: p.terrain,
            nbors: Vec::new(),
            border: Vec::new(),
        });
    }
    replay_map_text(world, &plane.map_text, first - 1);
    PlaneRange {
        first,
        last: world.provinces.len() - 1,
    }
}

fn write_back(world: &World, plane: &mut PlaneOut, range: PlaneRange) {
    for (i, p) in plane.provinces.iter_mut().enumerate().skip(1) {
        let g = &world.provinces[range.first + i - 1];
        p.terrain = g.terrain;
        p.nbors.clear();
        p.border.clear();
        for (n, b) in g.nbors.iter().zip(&g.border) {
            let n = *n as usize;
            if range.holds(n) {
                p.nbors.push((n - range.first + 1) as u16);
                p.border.push(*b);
            }
        }
    }
}

pub fn apply_new_game_passes(
    g: &mut Generated,
    opts: &Options,
    sink: &mut dyn Sink,
) -> Result<(), Cancelled> {
    let last = g.planes.last().expect("a plane");
    let mut world = World::new(0);
    world.w = g.planes[0].width;
    world.h = g.planes[0].height;
    world.hwrap = g.planes[0].wrap & 1 != 0;
    world.vwrap = g.planes[0].wrap >> 1 & 1 != 0;
    world.crt = last.crt;
    world.pool = last.pool;
    let surface = append_plane(&mut world, &g.planes[0]);
    roll_terrain(&mut world, opts);
    if world.emit(Stage::Terrain, sink) == Control::Cancel {
        return Err(Cancelled);
    }
    if g.planes.len() > 1 {
        let cave = append_plane(&mut world, &g.planes[1]);
        roll_undercave_terrain(&mut world, cave);
        if world.emit(Stage::Terrain, sink) == Control::Cancel {
            return Err(Cancelled);
        }
        let (mw, mh) = (g.planes[1].width, g.planes[1].height);
        g.gates = generate_cave_gates(&mut world, surface, cave, mw, mh);
        if world.emit(Stage::Gates, sink) == Control::Cancel {
            return Err(Cancelled);
        }
        write_back(&world, &mut g.planes[1], cave);
        rewrite_map_text(&mut g.planes[1]);
    }
    write_back(&world, &mut g.planes[0], surface);
    rewrite_map_text(&mut g.planes[0]);
    Ok(())
}

fn rewrite_map_text(p: &mut PlaneOut) {
    let name = p
        .map_text
        .lines()
        .find_map(|l| l.strip_prefix("#dom2title "))
        .unwrap_or("map")
        .to_string();
    let mut world = World::new(0);
    world.w = p.width;
    world.h = p.height;
    world.hwrap = p.wrap & 1 != 0;
    world.vwrap = p.wrap >> 1 & 1 != 0;
    world.provinces = std::mem::take(&mut p.provinces);
    p.map_text = write_map_text(&mut world, &name);
    p.provinces = world.provinces;
}

pub fn generate_with_terrain(
    opts: &Options,
    seed: u32,
    name: &str,
    sink: &mut dyn Sink,
) -> Result<Generated, Cancelled> {
    let mut g = generate(opts, seed, name, sink)?;
    apply_new_game_passes(&mut g, opts, sink)?;
    Ok(g)
}

pub fn generate_new_game(
    opts: &Options,
    seed: u32,
    players: i32,
    per_player_bucket: i32,
    name: &str,
    sink: &mut dyn Sink,
) -> Result<Generated, Cancelled> {
    generate_new_game_per_player(
        opts,
        seed,
        players,
        newgame_per_player(per_player_bucket),
        name,
        sink,
    )
}

pub fn generate_new_game_per_player(
    opts: &Options,
    seed: u32,
    players: i32,
    per_player: i32,
    name: &str,
    sink: &mut dyn Sink,
) -> Result<Generated, Cancelled> {
    let mut crt = CrtRng::seeded(seed);
    let (nprov, rugedness) = newgame_setup_per_player(&mut crt, players, per_player);
    let mut surface_opts = opts.clone();
    surface_opts.provinces = nprov;
    surface_opts.rugedness_f32 = Some(rugedness);
    surface_opts.caves_plane = false;
    let surface = generate_plane(&surface_opts, seed, &format!("__randommap_{name}"), sink)?;
    let mut planes = vec![surface];
    if opts.caves_plane && caves_plane_fits_with_requested_count(&planes[0], nprov) {
        let cave_opts = surface_opts.caves_plane_options(NEWGAME_CAVE_MAP_W, NEWGAME_CAVE_MAP_H);
        planes.push(generate_plane(
            &cave_opts,
            seed,
            &format!("__under_{name}"),
            sink,
        )?);
    }
    let mut g = Generated {
        planes,
        gates: Vec::new(),
    };
    apply_new_game_passes(&mut g, opts, sink)?;
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::RecordSink;

    fn small() -> Options {
        Options {
            width: 512,
            height: 512,
            provinces: 12,
            ..Options::default()
        }
    }

    #[test]
    fn small_map_generates_and_emits_every_stage_in_order() {
        let mut sink = RecordSink::default();
        let g = generate(&small(), 1, "t", &mut sink).unwrap();
        let p = &g.planes[0];
        assert!(p.provinces.len() > 12);
        assert!(p.d6m.len() > (p.width * p.height) as usize * 4);
        assert!(p.map_text.contains("#dom2title t"));
        assert_eq!(sink.entries[0].0, Stage::NoiseTable);
        assert_eq!(sink.entries.last().unwrap().0, Stage::MapText);
    }

    #[test]
    fn terrain_roll_reaches_the_map_text() {
        let g = generate_with_terrain(&small(), 3, "t", &mut crate::stage::NoSink).unwrap();
        let p = &g.planes[0];
        let rolled = p
            .provinces
            .iter()
            .skip(1)
            .filter(|q| q.terrain & 0x1f0 != 0)
            .count();
        assert!(rolled > 0);
        let lines: Vec<i64> = p
            .map_text
            .lines()
            .filter_map(|l| l.strip_prefix("#terrain "))
            .filter_map(|l| l.split_whitespace().nth(1))
            .filter_map(|v| v.parse().ok())
            .collect();
        assert_eq!(lines.len(), p.provinces.len() - 1);
        assert!(lines.iter().any(|t| t & 0x1f0 != 0));
    }

    #[test]
    fn same_seed_same_bytes() {
        let a = generate(&small(), 7, "t", &mut crate::stage::NoSink).unwrap();
        let b = generate(&small(), 7, "t", &mut crate::stage::NoSink).unwrap();
        assert_eq!(a.planes[0].d6m, b.planes[0].d6m);
        assert_eq!(a.planes[0].map_text, b.planes[0].map_text);
        let c = generate(&small(), 8, "t", &mut crate::stage::NoSink).unwrap();
        assert_ne!(a.planes[0].d6m, c.planes[0].d6m);
    }

    fn two_plane() -> Options {
        Options {
            width: 512,
            height: 512,
            provinces: 40,
            caves_plane: true,
            ..Options::default()
        }
    }

    #[test]
    fn caves_plane_is_a_second_generate_plane_with_its_own_options() {
        let opts = two_plane();
        let g = generate(&opts, 3, "t", &mut crate::stage::NoSink).unwrap();
        assert_eq!(g.planes.len(), 2);
        let cave = &g.planes[1];
        assert_eq!(cave.width, g.planes[0].width);
        assert_eq!(
            cave.height,
            g.planes[0].height + 2 * crate::features::MARGIN_PX
        );
        assert_eq!(cave.wrap, g.planes[0].wrap);
        assert!(cave.map_text.contains("#dom2title __under_t"));
        assert!(cave.map_text.contains("#imagefile __under_t.d6m"));
        assert_ne!(cave.d6m, g.planes[0].d6m);
        let walls = cave.provinces[1..]
            .iter()
            .filter(|p| p.terrain & crate::cave::TERRAIN_CAVE_WALL != 0)
            .count();
        assert!(walls > 0, "caves plane carries no cave walls");
        let alone = generate(
            &opts.caves_plane_options(g.planes[0].width, g.planes[0].height),
            3,
            "__under_t",
            &mut crate::stage::NoSink,
        )
        .unwrap();
        assert_eq!(alone.planes.len(), 1);
        assert_eq!(alone.planes[0].d6m, cave.d6m);
    }

    #[test]
    fn two_plane_generation_is_deterministic_and_opens_gates() {
        let opts = two_plane();
        let a = generate_with_terrain(&opts, 3, "t", &mut crate::stage::NoSink).unwrap();
        let b = generate_with_terrain(&opts, 3, "t", &mut crate::stage::NoSink).unwrap();
        assert_eq!(a.planes[1].d6m, b.planes[1].d6m);
        assert_eq!(a.gates, b.gates);
        assert!(!a.gates.is_empty(), "no cave gate was opened");
        for gate in &a.gates {
            assert!((gate.surface as usize) < a.planes[0].provinces.len());
            assert!((gate.cave as usize) < a.planes[1].provinces.len());
            assert_ne!(
                a.planes[0].provinces[gate.surface as usize].terrain & crate::cave::TERRAIN_GATE,
                0
            );
            assert_ne!(
                a.planes[1].provinces[gate.cave as usize].terrain & crate::cave::TERRAIN_GATE,
                0
            );
        }
        let mut surface_seen: Vec<u16> = a.gates.iter().map(|g| g.surface).collect();
        surface_seen.sort_unstable();
        let before = surface_seen.len();
        surface_seen.dedup();
        assert_eq!(
            before,
            surface_seen.len(),
            "a surface province took two gates"
        );
    }

    #[test]
    fn the_gates_stage_reports_once_at_the_end() {
        let mut sink = RecordSink::default();
        generate_with_terrain(&two_plane(), 3, "t", &mut sink).unwrap();
        assert_eq!(sink.entries.last().unwrap().0, Stage::Gates);
        assert_eq!(
            sink.entries.iter().filter(|e| e.0 == Stage::Gates).count(),
            1
        );
        assert_eq!(
            sink.entries
                .iter()
                .filter(|e| e.0 == Stage::MapText)
                .count(),
            2
        );
    }

    #[test]
    fn one_plane_generation_leaves_the_gate_list_empty() {
        let g = generate_with_terrain(&small(), 1, "t", &mut crate::stage::NoSink).unwrap();
        assert_eq!(g.planes.len(), 1);
        assert!(g.gates.is_empty());
    }
}
