use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct MapFile {
    pub path: PathBuf,
    lines: Vec<String>,
    newline: String,
    pub title: String,
    pub imagefile: Option<String>,
    pub mapsize: Option<(i32, i32)>,
    pub hwrap: bool,
    pub vwrap: bool,
    pub terrain: BTreeMap<u32, i64>,
    pub names: BTreeMap<u32, String>,
    pub gates: BTreeMap<u32, i32>,
    pub neighbours: Vec<(u32, u32)>,
    pub specs: BTreeMap<(u32, u32), i64>,
    pub pb_count: usize,
    pub modified: bool,
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn parse_i64(s: &str) -> Option<i64> {
    let t = s.trim();
    t.parse::<i64>()
        .ok()
        .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
}

fn command_of(line: &str) -> Option<(&str, &str)> {
    let l = line.trim_start();
    if !l.starts_with('#') {
        return None;
    }
    Some(match l.find(char::is_whitespace) {
        Some(i) => (&l[..i], l[i..].trim()),
        None => (l, ""),
    })
}

fn first_two(rest: &str) -> Option<(u32, u32)> {
    let mut it = rest.split_whitespace();
    let a = it.next()?.parse::<u32>().ok()?;
    let b = it.next()?.parse::<u32>().ok()?;
    Some((a, b))
}

fn pair(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

impl MapFile {
    pub fn parse(text: &str, path: &Path) -> MapFile {
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" }.to_string();
        let lines: Vec<String> = text
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        let mut m = MapFile {
            path: path.to_path_buf(),
            lines,
            newline,
            ..Default::default()
        };
        for line in &m.lines {
            let Some((cmd, rest)) = command_of(line) else {
                continue;
            };
            let args: Vec<&str> = rest.split_whitespace().collect();
            match cmd {
                "#dom2title" => m.title = rest.to_string(),
                "#imagefile" => m.imagefile = Some(rest.to_string()),
                "#mapsize" => {
                    if args.len() >= 2 {
                        if let (Ok(w), Ok(h)) = (args[0].parse(), args[1].parse()) {
                            m.mapsize = Some((w, h));
                        }
                    }
                }
                "#wraparound" => {
                    m.hwrap = true;
                    m.vwrap = true;
                }
                "#hwraparound" => m.hwrap = true,
                "#vwraparound" => m.vwrap = true,
                "#terrain" => {
                    if args.len() >= 2 {
                        if let (Ok(p), Some(v)) = (args[0].parse::<u32>(), parse_i64(args[1])) {
                            m.terrain.insert(p, v);
                        }
                    }
                }
                "#gate" => {
                    if args.len() >= 2 {
                        if let (Ok(p), Ok(v)) = (args[0].parse::<u32>(), args[1].parse::<i32>()) {
                            m.gates.insert(p, v);
                        }
                    }
                }
                "#landname" => {
                    if let Some(i) = rest.find(char::is_whitespace) {
                        if let Ok(p) = rest[..i].parse::<u32>() {
                            m.names.insert(p, unquote(&rest[i..]));
                        }
                    }
                }
                "#neighbour" => {
                    if let Some((a, b)) = first_two(rest) {
                        if !m.neighbours.contains(&pair(a, b)) {
                            m.neighbours.push(pair(a, b));
                        }
                    }
                }
                "#neighbourspec" => {
                    if args.len() >= 3 {
                        if let (Some((a, b)), Some(s)) = (first_two(rest), parse_i64(args[2])) {
                            m.specs.insert(pair(a, b), s);
                        }
                    }
                }
                "#pb" => m.pb_count += 1,
                _ => {}
            }
        }
        m
    }

    pub fn load(path: &Path) -> std::io::Result<MapFile> {
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(MapFile::parse(&text, path))
    }

    pub fn spec_between(&self, a: u32, b: u32) -> i64 {
        self.specs.get(&pair(a, b)).copied().unwrap_or(0)
    }

    pub fn are_neighbours(&self, a: u32, b: u32) -> bool {
        self.neighbours.contains(&pair(a, b))
    }

    pub fn neighbours_of(&self, p: u32) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .neighbours
            .iter()
            .filter_map(|&(a, b)| {
                if a == p {
                    Some(b)
                } else if b == p {
                    Some(a)
                } else {
                    None
                }
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    pub fn name_of(&self, p: u32) -> Option<&str> {
        self.names.get(&p).map(String::as_str)
    }

    pub fn gate_of(&self, p: u32) -> i32 {
        self.gates.get(&p).copied().unwrap_or(0)
    }

    fn find_line(&self, pred: impl Fn(&str, &str) -> bool) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| command_of(l).map(|(c, r)| pred(c, r)).unwrap_or(false))
    }

    fn last_line_with(&self, cmd: &str) -> Option<usize> {
        self.lines
            .iter()
            .rposition(|l| command_of(l).map(|(c, _)| c == cmd).unwrap_or(false))
    }

    fn insert_after_last(&mut self, anchors: &[&str], line: String) {
        for a in anchors {
            if let Some(i) = self.last_line_with(a) {
                self.lines.insert(i + 1, line);
                return;
            }
        }
        while self.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            self.lines.pop();
        }
        self.lines.push(String::new());
        self.lines.push(line);
        self.lines.push(String::new());
    }

    fn remove_where(&mut self, pred: impl Fn(&str, &str) -> bool) {
        self.lines
            .retain(|l| !command_of(l).map(|(c, r)| pred(c, r)).unwrap_or(false));
    }

    fn upsert(&mut self, matches: impl Fn(&str, &str) -> bool, line: String, anchors: &[&str]) {
        match self.find_line(&matches) {
            Some(i) => {
                self.lines[i] = line;
                let mut idx = 0;
                self.lines.retain(|l| {
                    let dup =
                        idx != i && command_of(l).map(|(c, r)| matches(c, r)).unwrap_or(false);
                    idx += 1;
                    !dup
                });
            }
            None => self.insert_after_last(anchors, line),
        }
        self.modified = true;
    }

    pub fn set_imagefile(&mut self, name: &str) {
        self.imagefile = Some(name.to_string());
        self.upsert(
            |c, _| c == "#imagefile",
            format!("#imagefile {name}"),
            &["#dom2title", "#imagefile"],
        );
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
        self.upsert(
            |c, _| c == "#dom2title",
            format!("#dom2title {title}"),
            &["#dom2title"],
        );
    }

    pub fn set_terrain(&mut self, p: u32, mask: i64) {
        if self.terrain.get(&p) == Some(&mask) {
            return;
        }
        self.terrain.insert(p, mask);
        let ps = p.to_string();
        self.upsert(
            |c, r| c == "#terrain" && r.split_whitespace().next() == Some(ps.as_str()),
            format!("#terrain {p} {mask}"),
            &["#terrain", "#landname"],
        );
    }

    pub fn set_name(&mut self, p: u32, name: Option<&str>) {
        let current = self.names.get(&p).map(String::as_str);
        if current == name {
            return;
        }
        let ps = p.to_string();
        match name {
            Some(n) if !n.trim().is_empty() => {
                let clean = n.replace('"', "'");
                self.names.insert(p, clean.clone());
                self.upsert(
                    |c, r| c == "#landname" && r.split_whitespace().next() == Some(ps.as_str()),
                    format!("#landname {p} \"{clean}\""),
                    &["#landname", "#terrain"],
                );
            }
            _ => {
                self.names.remove(&p);
                self.remove_where(|c, r| {
                    c == "#landname" && r.split_whitespace().next() == Some(ps.as_str())
                });
                self.modified = true;
            }
        }
    }

    pub fn set_gate(&mut self, p: u32, n: i32) {
        if self.gate_of(p) == n {
            return;
        }
        let ps = p.to_string();
        if n == 0 {
            self.gates.remove(&p);
            self.remove_where(|c, r| {
                c == "#gate" && r.split_whitespace().next() == Some(ps.as_str())
            });
            self.modified = true;
        } else {
            self.gates.insert(p, n);
            self.upsert(
                |c, r| c == "#gate" && r.split_whitespace().next() == Some(ps.as_str()),
                format!("#gate {p} {n}"),
                &["#gate", "#terrain", "#landname"],
            );
        }
    }

    pub fn set_neighbour(&mut self, a: u32, b: u32, present: bool) {
        let (a, b) = pair(a, b);
        if a == b || self.are_neighbours(a, b) == present {
            return;
        }
        let same = move |r: &str| {
            first_two(r)
                .map(|(x, y)| pair(x, y) == (a, b))
                .unwrap_or(false)
        };
        if present {
            self.neighbours.push((a, b));
            self.insert_after_last(&["#neighbour", "#terrain"], format!("#neighbour {a} {b}"));
        } else {
            self.neighbours.retain(|&n| n != (a, b));
            self.specs.remove(&(a, b));
            self.remove_where(|c, r| (c == "#neighbour" || c == "#neighbourspec") && same(r));
        }
        self.modified = true;
    }

    pub fn set_spec(&mut self, a: u32, b: u32, spec: i64) {
        let (a, b) = pair(a, b);
        if self.spec_between(a, b) == spec {
            return;
        }
        let same = move |r: &str| {
            first_two(r)
                .map(|(x, y)| pair(x, y) == (a, b))
                .unwrap_or(false)
        };
        if spec == 0 {
            self.specs.remove(&(a, b));
            self.remove_where(|c, r| c == "#neighbourspec" && same(r));
            self.modified = true;
        } else {
            self.specs.insert((a, b), spec);
            self.upsert(
                |c, r| c == "#neighbourspec" && same(r),
                format!("#neighbourspec {a} {b} {spec}"),
                &["#neighbourspec", "#neighbour"],
            );
        }
    }

    pub fn has_pb(&self) -> bool {
        self.pb_count > 0
    }

    pub fn replace_pb(&mut self, w: i32, h: i32, owners: &[i16]) {
        let first = self.find_line(|c, _| c == "#pb");
        let mut runs = Vec::new();
        for y in 0..h {
            let row = (y * w) as usize;
            let mut x = 0i32;
            while x < w {
                let id = owners[row + x as usize];
                if id <= 0 {
                    x += 1;
                    continue;
                }
                let mut len = 1;
                while x + len < w && owners[row + (x + len) as usize] == id {
                    len += 1;
                }
                runs.push(format!("#pb {x} {y} {len} {id}"));
                x += len;
            }
        }
        self.remove_where(|c, _| c == "#pb");
        let at = match first {
            Some(i) => i.min(self.lines.len()),
            None => {
                self.lines.push(String::new());
                self.lines.len()
            }
        };
        let tail = self.lines.split_off(at);
        self.lines.extend(runs);
        self.lines.extend(tail);
        self.pb_count = self
            .lines
            .iter()
            .filter(|l| l.trim_start().starts_with("#pb "))
            .count();
        self.modified = true;
    }

    pub fn to_text(&self) -> String {
        self.lines.join(&self.newline)
    }
}

pub fn strip_plane_suffix(stem: &str) -> (String, u32) {
    if let Some(i) = stem.rfind("_plane") {
        let tail = &stem[i + 6..];
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = tail.parse::<u32>() {
                return (stem[..i].to_string(), n);
            }
        }
    }
    (stem.to_string(), 1)
}

pub fn plane_file_name(base: &str, plane: u32, ext: &str) -> String {
    if plane <= 1 {
        format!("{base}.{ext}")
    } else {
        format!("{base}_plane{plane}.{ext}")
    }
}
