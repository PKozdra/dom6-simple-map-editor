use crate::d6m::{stored_from_units, units_from_stored, D6m, STORED_LIMIT};

pub const DEEP_SEA_FROM: f32 = -36.0;
use crate::decor::Rng;
use crate::mapfile::{plane_file_name, strip_plane_suffix, MapFile};
use crate::render::{province_bboxes, Options, Plane, Rect, Rendered};
use crate::terrain::{self, BORDER_CARVED, SEA, UNKNOWN};
use crate::textures::TexSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightOp {
    Flat(f32),
    Below(f32),
    Above(f32),
    Offset(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlagOp {
    Keep,
    Sea,
    DeepSea,
    Land,
    Set(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapChange {
    Terrain {
        p: u32,
        old: u64,
        new: u64,
    },
    Name {
        p: u32,
        old: Option<String>,
        new: Option<String>,
    },
    Gate {
        p: u32,
        old: i32,
        new: i32,
    },
    Neighbour {
        a: u32,
        b: u32,
        old: bool,
        new: bool,
    },
    Spec {
        a: u32,
        b: u32,
        old: i64,
        new: i64,
    },
    Capital {
        p: u32,
        old: (i16, i16),
        new: (i16, i16),
    },
    AddProvince {
        p: u32,
        x: i16,
        y: i16,
        terrain: u64,
    },
    RemoveProvince {
        p: u32,
        x: i16,
        y: i16,
        terrain: u64,
        name: String,
        gate: i32,
        links: Vec<(u32, i64)>,
        pixels: Vec<(u32, i16)>,
        baseline: Vec<u32>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Edit {
    pub label: String,
    pub heights: Vec<(u32, i16, i16)>,
    pub owners: Vec<(u32, i16, i16)>,
    pub map: Vec<MapChange>,
    pub rect: Option<Rect>,
}

impl Edit {
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty() && self.owners.is_empty() && self.map.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProvStats {
    pub pixels: u32,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub water_share: f32,
}

pub struct PlaneDoc {
    pub index: u32,
    pub d6m_path: PathBuf,
    pub map_path: Option<PathBuf>,
    pub d6m: D6m,
    pub map: Option<MapFile>,
    pub heights: Vec<f32>,
    pub flags: Vec<u64>,
    pub names: Vec<String>,
    pub gates: Vec<i32>,
    pub capitals: Vec<(i16, i16)>,
    pub rivers: Vec<(u32, u32)>,
    pub mountain_lines: Vec<(u32, u32)>,
    pub bridges: Vec<(u32, u32)>,
    pub baseline: Vec<i16>,
    pub pixel_counts: Vec<u32>,
    pub rendered: Rendered,
    pub dirty: bool,
    pub edited: bool,
    pub owners_changed: bool,
    pub undo: Vec<Edit>,
    pub redo: Vec<Edit>,
    river_repair: Vec<(u32, i16, i16)>,
    stroke: Option<Edit>,
}

fn count_pixels(owners: &[i16], n: usize) -> Vec<u32> {
    let mut counts = vec![0u32; n + 1];
    for &o in owners {
        if o > 0 && (o as usize) <= n {
            counts[o as usize] += 1;
        }
    }
    counts
}

pub fn wrap_point(x: i32, y: i32, w: i32, h: i32, hwrap: bool, vwrap: bool) -> (i32, i32) {
    let wx = if hwrap && w > 0 { x.rem_euclid(w) } else { x };
    let wy = if vwrap && h > 0 { y.rem_euclid(h) } else { y };
    (wx, wy)
}

pub fn wrap_delta(d: f32, n: f32, wrap: bool) -> f32 {
    if wrap && n > 0.0 {
        d - (d / n).round() * n
    } else {
        d
    }
}

pub fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        }),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

impl PlaneDoc {
    fn build(
        index: u32,
        d6m_path: PathBuf,
        map_path: Option<PathBuf>,
        d6m: D6m,
        map: Option<MapFile>,
        tex: &TexSet,
        opts: &Options,
    ) -> PlaneDoc {
        let n = d6m.provinces.len();
        let mut flags = vec![0u64; n + 1];
        let mut names = vec![String::new(); n + 1];
        let mut gates = vec![0i32; n + 1];
        for (i, p) in d6m.provinces.iter().enumerate() {
            let id = i as u32 + 1;
            let from_map = map.as_ref().and_then(|m| m.terrain.get(&id).copied());
            flags[i + 1] = from_map.unwrap_or(p.terrain) as u64;
            if let Some(m) = &map {
                if let Some(nm) = m.name_of(id) {
                    names[i + 1] = nm.to_string();
                }
                gates[i + 1] = m.gate_of(id);
            }
        }
        let capitals: Vec<(i16, i16)> = d6m.provinces.iter().map(|p| (p.x, p.y)).collect();
        let heights = d6m.heights_f32();
        let mut pixel_counts = vec![0u32; n + 1];
        for &o in &d6m.owners {
            if o > 0 && (o as usize) <= n {
                pixel_counts[o as usize] += 1;
            }
        }
        let mut doc = PlaneDoc {
            index,
            d6m_path,
            map_path,
            d6m,
            map,
            heights,
            flags,
            names,
            gates,
            capitals,
            rivers: Vec::new(),
            mountain_lines: Vec::new(),
            bridges: Vec::new(),
            baseline: Vec::new(),
            pixel_counts,
            rendered: Rendered::empty(),
            river_repair: Vec::new(),
            dirty: false,
            edited: false,
            owners_changed: false,
            undo: Vec::new(),
            redo: Vec::new(),
            stroke: None,
        };
        doc.baseline = doc.d6m.owners.clone();
        doc.rebuild_links();
        doc.rendered = Rendered::new(&doc.plane(), tex, opts);
        doc
    }

    pub fn rebuild_links(&mut self) {
        let n = self.d6m.provinces.len();
        let mut rivers = Vec::new();
        let mut lines = Vec::new();
        let mut bridges = Vec::new();
        if let Some(m) = &self.map {
            for &(a, b) in &m.neighbours {
                if a == 0 || b == 0 || a as usize > n || b as usize > n || a >= b {
                    continue;
                }
                if self.flags[a as usize] & UNKNOWN != 0 || self.flags[b as usize] & UNKNOWN != 0 {
                    continue;
                }
                let spec = m.spec_between(a, b);
                if spec as u64 & BORDER_CARVED != 0 {
                    rivers.push((a, b));
                }
                if crate::decor::is_mountain_line(spec) {
                    lines.push((a, b));
                }
                if spec as u64 & terrain::BORDER_BRIDGE != 0 {
                    bridges.push((a, b));
                }
            }
        }
        self.rivers = rivers;
        self.mountain_lines = lines;
        self.bridges = bridges;
    }

    pub fn plane(&self) -> Plane<'_> {
        Plane {
            w: self.d6m.width,
            h: self.d6m.height,
            heights: &self.heights,
            owners: &self.d6m.owners,
            flags: &self.flags,
            scale: self.d6m.map_scale(),
            hwrap: self.map.as_ref().map(|m| m.hwrap).unwrap_or(false),
            vwrap: self.map.as_ref().map(|m| m.vwrap).unwrap_or(false),
            capitals: &self.capitals,
            rivers: &self.rivers,
            mountain_lines: &self.mountain_lines,
            bridges: &self.bridges,
            cave_plane: self.index == 2,
        }
    }

    pub fn hwrap(&self) -> bool {
        self.map.as_ref().map(|m| m.hwrap).unwrap_or(false)
    }

    pub fn vwrap(&self) -> bool {
        self.map.as_ref().map(|m| m.vwrap).unwrap_or(false)
    }

    pub fn width(&self) -> i32 {
        self.d6m.width
    }

    pub fn height(&self) -> i32 {
        self.d6m.height
    }

    pub fn province_count(&self) -> usize {
        self.d6m.provinces.len()
    }

    pub fn has_map(&self) -> bool {
        self.map.is_some()
    }

    pub fn owner_at(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.d6m.width || y >= self.d6m.height {
            return 0;
        }
        let o = self.d6m.owners[(y * self.d6m.width + x) as usize];
        if o > 0 && (o as usize) <= self.d6m.provinces.len() {
            o as u32
        } else {
            0
        }
    }

    pub fn name(&self, prov: u32) -> &str {
        self.names
            .get(prov as usize)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn gate(&self, prov: u32) -> i32 {
        self.gates.get(prov as usize).copied().unwrap_or(0)
    }

    pub fn neighbours(&self, prov: u32) -> Vec<u32> {
        self.map
            .as_ref()
            .map(|m| m.neighbours_of(prov))
            .unwrap_or_default()
    }

    pub fn linked(&self, a: u32, b: u32) -> bool {
        self.map
            .as_ref()
            .map(|m| m.are_neighbours(a, b))
            .unwrap_or(false)
    }

    pub fn spec(&self, a: u32, b: u32) -> i64 {
        self.map.as_ref().map(|m| m.spec_between(a, b)).unwrap_or(0)
    }

    pub fn capital(&self, prov: u32) -> Option<(i32, i32)> {
        self.capitals
            .get(prov as usize - 1)
            .map(|&(x, y)| (x as i32, y as i32))
    }

    pub fn capital_inside(&self, prov: u32) -> bool {
        self.capital(prov)
            .map(|(x, y)| self.owner_at(x, y) == prov)
            .unwrap_or(false)
    }

    pub fn area_centre(&self, prov: u32) -> Option<(i32, i32)> {
        let r = self.bbox(prov)?;
        let w = self.d6m.width;
        let (mut sx, mut sy, mut n) = (0i64, 0i64, 0i64);
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                if self.d6m.owners[(y * w + x) as usize] as u32 == prov {
                    sx += x as i64;
                    sy += y as i64;
                    n += 1;
                }
            }
        }
        if n == 0 {
            return None;
        }
        let (cx, cy) = ((sx / n) as i32, (sy / n) as i32);
        let mut best = None;
        let mut best_d = i64::MAX;
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                if self.d6m.owners[(y * w + x) as usize] as u32 != prov {
                    continue;
                }
                let d = ((x - cx) as i64).pow(2) + ((y - cy) as i64).pow(2);
                if d < best_d {
                    best_d = d;
                    best = Some((x, y));
                }
            }
        }
        best
    }

    pub fn set_capital(&mut self, prov: u32, x: i32, y: i32, tex: &TexSet, opts: &Options) -> bool {
        let Some(old) = self.capitals.get(prov as usize - 1).copied() else {
            return false;
        };
        if self.owner_at(x, y) != prov {
            return false;
        }
        let new = (x as i16, y as i16);
        if new == old {
            return false;
        }
        let around = |(px, py): (i16, i16)| Rect {
            x0: px as i32 - 2,
            y0: py as i32 - 2,
            x1: px as i32 + 2,
            y1: py as i32 + 2,
        };
        let edit = Edit {
            label: "Move capital".to_string(),
            map: vec![MapChange::Capital { p: prov, old, new }],
            rect: Some(
                around(old)
                    .union(around(new))
                    .clamp_to(self.d6m.width, self.d6m.height),
            ),
            ..Default::default()
        };
        self.push(edit, tex, opts)
    }

    pub fn centre_capital(&mut self, prov: u32, tex: &TexSet, opts: &Options) -> bool {
        match self.area_centre(prov) {
            Some((x, y)) => self.set_capital(prov, x, y, tex, opts),
            None => false,
        }
    }

    pub fn add_province(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<u32> {
        let w = self.d6m.width;
        let h = self.d6m.height;
        if cx < 0 || cy < 0 || cx >= w || cy >= h || self.d6m.provinces.len() >= 32000 {
            return None;
        }
        let p = self.d6m.provinces.len() as u32 + 1;
        let under = self.owner_at(cx, cy);
        let inherit = SEA
            | terrain::DEEP_SEA
            | terrain::CAVE
            | terrain::CAVE_LOOK
            | terrain::WARMER
            | terrain::COLDER;
        let terrain = self.flags.get(under as usize).copied().unwrap_or(0) & inherit;
        let mut owners = Vec::new();
        let r = radius.max(1);
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                let Some(i) = self.brush_pixel(cx + dx, cy + dy) else {
                    continue;
                };
                owners.push((i as u32, self.d6m.owners[i], p as i16));
            }
        }
        let rect = Rect {
            x0: (cx - r).max(0),
            y0: (cy - r).max(0),
            x1: (cx + r).min(w - 1),
            y1: (cy + r).min(h - 1),
        };
        let edit = Edit {
            label: format!("New province {p}"),
            owners,
            map: vec![MapChange::AddProvince {
                p,
                x: cx as i16,
                y: cy as i16,
                terrain,
            }],
            rect: Some(rect),
            ..Default::default()
        };
        if self.push(edit, tex, opts) {
            Some(p)
        } else {
            None
        }
    }

    pub fn remove_province(&mut self, p: u32, tex: &TexSet, opts: &Options) -> bool {
        let n = self.d6m.provinces.len() as u32;
        if p == 0 || p > n || n < 2 {
            return false;
        }
        let w = self.d6m.width;
        let h = self.d6m.height;
        let (idx, fill) = fill_removed(w, h, &self.d6m.owners, &self.d6m.heights, p);
        let rec = self.d6m.provinces[p as usize - 1].clone();
        let links: Vec<(u32, i64)> = self
            .neighbours(p)
            .into_iter()
            .map(|q| (q, self.spec(p, q)))
            .collect();
        let pixels: Vec<(u32, i16)> = idx
            .iter()
            .zip(fill.iter())
            .map(|(&i, &f)| (i as u32, f))
            .collect();
        let baseline: Vec<u32> = self
            .baseline
            .iter()
            .enumerate()
            .filter(|(_, &b)| b as u32 == p)
            .map(|(i, _)| i as u32)
            .collect();
        let edit = Edit {
            label: format!("Remove province {p}"),
            map: vec![MapChange::RemoveProvince {
                p,
                x: rec.x,
                y: rec.y,
                terrain: self.flags[p as usize],
                name: self.names[p as usize].clone(),
                gate: self.gates[p as usize],
                links,
                pixels,
                baseline,
            }],
            rect: Some(Rect::full(w, h)),
            ..Default::default()
        };
        self.push(edit, tex, opts)
    }

    fn drop_province(&mut self, p: u32, pixels: &[(u32, i16)], baseline: &[u32]) {
        for &(i, f) in pixels {
            self.d6m.owners[i as usize] = f;
        }
        for &i in baseline {
            self.baseline[i as usize] = self.d6m.owners[i as usize];
        }
        for o in self.d6m.owners.iter_mut() {
            if *o as u32 > p {
                *o -= 1;
            }
        }
        for b in self.baseline.iter_mut() {
            if *b as u32 > p {
                *b -= 1;
            }
        }
        let n = p as usize;
        self.d6m.provinces.remove(n - 1);
        self.flags.remove(n);
        self.names.remove(n);
        self.gates.remove(n);
        self.capitals.remove(n - 1);
        self.pixel_counts = count_pixels(&self.d6m.owners, self.d6m.provinces.len());
        if let Some(m) = &mut self.map {
            m.remove_province(p);
            m.renumber(|q| if q > p { q - 1 } else { q });
        }
        self.owners_changed = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn restore_province(
        &mut self,
        p: u32,
        x: i16,
        y: i16,
        terrain: u64,
        name: &str,
        gate: i32,
        links: &[(u32, i64)],
        pixels: &[(u32, i16)],
        baseline: &[u32],
    ) {
        for o in self.d6m.owners.iter_mut() {
            if *o as u32 >= p {
                *o += 1;
            }
        }
        for &(i, _) in pixels {
            self.d6m.owners[i as usize] = p as i16;
        }
        for b in self.baseline.iter_mut() {
            if *b as u32 >= p {
                *b += 1;
            }
        }
        for &i in baseline {
            self.baseline[i as usize] = p as i16;
        }
        let n = p as usize;
        self.d6m.provinces.insert(
            n - 1,
            crate::d6m::Province {
                x,
                y,
                terrain: terrain as i64,
            },
        );
        self.flags.insert(n, terrain);
        self.names.insert(n, name.to_string());
        self.gates.insert(n, gate);
        self.capitals.insert(n - 1, (x, y));
        self.pixel_counts = count_pixels(&self.d6m.owners, self.d6m.provinces.len());
        if let Some(m) = &mut self.map {
            m.renumber(|q| if q >= p { q + 1 } else { q });
            m.set_terrain(p, terrain as i64);
            if !name.is_empty() {
                m.set_name(p, Some(name));
            }
            if gate != 0 {
                m.set_gate(p, gate);
            }
            for &(q, spec) in links {
                m.set_neighbour(p, q, true);
                if spec != 0 {
                    m.set_spec(p, q, spec);
                }
            }
        }
        self.owners_changed = true;
    }

    pub fn empty_provinces(&self) -> Vec<u32> {
        (1..=self.d6m.provinces.len() as u32)
            .filter(|&p| self.pixel_counts.get(p as usize).copied().unwrap_or(0) == 0)
            .collect()
    }

    fn grow_province(&mut self, p: u32, x: i16, y: i16, terrain: u64) {
        let n = p as usize;
        self.d6m.provinces.truncate(n - 1);
        self.d6m.provinces.push(crate::d6m::Province {
            x,
            y,
            terrain: terrain as i64,
        });
        self.flags.resize(n + 1, 0);
        self.flags[n] = terrain;
        self.names.resize(n + 1, String::new());
        self.gates.resize(n + 1, 0);
        self.capitals.truncate(n - 1);
        self.capitals.push((x, y));
        self.pixel_counts.resize(n + 1, 0);
        if let Some(m) = &mut self.map {
            m.set_terrain(p, terrain as i64);
        }
    }

    fn shrink_province(&mut self, p: u32) {
        let n = p as usize;
        self.d6m.provinces.truncate(n - 1);
        self.flags.truncate(n);
        self.names.truncate(n);
        self.gates.truncate(n);
        self.capitals.truncate(n - 1);
        self.pixel_counts.truncate(n);
        if let Some(m) = &mut self.map {
            m.remove_province(p);
        }
    }

    pub fn bbox(&self, prov: u32) -> Option<Rect> {
        let b = self.rendered.bboxes.get(prov as usize)?;
        if b[1] < 0 || b[3] < 0 {
            return None;
        }
        Some(Rect {
            x0: b[0],
            y0: b[2],
            x1: b[1],
            y1: b[3],
        })
    }

    pub fn stats(&self, prov: u32) -> ProvStats {
        let Some(r) = self.bbox(prov) else {
            return ProvStats::default();
        };
        let w = self.d6m.width;
        let mut s = ProvStats {
            pixels: 0,
            min: f32::MAX,
            max: f32::MIN,
            mean: 0.0,
            water_share: 0.0,
        };
        let mut sum = 0.0f64;
        let mut water = 0u32;
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                let i = (y * w + x) as usize;
                if self.d6m.owners[i] as u32 != prov {
                    continue;
                }
                let h = self.heights[i];
                s.pixels += 1;
                s.min = s.min.min(h);
                s.max = s.max.max(h);
                sum += h as f64;
                if h < 0.0 {
                    water += 1;
                }
            }
        }
        if s.pixels > 0 {
            s.mean = (sum / s.pixels as f64) as f32;
            s.water_share = water as f32 / s.pixels as f32;
        } else {
            s.min = 0.0;
            s.max = 0.0;
        }
        s
    }

    pub fn scar_count(&self) -> usize {
        self.d6m
            .heights
            .iter()
            .filter(|&&h| h <= -STORED_LIMIT)
            .count()
    }

    pub fn apply(
        &mut self,
        prov: u32,
        op: HeightOp,
        flag_op: FlagOp,
        label: &str,
        tex: &TexSet,
        opts: &Options,
    ) -> bool {
        let Some(r) = self.bbox(prov) else {
            return false;
        };
        let st = self.stats(prov);
        let delta = match op {
            HeightOp::Flat(_) => 0.0,
            HeightOp::Below(t) => t - st.max,
            HeightOp::Above(t) => t - st.min,
            HeightOp::Offset(d) => d,
        };
        let w = self.d6m.width;
        let mut edit = Edit {
            label: label.to_string(),
            rect: Some(r),
            ..Default::default()
        };
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                let i = (y * w + x) as usize;
                if self.d6m.owners[i] as u32 != prov {
                    continue;
                }
                let old = self.d6m.heights[i];
                let target = match op {
                    HeightOp::Flat(v) => v,
                    _ => units_from_stored(old) + delta,
                };
                let new = stored_from_units(target);
                if new != old {
                    edit.heights.push((i as u32, old, new));
                }
            }
        }
        let old_flags = self.flags[prov as usize];
        let new_flags = match flag_op {
            FlagOp::Keep => old_flags,
            FlagOp::Sea => terrain::make_sea(old_flags, false),
            FlagOp::DeepSea => terrain::make_sea(old_flags, true),
            FlagOp::Land => terrain::make_land(old_flags),
            FlagOp::Set(v) => v,
        };
        if new_flags != old_flags {
            edit.map.push(MapChange::Terrain {
                p: prov,
                old: old_flags,
                new: new_flags,
            });
        }
        self.push(edit, tex, opts)
    }

    pub fn set_flags(
        &mut self,
        prov: u32,
        new: u64,
        label: &str,
        tex: &TexSet,
        opts: &Options,
    ) -> bool {
        let old = self.flags.get(prov as usize).copied().unwrap_or(0);
        if old == new {
            return false;
        }
        let edit = Edit {
            label: label.to_string(),
            map: vec![MapChange::Terrain { p: prov, old, new }],
            rect: self.bbox(prov),
            ..Default::default()
        };
        self.push(edit, tex, opts)
    }

    pub fn set_name(&mut self, prov: u32, name: &str, tex: &TexSet, opts: &Options) -> bool {
        let old = self.name(prov).to_string();
        let new = name.trim().replace('"', "'");
        if old == new {
            return false;
        }
        let edit = Edit {
            label: "Rename".to_string(),
            map: vec![MapChange::Name {
                p: prov,
                old: if old.is_empty() { None } else { Some(old) },
                new: if new.is_empty() { None } else { Some(new) },
            }],
            rect: Some(Rect {
                x0: 0,
                y0: 0,
                x1: -1,
                y1: -1,
            }),
            ..Default::default()
        };
        self.push(edit, tex, opts)
    }

    pub fn set_gate(&mut self, prov: u32, n: i32, tex: &TexSet, opts: &Options) -> bool {
        let old = self.gate(prov);
        if old == n {
            return false;
        }
        let of = self.flags[prov as usize];
        let nf = if n != 0 {
            of | terrain::GATEWAY
        } else {
            of & !terrain::GATEWAY
        };
        let mut edit = Edit {
            label: "Gate".to_string(),
            map: vec![MapChange::Gate {
                p: prov,
                old,
                new: n,
            }],
            rect: self.bbox(prov),
            ..Default::default()
        };
        if nf != of {
            edit.map.push(MapChange::Terrain {
                p: prov,
                old: of,
                new: nf,
            });
        }
        self.push(edit, tex, opts)
    }

    pub fn set_link(
        &mut self,
        a: u32,
        b: u32,
        present: bool,
        tex: &TexSet,
        opts: &Options,
    ) -> bool {
        let Some(m) = &self.map else {
            return false;
        };
        if a == b || m.are_neighbours(a, b) == present {
            return false;
        }
        let mut edit = Edit {
            label: if present { "Link" } else { "Unlink" }.to_string(),
            rect: union(self.bbox(a), self.bbox(b)),
            ..Default::default()
        };
        if !present {
            let old = m.spec_between(a, b);
            if old != 0 {
                edit.map.push(MapChange::Spec { a, b, old, new: 0 });
                if old as u64 & BORDER_CARVED != 0 {
                    edit.heights = self.trench_repairs(a, b);
                }
            }
        }
        edit.map.push(MapChange::Neighbour {
            a,
            b,
            old: !present,
            new: present,
        });
        self.push(edit, tex, opts)
    }

    pub fn set_spec(&mut self, a: u32, b: u32, spec: i64, tex: &TexSet, opts: &Options) -> bool {
        let old = self.spec(a, b);
        if old == spec || self.map.is_none() {
            return false;
        }
        let mut edit = Edit {
            label: "Border".to_string(),
            map: vec![MapChange::Spec {
                a,
                b,
                old,
                new: spec,
            }],
            rect: union(self.bbox(a), self.bbox(b)),
            ..Default::default()
        };
        let was = old as u64 & BORDER_CARVED != 0;
        let is = spec as u64 & BORDER_CARVED != 0;
        if was && !is {
            edit.heights = self.trench_repairs(a, b);
        } else if !was && is {
            edit.heights = self.lift_border(a, b);
        }
        self.push(edit, tex, opts)
    }

    fn border_pixels(&self, a: u32, b: u32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        let Some(r) = union(self.bbox(a), self.bbox(b)) else {
            return out;
        };
        let w = self.d6m.width;
        let h = self.d6m.height;
        for y in r.y0..=r.y1 {
            for x in r.x0..=r.x1 {
                let id = self.owner_at(x, y);
                if id != a && id != b {
                    continue;
                }
                let right = if x + 1 < w {
                    self.owner_at(x + 1, y)
                } else {
                    0
                };
                let down = if y + 1 < h {
                    self.owner_at(x, y + 1)
                } else {
                    0
                };
                let other = if right != id {
                    right
                } else if down != id {
                    down
                } else {
                    continue;
                };
                if other == a || other == b {
                    out.push((x, y));
                }
            }
        }
        out
    }

    fn lift_border(&self, a: u32, b: u32) -> Vec<(u32, i16, i16)> {
        if self.flags[a as usize] & SEA != 0 || self.flags[b as usize] & SEA != 0 {
            return Vec::new();
        }
        let r = (self.d6m.map_scale() * 0.05) as i32 + 1;
        let w = self.d6m.width;
        let h = self.d6m.height;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (x, y) in self.border_pixels(a, b) {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let i = (ny * w + nx) as usize;
                    let old = self.d6m.heights[i];
                    if old < 16 && seen.insert(i) {
                        out.push((i as u32, old, 16));
                    }
                }
            }
        }
        out
    }

    fn trench_repairs(&self, a: u32, b: u32) -> Vec<(u32, i16, i16)> {
        let Some(r) = union(self.bbox(a), self.bbox(b)) else {
            return Vec::new();
        };
        let owners = &self.d6m.owners;
        self.repair_in(r, |i| owners[i] as u32 == a || owners[i] as u32 == b)
    }

    fn repair_in(&self, r: Rect, inside: impl Fn(usize) -> bool) -> Vec<(u32, i16, i16)> {
        let w = self.d6m.width as usize;
        let h = self.d6m.height as usize;
        let n = w * h;
        let mut bad = vec![false; n];
        let mut frontier = Vec::new();
        for y in r.y0.max(0)..=r.y1.min(h as i32 - 1) {
            for x in r.x0.max(0)..=r.x1.min(w as i32 - 1) {
                let i = y as usize * w + x as usize;
                if self.d6m.heights[i] <= -STORED_LIMIT && inside(i) {
                    bad[i] = true;
                    frontier.push(i);
                }
            }
        }
        if frontier.is_empty() {
            return Vec::new();
        }
        let scars: Vec<usize> = frontier.clone();
        let mut work: Vec<f32> = self.heights.clone();
        while !frontier.is_empty() {
            let mut next = Vec::new();
            let mut updates = Vec::new();
            for &i in &frontier {
                let x = (i % w) as i32;
                let y = (i / w) as i32;
                let mut vals = Vec::with_capacity(8);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if !bad[j] && self.d6m.heights[j] > -STORED_LIMIT {
                            vals.push(work[j]);
                        }
                    }
                }
                if vals.is_empty() {
                    next.push(i);
                } else {
                    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let m = vals.len();
                    let med = if m % 2 == 1 {
                        vals[m / 2]
                    } else {
                        (vals[m / 2 - 1] + vals[m / 2]) * 0.5
                    };
                    updates.push((i, med.max(1.0)));
                }
            }
            if updates.is_empty() {
                for &i in &next {
                    updates.push((i, 1.0));
                }
                next.clear();
            }
            for (i, v) in updates {
                work[i] = v;
                bad[i] = false;
            }
            frontier = next;
        }
        for _ in 0..2 {
            let snapshot = work.clone();
            for &i in &scars {
                let x = (i % w) as i32;
                let y = (i / w) as i32;
                let mut sum = 0.0;
                let mut cnt = 0.0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if self.d6m.heights[j] > -STORED_LIMIT || scars.binary_search(&j).is_ok() {
                            sum += snapshot[j];
                            cnt += 1.0;
                        }
                    }
                }
                if cnt > 0.0 {
                    work[i] = (sum / cnt).max(1.0);
                }
            }
        }
        scars
            .iter()
            .filter_map(|&i| {
                let old = self.d6m.heights[i];
                let new = stored_from_units(work[i]);
                if new != old {
                    Some((i as u32, old, new))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn set_river_repair(&mut self, on: bool, tex: &TexSet, opts: &Options) -> usize {
        let full = Rect::full(self.d6m.width, self.d6m.height);
        let changed = if on {
            if !self.river_repair.is_empty() {
                return 0;
            }
            let changes = self.repair_in(full, |_| true);
            for &(i, _, new) in &changes {
                self.d6m.heights[i as usize] = new;
                self.heights[i as usize] = units_from_stored(new);
            }
            self.river_repair = changes;
            self.river_repair.len()
        } else {
            let changes = std::mem::take(&mut self.river_repair);
            let mut n = 0;
            for &(i, old, new) in &changes {
                if self.d6m.heights[i as usize] == new {
                    self.d6m.heights[i as usize] = old;
                    self.heights[i as usize] = units_from_stored(old);
                    n += 1;
                }
            }
            n
        };
        if changed > 0 {
            let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
            r.render(&self.plane(), tex, opts, full);
            self.rendered = r;
        }
        changed
    }

    pub fn repair_scars(&mut self, tex: &TexSet, opts: &Options) -> usize {
        let full = Rect::full(self.d6m.width, self.d6m.height);
        let changes = self.repair_in(full, |_| true);
        let count = changes.len();
        let edit = Edit {
            label: "Repair river scars".to_string(),
            heights: changes,
            rect: Some(full),
            ..Default::default()
        };
        if self.push(edit, tex, opts) {
            count
        } else {
            0
        }
    }

    fn brush_pixel(&self, x: i32, y: i32) -> Option<usize> {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let x = if self.hwrap() { x.rem_euclid(w) } else { x };
        let y = if self.vwrap() { y.rem_euclid(h) } else { y };
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        Some((y * w + x) as usize)
    }

    fn brush_rect(&self, cx: i32, cy: i32, radius: i32) -> Rect {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let (x0, x1) = if self.hwrap() && (cx - radius < 0 || cx + radius >= w) {
            (0, w - 1)
        } else {
            ((cx - radius).max(0), (cx + radius).min(w - 1))
        };
        let (y0, y1) = if self.vwrap() && (cy - radius < 0 || cy + radius >= h) {
            (0, h - 1)
        } else {
            ((cy - radius).max(0), (cy + radius).min(h - 1))
        };
        Rect { x0, y0, x1, y1 }
    }

    pub fn paint_begin(&mut self, label: &str) {
        self.stroke = Some(Edit {
            label: label.to_string(),
            ..Default::default()
        });
    }

    pub fn paint(
        &mut self,
        prov: u32,
        cx: i32,
        cy: i32,
        radius: i32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let n = self.d6m.provinces.len();
        if prov as usize > n {
            return None;
        }
        let mut changes = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let Some(i) = self.brush_pixel(cx + dx, cy + dy) else {
                    continue;
                };
                let old = self.d6m.owners[i];
                if old as i32 == prov as i32 {
                    continue;
                }
                changes.push((i as u32, old, prov as i16));
            }
        }
        if changes.is_empty() {
            return None;
        }
        let r = self.brush_rect(cx, cy, radius);
        let step = Edit {
            owners: changes,
            rect: Some(r),
            ..Default::default()
        };
        self.commit(&step, false, tex, opts);
        if let Some(s) = &mut self.stroke {
            s.owners.extend(step.owners);
            s.rect = union(s.rect, Some(r));
        } else {
            self.redo.clear();
            self.undo.push(Edit {
                label: "Paint area".to_string(),
                ..step
            });
        }
        Some(r)
    }

    pub fn paint_restore(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let mut changes = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let Some(i) = self.brush_pixel(cx + dx, cy + dy) else {
                    continue;
                };
                let old = self.d6m.owners[i];
                let back = self.baseline[i];
                if old == back {
                    continue;
                }
                changes.push((i as u32, old, back));
            }
        }
        if changes.is_empty() {
            return None;
        }
        let r = self.brush_rect(cx, cy, radius);
        let step = Edit {
            owners: changes,
            rect: Some(r),
            ..Default::default()
        };
        self.commit(&step, false, tex, opts);
        if let Some(s) = &mut self.stroke {
            s.owners.extend(step.owners);
            s.rect = union(s.rect, Some(r));
        } else {
            self.redo.clear();
            self.undo.push(Edit {
                label: "Remove area".to_string(),
                ..step
            });
        }
        Some(r)
    }

    pub fn paint_height_stamps(
        &mut self,
        stamps: &[(i32, i32, f32)],
        radius: i32,
        keep_rivers: bool,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let r2 = (radius * radius) as f32;
        let mut all = Vec::new();
        let mut rect: Option<Rect> = None;
        for &(cx, cy, delta) in stamps {
            let mut changes = Vec::new();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let d2 = (dx * dx + dy * dy) as f32;
                    if d2 > r2 {
                        continue;
                    }
                    let Some(i) = self.brush_pixel(cx + dx, cy + dy) else {
                        continue;
                    };
                    if self.d6m.owners[i] <= 0 {
                        continue;
                    }
                    let old = self.d6m.heights[i];
                    if keep_rivers && old <= -STORED_LIMIT {
                        continue;
                    }
                    let t = 1.0 - d2 / r2.max(1.0);
                    let falloff = t * t;
                    let new = stored_from_units(units_from_stored(old) + delta * falloff);
                    if new != old {
                        changes.push((i as u32, old, new));
                    }
                }
            }
            if changes.is_empty() {
                continue;
            }
            let r = self.brush_rect(cx, cy, radius);
            let step = Edit {
                heights: changes,
                rect: Some(r),
                ..Default::default()
            };
            self.perform(&step, false);
            all.extend(step.heights);
            rect = union(rect, Some(r));
        }
        let r = self.seam_rect(rect?);
        let mut rd = std::mem::replace(&mut self.rendered, Rendered::empty());
        if self.stroke.is_some() {
            rd.render_quick(&self.plane(), tex, opts, r);
        } else {
            rd.render(&self.plane(), tex, opts, r);
        }
        self.rendered = rd;
        if let Some(s) = &mut self.stroke {
            s.heights.extend(all);
            s.rect = union(s.rect, Some(r));
        } else {
            self.redo.clear();
            self.undo.push(Edit {
                label: "Height brush".to_string(),
                heights: all,
                rect: Some(r),
                ..Default::default()
            });
        }
        Some(r)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_height_path(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        radius: i32,
        step: f32,
        keep_rivers: bool,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let r = radius.max(1) as f32;
        let (ax, ay) = (from.0 as f32, from.1 as f32);
        let (bx, by) = (to.0 as f32, to.1 as f32);
        let (vx, vy) = (bx - ax, by - ay);
        let len = (vx * vx + vy * vy).sqrt();
        if len < 0.5 {
            return None;
        }
        let (ux, uy) = (vx / len, vy / len);
        let k = step / (2.0 * r);
        let r2 = r * r;
        let r4 = r2 * r2;
        let x0 = ax.min(bx).floor() as i32 - radius;
        let x1 = ax.max(bx).ceil() as i32 + radius;
        let y0 = ay.min(by).floor() as i32 - radius;
        let y1 = ay.max(by).ceil() as i32 + radius;
        let f =
            |s: f32, c: f32| c * c * s - 2.0 * c * s * s * s / (3.0 * r2) + s.powi(5) / (5.0 * r4);
        let mut changes = Vec::new();
        for py in y0..=y1 {
            for px in x0..=x1 {
                let Some(i) = self.brush_pixel(px, py) else {
                    continue;
                };
                if self.d6m.owners[i] <= 0 {
                    continue;
                }
                let (dx, dy) = (px as f32 - ax, py as f32 - ay);
                let t = dx * ux + dy * uy;
                let d2 = (dx * dx + dy * dy - t * t).max(0.0);
                if d2 >= r2 {
                    continue;
                }
                let half = (r2 - d2).sqrt();
                let lo = (-t).max(-half);
                let hi = (len - t).min(half);
                if hi <= lo {
                    continue;
                }
                let c = 1.0 - d2 / r2;
                let delta = k * (f(hi, c) - f(lo, c));
                let old = self.d6m.heights[i];
                if keep_rivers && old <= -STORED_LIMIT {
                    continue;
                }
                let new = stored_from_units(units_from_stored(old) + delta);
                if new != old {
                    changes.push((i as u32, old, new));
                }
            }
        }
        if changes.is_empty() {
            return None;
        }
        let rect = self.path_rect(from, to, radius);
        let step_edit = Edit {
            heights: changes,
            rect: Some(rect),
            ..Default::default()
        };
        self.perform(&step_edit, false);
        let r = self.seam_rect(rect);
        let mut rd = std::mem::replace(&mut self.rendered, Rendered::empty());
        if self.stroke.is_some() {
            rd.render_quick(&self.plane(), tex, opts, r);
        } else {
            rd.render(&self.plane(), tex, opts, r);
        }
        self.rendered = rd;
        if let Some(s) = &mut self.stroke {
            s.heights.extend(step_edit.heights);
            s.rect = union(s.rect, Some(r));
        } else {
            self.redo.clear();
            self.undo.push(Edit {
                label: "Height brush".to_string(),
                heights: step_edit.heights,
                rect: Some(r),
                ..Default::default()
            });
        }
        Some(r)
    }

    fn path_rect(&self, from: (i32, i32), to: (i32, i32), radius: i32) -> Rect {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let (lx, hx) = (from.0.min(to.0) - radius, from.0.max(to.0) + radius);
        let (ly, hy) = (from.1.min(to.1) - radius, from.1.max(to.1) + radius);
        let (x0, x1) = if self.hwrap() && (lx < 0 || hx >= w) {
            (0, w - 1)
        } else {
            (lx.max(0), hx.min(w - 1))
        };
        let (y0, y1) = if self.vwrap() && (ly < 0 || hy >= h) {
            (0, h - 1)
        } else {
            (ly.max(0), hy.min(h - 1))
        };
        Rect { x0, y0, x1, y1 }
    }

    pub fn paint_height(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        delta: f32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        self.paint_height_stamps(&[(cx, cy, delta)], radius, true, tex, opts)
    }

    pub fn randomize_terrain(&mut self, tex: &TexSet, opts: &Options) -> usize {
        const KEEP: u64 = 0xffff_fffb_ffc0_1e0f;
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let mut rng = Rng::new(seed);
        let n = self.d6m.provinces.len();
        let mut edit = Edit {
            label: "Random terrain".to_string(),
            rect: Some(Rect::full(self.d6m.width, self.d6m.height)),
            ..Default::default()
        };
        for p in 1..=n as u32 {
            let old = self.flags[p as usize];
            if old & terrain::CAVE_WALL != 0 {
                continue;
            }
            let mut f = old & KEEP;
            if f & SEA == 0 {
                if f & terrain::CAVE == 0 {
                    let coastal = self
                        .neighbours(p)
                        .iter()
                        .any(|&q| self.flags.get(q as usize).copied().unwrap_or(0) & SEA != 0);
                    let r = rng.below(100);
                    if !coastal {
                        if r < 25 {
                            f |= terrain::FOREST;
                        } else if rng.below(125) < 15 {
                            f |= terrain::FARM;
                        } else if rng.below(50) < 5 {
                            f |= terrain::WASTE;
                        } else if rng.below(200) < 8 {
                            f |= terrain::SWAMP;
                        } else if rng.below(50) < 10 {
                            f |= terrain::HIGHLAND;
                        }
                    } else if r < 20 {
                        f |= terrain::FOREST;
                    } else if rng.below(75) < 15 {
                        f |= terrain::FARM;
                    } else if rng.below(200) < 5 {
                        f |= terrain::WASTE;
                    } else if rng.below(50) < 8 {
                        f |= terrain::SWAMP;
                    } else if rng.below(200) < 10 {
                        f |= terrain::HIGHLAND;
                    }
                    if rng.below(100) < 25 {
                        f |= 1 << 61;
                    }
                    if rng.below(100) < 50 {
                        f |= 1 << 62;
                    }
                } else if rng.below(100) < 15 {
                    f |= terrain::FOREST;
                } else if rng.below(100) < 20 {
                    f |= terrain::HIGHLAND;
                } else if rng.below(100) < 28 {
                    f |= terrain::SWAMP;
                }
            } else {
                let r = rng.below(100);
                if f & terrain::DEEP_SEA == 0 {
                    if r < 25 {
                        f |= terrain::FOREST;
                    }
                } else if r < 20 {
                    f |= terrain::HIGHLAND;
                }
            }
            if rng.below(100) < 5 {
                f |= 1u64 << (13 + rng.below(9));
            }
            if f != old {
                edit.map.push(MapChange::Terrain { p, old, new: f });
            }
        }
        let count = edit.map.len();
        if self.push(edit, tex, opts) {
            count
        } else {
            0
        }
    }

    pub fn connection_score(&self, prov: u32, crossing: f32) -> f32 {
        let mut score = 0.0;
        let wet = self.flags.get(prov as usize).copied().unwrap_or(0) & SEA != 0;
        for nb in self.neighbours(prov) {
            let other_wet = self.flags.get(nb as usize).copied().unwrap_or(0) & SEA != 0;
            if other_wet != wet {
                continue;
            }
            let spec = self.spec(prov, nb) as u64;
            if spec & terrain::BORDER_IMPASSABLE != 0 {
                continue;
            }
            let river = spec & terrain::BORDER_RIVER != 0 && spec & terrain::BORDER_BRIDGE == 0;
            let pass = spec & terrain::BORDER_MOUNTAIN_PASS != 0;
            score += if river || pass { crossing } else { 1.0 };
        }
        score
    }

    pub fn set_no_starts(
        &mut self,
        min_connections: f32,
        crossing: f32,
        tex: &TexSet,
        opts: &Options,
    ) -> usize {
        let n = self.d6m.provinces.len();
        let mut edit = Edit {
            label: "No start below connection count".to_string(),
            rect: Some(Rect::full(self.d6m.width, self.d6m.height)),
            ..Default::default()
        };
        for p in 1..=n as u32 {
            let old = self.flags[p as usize];
            if old & (UNKNOWN | terrain::CAVE_WALL) != 0 {
                continue;
            }
            if self.connection_score(p, crossing) >= min_connections {
                continue;
            }
            let new = (old | terrain::NO_START) & !terrain::GOOD_START;
            if new != old {
                edit.map.push(MapChange::Terrain { p, old, new });
            }
        }
        let count = edit.map.len();
        if self.push(edit, tex, opts) {
            count
        } else {
            0
        }
    }

    pub fn clear_no_starts(&mut self, tex: &TexSet, opts: &Options) -> usize {
        let n = self.d6m.provinces.len();
        let mut edit = Edit {
            label: "Clear no start".to_string(),
            rect: Some(Rect::full(self.d6m.width, self.d6m.height)),
            ..Default::default()
        };
        for p in 1..=n as u32 {
            let old = self.flags[p as usize];
            if old & terrain::NO_START == 0 {
                continue;
            }
            edit.map.push(MapChange::Terrain {
                p,
                old,
                new: old & !terrain::NO_START,
            });
        }
        let count = edit.map.len();
        if self.push(edit, tex, opts) {
            count
        } else {
            0
        }
    }

    pub fn refresh_decor(&mut self, rect: Rect, tex: &TexSet, opts: &Options) -> Option<Rect> {
        if !opts.decor || rect.is_empty() {
            return None;
        }
        let rect = self.seam_rect(rect);
        let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
        r.refresh_decor(&self.plane(), tex, opts, rect);
        self.rendered = r;
        Some(self.rendered.touched)
    }

    pub fn terrain_for_heights(&self, p: u32) -> Option<u64> {
        let b = self.bbox(p)?;
        let w = self.d6m.width;
        let mut hs: Vec<i16> = Vec::new();
        for y in b.y0..=b.y1 {
            let row = (y * w) as usize;
            for x in b.x0..=b.x1 {
                let i = row + x as usize;
                let h = self.d6m.heights[i];
                if self.d6m.owners[i] as u32 == p && h > -STORED_LIMIT {
                    hs.push(h);
                }
            }
        }
        if hs.is_empty() {
            return None;
        }
        let mid = hs.len() / 2;
        let (_, median, _) = hs.select_nth_unstable(mid);
        let units = units_from_stored(*median);
        let old = *self.flags.get(p as usize)?;
        Some(if units < 0.0 {
            terrain::make_sea(old, units <= DEEP_SEA_FROM)
        } else {
            terrain::make_land(old)
        })
    }

    fn terrain_changes(
        &mut self,
        provs: &[u32],
        tex: &TexSet,
        opts: &Options,
        became: &mut Vec<(u32, u64)>,
    ) -> (Vec<MapChange>, Option<Rect>) {
        let mut extra = Edit::default();
        let mut rect = None;
        for &p in provs {
            let Some(new) = self.terrain_for_heights(p) else {
                continue;
            };
            let old = self.flags[p as usize];
            if new != old {
                extra.map.push(MapChange::Terrain { p, old, new });
                rect = union(rect, self.bbox(p));
                became.push((p, new));
            }
        }
        if extra.map.is_empty() {
            return (Vec::new(), None);
        }
        self.commit(&extra, false, tex, opts);
        (extra.map, rect)
    }

    pub fn follow_terrain_in(
        &mut self,
        rect: Rect,
        tex: &TexSet,
        opts: &Options,
    ) -> (Option<Rect>, Vec<(u32, u64)>) {
        if self.stroke.is_none() {
            return (None, Vec::new());
        }
        let provs: Vec<u32> = (1..=self.d6m.provinces.len() as u32)
            .filter(|&p| self.bbox(p).is_some_and(|b| b.intersects(rect)))
            .collect();
        let mut became = Vec::new();
        let (extra, r) = self.terrain_changes(&provs, tex, opts, &mut became);
        if let Some(s) = &mut self.stroke {
            s.map.extend(extra);
            s.rect = union(s.rect, r);
        }
        (r, became)
    }

    pub fn paint_end(&mut self, tex: &TexSet, opts: &Options) -> Option<Rect> {
        self.paint_end_follow(false, tex, opts).0
    }

    pub fn paint_end_follow(
        &mut self,
        follow: bool,
        tex: &TexSet,
        opts: &Options,
    ) -> (Option<Rect>, Vec<(u32, u64)>) {
        let Some(mut s) = self.stroke.take() else {
            return (None, Vec::new());
        };
        if s.is_empty() {
            return (None, Vec::new());
        }
        let mut became = Vec::new();
        if follow && !s.heights.is_empty() {
            let mut provs: Vec<u32> = s
                .heights
                .iter()
                .map(|&(i, _, _)| self.d6m.owners[i as usize] as u32)
                .filter(|&p| p > 0)
                .collect();
            provs.sort_unstable();
            provs.dedup();
            let (extra, r) = self.terrain_changes(&provs, tex, opts, &mut became);
            s.map.extend(extra);
            s.rect = union(s.rect, r);
        }
        let Some(rect) = s.rect else {
            return (None, became);
        };
        let rect = self.seam_rect(rect);
        self.redo.clear();
        self.undo.push(s);
        let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
        r.refresh_decor(&self.plane(), tex, opts, rect);
        self.rendered = r;
        (Some(self.rendered.touched), became)
    }

    pub fn undo_all(&mut self, tex: &TexSet, opts: &Options) -> usize {
        let n = self.undo.len();
        if n == 0 {
            return 0;
        }
        while let Some(e) = self.undo.pop() {
            self.perform(&e, true);
            self.redo.push(e);
        }
        self.rerender(tex, opts);
        n
    }

    fn push(&mut self, edit: Edit, tex: &TexSet, opts: &Options) -> bool {
        if edit.is_empty() {
            return false;
        }
        self.redo.clear();
        self.commit(&edit, false, tex, opts);
        self.undo.push(edit);
        true
    }

    fn seam_rect(&self, r: Rect) -> Rect {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let pad = self.rendered.margin() * 2 + 16;
        let (x0, x1) = if self.hwrap() && (r.x0 < pad || r.x1 >= w - pad) {
            (0, w - 1)
        } else {
            (r.x0, r.x1)
        };
        let (y0, y1) = if self.vwrap() && (r.y0 < pad || r.y1 >= h - pad) {
            (0, h - 1)
        } else {
            (r.y0, r.y1)
        };
        Rect { x0, y0, x1, y1 }
    }

    fn commit(&mut self, e: &Edit, reverse: bool, tex: &TexSet, opts: &Options) {
        let Some(rect) = self.perform(e, reverse) else {
            return;
        };
        if rect.is_empty() {
            return;
        }
        let rect = self.seam_rect(rect);
        let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
        if self.stroke.is_some() {
            r.render_quick(&self.plane(), tex, opts, rect);
        } else {
            r.render(&self.plane(), tex, opts, rect);
        }
        self.rendered = r;
    }

    fn perform(&mut self, e: &Edit, reverse: bool) -> Option<Rect> {
        let mut rect = e.rect;
        let mut full = false;
        let order = |n: usize| -> Box<dyn Iterator<Item = usize>> {
            if reverse {
                Box::new((0..n).rev())
            } else {
                Box::new(0..n)
            }
        };
        for k in order(e.heights.len()) {
            let (i, old, new) = e.heights[k];
            let v = if reverse { old } else { new };
            self.d6m.heights[i as usize] = v;
            self.heights[i as usize] = units_from_stored(v);
        }
        let mut renumbered = false;
        for c in &e.map {
            match c {
                MapChange::AddProvince { p, x, y, terrain } if !reverse => {
                    self.grow_province(*p, *x, *y, *terrain);
                }
                MapChange::RemoveProvince {
                    p,
                    x,
                    y,
                    terrain,
                    name,
                    gate,
                    links,
                    pixels,
                    baseline,
                } => {
                    if reverse {
                        self.restore_province(
                            *p, *x, *y, *terrain, name, *gate, links, pixels, baseline,
                        );
                    } else {
                        self.drop_province(*p, pixels, baseline);
                    }
                    renumbered = true;
                }
                _ => {}
            }
        }
        if !e.owners.is_empty() {
            for k in order(e.owners.len()) {
                let (i, old, new) = e.owners[k];
                let v = if reverse { old } else { new };
                let prev = self.d6m.owners[i as usize];
                if prev > 0 && (prev as usize) < self.pixel_counts.len() {
                    self.pixel_counts[prev as usize] -= 1;
                }
                if v > 0 && (v as usize) < self.pixel_counts.len() {
                    self.pixel_counts[v as usize] += 1;
                }
                self.d6m.owners[i as usize] = v;
            }
            self.owners_changed = true;
            self.rendered.bboxes = province_bboxes(&self.plane());
        }
        for c in &e.map {
            match c {
                MapChange::Terrain { p, old, new } => {
                    let v = if reverse { *old } else { *new };
                    self.flags[*p as usize] = v;
                    if let Some(rec) = self.d6m.provinces.get_mut(*p as usize - 1) {
                        rec.terrain = v as i64;
                    }
                    if let Some(m) = &mut self.map {
                        m.set_terrain(*p, v as i64);
                    }
                    if (old ^ new) & UNKNOWN != 0 {
                        full = true;
                    } else {
                        rect = union(rect, self.bbox(*p));
                    }
                }
                MapChange::Name { p, old, new } => {
                    let v = if reverse { old } else { new };
                    self.names[*p as usize] = v.clone().unwrap_or_default();
                    if let Some(m) = &mut self.map {
                        m.set_name(*p, v.as_deref());
                    }
                }
                MapChange::Gate { p, old, new } => {
                    let v = if reverse { *old } else { *new };
                    self.gates[*p as usize] = v;
                    if let Some(m) = &mut self.map {
                        m.set_gate(*p, v);
                    }
                }
                MapChange::Neighbour { a, b, old, new } => {
                    let v = if reverse { *old } else { *new };
                    if let Some(m) = &mut self.map {
                        m.set_neighbour(*a, *b, v);
                    }
                    rect = union(rect, union(self.bbox(*a), self.bbox(*b)));
                }
                MapChange::Spec { a, b, old, new } => {
                    let v = if reverse { *old } else { *new };
                    if let Some(m) = &mut self.map {
                        m.set_spec(*a, *b, v);
                    }
                    rect = union(rect, union(self.bbox(*a), self.bbox(*b)));
                }
                MapChange::Capital { p, old, new } => {
                    let v = if reverse { *old } else { *new };
                    let i = *p as usize - 1;
                    if let Some(c) = self.capitals.get_mut(i) {
                        *c = v;
                    }
                    if let Some(rec) = self.d6m.provinces.get_mut(i) {
                        rec.x = v.0;
                        rec.y = v.1;
                    }
                }
                MapChange::AddProvince { .. } | MapChange::RemoveProvince { .. } => {}
            }
        }
        if renumbered {
            full = true;
            self.rendered.bboxes = province_bboxes(&self.plane());
        }
        if reverse {
            for c in e.map.iter().rev() {
                if let MapChange::AddProvince { p, .. } = c {
                    self.shrink_province(*p);
                }
            }
            if e.map
                .iter()
                .any(|c| matches!(c, MapChange::AddProvince { .. }))
            {
                self.rendered.bboxes = province_bboxes(&self.plane());
            }
        }
        if !e.map.is_empty() {
            self.rebuild_links();
        }
        self.dirty = true;
        self.edited = true;
        let pixels = !e.heights.is_empty() || !e.owners.is_empty();
        if full || (pixels && rect.is_none()) {
            Some(Rect::full(self.d6m.width, self.d6m.height))
        } else {
            rect
        }
    }

    pub fn undo_last(&mut self, tex: &TexSet, opts: &Options) -> Option<String> {
        let e = self.undo.pop()?;
        self.commit(&e, true, tex, opts);
        let label = e.label.clone();
        self.redo.push(e);
        Some(label)
    }

    pub fn redo_last(&mut self, tex: &TexSet, opts: &Options) -> Option<String> {
        let e = self.redo.pop()?;
        self.commit(&e, false, tex, opts);
        let label = e.label.clone();
        self.undo.push(e);
        Some(label)
    }

    pub fn rerender(&mut self, tex: &TexSet, opts: &Options) {
        let rect = Rect::full(self.d6m.width, self.d6m.height);
        let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
        r.render(&self.plane(), tex, opts, rect);
        self.rendered = r;
    }

    pub fn save(&mut self) -> Result<Vec<PathBuf>, String> {
        let empty = self.empty_provinces();
        if let Some(p) = empty.first() {
            return Err(format!(
                "Province {p} has no area. Paint some pixels for it or undo before saving"
            ));
        }
        let mut written = Vec::new();
        let bytes = self.d6m.to_bytes();
        write_replace(&self.d6m_path, &bytes)?;
        written.push(self.d6m_path.clone());
        if let (Some(m), Some(p)) = (&mut self.map, &self.map_path) {
            if self.owners_changed && m.has_pb() {
                m.replace_pb(self.d6m.width, self.d6m.height, &self.d6m.owners);
            }
            if m.modified {
                write_replace(p, m.to_text().as_bytes())?;
                m.modified = false;
                written.push(p.clone());
            }
        }
        self.owners_changed = false;
        self.dirty = false;
        self.edited = false;
        Ok(written)
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.display()))
}

pub fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::io::write(path, bytes)
}

pub fn write_with_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let bak = backup_path(path);
    if crate::io::exists(path) && !crate::io::exists(&bak) {
        crate::io::copy(path, &bak).map_err(|e| format!("backup {e}"))?;
    }
    crate::io::write(path, bytes)
}

pub struct Project {
    pub dir: PathBuf,
    pub base: String,
    pub planes: Vec<PlaneDoc>,
    pub notes: Vec<String>,
    pub unsaved: bool,
}

impl Project {
    pub fn open(path: &Path, tex: &TexSet, opts: &Options) -> Result<Project, String> {
        let dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("bad file name")?
            .to_string();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut base = strip_plane_suffix(&stem).0;
        if ext == "map" {
            let m = MapFile::load(path).map_err(|e| e.to_string())?;
            if let Some(img) = &m.imagefile {
                let img_path = Path::new(img);
                let istem = img_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&stem)
                    .to_string();
                let iext = img_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if iext != "d6m" {
                    return Err(format!(
                        "{} uses an image map ({img}); only .d6m recipes carry height data",
                        path.display()
                    ));
                }
                base = strip_plane_suffix(&istem).0;
            }
        } else if ext != "d6m" {
            return Err(format!("{} is not a .d6m or .map file", path.display()));
        }
        let mut planes = Vec::new();
        let mut notes = Vec::new();
        for plane in 1..=9u32 {
            let d6m_path = dir.join(plane_file_name(&base, plane, "d6m"));
            if !crate::io::exists(&d6m_path) {
                continue;
            }
            let d6m = D6m::load(&d6m_path).map_err(|e| format!("{}: {e}", d6m_path.display()))?;
            let map_path = dir.join(plane_file_name(&base, plane, "map"));
            let map = if crate::io::exists(&map_path) {
                match MapFile::load(&map_path) {
                    Ok(m) => Some(m),
                    Err(e) => {
                        notes.push(format!("{}: {e}", map_path.display()));
                        None
                    }
                }
            } else {
                notes.push(format!(
                    "no {} beside the recipe; terrain flags taken from the .d6m snapshot",
                    map_path.file_name().unwrap().to_string_lossy()
                ));
                None
            };
            let map_path = map.as_ref().map(|_| map_path);
            planes.push(PlaneDoc::build(
                plane, d6m_path, map_path, d6m, map, tex, opts,
            ));
        }
        if planes.is_empty() {
            return Err(format!("no {}.d6m found in {}", base, dir.display()));
        }
        Ok(Project {
            dir,
            base,
            planes,
            notes,
            unsaved: false,
        })
    }

    pub fn from_generated(
        dir: PathBuf,
        name: &str,
        planes: &[(&[u8], &str)],
        gates: &[(u16, u16)],
        tex: &TexSet,
        opts: &Options,
    ) -> Result<Project, String> {
        let base = strip_plane_suffix(name).0;
        if base.is_empty() {
            return Err("a generated map needs a name".to_string());
        }
        if planes.is_empty() {
            return Err("a generated map needs at least one plane".to_string());
        }
        if planes.len() > 9 {
            return Err("a map can hold at most 9 planes".to_string());
        }
        let mut parsed = Vec::new();
        for (i, (bytes, text)) in planes.iter().enumerate() {
            let index = i as u32 + 1;
            let d6m_name = plane_file_name(&base, index, "d6m");
            let d6m_path = dir.join(&d6m_name);
            let map_path = dir.join(plane_file_name(&base, index, "map"));
            let d6m = D6m::parse(bytes).map_err(|e| format!("{d6m_name}: {e}"))?;
            let mut map = MapFile::parse(text, &map_path);
            map.set_imagefile(&d6m_name);
            if index > 1 {
                map.set_title(&format!("{base} plane {index}"));
            }
            parsed.push((index, d6m_path, map_path, d6m, map));
        }
        if parsed.len() > 1 {
            for (n, (surface, cave)) in gates.iter().enumerate() {
                let n = n as i32 + 1;
                parsed[0].4.set_gate(*surface as u32, n);
                parsed[1].4.set_gate(*cave as u32, n);
            }
        }
        let mut docs = Vec::new();
        for (index, d6m_path, map_path, d6m, mut map) in parsed {
            map.modified = true;
            let mut doc =
                PlaneDoc::build(index, d6m_path, Some(map_path), d6m, Some(map), tex, opts);
            doc.dirty = true;
            docs.push(doc);
        }
        Ok(Project {
            dir,
            base,
            planes: docs,
            notes: Vec::new(),
            unsaved: true,
        })
    }

    pub fn retarget(&mut self, dir: PathBuf, base: &str) {
        let base = strip_plane_suffix(base).0;
        for d in &mut self.planes {
            let d6m_name = plane_file_name(&base, d.index, "d6m");
            let map_name = plane_file_name(&base, d.index, "map");
            d.d6m_path = dir.join(&d6m_name);
            let map_path = dir.join(&map_name);
            if let Some(m) = &mut d.map {
                m.path = map_path.clone();
                m.set_imagefile(&d6m_name);
                if d.index == 1 {
                    m.set_title(&base);
                }
                m.modified = true;
            }
            d.map_path = Some(map_path);
            d.dirty = true;
        }
        self.dir = dir;
        self.base = base;
        self.unsaved = false;
    }

    pub fn rename(&mut self, base: &str) {
        let base = strip_plane_suffix(base).0;
        if base.is_empty() || base == self.base {
            return;
        }
        if self.unsaved {
            for d in &mut self.planes {
                if let Some(m) = &mut d.map {
                    if d.index == 1 {
                        m.set_title(&base);
                    }
                    m.modified = true;
                }
                d.dirty = true;
            }
            self.base = base;
        } else {
            let dir = self.dir.clone();
            self.retarget(dir, &base);
        }
    }

    pub fn any_dirty(&self) -> bool {
        self.planes.iter().any(|p| p.dirty)
    }

    pub fn scar_total(&self) -> usize {
        self.planes.iter().map(|p| p.scar_count()).sum()
    }

    pub fn next_plane_index(&self) -> u32 {
        self.planes.iter().map(|p| p.index).max().unwrap_or(0) + 1
    }

    pub fn add_plane(
        &mut self,
        source: &Path,
        tex: &TexSet,
        opts: &Options,
    ) -> Result<u32, String> {
        let n = self.next_plane_index();
        if n > 9 {
            return Err("a map can hold at most 9 planes".to_string());
        }
        let ext = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let (src_d6m, src_map) = if ext == "map" {
            let m = MapFile::load(source).map_err(|e| e.to_string())?;
            let img = m
                .imagefile
                .clone()
                .ok_or("the .map has no #imagefile line")?;
            let d = source.with_file_name(&img);
            (d, Some(source.to_path_buf()))
        } else if ext == "d6m" {
            let m = source.with_extension("map");
            (
                source.to_path_buf(),
                if crate::io::exists(&m) { Some(m) } else { None },
            )
        } else {
            return Err(format!("{} is not a .d6m or .map file", source.display()));
        };
        let d6m = D6m::load(&src_d6m).map_err(|e| format!("{}: {e}", src_d6m.display()))?;
        let d6m_name = plane_file_name(&self.base, n, "d6m");
        let map_name = plane_file_name(&self.base, n, "map");
        let d6m_path = self.dir.join(&d6m_name);
        let map_path = self.dir.join(&map_name);
        if crate::io::exists(&d6m_path) || crate::io::exists(&map_path) {
            return Err(format!("{d6m_name} or {map_name} already exists"));
        }
        let mut map = match src_map {
            Some(mp) => MapFile::load(&mp).map_err(|e| format!("{}: {e}", mp.display()))?,
            None => MapFile::parse(&generated_map_text(&self.base, &d6m), &map_path),
        };
        map.path = map_path.clone();
        map.set_imagefile(&d6m_name);
        map.set_title(&format!("{} plane {n}", self.base));
        crate::io::copy(&src_d6m, &d6m_path)?;
        crate::io::write(&map_path, map.to_text().as_bytes())?;
        map.modified = false;
        self.planes.push(PlaneDoc::build(
            n,
            d6m_path,
            Some(map_path),
            d6m,
            Some(map),
            tex,
            opts,
        ));
        Ok(n)
    }

    pub fn remove_last_plane(&mut self) -> Result<Vec<PathBuf>, String> {
        if self.planes.len() < 2 {
            return Err("the surface plane cannot be removed".to_string());
        }
        let doc = self.planes.last().unwrap();
        let mut moved = Vec::new();
        for path in std::iter::once(&doc.d6m_path).chain(doc.map_path.iter()) {
            let parked = PathBuf::from(format!("{}.removed", path.display()));
            crate::io::rename(path, &parked)?;
            moved.push(parked);
        }
        self.planes.pop();
        Ok(moved)
    }

    pub fn gateway_ring(&self, gate: i32) -> Vec<(usize, u32)> {
        if gate == 0 {
            return Vec::new();
        }
        let mut ring = Vec::new();
        for (i, doc) in self.planes.iter().enumerate() {
            for prov in 1..=doc.province_count() as u32 {
                if doc.gate(prov) == gate {
                    ring.push((i, prov));
                }
            }
        }
        ring
    }

    pub fn next_gateway(&self, plane: usize, prov: u32) -> Option<(usize, u32)> {
        let gate = self.planes.get(plane)?.gate(prov);
        let ring = self.gateway_ring(gate);
        let here = ring.iter().position(|&e| e == (plane, prov))?;
        let next = ring[(here + 1) % ring.len()];
        (next != (plane, prov)).then_some(next)
    }
}

pub fn generated_map_text(base: &str, d6m: &D6m) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "#dom2title {base}\n#imagefile {base}.d6m\n#mapsize {} {}\n\n",
        d6m.width, d6m.height
    ));
    for (i, p) in d6m.provinces.iter().enumerate() {
        text.push_str(&format!("#terrain {} {}\n", i + 1, p.terrain));
    }
    text.push('\n');
    let w = d6m.width as usize;
    let h = d6m.height as usize;
    let n = d6m.provinces.len() as i16;
    let mut pairs = std::collections::BTreeSet::new();
    let own = |x: usize, y: usize| d6m.owners[y * w + x];
    for y in 0..h {
        for x in 0..w {
            let a = own(x, y);
            if a <= 0 || a > n {
                continue;
            }
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx >= w || ny >= h {
                    continue;
                }
                let b = own(nx, ny);
                if b > 0 && b <= n && b != a {
                    pairs.insert((a.min(b), a.max(b)));
                }
            }
        }
    }
    for (a, b) in pairs {
        text.push_str(&format!("#neighbour {a} {b}\n"));
    }
    text
}

pub fn fill_removed(
    w: i32,
    h: i32,
    owners: &[i16],
    heights: &[i16],
    p: u32,
) -> (Vec<usize>, Vec<i16>) {
    let mut idx: Vec<usize> = Vec::new();
    let mut pos: Vec<usize> = vec![usize::MAX; owners.len()];
    for (i, &o) in owners.iter().enumerate() {
        if o as u32 == p {
            pos[i] = idx.len();
            idx.push(i);
        }
    }
    let mut fill: Vec<i16> = vec![0; idx.len()];
    let water = |i: usize| heights.get(i).is_some_and(|&z| z < 0);
    let near = |i: usize| -> [Option<usize>; 8] {
        let x = (i as i32) % w;
        let y = (i as i32) / w;
        let at = |dx: i32, dy: i32| {
            let nx = x + dx;
            let ny = y + dy;
            (nx >= 0 && nx < w && ny >= 0 && ny < h).then(|| (ny * w + nx) as usize)
        };
        [
            at(-1, 0),
            at(1, 0),
            at(0, -1),
            at(0, 1),
            at(-1, -1),
            at(1, -1),
            at(-1, 1),
            at(1, 1),
        ]
    };
    let pick = |v: &[i16]| -> Option<i16> {
        let mut best: Option<(usize, i16)> = None;
        for &o in v {
            let c = v.iter().filter(|&&q| q == o).count();
            let better = match best {
                None => true,
                Some((bc, bo)) => c > bc || (c == bc && o < bo),
            };
            if better {
                best = Some((c, o));
            }
        }
        best.map(|b| b.1)
    };
    let mut queue = std::collections::VecDeque::new();
    let mut cross: Vec<(usize, i16)> = Vec::new();
    for (k, &i) in idx.iter().enumerate() {
        let mut same: Vec<i16> = Vec::new();
        let mut other: Vec<i16> = Vec::new();
        for (n, j) in near(i).into_iter().enumerate() {
            let Some(j) = j else { continue };
            let o = owners[j];
            if o as u32 == p || o <= 0 {
                continue;
            }
            let weight = if n < 4 { 2 } else { 1 };
            let bucket = if water(j) == water(i) {
                &mut same
            } else {
                &mut other
            };
            for _ in 0..weight {
                bucket.push(o);
            }
        }
        if let Some(o) = pick(&same) {
            fill[k] = o;
            queue.push_back(i);
        } else if let Some(o) = pick(&other) {
            cross.push((i, o));
        }
    }
    let mut edge: Vec<usize> = Vec::new();
    while let Some(i) = queue.pop_front() {
        let o = fill[pos[i]];
        for j in near(i).into_iter().take(4).flatten() {
            let k = pos[j];
            if k == usize::MAX || fill[k] != 0 {
                continue;
            }
            if water(j) != water(i) {
                edge.push(i);
                continue;
            }
            fill[k] = o;
            queue.push_back(j);
        }
    }
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for (i, o) in cross {
        let k = pos[i];
        if fill[k] == 0 {
            fill[k] = o;
            queue.push_back(i);
        }
    }
    queue.extend(edge);
    while let Some(i) = queue.pop_front() {
        let o = fill[pos[i]];
        for j in near(i).into_iter().take(4).flatten() {
            let k = pos[j];
            if k != usize::MAX && fill[k] == 0 {
                fill[k] = o;
                queue.push_back(j);
            }
        }
    }
    (idx, fill)
}
