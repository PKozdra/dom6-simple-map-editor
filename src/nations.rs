use crate::terrain::{CAVE, DEEP_SEA, FARM, FOREST, HIGHLAND, MOUNTAIN, SWAMP, WASTE};

pub const FIRST_MOD_ID: u32 = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nation {
    pub id: u32,
    pub name: String,
    pub epithet: String,
    pub era: u8,
    pub likesterr: u64,
    pub uw: bool,
    pub coast: bool,
    pub cave: u8,
    pub river: bool,
    pub from_mod: bool,
}

impl Nation {
    pub fn blank(id: u32) -> Nation {
        Nation {
            id,
            name: format!("Nation {id}"),
            epithet: String::new(),
            era: 0,
            likesterr: 0,
            uw: false,
            coast: false,
            cave: 0,
            river: false,
            from_mod: true,
        }
    }

    pub fn title(&self) -> String {
        if self.epithet.is_empty() {
            self.name.clone()
        } else {
            format!("{}, {}", self.name, self.epithet)
        }
    }

    pub fn start_label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.uw {
            parts.push(if self.likesterr & DEEP_SEA != 0 {
                "deep sea"
            } else {
                "sea"
            });
        } else if self.cave >= 2 {
            parts.push("cave");
        } else if self.coast {
            parts.push("coast");
        }
        for (bit, name) in [
            (FOREST, "forest"),
            (WASTE, "waste"),
            (HIGHLAND, "highland"),
            (SWAMP, "swamp"),
            (FARM, "farm"),
            (MOUNTAIN, "mountain"),
            (CAVE, "cave"),
        ] {
            if self.likesterr & bit != 0 && !parts.contains(&name) {
                parts.push(name);
            }
        }
        if self.cave == 1 && !parts.contains(&"cave") {
            parts.push("cave or land");
        }
        if parts.is_empty() {
            "any land".to_string()
        } else {
            parts.join(", ")
        }
    }
}

pub fn era_label(era: u8) -> &'static str {
    match era {
        1 => "Early",
        2 => "Middle",
        3 => "Late",
        _ => "Any",
    }
}

pub fn vanilla() -> Vec<Nation> {
    let text = include_str!("../assets/nations.tsv");
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 9 {
                return None;
            }
            Some(Nation {
                id: f[0].parse().ok()?,
                name: f[1].to_string(),
                epithet: f[2].to_string(),
                era: f[3].parse().ok()?,
                likesterr: f[4].parse::<i64>().ok()? as u64,
                uw: f[5] != "0",
                coast: f[6] != "0",
                cave: f[7].parse().unwrap_or(0),
                river: f[8] != "0",
                from_mod: false,
            })
        })
        .collect()
}

fn strip_comment(line: &str) -> &str {
    let mut in_quote = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'"' {
            in_quote = !in_quote;
        } else if !in_quote && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn first_number(rest: &str) -> Option<i64> {
    rest.split_whitespace().next()?.parse().ok()
}

pub fn parse_dm(text: &str, known: &[Nation]) -> Vec<Nation> {
    let mut out: Vec<Nation> = Vec::new();
    let mut current: Option<Nation> = None;
    let mut other_block = false;
    let taken = |id: u32, out: &[Nation]| {
        known.iter().any(|n| n.id == id) || out.iter().any(|n| n.id == id)
    };
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if !line.starts_with('#') {
            continue;
        }
        let (cmd, rest) = match line.find(char::is_whitespace) {
            Some(i) => (&line[..i], line[i..].trim()),
            None => (line, ""),
        };
        if current.is_none() {
            match cmd {
                "#selectnation" => {
                    let Some(id) = first_number(rest) else {
                        continue;
                    };
                    let id = id.max(0) as u32;
                    let mut n = known
                        .iter()
                        .find(|n| n.id == id)
                        .cloned()
                        .unwrap_or_else(|| Nation::blank(id));
                    n.from_mod = true;
                    current = Some(n);
                }
                "#newnation" => {
                    let mut id = FIRST_MOD_ID;
                    while taken(id, &out) {
                        id += 1;
                    }
                    current = Some(Nation::blank(id));
                }
                "#end" => other_block = false,
                c if c.starts_with("#new") || c.starts_with("#select") => other_block = true,
                _ => {}
            }
            continue;
        }
        if other_block {
            continue;
        }
        let n = current.as_mut().unwrap();
        match cmd {
            "#end" => {
                let done = current.take().unwrap();
                out.retain(|n| n.id != done.id);
                out.push(done);
            }
            "#name" => n.name = unquote(rest),
            "#epithet" => n.epithet = unquote(rest),
            "#era" => n.era = first_number(rest).unwrap_or(0).clamp(0, 3) as u8,
            "#likesterr" => n.likesterr = first_number(rest).unwrap_or(0).max(0) as u64,
            "#uwnation" => n.uw = true,
            "#coastnation" => n.coast = true,
            "#cavenation" => n.cave = first_number(rest).unwrap_or(0).clamp(0, 3) as u8,
            "#riverstart" => n.river = true,
            "#clearnation" => {
                n.likesterr = 0;
                n.uw = false;
                n.coast = false;
                n.cave = 0;
                n.river = false;
            }
            _ => {}
        }
    }
    if let Some(n) = current {
        out.retain(|o| o.id != n.id);
        out.push(n);
    }
    out
}

pub fn merge(base: &mut Vec<Nation>, extra: Vec<Nation>) {
    for n in extra {
        match base.iter_mut().find(|b| b.id == n.id) {
            Some(slot) => *slot = n,
            None => base.push(n),
        }
    }
    base.sort_by_key(|n| n.id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vanilla_table_carries_the_start_preferences() {
        let all = vanilla();
        assert!(all.len() >= 100);
        let atlantis = all.iter().find(|n| n.id == 43).unwrap();
        assert!(atlantis.uw);
        assert_eq!(atlantis.era, 1);
        assert_eq!(atlantis.start_label(), "deep sea");
        let pangaea = all.iter().find(|n| n.id == 7).unwrap();
        assert_eq!(pangaea.start_label(), "forest");
        let agartha = all.iter().find(|n| n.id == 15).unwrap();
        assert_eq!(agartha.cave, 2);
        assert!(agartha.start_label().starts_with("cave"));
    }

    #[test]
    fn a_mod_adds_and_changes_nations() {
        let known = vanilla();
        let dm = r#"
#modname "x"
#newmonster 5000
#name "not a nation"
#end
#newnation
#name "Frogfolk" -- comment
#epithet "Swamp Kings"
#era 2
#likesterr 32
#end
#selectnation 7
#likesterr 64
#end
#newnation
#name "Second"
#era 1
#uwnation
#end
"#;
        let got = parse_dm(dm, &known);
        let mut free = FIRST_MOD_ID;
        while known.iter().any(|n| n.id == free) {
            free += 1;
        }
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].id, free);
        assert_eq!(got[0].name, "Frogfolk");
        assert_eq!(got[0].epithet, "Swamp Kings");
        assert_eq!(got[0].era, 2);
        assert_eq!(got[0].start_label(), "swamp");
        assert_eq!(got[1].id, 7);
        assert_eq!(got[1].name, "Pangaea");
        assert_eq!(got[1].likesterr, 64);
        assert!(got[1].from_mod);
        let mut free2 = free + 1;
        while known.iter().any(|n| n.id == free2) {
            free2 += 1;
        }
        assert_eq!(got[2].id, free2);
        assert!(got[2].uw);
        let mut all = known;
        merge(&mut all, got);
        assert_eq!(all.iter().filter(|n| n.id == 7).count(), 1);
        assert!(all.iter().any(|n| n.id == free2));
    }
}
