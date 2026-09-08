use rayon::prelude::*;

use crate::graph::{get_border_flags, BORDER_BLOCKED, TERRAIN_NOSTART, TERRAIN_SEA};
use crate::world::{World, MAX_NBORS};

pub const MAGIC: i32 = 0x000d_b775;
pub const END_MAGIC: i32 = 0x0000_0483;
pub const RECIPE_VERSION: i32 = 3;
pub const DOM_VERSION: i32 = 0x23f;
pub const HEIGHT_LIMIT: f32 = 2000.0;
pub const HEIGHT_SCALE: f32 = 16.0;
pub const RIVER_SENTINEL: f32 = -10000.0;
pub const ZOOM_NUMERATOR: f64 = 75.0;
pub const MAP_HEADER: &str = "--\n-- Random map file for Dominions 6\n--\n-- Illwinter Game Design\n-- www.illwinter.com\n--\n\n";
pub const ZOOM_COMMENT: &str = "-- defaultmapzoom is not used in Dominions 6, 5 or 4, but it is here if you want the map for Dominions 3\n";

pub fn wrap_code(w: &World) -> u8 {
    let h = u8::from(w.hwrap);
    if w.vwrap {
        h + 2
    } else {
        h
    }
}

pub fn fixed_point_bytes(v: f32) -> [u8; 6] {
    let scaled = (v * 65536.0) as i64 as i16;
    let (frac, int) = if v >= 0.0 {
        (scaled as u16, v as i32)
    } else {
        ((-1i16 - scaled) as u16, (v as i32) - 1)
    };
    let mut out = [0u8; 6];
    out[0..2].copy_from_slice(&frac.to_le_bytes());
    out[2..6].copy_from_slice(&int.to_le_bytes());
    out
}

pub fn stored_height(h: f32, sea_level: f32) -> i16 {
    let v = (h - sea_level).clamp(-HEIGHT_LIMIT, HEIGHT_LIMIT);
    ((v * HEIGHT_SCALE) as i32) as i16
}

pub fn write_d6m(w: &World) -> Vec<u8> {
    let n = w.nprov() as i32;
    let px = (w.w * w.h).max(0) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(34 + 12 * n as usize + 4 * px);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&RECIPE_VERSION.to_le_bytes());
    out.extend_from_slice(&w.w.to_le_bytes());
    out.extend_from_slice(&w.h.to_le_bytes());
    out.extend_from_slice(&0i64.to_le_bytes());
    out.extend_from_slice(&fixed_point_bytes(w.spacing));
    out.extend_from_slice(&n.to_le_bytes());
    for p in w.provinces.iter().skip(1) {
        out.extend_from_slice(&(p.x as i16).to_le_bytes());
        out.extend_from_slice(&(p.y as i16).to_le_bytes());
        out.extend_from_slice(&p.terrain.to_le_bytes());
    }
    let sea = w.sea_level;
    let mut hbuf = vec![0u8; 2 * px];
    hbuf.par_chunks_mut(2)
        .zip(w.heights.par_iter())
        .for_each(|(cell, &h)| {
            cell.copy_from_slice(&stored_height(h, sea).to_le_bytes());
        });
    out.extend_from_slice(&hbuf);
    let mut obuf = vec![0u8; 2 * px];
    obuf.par_chunks_mut(2)
        .zip(w.owner.par_iter())
        .for_each(|(cell, &o)| {
            cell.copy_from_slice(&o.to_le_bytes());
        });
    out.extend_from_slice(&obuf);
    out.extend_from_slice(&END_MAGIC.to_le_bytes());
    out
}

#[allow(clippy::needless_range_loop)]
fn nearest_neighbour_sq(w: &World) -> Vec<f32> {
    let n = w.nprov();
    let mut d = vec![0.0f32; n + 2];
    for a in 1..=n {
        let ax = w.provinces[a].x as i16 as i32;
        let ay = w.provinces[a].y as i16 as i32;
        let mut best = 999_999.0f32;
        for b in 1..=n {
            if a == b {
                continue;
            }
            let bx = w.provinces[b].x as i16 as i32;
            let by = w.provinces[b].y as i16 as i32;
            let dy = (ay - by) as f32;
            let dx = (ax - bx) as f32;
            let v = dy * dy + dx * dx;
            if v < best {
                best = v;
            }
        }
        d[a] = best;
    }
    d
}

pub fn default_map_zoom(w: &World) -> f64 {
    let n = w.nprov() as i32;
    let mut d = nearest_neighbour_sq(w);
    if n >= 2 {
        let s = &mut d[1..=(n as usize)];
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }
    let lo = (n / 5 + 1).min(n - 1).max(1) as usize;
    let hi = (n / 5 + 2).min(n).max(1) as usize;
    let sum = f64::from(d[hi]).sqrt() + f64::from(d[lo].sqrt());
    let half = f64::from(sum as f32) * 0.5;
    ZOOM_NUMERATOR / half
}

pub fn apply_nostart_rule(w: &mut World) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        let p = w.provinces[a as usize].clone();
        let mut land = 0;
        let mut sea = 0;
        for i in 0..p.nbors.len().min(MAX_NBORS) {
            let b = i32::from(p.nbors[i]);
            if b < 1 {
                break;
            }
            if b != a
                && (w.provinces[b as usize].terrain & TERRAIN_SEA) == 0
                && (get_border_flags(w, a, b) & BORDER_BLOCKED) == 0
            {
                land += 1;
            }
        }
        for i in 0..p.nbors.len().min(MAX_NBORS) {
            let b = i32::from(p.nbors[i]);
            if b < 1 {
                break;
            }
            if b != a
                && (w.provinces[b as usize].terrain & TERRAIN_SEA) != 0
                && (get_border_flags(w, a, b) & BORDER_BLOCKED) == 0
            {
                sea += 1;
            }
        }
        if land + sea < 2 {
            w.provinces[a as usize].terrain |= TERRAIN_NOSTART;
        }
    }
}

pub fn write_map_text(w: &mut World, name: &str) -> String {
    let n = w.nprov() as i32;
    let mut s = String::new();
    s.push_str(MAP_HEADER);
    s.push_str(&format!("#dom2title {name}\n"));
    s.push_str(&format!("#imagefile {name}.d6m\n"));
    s.push_str(&format!("#mapsize {} {}\n", w.w, w.h));
    s.push_str(&format!("#domversion {DOM_VERSION}\n"));
    match wrap_code(w) {
        3 => s.push_str("#wraparound\n"),
        1 => s.push_str("#hwraparound\n"),
        2 => s.push_str("#vwraparound\n"),
        _ => {}
    }
    s.push_str(ZOOM_COMMENT);
    s.push_str(&format!("#defaultmapzoom {:.6}\n", default_map_zoom(w)));
    s.push('\n');
    s.push_str(&format!(
        "#description \"This is a randomly created map. It has {} provinces and is {} x {} pixels large.\"\n",
        n, w.w, w.h
    ));
    s.push('\n');
    for a in 1..=n {
        let p = w.provinces[a as usize].clone();
        for i in 0..p.nbors.len().min(MAX_NBORS) {
            let b = i32::from(p.nbors[i]);
            if b < 1 {
                break;
            }
            if a < b {
                s.push_str(&format!("#neighbour {a} {b}\n"));
                if p.border[i] != 0 {
                    s.push_str(&format!("#neighbourspec {} {} {}\n", a, b, p.border[i]));
                }
            }
        }
    }
    s.push('\n');
    apply_nostart_rule(w);
    for a in 1..=n {
        s.push_str(&format!(
            "#terrain {} {}\n",
            a, w.provinces[a as usize].terrain
        ));
    }
    s.push_str("\n\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{addnbor, set_border_flags, BORDER_RIVER};

    fn world(n: usize) -> World {
        let mut w = World::new(3);
        w.w = 4;
        w.h = 2;
        w.owner = vec![0i16; 8];
        w.heights = vec![0.0f32; 8];
        w.sea_level = 0.0;
        w.spacing = 51.0;
        w.provinces = vec![Default::default(); n + 1];
        w
    }

    #[test]
    fn fixed_point_is_six_bytes_split_frac_then_int() {
        assert_eq!(fixed_point_bytes(51.0), [0, 0, 51, 0, 0, 0]);
        let b = fixed_point_bytes(0.5);
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 0x8000);
        assert_eq!(i32::from_le_bytes([b[2], b[3], b[4], b[5]]), 0);
    }

    #[test]
    fn heights_clamp_then_quantise_by_sixteen() {
        assert_eq!(stored_height(10.0, 0.0), 160);
        assert_eq!(stored_height(-10.5, 0.0), -168);
        assert_eq!(stored_height(9999.0, 0.0), 32000);
        assert_eq!(stored_height(-9999.0, 0.0), -32000);
        assert_eq!(stored_height(RIVER_SENTINEL, 0.0), -32000);
    }

    #[test]
    fn d6m_layout_matches_the_documented_offsets() {
        let mut w = world(2);
        w.provinces[1].x = 1;
        w.provinces[1].y = 0;
        w.provinces[1].terrain = 4;
        w.provinces[2].x = 3;
        w.provinces[2].y = 1;
        let b = write_d6m(&w);
        assert_eq!(b.len(), 34 + 2 * 12 + 8 * 2 + 8 * 2 + 4);
        assert_eq!(i32::from_le_bytes(b[0..4].try_into().unwrap()), MAGIC);
        assert_eq!(i32::from_le_bytes(b[4..8].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(b[8..12].try_into().unwrap()), 4);
        assert_eq!(i32::from_le_bytes(b[12..16].try_into().unwrap()), 2);
        assert_eq!(i64::from_le_bytes(b[16..24].try_into().unwrap()), 0);
        assert_eq!(i32::from_le_bytes(b[26..30].try_into().unwrap()), 51);
        assert_eq!(i32::from_le_bytes(b[30..34].try_into().unwrap()), 2);
        assert_eq!(i16::from_le_bytes(b[34..36].try_into().unwrap()), 1);
        assert_eq!(i64::from_le_bytes(b[38..46].try_into().unwrap()), 4);
        let tail = b.len() - 4;
        assert_eq!(i32::from_le_bytes(b[tail..].try_into().unwrap()), END_MAGIC);
    }

    #[test]
    fn map_text_emits_each_pair_once_and_specs_only_when_set() {
        let mut w = world(3);
        w.hwrap = true;
        for (i, p) in w.provinces.iter_mut().enumerate().skip(1) {
            p.x = (i as i32) * 2;
            p.y = 0;
        }
        addnbor(&mut w, 1, 2);
        addnbor(&mut w, 2, 3);
        addnbor(&mut w, 1, 3);
        set_border_flags(&mut w, 1, 2, BORDER_RIVER);
        let t = write_map_text(&mut w, "demo");
        assert_eq!(t.matches("#neighbour ").count(), 3);
        assert_eq!(t.matches("#neighbourspec ").count(), 1);
        assert!(t.contains("#neighbourspec 1 2 2\n"));
        assert!(t.contains("#hwraparound\n"));
        assert!(t.contains("#imagefile demo.d6m\n"));
        assert!(t.contains("#mapsize 4 2\n"));
        assert!(t.ends_with("\n\n"));
    }

    #[test]
    fn dead_end_provinces_get_nostart() {
        let mut w = world(3);
        addnbor(&mut w, 1, 2);
        addnbor(&mut w, 2, 3);
        apply_nostart_rule(&mut w);
        assert_eq!(w.provinces[1].terrain & TERRAIN_NOSTART, TERRAIN_NOSTART);
        assert_eq!(w.provinces[2].terrain & TERRAIN_NOSTART, 0);
        assert_eq!(w.provinces[3].terrain & TERRAIN_NOSTART, TERRAIN_NOSTART);
    }

    #[test]
    fn blocked_borders_do_not_count_as_exits() {
        let mut w = world(3);
        addnbor(&mut w, 1, 2);
        addnbor(&mut w, 1, 3);
        set_border_flags(&mut w, 1, 2, crate::graph::BORDER_MOUNTAIN);
        apply_nostart_rule(&mut w);
        assert_eq!(w.provinces[1].terrain & TERRAIN_NOSTART, TERRAIN_NOSTART);
    }

    #[test]
    fn zoom_is_seventy_five_over_half_the_order_statistic() {
        let mut w = world(6);
        for (i, p) in w.provinces.iter_mut().enumerate().skip(1) {
            p.x = (i as i32) * 10;
            p.y = 0;
        }
        let z = default_map_zoom(&w);
        assert!((z - 75.0 / 10.0).abs() < 1e-9, "z={z}");
    }
}
