#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tex {
    Plain,
    Ash,
    Highland,
    Farm,
    Swamp,
    Waste,
    Desert,
    Forest,
    Cavefloor,
    Cave,
    Caveforest,
    Dripcave,
    Floodcave,
    Floodcrys,
    Floodedplain,
    Floodedswamp,
    Floodedwaste,
    Winter,
    Winterfarm,
    Winterwood,
    Frozendrip,
    Frozen,
    Lava,
    Water,
    Deepsea,
    Shallowsea,
    Kelpforest,
    Unknown,
}

pub const ALL: [Tex; 28] = [
    Tex::Plain,
    Tex::Ash,
    Tex::Highland,
    Tex::Farm,
    Tex::Swamp,
    Tex::Waste,
    Tex::Desert,
    Tex::Forest,
    Tex::Cavefloor,
    Tex::Cave,
    Tex::Caveforest,
    Tex::Dripcave,
    Tex::Floodcave,
    Tex::Floodcrys,
    Tex::Floodedplain,
    Tex::Floodedswamp,
    Tex::Floodedwaste,
    Tex::Winter,
    Tex::Winterfarm,
    Tex::Winterwood,
    Tex::Frozendrip,
    Tex::Frozen,
    Tex::Lava,
    Tex::Water,
    Tex::Deepsea,
    Tex::Shallowsea,
    Tex::Kelpforest,
    Tex::Unknown,
];

impl Tex {
    pub fn file_stem(self) -> &'static str {
        match self {
            Tex::Plain => "bg_plain",
            Tex::Ash => "bg_ash",
            Tex::Highland => "bg_highland",
            Tex::Farm => "bg_farm",
            Tex::Swamp => "bg_swamp",
            Tex::Waste => "bg_waste",
            Tex::Desert => "bg_desert",
            Tex::Forest => "bg_forest",
            Tex::Cavefloor => "bg_cavefloor",
            Tex::Cave => "bg_cave",
            Tex::Caveforest => "bg_caveforest",
            Tex::Dripcave => "bg_dripcave",
            Tex::Floodcave => "bg_floodcave",
            Tex::Floodcrys => "bg_floodcrys",
            Tex::Floodedplain => "bg_floodedplain",
            Tex::Floodedswamp => "bg_floodedswamp",
            Tex::Floodedwaste => "bg_floodedwaste",
            Tex::Winter => "bg_winter",
            Tex::Winterfarm => "bg_winterfarm",
            Tex::Winterwood => "bg_winterwood",
            Tex::Frozendrip => "bg_frozendrip",
            Tex::Frozen => "bg_frozen",
            Tex::Lava => "bg_lava",
            Tex::Water => "bg_water",
            Tex::Deepsea => "bg_deepsea",
            Tex::Shallowsea => "bg_shallowsea",
            Tex::Kelpforest => "bg_kelpforest",
            Tex::Unknown => "bg_unknown",
        }
    }

    fn embedded(self) -> &'static [u8] {
        match self {
            Tex::Plain => include_bytes!("../assets/bg/bg_plain.png"),
            Tex::Ash => include_bytes!("../assets/bg/bg_ash.png"),
            Tex::Highland => include_bytes!("../assets/bg/bg_highland.png"),
            Tex::Farm => include_bytes!("../assets/bg/bg_farm.png"),
            Tex::Swamp => include_bytes!("../assets/bg/bg_swamp.png"),
            Tex::Waste => include_bytes!("../assets/bg/bg_waste.png"),
            Tex::Desert => include_bytes!("../assets/bg/bg_desert.png"),
            Tex::Forest => include_bytes!("../assets/bg/bg_forest.png"),
            Tex::Cavefloor => include_bytes!("../assets/bg/bg_cavefloor.png"),
            Tex::Cave => include_bytes!("../assets/bg/bg_cave.png"),
            Tex::Caveforest => include_bytes!("../assets/bg/bg_caveforest.png"),
            Tex::Dripcave => include_bytes!("../assets/bg/bg_dripcave.png"),
            Tex::Floodcave => include_bytes!("../assets/bg/bg_floodcave.png"),
            Tex::Floodcrys => include_bytes!("../assets/bg/bg_floodcrys.png"),
            Tex::Floodedplain => include_bytes!("../assets/bg/bg_floodedplain.png"),
            Tex::Floodedswamp => include_bytes!("../assets/bg/bg_floodedswamp.png"),
            Tex::Floodedwaste => include_bytes!("../assets/bg/bg_floodedwaste.png"),
            Tex::Winter => include_bytes!("../assets/bg/bg_winter.png"),
            Tex::Winterfarm => include_bytes!("../assets/bg/bg_winterfarm.png"),
            Tex::Winterwood => include_bytes!("../assets/bg/bg_winterwood.png"),
            Tex::Frozendrip => include_bytes!("../assets/bg/bg_frozendrip.png"),
            Tex::Frozen => include_bytes!("../assets/bg/bg_frozen.png"),
            Tex::Lava => include_bytes!("../assets/bg/bg_lava.png"),
            Tex::Water => include_bytes!("../assets/bg/bg_water.png"),
            Tex::Deepsea => include_bytes!("../assets/bg/bg_deepsea.png"),
            Tex::Shallowsea => include_bytes!("../assets/bg/bg_shallowsea.png"),
            Tex::Kelpforest => include_bytes!("../assets/bg/bg_kelpforest.png"),
            Tex::Unknown => include_bytes!("../assets/bg/bg_unknown.png"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn flip_rows(&mut self) {
        let stride = self.w * 4;
        let mut top = 0;
        let mut bot = self.h.saturating_sub(1) * stride;
        let mut tmp = vec![0u8; stride];
        while top < bot {
            tmp.copy_from_slice(&self.rgba[top..top + stride]);
            self.rgba.copy_within(bot..bot + stride, top);
            self.rgba[bot..bot + stride].copy_from_slice(&tmp);
            top += stride;
            bot -= stride;
        }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    #[inline]
    pub fn tiled(&self, x: i32, y: i32) -> [u8; 4] {
        self.at((x % self.w as i32) as usize, (y % self.h as i32) as usize)
    }
}

pub fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let w = info.width as usize;
    let h = info.height as usize;
    let data = &buf[..info.buffer_size()];
    let rgba = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => data.to_vec(),
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            let mut v = Vec::with_capacity(w * h * 4);
            for p in data.chunks_exact(3) {
                v.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            v
        }
        (png::ColorType::Grayscale, png::BitDepth::Eight) => {
            let mut v = Vec::with_capacity(w * h * 4);
            for &g in data {
                v.extend_from_slice(&[g, g, g, 255]);
            }
            v
        }
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => {
            let mut v = Vec::with_capacity(w * h * 4);
            for p in data.chunks_exact(2) {
                v.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
            v
        }
        (c, d) => return Err(format!("unsupported png {c:?} {d:?}")),
    };
    Ok(Image { w, h, rgba })
}

pub const SPRITE_COUNT: usize = 98;

pub const SPRITES: [&[u8]; SPRITE_COUNT] = [
    include_bytes!("../assets/mapstuff/000.png"),
    include_bytes!("../assets/mapstuff/001.png"),
    include_bytes!("../assets/mapstuff/002.png"),
    include_bytes!("../assets/mapstuff/003.png"),
    include_bytes!("../assets/mapstuff/004.png"),
    include_bytes!("../assets/mapstuff/005.png"),
    include_bytes!("../assets/mapstuff/006.png"),
    include_bytes!("../assets/mapstuff/007.png"),
    include_bytes!("../assets/mapstuff/008.png"),
    include_bytes!("../assets/mapstuff/009.png"),
    include_bytes!("../assets/mapstuff/010.png"),
    include_bytes!("../assets/mapstuff/011.png"),
    include_bytes!("../assets/mapstuff/012.png"),
    include_bytes!("../assets/mapstuff/013.png"),
    include_bytes!("../assets/mapstuff/014.png"),
    include_bytes!("../assets/mapstuff/015.png"),
    include_bytes!("../assets/mapstuff/016.png"),
    include_bytes!("../assets/mapstuff/017.png"),
    include_bytes!("../assets/mapstuff/018.png"),
    include_bytes!("../assets/mapstuff/019.png"),
    include_bytes!("../assets/mapstuff/020.png"),
    include_bytes!("../assets/mapstuff/021.png"),
    include_bytes!("../assets/mapstuff/022.png"),
    include_bytes!("../assets/mapstuff/023.png"),
    include_bytes!("../assets/mapstuff/024.png"),
    include_bytes!("../assets/mapstuff/025.png"),
    include_bytes!("../assets/mapstuff/026.png"),
    include_bytes!("../assets/mapstuff/027.png"),
    include_bytes!("../assets/mapstuff/028.png"),
    include_bytes!("../assets/mapstuff/029.png"),
    include_bytes!("../assets/mapstuff/030.png"),
    include_bytes!("../assets/mapstuff/031.png"),
    include_bytes!("../assets/mapstuff/032.png"),
    include_bytes!("../assets/mapstuff/033.png"),
    include_bytes!("../assets/mapstuff/034.png"),
    include_bytes!("../assets/mapstuff/035.png"),
    include_bytes!("../assets/mapstuff/036.png"),
    include_bytes!("../assets/mapstuff/037.png"),
    include_bytes!("../assets/mapstuff/038.png"),
    include_bytes!("../assets/mapstuff/039.png"),
    include_bytes!("../assets/mapstuff/040.png"),
    include_bytes!("../assets/mapstuff/041.png"),
    include_bytes!("../assets/mapstuff/042.png"),
    include_bytes!("../assets/mapstuff/043.png"),
    include_bytes!("../assets/mapstuff/044.png"),
    include_bytes!("../assets/mapstuff/045.png"),
    include_bytes!("../assets/mapstuff/046.png"),
    include_bytes!("../assets/mapstuff/047.png"),
    include_bytes!("../assets/mapstuff/048.png"),
    include_bytes!("../assets/mapstuff/049.png"),
    include_bytes!("../assets/mapstuff/050.png"),
    include_bytes!("../assets/mapstuff/051.png"),
    include_bytes!("../assets/mapstuff/052.png"),
    include_bytes!("../assets/mapstuff/053.png"),
    include_bytes!("../assets/mapstuff/054.png"),
    include_bytes!("../assets/mapstuff/055.png"),
    include_bytes!("../assets/mapstuff/056.png"),
    include_bytes!("../assets/mapstuff/057.png"),
    include_bytes!("../assets/mapstuff/058.png"),
    include_bytes!("../assets/mapstuff/059.png"),
    include_bytes!("../assets/mapstuff/060.png"),
    include_bytes!("../assets/mapstuff/061.png"),
    include_bytes!("../assets/mapstuff/062.png"),
    include_bytes!("../assets/mapstuff/063.png"),
    include_bytes!("../assets/mapstuff/064.png"),
    include_bytes!("../assets/mapstuff/065.png"),
    include_bytes!("../assets/mapstuff/066.png"),
    include_bytes!("../assets/mapstuff/067.png"),
    include_bytes!("../assets/mapstuff/068.png"),
    include_bytes!("../assets/mapstuff/069.png"),
    include_bytes!("../assets/mapstuff/070.png"),
    include_bytes!("../assets/mapstuff/071.png"),
    include_bytes!("../assets/mapstuff/072.png"),
    include_bytes!("../assets/mapstuff/073.png"),
    include_bytes!("../assets/mapstuff/074.png"),
    include_bytes!("../assets/mapstuff/075.png"),
    include_bytes!("../assets/mapstuff/076.png"),
    include_bytes!("../assets/mapstuff/077.png"),
    include_bytes!("../assets/mapstuff/078.png"),
    include_bytes!("../assets/mapstuff/079.png"),
    include_bytes!("../assets/mapstuff/080.png"),
    include_bytes!("../assets/mapstuff/081.png"),
    include_bytes!("../assets/mapstuff/082.png"),
    include_bytes!("../assets/mapstuff/083.png"),
    include_bytes!("../assets/mapstuff/084.png"),
    include_bytes!("../assets/mapstuff/085.png"),
    include_bytes!("../assets/mapstuff/086.png"),
    include_bytes!("../assets/mapstuff/087.png"),
    include_bytes!("../assets/mapstuff/088.png"),
    include_bytes!("../assets/mapstuff/089.png"),
    include_bytes!("../assets/mapstuff/090.png"),
    include_bytes!("../assets/mapstuff/091.png"),
    include_bytes!("../assets/mapstuff/092.png"),
    include_bytes!("../assets/mapstuff/093.png"),
    include_bytes!("../assets/mapstuff/094.png"),
    include_bytes!("../assets/mapstuff/095.png"),
    include_bytes!("../assets/mapstuff/096.png"),
    include_bytes!("../assets/mapstuff/097.png"),
];

pub struct Frame {
    pub mips: Vec<Image>,
}

impl Frame {
    pub fn from_image(mut img: Image) -> Frame {
        for px in img.rgba.chunks_exact_mut(4) {
            let a = px[3] as u32;
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        }
        let mut mips = vec![img];
        while mips.last().unwrap().w > 2 && mips.last().unwrap().h > 2 {
            let src = mips.last().unwrap();
            let w = src.w / 2;
            let h = src.h / 2;
            let mut rgba = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    for c in 0..4 {
                        let sum = src.rgba[((y * 2) * src.w + x * 2) * 4 + c] as u32
                            + src.rgba[((y * 2) * src.w + x * 2 + 1) * 4 + c] as u32
                            + src.rgba[((y * 2 + 1) * src.w + x * 2) * 4 + c] as u32
                            + src.rgba[((y * 2 + 1) * src.w + x * 2 + 1) * 4 + c] as u32;
                        rgba[(y * w + x) * 4 + c] = ((sum + 2) / 4) as u8;
                    }
                }
            }
            mips.push(Image { w, h, rgba });
        }
        Frame { mips }
    }

    pub fn level_for(&self, size: i32) -> &Image {
        let mut best = &self.mips[0];
        for m in &self.mips {
            if m.w as i32 >= size && m.h as i32 >= size {
                best = m;
            } else {
                break;
            }
        }
        best
    }
}

#[inline]
pub fn sample_bilinear(img: &Image, u: f32, v: f32) -> [u32; 4] {
    let fx = (u * img.w as f32 - 0.5).max(0.0);
    let fy = (v * img.h as f32 - 0.5).max(0.0);
    let x0 = (fx as usize).min(img.w - 1);
    let y0 = (fy as usize).min(img.h - 1);
    let x1 = (x0 + 1).min(img.w - 1);
    let y1 = (y0 + 1).min(img.h - 1);
    let tx = ((fx - x0 as f32).clamp(0.0, 1.0) * 256.0) as u32;
    let ty = ((fy - y0 as f32).clamp(0.0, 1.0) * 256.0) as u32;
    let p00 = img.at(x0, y0);
    let p10 = img.at(x1, y0);
    let p01 = img.at(x0, y1);
    let p11 = img.at(x1, y1);
    let mut out = [0u32; 4];
    for c in 0..4 {
        let top = p00[c] as u32 * (256 - tx) + p10[c] as u32 * tx;
        let bot = p01[c] as u32 * (256 - tx) + p11[c] as u32 * tx;
        out[c] = (top * (256 - ty) + bot * ty) >> 16;
    }
    out
}

pub struct TexSet {
    imgs: Vec<Image>,
    frames: Vec<Frame>,
}

impl TexSet {
    pub fn embedded() -> TexSet {
        let imgs = ALL
            .iter()
            .map(|t| {
                let mut img = decode_png(t.embedded()).expect("embedded texture");
                img.flip_rows();
                img
            })
            .collect();
        let frames = SPRITES
            .iter()
            .map(|b| Frame::from_image(decode_png(b).expect("embedded sprite")))
            .collect();
        TexSet { imgs, frames }
    }

    pub fn from_images(imgs: Vec<Image>) -> TexSet {
        TexSet {
            imgs,
            frames: Vec::new(),
        }
    }

    pub fn with_frames(mut self, frames: Vec<Frame>) -> TexSet {
        self.frames = frames;
        self
    }

    #[inline]
    pub fn frame(&self, i: i16) -> Option<&Frame> {
        if i < 0 {
            None
        } else {
            self.frames.get(i as usize)
        }
    }

    #[inline]
    pub fn get(&self, t: Tex) -> &Image {
        &self.imgs[t as usize]
    }

    #[inline]
    pub fn sample(&self, t: Tex, x: i32, y: i32) -> [u8; 4] {
        self.get(t).tiled(x, y)
    }
}
