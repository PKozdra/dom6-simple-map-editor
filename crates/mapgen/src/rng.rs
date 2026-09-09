pub const POOL_LEN: usize = 500;
const POOL_BYTES: &[u8; POOL_LEN * 4] = include_bytes!(concat!(env!("OUT_DIR"), "/rng_pool.bin"));
pub const ENGINE_POOL: bool = cfg!(engine_pool);

pub fn pool_id() -> String {
    let kind = if ENGINE_POOL { "engine" } else { "synthetic" };
    format!("{kind} {:016x}", crate::hash::fnv64(POOL_BYTES))
}
pub const POOL: [u32; POOL_LEN] = decode_pool();

const fn decode_pool() -> [u32; POOL_LEN] {
    let mut out = [0u32; POOL_LEN];
    let mut i = 0;
    while i < POOL_LEN {
        let b = i * 4;
        out[i] = u32::from_le_bytes([
            POOL_BYTES[b],
            POOL_BYTES[b + 1],
            POOL_BYTES[b + 2],
            POOL_BYTES[b + 3],
        ]);
        i += 1;
    }
    out
}

const ROTATE_LAST: i32 = 29;
const XOR_KEY_ADVANCE: u32 = 0x1021;
const FLOAT_MOD: u32 = 50001;
const FLOAT_SCALE: f64 = 2e-5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolRng {
    pub index: i32,
    pub rotate: i32,
    pub xor_key: u32,
}

impl PoolRng {
    pub fn seeded(seed: u32) -> Self {
        let mut r = PoolRng {
            index: 0,
            rotate: 0,
            xor_key: 0,
        };
        r.seedrand(seed);
        r
    }

    pub fn seedrand(&mut self, seed: u32) {
        self.xor_key = seed;
        let mut idx = (seed & 0x1ff) as i32;
        if idx > POOL_LEN as i32 - 1 {
            idx %= POOL_LEN as i32;
        }
        self.index = idx;
        self.rotate = ((seed as i32) >> 10) & 0xf;
    }

    fn derive(&self) -> u32 {
        let v = POOL[self.index as usize];
        let rot = self.rotate as u32;
        (v.wrapping_shl(30u32.wrapping_sub(rot) & 31) | (v >> (rot & 31))) ^ self.xor_key
    }

    fn step(&mut self) {
        self.index += 1;
        if self.index > POOL_LEN as i32 - 1 {
            self.index = 0;
            self.rotate += 1;
            if self.rotate > ROTATE_LAST {
                self.xor_key = self.xor_key.wrapping_add(XOR_KEY_ADVANCE);
                self.rotate = 0;
            }
        }
    }

    pub fn advanced(&self, k: u64) -> Self {
        let total = self.index as u64 + k;
        let carries = total / POOL_LEN as u64;
        let rt = self.rotate as u64 + carries;
        PoolRng {
            index: (total % POOL_LEN as u64) as i32,
            rotate: (rt % (ROTATE_LAST as u64 + 1)) as i32,
            xor_key: self
                .xor_key
                .wrapping_add(XOR_KEY_ADVANCE.wrapping_mul((rt / (ROTATE_LAST as u64 + 1)) as u32)),
        }
    }

    pub fn rnd(&mut self, n: u32) -> u32 {
        if (n as i32) < 2 {
            return 0;
        }
        let x = self.derive();
        self.step();
        match n {
            100 => x % 100,
            _ => x % n,
        }
    }

    pub fn rndfloat(&mut self) -> f32 {
        let x = self.derive();
        self.step();
        let v = x % FLOAT_MOD;
        (f64::from(v) * FLOAT_SCALE) as f32
    }
}

const CRT_MUL: u32 = 214_013;
const CRT_INC: u32 = 2_531_011;
const CRT_MUL2: u32 = CRT_MUL.wrapping_mul(CRT_MUL);
const CRT_INC2: u32 = CRT_INC.wrapping_mul(CRT_MUL).wrapping_add(CRT_INC);
const CRT_SCALE: f64 = 1.0 / 32767.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrtRng {
    pub state: u32,
}

impl CrtRng {
    pub fn seeded(seed: u32) -> Self {
        CrtRng { state: seed }
    }

    pub fn srand(&mut self, seed: u32) {
        self.state = seed;
    }

    pub fn rand(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(CRT_MUL).wrapping_add(CRT_INC);
        (self.state >> 16) & 0x7fff
    }

    pub fn below(&mut self, n: i32) -> i32 {
        if n < 1 {
            return 0;
        }
        let r = self.rand() as i32;
        match n {
            3 => r % 3,
            5 => r % 5,
            100 => r % 100,
            _ => r % n,
        }
    }

    pub fn float(&mut self) -> f32 {
        (f64::from(self.rand()) * CRT_SCALE) as f32
    }

    pub fn below3_pair(&mut self) -> (i32, i32) {
        let (a, b) = below3_pair_of(self.state);
        self.state = self.state.wrapping_mul(CRT_MUL2).wrapping_add(CRT_INC2);
        (a, b)
    }
}

#[inline]
pub fn below3_pair_of(state: u32) -> (i32, i32) {
    let s1 = state.wrapping_mul(CRT_MUL).wrapping_add(CRT_INC);
    let s2 = state.wrapping_mul(CRT_MUL2).wrapping_add(CRT_INC2);
    let a = ((s1 >> 16) & 0x7fff) % 3;
    let b = ((s2 >> 16) & 0x7fff) % 3;
    (a as i32, b as i32)
}

#[inline]
pub fn below_pair_of(state: u32, n: i32) -> (i32, i32) {
    if n < 1 {
        return (0, 0);
    }
    let s1 = state.wrapping_mul(CRT_MUL).wrapping_add(CRT_INC);
    let s2 = state.wrapping_mul(CRT_MUL2).wrapping_add(CRT_INC2);
    let a = ((s1 >> 16) & 0x7fff) as i32 % n;
    let b = ((s2 >> 16) & 0x7fff) as i32 % n;
    (a, b)
}

#[inline]
pub fn crt_advance2(state: u32) -> u32 {
    state.wrapping_mul(CRT_MUL2).wrapping_add(CRT_INC2)
}

#[inline]
pub fn crt_below_of(state: u32, n: i32) -> (i32, u32) {
    if n < 1 {
        return (0, state);
    }
    let next = state.wrapping_mul(CRT_MUL).wrapping_add(CRT_INC);
    let r = ((next >> 16) & 0x7fff) as i32;
    let v = match n {
        3 => r % 3,
        5 => r % 5,
        100 => r % 100,
        _ => r % n,
    };
    (v, next)
}

pub const NOISE_LEN: usize = 0x9c87;
pub const NOISE_LAST: usize = NOISE_LEN - 1;

#[derive(Clone, Debug)]
pub struct NoiseTable {
    pub values: Vec<f32>,
    pub cursor: usize,
}

impl NoiseTable {
    pub fn fill(crt: &mut CrtRng) -> Self {
        let values = (0..NOISE_LEN).map(|_| crt.float()).collect();
        NoiseTable { values, cursor: 0 }
    }

    pub fn advance(&mut self) -> f32 {
        self.cursor = (self.cursor + 1).min(NOISE_LAST);
        self.values[self.cursor]
    }

    pub fn advance_wrapping(&mut self) -> f32 {
        self.cursor = wrap_noise_cursor(self.cursor);
        self.values[self.cursor]
    }
}

pub fn wrap_noise_cursor(cursor: usize) -> usize {
    if cursor + 1 < NOISE_LEN {
        cursor + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_matches_msvc_rand_after_srand_1() {
        let mut r = CrtRng::seeded(1);
        assert_eq!(r.rand(), 41);
        assert_eq!(r.rand(), 18467);
        assert_eq!(r.rand(), 6334);
        assert_eq!(r.rand(), 26500);
    }

    #[test]
    fn below3_pair_matches_two_below_three_calls() {
        for seed in [1u32, 7, 12345, 0xdead_beef, 0] {
            let mut a = CrtRng::seeded(seed);
            let mut b = CrtRng::seeded(seed);
            for _ in 0..1000 {
                let want = (b.below(3), b.below(3));
                assert_eq!(a.below3_pair(), want);
                assert_eq!(a.state, b.state);
            }
        }
    }

    #[test]
    fn crt_below_of_matches_the_method() {
        for seed in [1u32, 99, 0x5555_5555] {
            let mut a = CrtRng::seeded(seed);
            let mut st = seed;
            for n in [0i32, 1, 3, 5, 100, 7, 250] {
                let want = a.below(n);
                let (got, next) = crt_below_of(st, n);
                st = next;
                assert_eq!(got, want);
                assert_eq!(st, a.state);
            }
        }
    }

    #[test]
    fn below_pair_matches_two_below_calls() {
        for n in [1i32, 2, 3, 17, 150, 1025] {
            let mut a = CrtRng::seeded(2026);
            let mut st = 2026u32;
            for _ in 0..500 {
                let want = (a.below(n), a.below(n));
                assert_eq!(below_pair_of(st, n), want, "n {n}");
                st = crt_advance2(st);
                assert_eq!(st, a.state);
            }
        }
    }

    #[test]
    fn crt_below_is_plain_modulo_and_rejects_small_n() {
        let mut a = CrtRng::seeded(7);
        let mut b = CrtRng::seeded(7);
        assert_eq!(a.below(0), 0);
        assert_eq!(a.below(10), (b.rand() as i32) % 10);
    }

    #[test]
    fn pool_rnd_below_two_does_not_consume() {
        let mut a = PoolRng::seeded(20260726);
        let b = a;
        assert_eq!(a.rnd(1), 0);
        assert_eq!(a.rnd(0), 0);
        assert_eq!(a, b);
        a.rnd(2);
        assert_ne!(a, b);
    }

    #[test]
    fn seedrand_layout() {
        let r = PoolRng::seeded(999);
        assert_eq!(r.xor_key, 999);
        assert_eq!(r.index, 999 & 0x1ff);
        assert_eq!(r.rotate, 0);
        let r = PoolRng::seeded(0x1ff);
        assert_eq!(r.index, 0x1ff % 500);
    }

    #[test]
    fn advanced_matches_repeated_stepping() {
        for seed in [1u32, 999, 20260908, 0xffff_ffff] {
            let base = PoolRng::seeded(seed);
            let mut walk = base;
            for k in 0..20_000u64 {
                assert_eq!(base.advanced(k), walk, "seed {seed} k {k}");
                walk.step();
            }
        }
        let base = PoolRng::seeded(7);
        let mut walk = base;
        for _ in 0..500_000 {
            walk.step();
        }
        assert_eq!(base.advanced(500_000), walk);
    }

    #[test]
    fn rndfloat_range() {
        let mut r = PoolRng::seeded(42);
        for _ in 0..10_000 {
            let f = r.rndfloat();
            assert!((0.0..=1.0).contains(&f));
        }
    }

    #[test]
    fn noise_table_consumes_exactly_its_length() {
        let mut a = CrtRng::seeded(5);
        let t = NoiseTable::fill(&mut a);
        let mut b = CrtRng::seeded(5);
        for _ in 0..NOISE_LEN {
            b.rand();
        }
        assert_eq!(a, b);
        assert_eq!(t.values.len(), NOISE_LEN);
        assert_eq!(t.cursor, 0);
    }
}
