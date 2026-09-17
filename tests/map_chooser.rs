#![cfg(not(target_arch = "wasm32"))]

use dom6_simple_map_editor::d6m::{stored_from_units, D6m, Province};
use dom6_simple_map_editor::map_chooser::{
    folder_of, order, picture_preview, recipe_preview, stats_of, Kind, Slots,
};
use dom6_simple_map_editor::textures::Image;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("d6sme_chooser_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn order_is_case_insensitive_and_stable() {
    let root = Path::new("/root");
    let paths: Vec<PathBuf> = [
        "/root/Zeta/zeta.map",
        "/root/alpha.map",
        "/root/Beta/beta.map",
        "/root/aardvark/ax.map",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    let first = order(root, &paths);
    let rels: Vec<String> = first.iter().map(|p| folder_of(root, p)).collect();
    assert_eq!(rels, vec!["aardvark", "", "Beta", "Zeta"]);
    let mut reversed = paths.clone();
    reversed.reverse();
    assert_eq!(order(root, &reversed), first);
}

#[test]
fn prefix_reveal_never_reorders() {
    let mut slots: Slots<&str> = Slots::new(5);
    assert_eq!(slots.visible().len(), 0);
    let mut seen: Vec<&str> = Vec::new();
    for i in [3usize, 1, 4, 0, 2] {
        let names = ["a", "b", "c", "d", "e"];
        slots.put(i, names[i]);
        let now: Vec<&str> = slots.visible().into_iter().copied().collect();
        assert!(now.len() >= seen.len());
        assert_eq!(now[..seen.len()], seen[..]);
        seen = now;
    }
    assert_eq!(seen, vec!["a", "b", "c", "d", "e"]);
    assert_eq!(slots.filled(), 5);
}

#[test]
fn ready_only_grows_on_contiguous_fill() {
    let mut slots: Slots<u32> = Slots::new(3);
    slots.put(2, 20);
    assert_eq!(slots.ready(), 0);
    assert_eq!(slots.filled(), 1);
    slots.put(0, 0);
    assert_eq!(slots.ready(), 1);
    slots.put(1, 10);
    assert_eq!(slots.ready(), 3);
    assert_eq!(slots.visible(), vec![&0, &10, &20]);
}

fn tiny_d6m(w: i32, h: i32) -> Vec<u8> {
    let n = (w * h) as usize;
    let mut heights = vec![stored_from_units(120.0); n];
    for h in heights.iter_mut().take(n / 2) {
        *h = stored_from_units(-90.0);
    }
    D6m {
        version: 3,
        width: w,
        height: h,
        passthrough: 0,
        scale_frac: 0,
        scale_int: 1,
        provinces: vec![Province {
            x: 1,
            y: 1,
            terrain: 0,
        }],
        heights,
        owners: vec![1; n],
        trailing: Vec::new(),
    }
    .to_bytes()
}

fn tiny_tga(w: usize, h: usize) -> Vec<u8> {
    let img = Image {
        w,
        h,
        rgba: (0..w * h)
            .flat_map(|i| {
                let v = (i % 251) as u8;
                [v, 40, 200, 255]
            })
            .collect(),
    };
    dom6_simple_map_editor::tga::encode_rgba_bottom_up(&img)
}

#[test]
fn stats_of_a_recipe_map() {
    let dir = temp_dir("recipe");
    std::fs::write(dir.join("tiny.d6m"), tiny_d6m(40, 24)).unwrap();
    std::fs::write(
        dir.join("tiny.map"),
        "#dom2title tiny\n#imagefile tiny.d6m\n#mapsize 40 24\n#terrain 1 4\n#terrain 2 0\n#terrain 3 4\n",
    )
    .unwrap();
    let (stats, image) = stats_of(&dir.join("tiny.map")).unwrap();
    assert_eq!(stats.kind, Kind::Recipe);
    assert_eq!(stats.size, Some((40, 24)));
    assert_eq!(stats.provinces, 3);
    assert_eq!(stats.sea, 2);
    assert_eq!(stats.land, 1);
    assert_eq!(stats.planes, 1);
    let d = D6m::load(&image.unwrap()).unwrap();
    let preview = recipe_preview(&d).unwrap();
    assert_eq!((preview.w, preview.h), (40, 24));
    assert_eq!(preview.rgba.len(), 40 * 24 * 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stats_of_a_picture_map_with_two_planes() {
    let dir = temp_dir("picture");
    std::fs::write(dir.join("pic.tga"), tiny_tga(600, 300)).unwrap();
    std::fs::write(
        dir.join("pic.map"),
        "#dom2title pic\n#imagefile pic.tga\n#mapsize 600 300\n#terrain 1 0\n",
    )
    .unwrap();
    std::fs::write(dir.join("pic_plane2.map"), "#dom2title cave\n").unwrap();
    let (stats, image) = stats_of(&dir.join("pic.map")).unwrap();
    assert_eq!(stats.kind, Kind::Picture);
    assert_eq!(stats.planes, 2);
    assert_eq!(stats.sea, 0);
    assert_eq!(stats.land, 1);
    let bytes = std::fs::read(image.unwrap()).unwrap();
    let preview = picture_preview(&bytes).unwrap();
    let step = 600usize
        .div_ceil(dom6_simple_map_editor::map_chooser::PREVIEW_W)
        .max(1);
    assert_eq!(preview.w, 600usize.div_ceil(step));
    assert_eq!(preview.h, 300usize.div_ceil(step));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_map_reports_an_error() {
    let dir = temp_dir("broken");
    std::fs::write(dir.join("bad.map"), "#dom2title bad\n#imagefile bad.d6m\n").unwrap();
    std::fs::write(dir.join("bad.d6m"), b"not a d6m at all").unwrap();
    let (stats, image) = stats_of(&dir.join("bad.map")).unwrap();
    assert_eq!(stats.kind, Kind::Recipe);
    assert!(D6m::load(&image.unwrap()).is_err());
    assert!(stats_of(&dir.join("missing.map")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn maps_under_finds_subfolder_maps() {
    let dir = temp_dir("under");
    std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
    std::fs::write(dir.join("top.map"), "#dom2title top\n").unwrap();
    std::fs::write(dir.join("sub/mid.map"), "#dom2title mid\n").unwrap();
    std::fs::write(dir.join("sub/mid_plane2.map"), "#dom2title mid2\n").unwrap();
    std::fs::write(dir.join("sub/deep/low.map"), "#dom2title low\n").unwrap();
    let found = dom6_simple_map_editor::io::maps_under(&dir, 3);
    let names: Vec<String> = order(&dir, &found)
        .iter()
        .map(|p| {
            p.strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(names, vec!["sub/deep/low.map", "sub/mid.map", "top.map"]);
    assert_eq!(dom6_simple_map_editor::io::maps_under(&dir, 0).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
