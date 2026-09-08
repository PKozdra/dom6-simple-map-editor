use crate::graph::{
    is_coastal_province, TERRAIN_CAVE, TERRAIN_CAVE_WALL, TERRAIN_DEEP, TERRAIN_DESERT,
    TERRAIN_FARM, TERRAIN_FOREST, TERRAIN_HIGHLAND, TERRAIN_SEA, TERRAIN_SWAMP, TERRAIN_VARIANT,
    TERRAIN_WASTE,
};
use crate::options::Options;
use crate::world::World;

pub const CAVE_FOREST_PCT: i32 = 15;
pub const CAVE_HIGHLAND_PCT: i32 = 20;
pub const CAVE_SWAMP_PCT: i32 = 28;
pub const DESERT_PCT: i32 = 25;
pub const VARIANT_PCT: i32 = 50;
pub const SITE_AFFINITY_PCT: i32 = 5;
pub const SITE_AFFINITY_BASE: i64 = 0x2000;
pub const SITE_AFFINITY_PATHS: i32 = 9;

pub fn roll_terrain(w: &mut World, opts: &Options) {
    let n = w.nprov() as i32;
    for a in 1..=n {
        let t = w.provinces[a as usize].terrain;
        if (t & TERRAIN_CAVE_WALL) == 0 {
            if (t & TERRAIN_SEA) != 0 {
                let r = w.crt.below(100);
                if (t & TERRAIN_DEEP) == 0 {
                    if r < opts.kelp_part {
                        w.provinces[a as usize].terrain |= TERRAIN_FOREST;
                    }
                } else if r < opts.gorge_part {
                    w.provinces[a as usize].terrain |= TERRAIN_HIGHLAND;
                }
            } else if (t & TERRAIN_CAVE) != 0 {
                if w.pool.rnd(100) < CAVE_FOREST_PCT as u32 {
                    w.provinces[a as usize].terrain |= TERRAIN_FOREST;
                } else if w.pool.rnd(100) < CAVE_HIGHLAND_PCT as u32 {
                    w.provinces[a as usize].terrain |= TERRAIN_HIGHLAND;
                } else if w.pool.rnd(100) < CAVE_SWAMP_PCT as u32 {
                    w.provinces[a as usize].terrain |= TERRAIN_SWAMP;
                }
            } else {
                let coastal = is_coastal_province(w, a);
                let r = w.crt.below(100);
                let bit = if coastal {
                    if r < opts.forest_part {
                        Some(TERRAIN_FOREST)
                    } else if w.crt.below(75) < opts.farm_part {
                        Some(TERRAIN_FARM)
                    } else if w.crt.below(200) < opts.waste_part {
                        Some(TERRAIN_WASTE)
                    } else if w.crt.below(50) < opts.swamp_part {
                        Some(TERRAIN_SWAMP)
                    } else if w.crt.below(200) < opts.highland_part {
                        Some(TERRAIN_HIGHLAND)
                    } else {
                        None
                    }
                } else if r < opts.forest_part + 5 {
                    Some(TERRAIN_FOREST)
                } else if w.crt.below(125) < opts.farm_part {
                    Some(TERRAIN_FARM)
                } else if w.crt.below(50) < opts.waste_part {
                    Some(TERRAIN_WASTE)
                } else if w.crt.below(200) < opts.swamp_part {
                    Some(TERRAIN_SWAMP)
                } else if w.crt.below(50) < opts.highland_part {
                    Some(TERRAIN_HIGHLAND)
                } else {
                    None
                };
                if let Some(b) = bit {
                    w.provinces[a as usize].terrain |= b;
                }
                if w.crt.below(100) < DESERT_PCT {
                    w.provinces[a as usize].terrain |= TERRAIN_DESERT;
                }
                if w.crt.below(100) < VARIANT_PCT {
                    w.provinces[a as usize].terrain |= TERRAIN_VARIANT;
                }
            }
        }
        if (w.provinces[a as usize].terrain & TERRAIN_CAVE_WALL) == 0
            && w.crt.below(100) < SITE_AFFINITY_PCT
        {
            let k = w.crt.below(SITE_AFFINITY_PATHS);
            w.provinces[a as usize].terrain |= SITE_AFFINITY_BASE << (k & 0x1f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(n: usize) -> World {
        let mut w = World::new(11);
        w.w = 4;
        w.h = 4;
        w.owner = vec![0; 16];
        w.heights = vec![0.0; 16];
        w.provinces = vec![Default::default(); n + 1];
        w
    }

    #[test]
    fn cave_walls_are_skipped_entirely() {
        let mut w = world(1);
        w.provinces[1].terrain = TERRAIN_CAVE_WALL;
        let before = w.crt;
        roll_terrain(&mut w, &Options::default());
        assert_eq!(w.provinces[1].terrain, TERRAIN_CAVE_WALL);
        assert_eq!(w.crt, before);
    }

    #[test]
    fn a_shallow_sea_province_draws_the_kelp_roll_then_the_affinity_roll() {
        let mut w = world(1);
        w.provinces[1].terrain = TERRAIN_SEA;
        let mut probe = w.crt;
        let kelp = probe.below(100);
        let affinity = probe.below(100);
        if affinity < SITE_AFFINITY_PCT {
            probe.below(SITE_AFFINITY_PATHS);
        }
        roll_terrain(&mut w, &Options::default());
        assert_eq!(w.crt, probe);
        let want = kelp < Options::default().kelp_part;
        assert_eq!(w.provinces[1].terrain & TERRAIN_FOREST != 0, want);
    }

    #[test]
    fn cave_provinces_use_the_pool_stream_not_the_crt_one() {
        let mut w = world(1);
        w.provinces[1].terrain = TERRAIN_CAVE;
        let pool_before = w.pool;
        roll_terrain(&mut w, &Options::default());
        assert_ne!(w.pool, pool_before);
    }

    #[test]
    fn site_affinity_sets_one_bit_in_the_documented_range() {
        let mut w = world(200);
        for p in w.provinces.iter_mut().skip(1) {
            p.terrain = TERRAIN_SEA;
        }
        roll_terrain(&mut w, &Options::default());
        let mask: i64 = (0..SITE_AFFINITY_PATHS)
            .map(|k| SITE_AFFINITY_BASE << k)
            .fold(0, |a, b| a | b);
        let mut seen = 0;
        for p in w.provinces.iter().skip(1) {
            let bits = p.terrain & mask;
            if bits != 0 {
                assert_eq!(bits.count_ones(), 1);
                seen += 1;
            }
        }
        assert!(seen > 0);
    }
}
