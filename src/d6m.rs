use std::fmt;
use std::path::Path;

pub const MAGIC: i32 = 0x000d_b775;
pub const END_MAGIC: i32 = 0x0000_0483;
pub const HEIGHT_SCALE: f32 = 0.0625;
pub const RIVER_SENTINEL: f32 = -10000.0;
pub const STORED_LIMIT: i16 = 32000;

#[derive(Clone, Debug, PartialEq)]
pub struct Province {
    pub x: i16,
    pub y: i16,
    pub terrain: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct D6m {
    pub version: i32,
    pub width: i32,
    pub height: i32,
    pub passthrough: i64,
    pub scale_frac: u16,
    pub scale_int: i32,
    pub provinces: Vec<Province>,
    pub heights: Vec<i16>,
    pub owners: Vec<i16>,
    pub trailing: Vec<u8>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Truncated(usize),
    BadMagic(i32),
    BadEndMagic(i32),
    BadSize(i32, i32),
    BadProvinceCount(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Truncated(at) => write!(f, "file ends early at byte {at}"),
            Error::BadMagic(m) => write!(f, "not a .d6m file (magic {m:#x})"),
            Error::BadEndMagic(m) => write!(f, "corrupt .d6m (end marker {m:#x})"),
            Error::BadSize(w, h) => write!(f, "bad map size {w}x{h}"),
            Error::BadProvinceCount(n) => write!(f, "bad province count {n}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.pos + n > self.b.len() {
            return Err(Error::Truncated(self.pos));
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn i16(&mut self) -> Result<i16, Error> {
        let s = self.take(2)?;
        Ok(i16::from_le_bytes([s[0], s[1]]))
    }
    fn u16(&mut self) -> Result<u16, Error> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Result<i32, Error> {
        let s = self.take(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i64(&mut self) -> Result<i64, Error> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(i64::from_le_bytes(a))
    }
}

impl D6m {
    pub fn parse(b: &[u8]) -> Result<D6m, Error> {
        let mut r = Reader { b, pos: 0 };
        let magic = r.i32()?;
        if magic != MAGIC {
            return Err(Error::BadMagic(magic));
        }
        let version = r.i32()?;
        let width = r.i32()?;
        let height = r.i32()?;
        if width <= 0 || height <= 0 || width > 30000 || height > 30000 {
            return Err(Error::BadSize(width, height));
        }
        let passthrough = if version > 1 { r.i64()? } else { 0 };
        let scale_frac = r.u16()?;
        let scale_int = r.i32()?;
        let nprov = r.i32()?;
        if !(0..=30000).contains(&nprov) {
            return Err(Error::BadProvinceCount(nprov));
        }
        let mut provinces = Vec::with_capacity(nprov as usize);
        for _ in 0..nprov {
            let x = r.i16()?;
            let y = r.i16()?;
            let terrain = if version > 2 { r.i64()? } else { 0 };
            provinces.push(Province { x, y, terrain });
        }
        let n = width as usize * height as usize;
        let hs = r.take(n * 2)?;
        let mut heights = Vec::with_capacity(n);
        for c in hs.chunks_exact(2) {
            heights.push(i16::from_le_bytes([c[0], c[1]]));
        }
        let os = r.take(n * 2)?;
        let mut owners = Vec::with_capacity(n);
        for c in os.chunks_exact(2) {
            owners.push(i16::from_le_bytes([c[0], c[1]]));
        }
        let end = r.i32()?;
        if end != END_MAGIC {
            return Err(Error::BadEndMagic(end));
        }
        let trailing = b[r.pos..].to_vec();
        Ok(D6m {
            version,
            width,
            height,
            passthrough,
            scale_frac,
            scale_int,
            provinces,
            heights,
            owners,
            trailing,
        })
    }

    pub fn load(path: &Path) -> Result<D6m, Error> {
        let b = std::fs::read(path)?;
        D6m::parse(&b)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.heights.len();
        let mut out =
            Vec::with_capacity(34 + self.provinces.len() * 12 + n * 4 + 4 + self.trailing.len());
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        if self.version > 1 {
            out.extend_from_slice(&self.passthrough.to_le_bytes());
        }
        out.extend_from_slice(&self.scale_frac.to_le_bytes());
        out.extend_from_slice(&self.scale_int.to_le_bytes());
        out.extend_from_slice(&(self.provinces.len() as i32).to_le_bytes());
        for p in &self.provinces {
            out.extend_from_slice(&p.x.to_le_bytes());
            out.extend_from_slice(&p.y.to_le_bytes());
            if self.version > 2 {
                out.extend_from_slice(&p.terrain.to_le_bytes());
            }
        }
        for h in &self.heights {
            out.extend_from_slice(&h.to_le_bytes());
        }
        for o in &self.owners {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&END_MAGIC.to_le_bytes());
        out.extend_from_slice(&self.trailing);
        out
    }

    pub fn map_scale(&self) -> f32 {
        let mut v = self.scale_frac as f32 * (1.0 / 65536.0) + self.scale_int as f32;
        if self.scale_frac == 0xffff {
            v += 1.0 / 65536.0;
        }
        v
    }

    pub fn pixel_count(&self) -> usize {
        self.heights.len()
    }

    pub fn heights_f32(&self) -> Vec<f32> {
        self.heights
            .iter()
            .map(|&h| h as f32 * HEIGHT_SCALE)
            .collect()
    }

    pub fn max_owner(&self) -> i16 {
        self.owners.iter().copied().max().unwrap_or(0)
    }
}

pub fn stored_from_units(units: f32) -> i16 {
    let v = (units * 16.0).round();
    v.clamp(-(STORED_LIMIT as f32), STORED_LIMIT as f32) as i16
}

pub fn units_from_stored(v: i16) -> f32 {
    v as f32 * HEIGHT_SCALE
}
