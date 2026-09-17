use crate::render::{province_winter, Plane, Rect};
use crate::terrain::{
    ALWAYS_WATER, CAVE, FARM, FOREST, HIGHLAND, KELP_EXACT, SEA, SWAMP, UNKNOWN, WASTE,
};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const LOOK_CHANGE_MASK: u64 = 0x4_0000_19fc;
const LAND_ONLY: u64 = CAVE | SEA;
const PLAIN_EXCLUDE: u64 = CAVE | SEA | HIGHLAND | SWAMP | WASTE | FOREST | FARM;

pub struct Pass {
    pub mask: u64,
    pub exclude: u64,
    pub summer: Option<&'static str>,
    pub winter: &'static str,
}

pub const PASSES: [Pass; 9] = [
    Pass {
        mask: KELP_EXACT,
        exclude: CAVE,
        summer: Some("_kelp"),
        winter: "_kelpw",
    },
    Pass {
        mask: ALWAYS_WATER,
        exclude: 0,
        summer: Some("_water"),
        winter: "_waterw",
    },
    Pass {
        mask: HIGHLAND,
        exclude: LAND_ONLY,
        summer: Some("_highland"),
        winter: "_highlandw",
    },
    Pass {
        mask: FOREST,
        exclude: LAND_ONLY,
        summer: Some("_forest"),
        winter: "_forestw",
    },
    Pass {
        mask: WASTE,
        exclude: LAND_ONLY,
        summer: Some("_waste"),
        winter: "_wastew",
    },
    Pass {
        mask: SWAMP,
        exclude: LAND_ONLY,
        summer: Some("_swamp"),
        winter: "_swampw",
    },
    Pass {
        mask: FARM,
        exclude: LAND_ONLY,
        summer: Some("_farm"),
        winter: "_farmw",
    },
    Pass {
        mask: 0,
        exclude: PLAIN_EXCLUDE,
        summer: Some("_plain"),
        winter: "_plainw",
    },
    Pass {
        mask: u64::MAX,
        exclude: 0,
        summer: None,
        winter: "_winter",
    },
];

pub fn picture_names(imagefile: &str) -> Vec<String> {
    let path = Path::new(imagefile);
    let (Some(stem), Some(ext)) = (
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|s| s.to_str()),
    ) else {
        return Vec::new();
    };
    if !ext.eq_ignore_ascii_case("tga") {
        return Vec::new();
    }
    let mut out = vec![imagefile.to_string()];
    for pass in &PASSES {
        if let Some(s) = pass.summer {
            out.push(format!("{stem}{s}.{ext}"));
        }
        out.push(format!("{stem}{}.{ext}", pass.winter));
    }
    out
}

pub const STRIPPED_WORDS: [&str; 17] = [
    "_forestw",
    "_forest",
    "_plainw",
    "_plain",
    "_kelpw",
    "_kelp",
    "_highlandw",
    "_highland",
    "_farmw",
    "_farm",
    "_swampw",
    "_swamp",
    "_wastew",
    "_waste",
    "_waterw",
    "_water",
    "_winter",
];

pub fn name_trap(imagefile: &str) -> Option<&'static str> {
    STRIPPED_WORDS
        .iter()
        .copied()
        .find(|w| imagefile.contains(w))
}

pub const WHITE: [u8; 3] = [255, 255, 255];
pub const NEAR_WHITE: [u8; 3] = [254, 254, 254];

pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

pub fn scan_key(w: usize, p: (i16, i16)) -> i64 {
    p.1 as i64 * w as i64 + p.0 as i64
}

pub fn slot_for(capitals: &[(i16, i16)], w: usize, skip: Option<u32>, at: (i16, i16)) -> u32 {
    let key = scan_key(w, at);
    let mut slot = 1u32;
    for (i, &c) in capitals.iter().enumerate() {
        if skip == Some(i as u32 + 1) {
            continue;
        }
        if scan_key(w, c) < key {
            slot += 1;
        }
    }
    slot
}

pub fn slot_after_move(from: u32, to: u32, q: u32) -> u32 {
    if q == 0 || from == to {
        q
    } else if q == from {
        to
    } else if from < to && q > from && q <= to {
        q - 1
    } else if to < from && q >= to && q < from {
        q + 1
    } else {
        q
    }
}

impl Rgb {
    pub fn from_tga(bytes: &[u8]) -> Result<Rgb, String> {
        let img = crate::tga::decode(bytes)?;
        let mut data = Vec::with_capacity(img.w * img.h * 3);
        for p in img.rgba.chunks_exact(4) {
            data.extend_from_slice(&p[..3]);
        }
        Ok(Rgb {
            w: img.w,
            h: img.h,
            data,
        })
    }

    pub fn tiled(&self, x: i32, y: i32) -> [u8; 3] {
        let o = ((y as usize % self.h) * self.w + x as usize % self.w) * 3;
        [self.data[o], self.data[o + 1], self.data[o + 2]]
    }

    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return None;
        }
        Some(y as usize * self.w + x as usize)
    }

    pub fn pixel(&self, x: i32, y: i32) -> [u8; 3] {
        match self.index(x, y) {
            Some(i) => [self.data[i * 3], self.data[i * 3 + 1], self.data[i * 3 + 2]],
            None => [0; 3],
        }
    }

    pub fn is_white(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some() && self.pixel(x, y) == WHITE
    }

    pub fn set_at(&mut self, i: usize, c: [u8; 3]) {
        self.data[i * 3..i * 3 + 3].copy_from_slice(&c);
    }

    pub fn unwhitened(&self, x: i32, y: i32) -> [u8; 3] {
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if self.index(x + dx, y + dy).is_none() {
                    continue;
                }
                let c = self.pixel(x + dx, y + dy);
                if c == WHITE {
                    continue;
                }
                for k in 0..3 {
                    sum[k] += c[k] as u32;
                }
                n += 1;
            }
        }
        if n == 0 {
            return NEAR_WHITE;
        }
        let c = [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8];
        if c == WHITE {
            NEAR_WHITE
        } else {
            c
        }
    }

    pub fn white_pixels(&self) -> Vec<(i16, i16)> {
        let mut out = Vec::new();
        for (i, p) in self.data.chunks_exact(3).enumerate() {
            if p == [255, 255, 255] {
                out.push(((i % self.w) as i16, (i / self.w) as i16));
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Base,
    BaseWhitened,
    Summer(usize, bool),
    Winter(usize),
}

pub fn whiten(c: u8) -> u8 {
    ((c as u32 * 0x9b + 0x639c) / 0xff) as u8
}

pub struct Looks {
    pub base: Rgb,
    pub base_path: PathBuf,
    pub bpp: u8,
    pub original: Vec<u64>,
    opened: Option<Vec<u8>>,
    summer_paths: Vec<Option<PathBuf>>,
    winter_paths: Vec<Option<PathBuf>>,
    summer: Vec<OnceLock<Option<Rgb>>>,
    winter: Vec<OnceLock<Option<Rgb>>>,
}

fn sibling(base: &Path, suffix: &str) -> Option<PathBuf> {
    let stem = base.file_stem()?.to_str()?;
    let ext = base.extension()?.to_str()?;
    let p = base.with_file_name(format!("{stem}{suffix}.{ext}"));
    crate::io::exists(&p).then_some(p)
}

fn fresh(n: usize) -> Vec<OnceLock<Option<Rgb>>> {
    (0..n).map(|_| OnceLock::new()).collect()
}

fn load(path: &Option<PathBuf>) -> Option<Rgb> {
    let p = path.as_ref()?;
    let bytes = crate::io::read(p).ok()?;
    Rgb::from_tga(&bytes).ok()
}

impl Looks {
    pub fn open(base_path: &Path) -> Result<Looks, String> {
        let bytes =
            crate::io::read(base_path).map_err(|e| format!("{}: {e}", base_path.display()))?;
        let base = Rgb::from_tga(&bytes).map_err(|e| format!("{}: {e}", base_path.display()))?;
        let summer_paths = PASSES
            .iter()
            .map(|p| p.summer.and_then(|s| sibling(base_path, s)))
            .collect();
        let winter_paths = PASSES
            .iter()
            .map(|p| sibling(base_path, p.winter))
            .collect();
        Ok(Looks {
            base,
            base_path: base_path.to_path_buf(),
            bpp: crate::tga::depth(&bytes),
            original: Vec::new(),
            opened: None,
            summer_paths,
            winter_paths,
            summer: fresh(PASSES.len()),
            winter: fresh(PASSES.len()),
        })
    }

    pub fn from_base(base: Rgb) -> Looks {
        Looks {
            base,
            base_path: PathBuf::new(),
            bpp: 32,
            original: Vec::new(),
            opened: None,
            summer_paths: vec![None; PASSES.len()],
            winter_paths: vec![None; PASSES.len()],
            summer: fresh(PASSES.len()),
            winter: fresh(PASSES.len()),
        }
    }

    pub fn set_summer(&mut self, pass: usize, img: Rgb) {
        self.summer[pass] = OnceLock::from(Some(img));
        self.summer_paths[pass] = Some(PathBuf::new());
    }

    pub fn set_winter(&mut self, pass: usize, img: Rgb) {
        self.winter[pass] = OnceLock::from(Some(img));
        self.winter_paths[pass] = Some(PathBuf::new());
    }

    pub fn has_winter_art(&self) -> bool {
        self.winter_paths.iter().any(Option::is_some)
    }

    pub fn has_terrain_art(&self) -> bool {
        self.summer_paths.iter().any(Option::is_some)
    }

    pub fn files(&self) -> Vec<PathBuf> {
        let mut out = vec![self.base_path.clone()];
        out.extend(self.summer_paths.iter().flatten().cloned());
        out.extend(self.winter_paths.iter().flatten().cloned());
        out
    }

    pub fn encode(&self) -> Vec<u8> {
        crate::tga::encode_rgb_bottom_up(self.base.w, self.base.h, &self.base.data, self.bpp)
    }

    pub fn keep_opened(&mut self) {
        if self.opened.is_none() {
            self.opened = Some(self.base.data.clone());
        }
    }

    pub fn opened_pixel(&self, i: usize) -> Option<[u8; 3]> {
        let d = self.opened.as_ref()?;
        Some([*d.get(i * 3)?, *d.get(i * 3 + 1)?, *d.get(i * 3 + 2)?])
    }

    pub fn terrain_pixel(&self, pass: usize, x: i32, y: i32) -> Option<[u8; 3]> {
        Some(self.summer_img(pass)?.tiled(x, y))
    }

    pub fn terrain_choices(&self) -> Vec<(usize, &'static str)> {
        PASSES
            .iter()
            .enumerate()
            .filter(|(i, p)| p.summer.is_some() && self.summer_paths[*i].is_some())
            .map(|(i, p)| (i, p.summer.unwrap_or("")))
            .collect()
    }

    pub fn trim(&mut self) {
        self.summer = fresh(PASSES.len());
        self.winter = fresh(PASSES.len());
    }

    fn summer_img(&self, i: usize) -> Option<&Rgb> {
        self.summer_paths[i].as_ref()?;
        self.summer[i]
            .get_or_init(|| load(&self.summer_paths[i]))
            .as_ref()
    }

    fn winter_img(&self, i: usize) -> Option<&Rgb> {
        self.winter_paths[i].as_ref()?;
        self.winter[i]
            .get_or_init(|| load(&self.winter_paths[i]))
            .as_ref()
    }

    pub fn look_of(&self, prov: usize, flags: u64, season: bool, all_looks: bool) -> Look {
        if flags & UNKNOWN != 0 {
            return Look::Base;
        }
        let original = self.original.get(prov).copied().unwrap_or(flags);
        let unchanged = !all_looks && (original ^ flags) & LOOK_CHANGE_MASK == 0;
        let cold = province_winter(flags, season);
        for (i, pass) in PASSES.iter().enumerate() {
            let every = pass.mask == u64::MAX;
            if flags & pass.exclude != 0 || (!every && flags & pass.mask != pass.mask) {
                continue;
            }
            if cold && self.winter_img(i).is_some() {
                return Look::Winter(i);
            }
            if unchanged && !cold {
                return Look::Base;
            }
            if pass.summer.is_some() && self.summer_img(i).is_some() {
                return Look::Summer(i, cold);
            }
            if every && cold {
                return Look::BaseWhitened;
            }
        }
        Look::Base
    }

    pub fn paint(&self, p: &Plane, rect: Rect, season: bool, all_looks: bool, out: &mut [u8]) {
        let rect = rect.clamp_to(p.w, p.h);
        if rect.is_empty() {
            return;
        }
        let looks: Vec<Look> = p
            .flags
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                if i == 0 {
                    Look::Base
                } else {
                    self.look_of(i, f, season, all_looks)
                }
            })
            .collect();
        for y in rect.y0..=rect.y1 {
            for x in rect.x0..=rect.x1 {
                let i = (y * p.w + x) as usize;
                let owner = p.owners[i];
                let look = if owner > 0 {
                    looks.get(owner as usize).copied().unwrap_or(Look::Base)
                } else {
                    Look::Base
                };
                let mut c = match look {
                    Look::Base => self.base.tiled(x, y),
                    Look::BaseWhitened => self.base.tiled(x, y).map(whiten),
                    Look::Summer(k, cold) => {
                        let c = self.summer_img(k).map(|m| m.tiled(x, y)).unwrap_or([0; 3]);
                        if cold {
                            c.map(whiten)
                        } else {
                            c
                        }
                    }
                    Look::Winter(k) => self.winter_img(k).map(|m| m.tiled(x, y)).unwrap_or([0; 3]),
                };
                if c == [255, 255, 255] {
                    c = [254, 254, 254];
                }
                out[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
}

pub fn owners_from_runs(w: i32, h: i32, runs: &[(i32, i32, i32, i32)]) -> Vec<i16> {
    let mut owners = vec![0i16; (w * h) as usize];
    for &(x, y, len, prov) in runs {
        if y < 0 || y >= h || prov <= 0 || prov > i16::MAX as i32 {
            continue;
        }
        let x0 = x.max(0);
        let x1 = (x + len).min(w);
        if x0 < x1 {
            owners[(y * w + x0) as usize..(y * w + x1) as usize].fill(prov as i16);
        }
    }
    owners
}

pub fn guess_owners(w: i32, h: i32, capitals: &[(i16, i16)], hwrap: bool, vwrap: bool) -> Vec<i16> {
    let mut owners = vec![0i16; (w * h) as usize];
    let mut queue = VecDeque::new();
    for (k, &(x, y)) in capitals.iter().enumerate() {
        let (x, y) = (x as i32, y as i32);
        if x >= 0 && y >= 0 && x < w && y < h && k < i16::MAX as usize {
            owners[(y * w + x) as usize] = k as i16 + 1;
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let id = owners[(y * w + x) as usize];
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (mut nx, mut ny) = (x + dx, y + dy);
            if hwrap {
                nx = nx.rem_euclid(w);
            }
            if vwrap {
                ny = ny.rem_euclid(h);
            }
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let o = (ny * w + nx) as usize;
            if owners[o] == 0 {
                owners[o] = id;
                queue.push_back((nx, ny));
            }
        }
    }
    owners
}
