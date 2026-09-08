use dom6_mapgen::cave::Gate;
use dom6_mapgen::generate::{
    generate_new_game, generate_new_game_per_player, generate_with_terrain, Cancelled, Generated,
};
use dom6_mapgen::{Blueprint, Options, Sink, Stage};

use crate::generator_panel::GeneratedPlane;

pub const PROGRESS: u8 = 0;
pub const DONE: u8 = 1;
pub const CANCELLED: u8 = 2;
pub const READY: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Terrain,
    NewGame {
        players: i32,
        per_player_bucket: i32,
    },
    PerPlayer {
        players: i32,
        per: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub opts: Options,
    pub seed: u32,
    pub name: String,
    pub mode: Mode,
}

pub fn run_job(job: &Job, sink: &mut dyn Sink) -> Result<Generated, Cancelled> {
    match job.mode {
        Mode::Terrain => generate_with_terrain(&job.opts, job.seed, &job.name, sink),
        Mode::NewGame {
            players,
            per_player_bucket,
        } => generate_new_game(
            &job.opts,
            job.seed,
            players,
            per_player_bucket,
            &job.name,
            sink,
        ),
        Mode::PerPlayer { players, per } => {
            generate_new_game_per_player(&job.opts, job.seed, players, per, &job.name, sink)
        }
    }
}

pub fn planes_of(g: Generated) -> (Vec<GeneratedPlane>, Vec<Gate>) {
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
    (planes, g.gates)
}

#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
    }

    pub fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos + n;
        if end > self.buf.len() {
            return Err(format!(
                "message truncated at byte {} of {}",
                self.pos,
                self.buf.len()
            ));
        }
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }

    pub fn i32(&mut self) -> Result<i32, String> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn bytes(&mut self) -> Result<&'a [u8], String> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    pub fn str(&mut self) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
    }

    pub fn done(&self) -> bool {
        self.pos >= self.buf.len()
    }
}

fn put_blueprint(w: &mut Writer, b: &Option<Blueprint>) {
    match b {
        Some(b) => {
            w.bool(true);
            w.i32(b.w);
            w.i32(b.h);
            w.bytes(&b.bgra);
        }
        None => w.bool(false),
    }
}

fn get_blueprint(r: &mut Reader) -> Result<Option<Blueprint>, String> {
    if !r.bool()? {
        return Ok(None);
    }
    let w = r.i32()?;
    let h = r.i32()?;
    let bgra = r.bytes()?.to_vec();
    Ok(Some(Blueprint { w, h, bgra }))
}

fn put_options(w: &mut Writer, o: &Options) {
    let Options {
        width,
        height,
        provinces,
        sea_part,
        mount_part,
        forest_part,
        farm_part,
        swamp_part,
        waste_part,
        highland_part,
        kelp_part,
        gorge_part,
        river_part,
        hills,
        rugedness,
        sea_size,
        extra_islands,
        bridges,
        no_water_prov,
        hwrap,
        vwrap,
        blueprint,
        blue_acc,
        cave_world,
        caves_plane,
        cave_part,
        cave_blueprint,
        rugedness_f32,
    } = o;
    for v in [
        width,
        height,
        provinces,
        sea_part,
        mount_part,
        forest_part,
        farm_part,
        swamp_part,
        waste_part,
        highland_part,
        kelp_part,
        gorge_part,
        river_part,
        hills,
        rugedness,
        sea_size,
        extra_islands,
        bridges,
        blue_acc,
        cave_part,
    ] {
        w.i32(*v);
    }
    for v in [no_water_prov, hwrap, vwrap, cave_world, caves_plane] {
        w.bool(*v);
    }
    put_blueprint(w, blueprint);
    put_blueprint(w, cave_blueprint);
    match rugedness_f32 {
        Some(f) => {
            w.bool(true);
            w.f32(*f);
        }
        None => w.bool(false),
    }
}

fn get_options(r: &mut Reader) -> Result<Options, String> {
    Ok(Options {
        width: r.i32()?,
        height: r.i32()?,
        provinces: r.i32()?,
        sea_part: r.i32()?,
        mount_part: r.i32()?,
        forest_part: r.i32()?,
        farm_part: r.i32()?,
        swamp_part: r.i32()?,
        waste_part: r.i32()?,
        highland_part: r.i32()?,
        kelp_part: r.i32()?,
        gorge_part: r.i32()?,
        river_part: r.i32()?,
        hills: r.i32()?,
        rugedness: r.i32()?,
        sea_size: r.i32()?,
        extra_islands: r.i32()?,
        bridges: r.i32()?,
        blue_acc: r.i32()?,
        cave_part: r.i32()?,
        no_water_prov: r.bool()?,
        hwrap: r.bool()?,
        vwrap: r.bool()?,
        cave_world: r.bool()?,
        caves_plane: r.bool()?,
        blueprint: get_blueprint(r)?,
        cave_blueprint: get_blueprint(r)?,
        rugedness_f32: if r.bool()? { Some(r.f32()?) } else { None },
    })
}

pub fn encode_job(job: &Job) -> Vec<u8> {
    let mut w = Writer::default();
    put_options(&mut w, &job.opts);
    w.u32(job.seed);
    w.str(&job.name);
    match &job.mode {
        Mode::Terrain => w.u8(0),
        Mode::NewGame {
            players,
            per_player_bucket,
        } => {
            w.u8(1);
            w.i32(*players);
            w.i32(*per_player_bucket);
        }
        Mode::PerPlayer { players, per } => {
            w.u8(2);
            w.i32(*players);
            w.i32(*per);
        }
    }
    w.finish()
}

pub fn decode_job(bytes: &[u8]) -> Result<Job, String> {
    let mut r = Reader::new(bytes);
    let opts = get_options(&mut r)?;
    let seed = r.u32()?;
    let name = r.str()?;
    let mode = match r.u8()? {
        0 => Mode::Terrain,
        1 => Mode::NewGame {
            players: r.i32()?,
            per_player_bucket: r.i32()?,
        },
        2 => Mode::PerPlayer {
            players: r.i32()?,
            per: r.i32()?,
        },
        k => return Err(format!("unknown generation mode {k}")),
    };
    Ok(Job {
        opts,
        seed,
        name,
        mode,
    })
}

pub fn encode_progress(stage: Stage, done: u32, total: u32) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(PROGRESS);
    let index = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(0);
    w.u8(index as u8);
    w.u32(done);
    w.u32(total);
    w.finish()
}

pub fn decode_progress(bytes: &[u8]) -> Result<(Stage, u32, u32), String> {
    let mut r = Reader::new(bytes);
    if r.u8()? != PROGRESS {
        return Err("not a progress message".to_string());
    }
    let index = r.u8()? as usize;
    let stage = Stage::ALL
        .get(index)
        .copied()
        .ok_or_else(|| format!("unknown stage {index}"))?;
    Ok((stage, r.u32()?, r.u32()?))
}

pub fn encode_done(planes: &[GeneratedPlane], gates: &[Gate]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(DONE);
    w.u32(planes.len() as u32);
    for p in planes {
        w.i32(p.width);
        w.i32(p.height);
        w.u32(p.provinces as u32);
        w.bytes(&p.d6m);
        w.str(&p.map_text);
    }
    w.u32(gates.len() as u32);
    for g in gates {
        w.u32(g.surface as u32);
        w.u32(g.cave as u32);
    }
    w.finish()
}

pub fn decode_done(bytes: &[u8]) -> Result<(Vec<GeneratedPlane>, Vec<Gate>), String> {
    let mut r = Reader::new(bytes);
    if r.u8()? != DONE {
        return Err("not a result message".to_string());
    }
    let n = r.u32()? as usize;
    let mut planes = Vec::with_capacity(n);
    for _ in 0..n {
        let width = r.i32()?;
        let height = r.i32()?;
        let provinces = r.u32()? as usize;
        let d6m = r.bytes()?.to_vec();
        let map_text = r.str()?;
        planes.push(GeneratedPlane {
            d6m,
            map_text,
            width,
            height,
            provinces,
        });
    }
    let n = r.u32()? as usize;
    let mut gates = Vec::with_capacity(n);
    for _ in 0..n {
        let surface = r.u32()? as u16;
        let cave = r.u32()? as u16;
        gates.push(Gate { surface, cave });
    }
    if !r.done() {
        return Err("result message has trailing bytes".to_string());
    }
    Ok((planes, gates))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_survives_the_round_trip() {
        let opts = Options {
            blueprint: Some(Blueprint {
                w: 2,
                h: 1,
                bgra: vec![1, 2, 3, 4, 5, 6, 7, 8],
            }),
            rugedness_f32: Some(0.25),
            caves_plane: true,
            ..Options::default()
        };
        let job = Job {
            opts,
            seed: 77,
            name: "ring".to_string(),
            mode: Mode::PerPlayer {
                players: 5,
                per: 13,
            },
        };
        let back = decode_job(&encode_job(&job)).expect("decode");
        assert_eq!(back, job);
    }

    #[test]
    fn a_result_survives_the_round_trip() {
        let planes = vec![GeneratedPlane {
            d6m: vec![9, 8, 7],
            map_text: "#dom2title x\n".to_string(),
            width: 40,
            height: 30,
            provinces: 12,
        }];
        let gates = vec![Gate {
            surface: 3,
            cave: 14,
        }];
        let (p, g) = decode_done(&encode_done(&planes, &gates)).expect("decode");
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].d6m, vec![9, 8, 7]);
        assert_eq!(p[0].map_text, "#dom2title x\n");
        assert_eq!(p[0].provinces, 12);
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].surface, g[0].cave), (3, 14));
        let (stage, done, total) = decode_progress(&encode_progress(Stage::Rivers, 4, 9)).unwrap();
        assert_eq!((stage, done, total), (Stage::Rivers, 4, 9));
    }
}
