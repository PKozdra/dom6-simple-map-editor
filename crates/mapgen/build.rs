use std::env;
use std::fs;
use std::path::Path;

const POOL_LEN: usize = 500;

fn synthetic_pool() -> Vec<u8> {
    let mut state: u64 = 0x646f_6d36_6d61_7067;
    let mut out = Vec::with_capacity(POOL_LEN * 4);
    for _ in 0..POOL_LEN {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        out.extend_from_slice(&((z >> 32) as u32).to_le_bytes());
    }
    out
}

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let src = Path::new(&manifest).join("src").join("rng_pool.bin");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("rng_pool.bin");
    println!("cargo:rerun-if-changed=src/rng_pool.bin");
    println!("cargo:rustc-check-cfg=cfg(engine_pool)");
    println!("cargo:rerun-if-env-changed=DOM6_MAPGEN_SYNTHETIC_POOL");
    let forced = env::var_os("DOM6_MAPGEN_SYNTHETIC_POOL").is_some();
    match fs::read(&src) {
        Ok(bytes) if bytes.len() == POOL_LEN * 4 && !forced => {
            fs::write(&out, bytes).unwrap();
            println!("cargo:rustc-cfg=engine_pool");
        }
        _ => fs::write(&out, synthetic_pool()).unwrap(),
    }
}
