use crate::textures::Image;

pub fn decode(b: &[u8]) -> Result<Image, String> {
    if b.len() < 18 {
        return Err("tga too short".into());
    }
    let id_len = b[0] as usize;
    if b[1] != 0 {
        return Err("colour-mapped tga not supported".into());
    }
    let kind = b[2];
    if kind != 2 && kind != 10 {
        return Err(format!("unsupported tga type {kind}"));
    }
    let w = u16::from_le_bytes([b[12], b[13]]) as usize;
    let h = u16::from_le_bytes([b[14], b[15]]) as usize;
    let bpp = b[16];
    if bpp != 24 && bpp != 32 {
        return Err(format!("unsupported tga depth {bpp}"));
    }
    let top_left = b[17] & 0x20 != 0;
    let bytes_pp = (bpp / 8) as usize;
    let mut src = &b[18 + id_len..];
    let n = w * h;
    let mut rgba = Vec::with_capacity(n * 4);
    let mut push = |p: &[u8]| {
        let a = if bytes_pp == 4 { p[3] } else { 255 };
        rgba.extend_from_slice(&[p[2], p[1], p[0], a]);
    };
    if kind == 2 {
        if src.len() < n * bytes_pp {
            return Err("tga pixel data truncated".into());
        }
        for p in src[..n * bytes_pp].chunks_exact(bytes_pp) {
            push(p);
        }
    } else {
        let mut count = 0usize;
        while count < n {
            if src.is_empty() {
                return Err("tga rle truncated".into());
            }
            let head = src[0];
            src = &src[1..];
            let run = (head & 0x7f) as usize + 1;
            if head & 0x80 != 0 {
                if src.len() < bytes_pp {
                    return Err("tga rle truncated".into());
                }
                for _ in 0..run {
                    if count < n {
                        push(&src[..bytes_pp]);
                        count += 1;
                    }
                }
                src = &src[bytes_pp..];
            } else {
                if src.len() < run * bytes_pp {
                    return Err("tga rle truncated".into());
                }
                for p in src[..run * bytes_pp].chunks_exact(bytes_pp) {
                    if count < n {
                        push(p);
                        count += 1;
                    }
                }
                src = &src[run * bytes_pp..];
            }
        }
    }
    let mut img = Image { w, h, rgba };
    if top_left {
        img.flip_rows();
    }
    Ok(img)
}

pub fn encode_rgba_bottom_up(img: &Image) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + img.w * img.h * 4);
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&(img.w as u16).to_le_bytes());
    out.extend_from_slice(&(img.h as u16).to_le_bytes());
    out.push(32);
    out.push(8);
    for p in img.rgba.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    out
}
