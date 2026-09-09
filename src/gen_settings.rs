use crate::generator_panel::{Form, OwnImage};
use dom6_mapgen::layouts::CaveLayout;
use dom6_mapgen::{Blueprint, Layout};

pub const PREFIX: &str = "-- gen.";
const CHUNK: usize = 120;

pub struct Applied {
    pub form: Form,
    pub pool: Option<String>,
    pub app: Option<String>,
    pub known: usize,
    pub unknown: usize,
}

fn flag(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

fn is_true(v: &str) -> bool {
    matches!(v.trim(), "1" | "true" | "yes" | "on")
}

fn push(out: &mut Vec<String>, key: &str, value: impl std::fmt::Display) {
    let value = value.to_string().replace('#', "_");
    out.push(format!("{PREFIX}{key} {value}"));
}

fn push_image(out: &mut Vec<String>, key: &str, own: &OwnImage) {
    push(
        out,
        &format!("{key}.label"),
        own.label.replace(['\r', '\n'], " "),
    );
    let bp = &own.image;
    let mut rgba = Vec::with_capacity(bp.bgra.len());
    for px in bp.bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let Ok(png) = crate::blueprint_editor::encode_png(bp.w as usize, bp.h as usize, &rgba) else {
        return;
    };
    let text = base64_encode(&png);
    let total = text.len().div_ceil(CHUNK);
    for (i, chunk) in text.as_bytes().chunks(CHUNK).enumerate() {
        push(
            out,
            &format!("{key}.png"),
            format!(
                "{}/{} {}",
                i + 1,
                total,
                std::str::from_utf8(chunk).unwrap_or("")
            ),
        );
    }
}

pub fn encode(form: &Form) -> Vec<String> {
    let mut out = Vec::new();
    out.push("-- generator settings; loading this map restores them".to_string());
    push(&mut out, "app", env!("CARGO_PKG_VERSION"));
    push(&mut out, "pool", dom6_mapgen::rng::pool_id());
    push(&mut out, "seed", form.seed);
    push(&mut out, "name", &form.name);
    push(&mut out, "new_game", flag(form.new_game));
    push(&mut out, "players", form.players);
    push(&mut out, "per_player", form.per_player_bucket);
    push(&mut out, "custom_count", flag(form.custom_count));
    push(&mut out, "auto_size", flag(form.auto_size));
    push(&mut out, "width", form.width);
    push(&mut out, "height", form.height);
    push(&mut out, "layout", format!("{:?}", form.layout));
    push(&mut out, "cave_layout", format!("{:?}", form.cave_layout));
    let o = &form.opts;
    push(&mut out, "provinces", o.provinces);
    push(&mut out, "sea_part", o.sea_part);
    push(&mut out, "mount_part", o.mount_part);
    push(&mut out, "forest_part", o.forest_part);
    push(&mut out, "farm_part", o.farm_part);
    push(&mut out, "swamp_part", o.swamp_part);
    push(&mut out, "waste_part", o.waste_part);
    push(&mut out, "highland_part", o.highland_part);
    push(&mut out, "kelp_part", o.kelp_part);
    push(&mut out, "gorge_part", o.gorge_part);
    push(&mut out, "river_part", o.river_part);
    push(&mut out, "hills", o.hills);
    push(&mut out, "rugedness", o.rugedness);
    push(&mut out, "sea_size", o.sea_size);
    push(&mut out, "extra_islands", o.extra_islands);
    push(&mut out, "bridges", o.bridges);
    push(&mut out, "no_water_prov", flag(o.no_water_prov));
    push(&mut out, "hwrap", flag(o.hwrap));
    push(&mut out, "vwrap", flag(o.vwrap));
    push(&mut out, "blue_acc", o.blue_acc);
    push(&mut out, "cave_world", flag(o.cave_world));
    push(&mut out, "caves_plane", flag(o.caves_plane));
    push(&mut out, "cave_part", o.cave_part);
    if !form.roster.is_empty() {
        let ids: Vec<String> = form.roster.iter().map(u32::to_string).collect();
        push(&mut out, "roster", ids.join(","));
    }
    push(&mut out, "era", form.era);
    push(&mut out, "generic_starts", flag(form.generic_starts));
    push(&mut out, "auto_starts", flag(form.auto_starts));
    if let Some(own) = &form.own {
        push_image(&mut out, "own", own);
    }
    if let Some(own) = &form.cave_own {
        push_image(&mut out, "cave_own", own);
    }
    out
}

fn pairs(lines: &[String]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix(PREFIX)?;
            let (k, v) = match rest.find(' ') {
                Some(i) => (&rest[..i], rest[i + 1..].trim()),
                None => (rest, ""),
            };
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

fn image_from(label: Option<&str>, chunks: &[String]) -> Option<OwnImage> {
    if chunks.is_empty() {
        return None;
    }
    let mut text = String::new();
    for c in chunks {
        let body = c.split_once(' ').map(|(_, b)| b).unwrap_or(c);
        text.push_str(body.trim());
    }
    let png = base64_decode(&text)?;
    let img = crate::textures::decode_png(&png).ok()?;
    let mut bgra = Vec::with_capacity(img.rgba.len());
    for px in img.rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    Some(OwnImage {
        label: label.unwrap_or("saved image").to_string(),
        image: Blueprint {
            w: img.w as i32,
            h: img.h as i32,
            bgra,
        },
    })
}

pub fn decode(lines: &[String]) -> Option<Applied> {
    let kv = pairs(lines);
    if kv.is_empty() {
        return None;
    }
    let mut form = Form::default();
    let mut pool = None;
    let mut app = None;
    let mut known = 0;
    let mut unknown = 0;
    let mut own_label = None;
    let mut own_png = Vec::new();
    let mut cave_label = None;
    let mut cave_png = Vec::new();
    let int = |v: &str| v.trim().parse::<i32>();
    for (k, v) in &kv {
        let v = v.as_str();
        let mut hit = true;
        match k.as_str() {
            "app" => app = Some(v.to_string()),
            "pool" => pool = Some(v.to_string()),
            "seed" => {
                if let Ok(s) = v.trim().parse::<u32>() {
                    form.seed = s;
                }
            }
            "name" => form.name = v.to_string(),
            "new_game" => form.new_game = is_true(v),
            "players" => form.players = int(v).unwrap_or(form.players),
            "per_player" => form.per_player_bucket = int(v).unwrap_or(form.per_player_bucket),
            "custom_count" => form.custom_count = is_true(v),
            "auto_size" => form.auto_size = is_true(v),
            "width" => form.width = int(v).unwrap_or(form.width),
            "height" => form.height = int(v).unwrap_or(form.height),
            "layout" => {
                if let Some(l) = Layout::ALL.iter().find(|l| format!("{l:?}") == v.trim()) {
                    form.layout = *l;
                }
            }
            "cave_layout" => {
                if let Some(l) = CaveLayout::ALL
                    .iter()
                    .find(|l| format!("{l:?}") == v.trim())
                {
                    form.cave_layout = *l;
                }
            }
            "provinces" => form.opts.provinces = int(v).unwrap_or(form.opts.provinces),
            "sea_part" => form.opts.sea_part = int(v).unwrap_or(form.opts.sea_part),
            "mount_part" => form.opts.mount_part = int(v).unwrap_or(form.opts.mount_part),
            "forest_part" => form.opts.forest_part = int(v).unwrap_or(form.opts.forest_part),
            "farm_part" => form.opts.farm_part = int(v).unwrap_or(form.opts.farm_part),
            "swamp_part" => form.opts.swamp_part = int(v).unwrap_or(form.opts.swamp_part),
            "waste_part" => form.opts.waste_part = int(v).unwrap_or(form.opts.waste_part),
            "highland_part" => form.opts.highland_part = int(v).unwrap_or(form.opts.highland_part),
            "kelp_part" => form.opts.kelp_part = int(v).unwrap_or(form.opts.kelp_part),
            "gorge_part" => form.opts.gorge_part = int(v).unwrap_or(form.opts.gorge_part),
            "river_part" => form.opts.river_part = int(v).unwrap_or(form.opts.river_part),
            "hills" => form.opts.hills = int(v).unwrap_or(form.opts.hills),
            "rugedness" => form.opts.rugedness = int(v).unwrap_or(form.opts.rugedness),
            "sea_size" => form.opts.sea_size = int(v).unwrap_or(form.opts.sea_size),
            "extra_islands" => form.opts.extra_islands = int(v).unwrap_or(form.opts.extra_islands),
            "bridges" => form.opts.bridges = int(v).unwrap_or(form.opts.bridges),
            "no_water_prov" => form.opts.no_water_prov = is_true(v),
            "hwrap" => form.opts.hwrap = is_true(v),
            "vwrap" => form.opts.vwrap = is_true(v),
            "blue_acc" => form.opts.blue_acc = int(v).unwrap_or(form.opts.blue_acc),
            "cave_world" => form.opts.cave_world = is_true(v),
            "caves_plane" => form.opts.caves_plane = is_true(v),
            "cave_part" => form.opts.cave_part = int(v).unwrap_or(form.opts.cave_part),
            "roster" => {
                form.roster = v
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u32>().ok())
                    .collect()
            }
            "era" => form.era = v.trim().parse::<u8>().unwrap_or(form.era),
            "generic_starts" => form.generic_starts = is_true(v),
            "auto_starts" => form.auto_starts = is_true(v),
            "own.label" => own_label = Some(v.to_string()),
            "own.png" => own_png.push(v.to_string()),
            "cave_own.label" => cave_label = Some(v.to_string()),
            "cave_own.png" => cave_png.push(v.to_string()),
            _ => hit = false,
        }
        if hit {
            known += 1;
        } else {
            unknown += 1;
        }
    }
    form.own = image_from(own_label.as_deref(), &own_png);
    form.cave_own = image_from(cave_label.as_deref(), &cave_png);
    form.manual_seed = true;
    Some(Applied {
        form,
        pool,
        app,
        known,
        unknown,
    })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_length() {
        for n in 0..40usize {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let text = base64_encode(&bytes);
            assert_eq!(text.len() % 4, 0);
            assert_eq!(base64_decode(&text).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn settings_round_trip_including_an_own_image_and_ignore_unknown_keys() {
        let mut form = Form {
            seed: 424242,
            name: "keep#me".to_string(),
            new_game: false,
            players: 7,
            layout: Layout::TwirlingSea,
            cave_layout: CaveLayout::OneCave,
            ..Form::default()
        };
        form.opts.sea_part = 61;
        form.opts.hwrap = false;
        form.opts.vwrap = true;
        form.opts.caves_plane = true;
        form.opts.cave_part = 33;
        form.roster = vec![5, 43];
        form.own = Some(OwnImage {
            label: "drawing".to_string(),
            image: Blueprint {
                w: 3,
                h: 2,
                bgra: vec![
                    200, 10, 0, 255, 0, 200, 0, 255, 0, 0, 255, 255, 40, 40, 40, 255, 200, 10, 0,
                    255, 0, 200, 0, 255,
                ],
            },
        });
        let mut lines = encode(&form);
        lines.push(format!("{PREFIX}future_key something"));
        lines.push("#terrain 1 4".to_string());
        let back = decode(&lines).unwrap();
        assert_eq!(back.unknown, 1);
        assert!(back.known > 30);
        assert_eq!(
            back.pool.as_deref(),
            Some(dom6_mapgen::rng::pool_id().as_str())
        );
        let f = back.form;
        assert_eq!(f.seed, 424242);
        assert_eq!(f.name, "keep_me");
        assert!(lines.iter().all(|l| !l.contains('#') || l.starts_with('#')));
        assert!(!f.new_game);
        assert_eq!(f.players, 7);
        assert_eq!(f.layout, Layout::TwirlingSea);
        assert_eq!(f.cave_layout, CaveLayout::OneCave);
        assert_eq!(f.opts.sea_part, 61);
        assert!(!f.opts.hwrap && f.opts.vwrap && f.opts.caves_plane);
        assert_eq!(f.opts.cave_part, 33);
        assert_eq!(f.roster, vec![5, 43]);
        assert!(f.manual_seed);
        let own = f.own.unwrap();
        assert_eq!(own.label, "drawing");
        assert_eq!(own.image, form.own.unwrap().image);
        assert!(f.cave_own.is_none());
    }

    #[test]
    fn a_map_without_settings_yields_nothing() {
        assert!(decode(&["#terrain 1 4".to_string()]).is_none());
    }
}
