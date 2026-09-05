use dom6_simple_map_editor::d6m::{
    stored_from_units, units_from_stored, D6m, Province, END_MAGIC, MAGIC,
};
use dom6_simple_map_editor::mapfile::{plane_file_name, strip_plane_suffix, MapFile};
use std::path::Path;

fn sample(version: i32) -> D6m {
    let w = 5;
    let h = 3;
    let mut heights = Vec::new();
    let mut owners = Vec::new();
    for i in 0..(w * h) {
        heights.push((i as i16 - 7) * 16);
        owners.push(if i % 5 == 0 { 0 } else { (i % 3) as i16 + 1 });
    }
    D6m {
        version,
        width: w,
        height: h,
        passthrough: 0,
        scale_frac: 0x8000,
        scale_int: 51,
        provinces: vec![
            Province {
                x: 1,
                y: 1,
                terrain: 4,
            },
            Province {
                x: 2,
                y: 2,
                terrain: 0,
            },
            Province {
                x: 3,
                y: 0,
                terrain: 128,
            },
        ],
        heights,
        owners,
        trailing: Vec::new(),
    }
}

#[test]
fn roundtrip_synthetic_versions() {
    for v in [1, 2, 3] {
        let d = sample(v);
        let bytes = d.to_bytes();
        let back = D6m::parse(&bytes).unwrap();
        assert_eq!(back.width, 5);
        assert_eq!(back.height, 3);
        assert_eq!(back.heights, d.heights);
        assert_eq!(back.owners, d.owners);
        assert_eq!(back.to_bytes(), bytes);
        if v > 2 {
            assert_eq!(back.provinces, d.provinces);
        }
        if v > 1 {
            assert_eq!(back.passthrough, 0);
        }
    }
}

#[test]
fn header_layout_is_34_bytes_for_version_3() {
    let d = sample(3);
    let bytes = d.to_bytes();
    assert_eq!(&bytes[0..4], &MAGIC.to_le_bytes());
    assert_eq!(bytes.len(), 34 + 3 * 12 + 15 * 4 + 4);
    assert_eq!(&bytes[bytes.len() - 4..], &END_MAGIC.to_le_bytes());
    assert_eq!(i32::from_le_bytes(bytes[0x1e..0x22].try_into().unwrap()), 3);
    assert_eq!(
        u16::from_le_bytes(bytes[0x18..0x1a].try_into().unwrap()),
        0x8000
    );
}

#[test]
fn map_scale_reassembles_fraction() {
    let mut d = sample(3);
    assert!((d.map_scale() - 51.5).abs() < 1e-6);
    d.scale_frac = 0xffff;
    assert!((d.map_scale() - 52.0).abs() < 1e-4);
}

#[test]
fn rejects_bad_magic_and_truncation() {
    let d = sample(3);
    let mut bytes = d.to_bytes();
    assert!(D6m::parse(&bytes[..bytes.len() - 6]).is_err());
    bytes[0] ^= 1;
    assert!(D6m::parse(&bytes).is_err());
}

#[test]
fn height_quantisation_matches_engine() {
    assert_eq!(stored_from_units(-20.0), -320);
    assert_eq!(units_from_stored(-320), -20.0);
    assert_eq!(stored_from_units(5000.0), 32000);
    assert_eq!(stored_from_units(-5000.0), -32000);
    assert_eq!(units_from_stored(1), 0.0625);
}

#[test]
fn plane_names() {
    assert_eq!(strip_plane_suffix("mymap"), ("mymap".to_string(), 1));
    assert_eq!(strip_plane_suffix("mymap_plane2"), ("mymap".to_string(), 2));
    assert_eq!(
        strip_plane_suffix("my_plane_x"),
        ("my_plane_x".to_string(), 1)
    );
    assert_eq!(plane_file_name("mymap", 1, "d6m"), "mymap.d6m");
    assert_eq!(plane_file_name("mymap", 3, "map"), "mymap_plane3.map");
}

#[test]
fn mapfile_parse_and_terrain_rewrite() {
    let text = "#dom2title test\r\n#imagefile test.d6m\r\n#hwraparound\r\n\r\n#landname 1 \"First Land\"\r\n#terrain 1 4\r\n#terrain 2 128\r\n#neighbour 1 2\r\n#neighbourspec 1 2 2\r\n";
    let mut m = MapFile::parse(text, Path::new("test.map"));
    assert_eq!(m.title, "test");
    assert_eq!(m.imagefile.as_deref(), Some("test.d6m"));
    assert!(m.hwrap && !m.vwrap);
    assert_eq!(m.name_of(1), Some("First Land"));
    assert_eq!(m.terrain[&2], 128);
    assert_eq!(m.spec_between(2, 1), 2);
    m.set_terrain(2, 132);
    m.set_terrain(3, 4);
    let out = m.to_text();
    assert!(out.contains("#terrain 2 132\r\n#terrain 3 4\r\n#neighbour 1 2"));
    assert!(out.starts_with("#dom2title test\r\n"));
    assert!(m.modified);
    m.modified = false;
    m.set_terrain(3, 4);
    assert!(!m.modified);
}

#[test]
fn real_recipes_roundtrip_byte_exact() {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return;
    };
    let dir = Path::new(&appdata).join("Dominions6").join("maps");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut checked = 0;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("d6m") {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let d = D6m::parse(&bytes).unwrap();
        assert_eq!(d.to_bytes(), bytes, "{}", p.display());
        checked += 1;
        if checked >= 4 {
            break;
        }
    }
}
