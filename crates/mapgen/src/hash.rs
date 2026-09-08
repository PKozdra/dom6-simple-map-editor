const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug)]
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    pub fn new() -> Self {
        Fnv64(OFFSET)
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        let mut h = self.0;
        for &x in b {
            h ^= u64::from(x);
            h = h.wrapping_mul(PRIME);
        }
        self.0 = h;
        self
    }

    pub fn f32s(&mut self, v: &[f32]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }

    pub fn i16s(&mut self, v: &[i16]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }

    pub fn i32(&mut self, x: i32) -> &mut Self {
        self.bytes(&x.to_le_bytes())
    }

    pub fn i64(&mut self, x: i64) -> &mut Self {
        self.bytes(&x.to_le_bytes())
    }

    pub fn f32(&mut self, x: f32) -> &mut Self {
        self.bytes(&x.to_le_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

pub fn fnv64(b: &[u8]) -> u64 {
    Fnv64::new().bytes(b).finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn typed_feeds_match_bytes() {
        let a = Fnv64::new().f32(1.5).i16s(&[-1, 2]).finish();
        let mut raw = Vec::new();
        raw.extend_from_slice(&1.5f32.to_le_bytes());
        raw.extend_from_slice(&(-1i16).to_le_bytes());
        raw.extend_from_slice(&2i16.to_le_bytes());
        assert_eq!(a, fnv64(&raw));
    }
}
