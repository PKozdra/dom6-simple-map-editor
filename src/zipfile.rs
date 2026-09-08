fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn build(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let name = name.as_bytes();
        let crc = crc32(data);
        let offset = out.len() as u32;
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 20);
        put16(&mut out, 0x0800);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0x21);
        put32(&mut out, crc);
        put32(&mut out, data.len() as u32);
        put32(&mut out, data.len() as u32);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 20);
        put16(&mut central, 0x0800);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0x21);
        put32(&mut central, crc);
        put32(&mut central, data.len() as u32);
        put32(&mut central, data.len() as u32);
        put16(&mut central, name.len() as u16);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let central_offset = out.len() as u32;
    let central_len = central.len() as u32;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, files.len() as u16);
    put16(&mut out, files.len() as u16);
    put32(&mut out, central_len);
    put32(&mut out, central_offset);
    put16(&mut out, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn the_archive_lists_every_file_in_the_central_directory() {
        let z = build(&[
            ("a.map".to_string(), b"#dom2title a\n".to_vec()),
            ("a.d6m".to_string(), vec![1, 2, 3]),
        ]);
        assert_eq!(&z[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let end = z.len() - 22;
        assert_eq!(&z[end..end + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([z[end + 10], z[end + 11]]), 2);
        let central_offset =
            u32::from_le_bytes([z[end + 16], z[end + 17], z[end + 18], z[end + 19]]) as usize;
        assert_eq!(
            &z[central_offset..central_offset + 4],
            &[0x50, 0x4b, 0x01, 0x02]
        );
        assert!(z.windows(5).any(|w| w == b"a.d6m"));
    }
}
