use crate::d6m::{stored_from_units, units_from_stored, D6m, STORED_LIMIT};
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
    pub owners_changed: bool,
    pub undo: Vec<Edit>,
    pub redo: Vec<Edit>,
    stroke: Option<Edit>,
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
            dirty: false,
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
        let new = name.trim().to_string();
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
        let w = self.d6m.width;
        let h = self.d6m.height;
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
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let i = (y * w + x) as usize;
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
        let r = Rect {
            x0: (cx - radius).max(0),
            y0: (cy - radius).max(0),
            x1: (cx + radius).min(w - 1),
            y1: (cy + radius).min(h - 1),
        };
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
        prov: u32,
        cx: i32,
        cy: i32,
        radius: i32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let mut changes = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let i = (y * w + x) as usize;
                let old = self.d6m.owners[i];
                if old as u32 != prov {
                    continue;
                }
                let back = self.baseline[i];
                let new = if back as u32 == prov { 0 } else { back };
                changes.push((i as u32, old, new));
            }
        }
        if changes.is_empty() {
            return None;
        }
        let r = Rect {
            x0: (cx - radius).max(0),
            y0: (cy - radius).max(0),
            x1: (cx + radius).min(w - 1),
            y1: (cy + radius).min(h - 1),
        };
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

    pub fn paint_height(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        delta: f32,
        tex: &TexSet,
        opts: &Options,
    ) -> Option<Rect> {
        let w = self.d6m.width;
        let h = self.d6m.height;
        let mut changes = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = (dx * dx + dy * dy) as f32;
                let r2 = (radius * radius) as f32;
                if d2 > r2 {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let i = (y * w + x) as usize;
                if self.d6m.owners[i] <= 0 {
                    continue;
                }
                let falloff = 1.0 - (d2 / r2.max(1.0)) * 0.5;
                let old = self.d6m.heights[i];
                let new = stored_from_units(units_from_stored(old) + delta * falloff);
                if new != old {
                    changes.push((i as u32, old, new));
                }
            }
        }
        if changes.is_empty() {
            return None;
        }
        let r = Rect {
            x0: (cx - radius).max(0),
            y0: (cy - radius).max(0),
            x1: (cx + radius).min(w - 1),
            y1: (cy + radius).min(h - 1),
        };
        let step = Edit {
            heights: changes,
            rect: Some(r),
            ..Default::default()
        };
        self.commit(&step, false, tex, opts);
        if let Some(s) = &mut self.stroke {
            s.heights.extend(step.heights);
            s.rect = union(s.rect, Some(r));
        } else {
            self.redo.clear();
            self.undo.push(Edit {
                label: "Height brush".to_string(),
                ..step
            });
        }
        Some(r)
    }

    pub fn randomize_terrain(&mut self, tex: &TexSet, opts: &Options) -> usize {
        const KEEP: u64 = 0xffff_fffb_ffc0_1e0f;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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

    pub fn paint_end(&mut self) {
        if let Some(s) = self.stroke.take() {
            if !s.is_empty() {
                self.redo.clear();
                self.undo.push(s);
            }
        }
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

    fn commit(&mut self, e: &Edit, reverse: bool, tex: &TexSet, opts: &Options) {
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
            }
        }
        if !e.map.is_empty() {
            self.rebuild_links();
        }
        self.dirty = true;
        let rect = if full {
            Rect::full(self.d6m.width, self.d6m.height)
        } else {
            rect.unwrap_or(Rect::full(self.d6m.width, self.d6m.height))
        };
        if rect.is_empty() {
            return;
        }
        let mut r = std::mem::replace(&mut self.rendered, Rendered::empty());
        r.render(&self.plane(), tex, opts, rect);
        self.rendered = r;
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
        let mut written = Vec::new();
        let bytes = self.d6m.to_bytes();
        write_with_backup(&self.d6m_path, &bytes)?;
        written.push(self.d6m_path.clone());
        if let (Some(m), Some(p)) = (&mut self.map, &self.map_path) {
            if self.owners_changed && m.has_pb() {
                m.replace_pb(self.d6m.width, self.d6m.height, &self.d6m.owners);
            }
            if m.modified {
                write_with_backup(p, m.to_text().as_bytes())?;
                m.modified = false;
                written.push(p.clone());
            }
        }
        self.owners_changed = false;
        self.dirty = false;
        Ok(written)
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.display()))
}

pub fn write_with_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let bak = backup_path(path);
    if path.exists() && !bak.exists() {
        std::fs::copy(path, &bak).map_err(|e| format!("backup {}: {e}", bak.display()))?;
    }
    let tmp = path.with_extension("tmp_write");
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("replace {}: {e}", path.display()))?;
    Ok(())
}

pub struct Project {
    pub dir: PathBuf,
    pub base: String,
    pub planes: Vec<PlaneDoc>,
    pub notes: Vec<String>,
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
            if !d6m_path.exists() {
                continue;
            }
            let d6m = D6m::load(&d6m_path).map_err(|e| format!("{}: {e}", d6m_path.display()))?;
            let map_path = dir.join(plane_file_name(&base, plane, "map"));
            let map = if map_path.exists() {
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
        })
    }

    pub fn any_dirty(&self) -> bool {
        self.planes.iter().any(|p| p.dirty)
    }

    pub fn scar_total(&self) -> usize {
        self.planes.iter().map(|p| p.scar_count()).sum()
    }

    pub fn export_image_map(&self) -> Result<Vec<PathBuf>, String> {
        let base = format!("{}_image", self.base);
        let mut written = Vec::new();
        for doc in &self.planes {
            let tga_name = plane_file_name(&base, doc.index, "tga");
            let map_name = plane_file_name(&base, doc.index, "map");
            let img = crate::textures::Image {
                w: doc.width() as usize,
                h: doc.height() as usize,
                rgba: doc.rendered.composed(&doc.capitals),
            };
            let tga_path = self.dir.join(&tga_name);
            std::fs::write(&tga_path, crate::tga::encode_rgba_bottom_up(&img))
                .map_err(|e| format!("write {}: {e}", tga_path.display()))?;
            written.push(tga_path);
            let map_path = self.dir.join(&map_name);
            let mut map = match &doc.map {
                Some(m) => m.clone(),
                None => MapFile::parse(&generated_map_text(&self.base, &doc.d6m), &map_path),
            };
            map.path = map_path.clone();
            map.set_imagefile(&tga_name);
            if doc.index == 1 {
                let title = if map.title.is_empty() {
                    self.base.clone()
                } else {
                    map.title.clone()
                };
                map.set_title(&format!("{title} (image)"));
            }
            map.replace_pb(doc.width(), doc.height(), &doc.d6m.owners);
            std::fs::write(&map_path, map.to_text())
                .map_err(|e| format!("write {}: {e}", map_path.display()))?;
            written.push(map_path);
        }
        Ok(written)
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
                if m.exists() { Some(m) } else { None },
            )
        } else {
            return Err(format!("{} is not a .d6m or .map file", source.display()));
        };
        let d6m = D6m::load(&src_d6m).map_err(|e| format!("{}: {e}", src_d6m.display()))?;
        let d6m_name = plane_file_name(&self.base, n, "d6m");
        let map_name = plane_file_name(&self.base, n, "map");
        let d6m_path = self.dir.join(&d6m_name);
        let map_path = self.dir.join(&map_name);
        if d6m_path.exists() || map_path.exists() {
            return Err(format!("{d6m_name} or {map_name} already exists"));
        }
        let mut map = match src_map {
            Some(mp) => MapFile::load(&mp).map_err(|e| format!("{}: {e}", mp.display()))?,
            None => MapFile::parse(&generated_map_text(&self.base, &d6m), &map_path),
        };
        map.path = map_path.clone();
        map.set_imagefile(&d6m_name);
        map.set_title(&format!("{} plane {n}", self.base));
        std::fs::copy(&src_d6m, &d6m_path)
            .map_err(|e| format!("copy {}: {e}", d6m_path.display()))?;
        std::fs::write(&map_path, map.to_text())
            .map_err(|e| format!("write {}: {e}", map_path.display()))?;
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
            std::fs::rename(path, &parked).map_err(|e| format!("move {}: {e}", path.display()))?;
            moved.push(parked);
        }
        self.planes.pop();
        Ok(moved)
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
