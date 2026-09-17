use dom6_simple_map_editor::namefix::{apply, plan, renamed, rewrite_map_text, safe_base, trap_in};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d6sme_namefix_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn finds_the_words_the_game_strips() {
    assert_eq!(trap_in("september_uw_waterbowl.tga"), Some("_water"));
    assert_eq!(trap_in("my_forestwalk"), Some("_forestw"));
    assert_eq!(trap_in("september_uw_bowl.tga"), None);
    assert_eq!(trap_in("waterbowl"), None);
}

#[test]
fn safe_base_drops_the_underscore_until_clean() {
    assert_eq!(safe_base("september_uw_waterbowl"), "september_uwwaterbowl");
    assert_eq!(safe_base("a_farm_winter"), "afarmwinter");
    assert_eq!(safe_base("plainmap"), "plainmap");
    assert_eq!(trap_in(&safe_base("x_kelpw_swamp_plain")), None);
}

#[test]
fn only_files_of_the_map_are_renamed_and_suffixes_survive() {
    assert_eq!(
        renamed("wb_water_winter.tga", "wb_water", "wbwater"),
        Some("wbwater_winter.tga".to_string())
    );
    assert_eq!(
        renamed("wb_water_plane2_forest.tga", "wb_water", "wbwater"),
        Some("wbwater_plane2_forest.tga".to_string())
    );
    assert_eq!(renamed("wb_waterfall.tga", "wb_water", "wbwater"), None);
    assert_eq!(renamed("banner.png", "wb_water", "wbwater"), None);
}

#[test]
fn map_text_changes_only_the_naming_lines() {
    let text = "#dom2title wb_water\r\n#imagefile wb_water.tga\r\n#winterimagefile wb_water_winter.tga\r\n#description \"wb_water\"\r\n#terrain 1 4\r\n";
    let out = rewrite_map_text(text, "wb_water", "wbwater");
    assert_eq!(
        out,
        "#dom2title wbwater\r\n#imagefile wbwater.tga\r\n#winterimagefile wbwater_winter.tga\r\n#description \"wb_water\"\r\n#terrain 1 4\r\n"
    );
}

#[test]
fn apply_renames_files_and_keeps_the_old_map_as_backup() {
    let d = temp_dir("apply");
    let write = |n: &str, t: &str| std::fs::write(d.join(n), t).unwrap();
    write(
        "wb_water.map",
        "#dom2title wb_water\n#imagefile wb_water.tga\n",
    );
    write("wb_water_plane2.map", "#imagefile wb_water_plane2.tga\n");
    write("wb_water.tga", "a");
    write("wb_water_winter.tga", "b");
    write("wb_water_plane2.tga", "c");
    write("banner.png", "d");
    let files: Vec<PathBuf> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let maps = vec![d.join("wb_water.map"), d.join("wb_water_plane2.map")];
    let p = plan(&files, &maps, "wb_water", "wbwater");
    assert_eq!(p.moves.len(), 5);
    assert!(p.rewrite.is_empty());
    let first = apply(&p).unwrap();
    assert_eq!(first, d.join("wbwater.map"));
    assert_eq!(
        std::fs::read_to_string(&first).unwrap(),
        "#dom2title wbwater\n#imagefile wbwater.tga\n"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("wbwater_plane2.map")).unwrap(),
        "#imagefile wbwater_plane2.tga\n"
    );
    assert!(d.join("wbwater_winter.tga").exists());
    assert!(d.join("wb_water.map.bak").exists());
    assert!(!d.join("wb_water.map").exists());
    assert!(d.join("banner.png").exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_map_with_its_own_stem_is_rewritten_in_place() {
    let d = temp_dir("stem");
    std::fs::write(d.join("wb.map"), "#imagefile pic_farm.tga\n").unwrap();
    std::fs::write(d.join("pic_farm.tga"), "a").unwrap();
    let files = vec![d.join("wb.map"), d.join("pic_farm.tga")];
    let maps = vec![d.join("wb.map")];
    let p = plan(&files, &maps, "pic_farm", "picfarm");
    assert_eq!(p.rewrite, maps);
    let first = apply(&p).unwrap();
    assert_eq!(first, d.join("wb.map"));
    assert_eq!(
        std::fs::read_to_string(&first).unwrap(),
        "#imagefile picfarm.tga\n"
    );
    assert!(d.join("wb.map.bak").exists());
    assert!(d.join("picfarm.tga").exists());
    let _ = std::fs::remove_dir_all(&d);
}
