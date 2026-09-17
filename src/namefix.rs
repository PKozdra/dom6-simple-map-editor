use crate::imagemap::STRIPPED_WORDS;
use std::path::{Path, PathBuf};

pub fn trap_in(name: &str) -> Option<&'static str> {
    STRIPPED_WORDS.iter().copied().find(|w| name.contains(w))
}

pub fn safe_base(base: &str) -> String {
    let mut out = base.to_string();
    while let Some(word) = trap_in(&out) {
        out = out.replacen(word, &word[1..], 1);
    }
    out
}

pub fn renamed(file_name: &str, old: &str, new: &str) -> Option<String> {
    let rest = file_name.strip_prefix(old)?;
    if rest.starts_with('.') || rest.starts_with('_') {
        Some(format!("{new}{rest}"))
    } else {
        None
    }
}

pub fn rewrite_map_text(text: &str, old: &str, new: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    text.split('\n')
        .map(|raw| {
            let line = raw.trim_end_matches('\r');
            let head = line.trim_start();
            let named = ["#imagefile", "#winterimagefile", "#dom2title"]
                .iter()
                .any(|c| head.starts_with(c));
            if named {
                line.replace(old, new)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(newline)
}

pub struct Plan {
    pub old: String,
    pub new: String,
    pub moves: Vec<(PathBuf, PathBuf)>,
    pub rewrite: Vec<PathBuf>,
}

pub fn plan(files: &[PathBuf], maps: &[PathBuf], old: &str, new: &str) -> Plan {
    let mut moves = Vec::new();
    for f in files {
        let Some(name) = f.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with(".bak") {
            continue;
        }
        if let Some(to) = renamed(name, old, new) {
            moves.push((f.clone(), f.with_file_name(to)));
        }
    }
    moves.sort();
    let rewrite = maps
        .iter()
        .filter(|m| !moves.iter().any(|(from, _)| from == *m))
        .cloned()
        .collect();
    Plan {
        old: old.to_string(),
        new: new.to_string(),
        moves,
        rewrite,
    }
}

pub fn apply(plan: &Plan) -> Result<PathBuf, String> {
    for (_, to) in &plan.moves {
        if crate::io::exists(to) {
            return Err(format!("{} already exists", to.display()));
        }
    }
    let mut first_map = None;
    for map in &plan.rewrite {
        let bytes = crate::io::read(map).map_err(|e| format!("{}: {e}", map.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let fixed = rewrite_map_text(&text, &plan.old, &plan.new);
        crate::project::write_with_backup(map, fixed.as_bytes())?;
        let stem = map.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if crate::mapfile::strip_plane_suffix(stem).1 == 1 {
            first_map = Some(map.clone());
        }
    }
    for (from, to) in &plan.moves {
        let is_map = to
            .extension()
            .map(|e| e.eq_ignore_ascii_case("map"))
            .unwrap_or(false);
        if is_map {
            let bytes = crate::io::read(from).map_err(|e| format!("{}: {e}", from.display()))?;
            let text = String::from_utf8_lossy(&bytes);
            let fixed = rewrite_map_text(&text, &plan.old, &plan.new);
            crate::io::write(to, fixed.as_bytes())?;
            crate::io::rename(from, &crate::project::backup_path(from))?;
            let stem = to.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if crate::mapfile::strip_plane_suffix(stem).1 == 1 && first_map.is_none() {
                first_map = Some(to.clone());
            }
        } else {
            crate::io::rename(from, to)?;
        }
    }
    first_map.ok_or_else(|| "no .map among the renamed files".to_string())
}

pub fn base_of(map_path: &Path) -> Option<String> {
    let stem = map_path.file_stem()?.to_str()?;
    Some(crate::mapfile::strip_plane_suffix(stem).0)
}
