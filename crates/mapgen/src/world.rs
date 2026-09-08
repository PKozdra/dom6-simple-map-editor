use crate::hash::Fnv64;
use crate::rng::{CrtRng, NoiseTable, PoolRng};
use crate::stage::{Control, Sink, Stage};

pub const MAX_NBORS: usize = 20;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Province {
    pub x: i32,
    pub y: i32,
    pub terrain: i64,
    pub nbors: Vec<u16>,
    pub border: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct World {
    pub w: i32,
    pub h: i32,
    pub hwrap: bool,
    pub vwrap: bool,
    pub cave_world: bool,
    pub heights: Vec<f32>,
    pub owner: Vec<i16>,
    pub provinces: Vec<Province>,
    pub sea_level: f32,
    pub deep_level: f32,
    pub deep_frac: f32,
    pub mount_level: f32,
    pub mount_frac: f32,
    pub real_water_part: f32,
    pub spacing: f32,
    pub mean_sea_area: f32,
    pub mean_land_area: f32,
    pub bbox: Vec<[i16; 4]>,
    pub edge_dist: Vec<i16>,
    pub pool: PoolRng,
    pub crt: CrtRng,
    pub noise: NoiseTable,
    pub calls: [u32; Stage::ALL.len()],
    pub blueprint_mask: Vec<i8>,
    pub blueprint_mask_w: i32,
    pub blueprint_mask_h: i32,
    pub blueprint_sea_frac: f32,
    pub paint_noise_cursor: Option<usize>,
}

impl World {
    pub fn new(seed: u32) -> Self {
        let mut crt = CrtRng::seeded(seed);
        let noise = NoiseTable::fill(&mut crt);
        World {
            w: 0,
            h: 0,
            hwrap: false,
            vwrap: false,
            cave_world: false,
            heights: Vec::new(),
            owner: Vec::new(),
            provinces: vec![Province::default()],
            sea_level: 0.0,
            deep_level: 0.0,
            deep_frac: 0.0,
            mount_level: 0.0,
            mount_frac: 0.0,
            real_water_part: 0.0,
            spacing: 0.0,
            mean_sea_area: 0.0,
            mean_land_area: 0.0,
            bbox: Vec::new(),
            edge_dist: Vec::new(),
            pool: PoolRng::seeded(seed),
            crt,
            noise,
            calls: [0; Stage::ALL.len()],
            blueprint_mask: Vec::new(),
            blueprint_mask_w: 0,
            blueprint_mask_h: 0,
            blueprint_sea_frac: 0.0,
            paint_noise_cursor: None,
        }
    }

    pub fn nprov(&self) -> usize {
        self.provinces.len() - 1
    }

    pub fn hash_for(&self, stage: Stage) -> u64 {
        let mut h = Fnv64::new();
        match stage {
            Stage::NoiseTable => {
                h.f32s(&self.noise.values);
            }
            Stage::Height | Stage::Seams => {
                h.f32s(&self.heights);
            }
            Stage::SeaLevel => {
                h.f32(self.sea_level)
                    .f32(self.deep_level)
                    .f32(self.deep_frac)
                    .f32(self.mount_level)
                    .f32(self.mount_frac)
                    .f32(self.real_water_part);
            }
            Stage::Capitals => {
                self.hash_provinces(&mut h);
                h.f32(self.spacing);
            }
            Stage::Growth | Stage::Upsample | Stage::Paint => {
                h.i16s(&self.owner);
            }
            Stage::Graph | Stage::Edges | Stage::Gates => {
                self.hash_graph(&mut h);
            }
            Stage::Islands | Stage::Rivers | Stage::Mountains | Stage::Bridges | Stage::Cave => {
                self.hash_provinces(&mut h);
                self.hash_graph(&mut h);
                h.f32s(&self.heights);
            }
            Stage::Sizes | Stage::Terrain => {
                for p in &self.provinces[1..] {
                    h.i64(p.terrain);
                }
            }
            Stage::Margin => {
                self.hash_provinces(&mut h);
                h.i16s(&self.owner).f32s(&self.heights);
            }
            Stage::Recipe | Stage::MapText => {}
        }
        h.finish()
    }

    fn hash_provinces(&self, h: &mut Fnv64) {
        for p in &self.provinces[1..] {
            h.i32(p.x).i32(p.y).i64(p.terrain);
        }
    }

    fn hash_graph(&self, h: &mut Fnv64) {
        for p in &self.provinces[1..] {
            h.i32(p.nbors.len() as i32);
            for (n, b) in p.nbors.iter().zip(&p.border) {
                h.bytes(&n.to_le_bytes()).i64(*b);
            }
        }
    }

    pub fn emit(&mut self, stage: Stage, sink: &mut dyn Sink) -> Control {
        let i = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(0);
        let call = self.calls[i];
        self.calls[i] += 1;
        let hash = if sink.wants_hash() {
            self.hash_for(stage)
        } else {
            0
        };
        sink.stage(stage, call, hash)
    }

    pub fn emit_bytes(&mut self, stage: Stage, bytes: &[u8], sink: &mut dyn Sink) -> Control {
        let i = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(0);
        let call = self.calls[i];
        self.calls[i] += 1;
        let hash = if sink.wants_hash() {
            Fnv64::new().bytes(bytes).finish()
        } else {
            0
        };
        sink.stage(stage, call, hash)
    }
}
