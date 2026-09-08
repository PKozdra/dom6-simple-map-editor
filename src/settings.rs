use std::path::{Path, PathBuf};

pub const FOLDER_NAME: &str = "dom6-simple-map-editor";
pub const CONFIG_NAME: &str = "settings.txt";
pub const MAPS: &str = "maps";
pub const BLUEPRINTS: &str = "blueprints";
const DATA_FOLDER_KEY: &str = "data_folder";

fn env_dir(key: &str) -> Option<PathBuf> {
    let v = std::env::var_os(key)?;
    if v.is_empty() {
        None
    } else {
        Some(PathBuf::from(v))
    }
}

fn home() -> Option<PathBuf> {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE"))
}

fn same_component(a: &std::path::Component, b: &std::path::Component) -> bool {
    if cfg!(target_os = "windows") {
        a.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
    } else {
        a == b
    }
}

fn under(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut rest = path.components();
    for want in base.components() {
        let got = rest.next()?;
        if !same_component(&got, &want) {
            return None;
        }
    }
    Some(rest.as_path().to_path_buf())
}

pub struct ShownPath {
    pub placeholder: Option<&'static str>,
    pub rest: String,
}

impl ShownPath {
    pub fn text(&self) -> String {
        match self.placeholder {
            Some(p) => format!("{p}{}", self.rest),
            None => self.rest.clone(),
        }
    }
}

pub fn shown_parts(path: &Path) -> ShownPath {
    let mut bases: Vec<(&'static str, PathBuf)> = Vec::new();
    if cfg!(target_os = "windows") {
        if let Some(d) = env_dir("LOCALAPPDATA") {
            bases.push(("%LOCALAPPDATA%", d));
        }
        if let Some(d) = env_dir("APPDATA") {
            bases.push(("%APPDATA%", d));
        }
        if let Some(d) = home() {
            bases.push(("%USERPROFILE%", d));
        }
    } else if let Some(d) = home() {
        bases.push(("~", d));
    }
    for (name, base) in bases {
        if let Some(rest) = under(path, &base) {
            let rest = if rest.as_os_str().is_empty() {
                String::new()
            } else {
                format!("{}{}", std::path::MAIN_SEPARATOR, rest.display())
            };
            return ShownPath {
                placeholder: Some(name),
                rest,
            };
        }
    }
    ShownPath {
        placeholder: None,
        rest: path.display().to_string(),
    }
}

pub fn shown(path: &Path) -> String {
    shown_parts(path).text()
}

pub fn game_user_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_dir("APPDATA").map(|d| d.join("Dominions6"))
    } else {
        home().map(|h| h.join(".dominions6"))
    }
}

pub fn game_maps_dir() -> Option<PathBuf> {
    game_sub_dir(MAPS)
}

pub fn game_blueprints_dir() -> Option<PathBuf> {
    game_sub_dir(BLUEPRINTS)
}

fn game_sub_dir(name: &str) -> Option<PathBuf> {
    let dir = game_user_dir()?;
    if !crate::io::is_dir(&dir) {
        return None;
    }
    Some(dir.join(name))
}

pub fn default_data_folder() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        env_dir("APPDATA").or_else(|| home().map(|h| h.join("AppData").join("Roaming")))
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library").join("Application Support"))
    } else {
        env_dir("XDG_DATA_HOME").or_else(|| home().map(|h| h.join(".local").join("share")))
    };
    match base {
        Some(b) => b.join(FOLDER_NAME),
        None => PathBuf::from(FOLDER_NAME),
    }
}

pub fn default_config_path() -> PathBuf {
    default_data_folder().join(CONFIG_NAME)
}

pub fn parse_config(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != DATA_FOLDER_KEY {
            continue;
        }
        let value = value.trim();
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    None
}

pub fn format_config(folder: &Path) -> String {
    format!("{DATA_FOLDER_KEY} = {}\n", folder.display())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub data_folder: PathBuf,
    config: PathBuf,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            data_folder: default_data_folder(),
            config: default_config_path(),
        }
    }
}

impl Settings {
    pub fn with_config(config: PathBuf) -> Settings {
        let data_folder = config
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(default_data_folder);
        Settings {
            data_folder,
            config,
        }
    }

    pub fn load() -> Settings {
        Settings::default().reload()
    }

    pub fn reload(mut self) -> Settings {
        if let Ok(text) = crate::io::read_to_string(&self.config) {
            if let Some(folder) = parse_config(&text) {
                self.data_folder = folder;
            }
        }
        self
    }

    pub fn default_folder(&self) -> PathBuf {
        self.config
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(default_data_folder)
    }

    pub fn is_default(&self) -> bool {
        self.data_folder == self.default_folder()
    }

    pub fn set_data_folder(&mut self, folder: PathBuf) {
        self.data_folder = folder;
    }

    pub fn reset(&mut self) {
        self.data_folder = self.default_folder();
    }

    pub fn save(&self) -> Result<(), String> {
        if self.is_default() {
            return crate::io::remove_file(&self.config);
        }
        if let Some(dir) = self.config.parent() {
            crate::io::create_dir_all(dir)?;
        }
        crate::io::write_plain(&self.config, format_config(&self.data_folder).as_bytes())
    }

    pub fn root_dir(&self) -> PathBuf {
        if self.is_default() {
            if let Some(d) = game_user_dir().filter(|d| crate::io::is_dir(d)) {
                return d;
            }
        }
        self.data_folder.clone()
    }

    pub fn maps_dir(&self) -> PathBuf {
        self.root_dir().join(MAPS)
    }

    pub fn blueprints_dir(&self) -> PathBuf {
        self.root_dir().join(BLUEPRINTS)
    }

    pub fn ensure_maps_dir(&self) -> Result<PathBuf, String> {
        ensure(self.maps_dir())
    }

    pub fn ensure_blueprints_dir(&self) -> Result<PathBuf, String> {
        ensure(self.blueprints_dir())
    }
}

pub fn ensure(dir: PathBuf) -> Result<PathBuf, String> {
    crate::io::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !crate::io::exists(&first) {
        return first;
    }
    let mut n = 2u32;
    loop {
        let candidate = dir.join(format!("{stem}-{n}.{ext}"));
        if !crate::io::exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("d6sme_{tag}_{n}"));
            std::fs::create_dir_all(&path).expect("temp dir");
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn a_path_under_the_profile_hides_the_user_name() {
        let Some(h) = home() else { return };
        let inside = h.join("maps").join("x.d6m");
        let text = shown(&inside);
        assert!(!text.contains(&h.display().to_string()), "{text}");
        assert!(text.ends_with("x.d6m"), "{text}");
        assert!(text.starts_with('~') || text.starts_with('%'), "{text}");
        assert_eq!(shown(Path::new("Z:/elsewhere/x.d6m")), "Z:/elsewhere/x.d6m");
    }

    #[test]
    fn the_default_folder_is_named_after_the_program() {
        assert_eq!(
            default_data_folder().file_name().and_then(|n| n.to_str()),
            Some(FOLDER_NAME)
        );
    }

    #[test]
    fn a_config_line_names_the_data_folder() {
        let text = format_config(Path::new("D:/maps stuff"));
        assert_eq!(parse_config(&text), Some(PathBuf::from("D:/maps stuff")));
    }

    #[test]
    fn config_text_without_the_key_leaves_the_default_in_place() {
        assert_eq!(parse_config("other = 3\n\n"), None);
    }

    #[test]
    fn a_changed_folder_is_written_and_read_back() {
        let tmp = TempDir::new("cfg");
        let config = tmp.path.join(CONFIG_NAME);
        let elsewhere = tmp.path.join("elsewhere");
        let mut s = Settings::with_config(config.clone());
        assert!(s.is_default());
        s.set_data_folder(elsewhere.clone());
        s.save().expect("write config");
        let back = Settings::with_config(config).reload();
        assert_eq!(back.data_folder, elsewhere);
        assert_eq!(back.maps_dir(), elsewhere.join(MAPS));
        assert_eq!(back.blueprints_dir(), elsewhere.join(BLUEPRINTS));
    }

    #[test]
    fn resetting_removes_the_config_file() {
        let tmp = TempDir::new("reset");
        let config = tmp.path.join(CONFIG_NAME);
        let mut s = Settings::with_config(config.clone());
        s.set_data_folder(tmp.path.join("elsewhere"));
        s.save().expect("write config");
        assert!(config.exists());
        s.reset();
        s.save().expect("clear config");
        assert!(!config.exists());
        assert_eq!(Settings::with_config(config).reload().data_folder, tmp.path);
    }

    #[test]
    fn the_folders_are_created_on_demand() {
        let tmp = TempDir::new("dirs");
        let s = Settings::with_config(tmp.path.join("deeper").join(CONFIG_NAME));
        let maps = s.ensure_maps_dir().expect("maps dir");
        let blueprints = s.ensure_blueprints_dir().expect("blueprints dir");
        assert!(maps.is_dir());
        assert!(blueprints.is_dir());
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let tmp = TempDir::new("unique");
        assert_eq!(
            unique_path(&tmp.path, "world", "png"),
            tmp.path.join("world.png")
        );
        std::fs::write(tmp.path.join("world.png"), b"x").expect("write");
        assert_eq!(
            unique_path(&tmp.path, "world", "png"),
            tmp.path.join("world-2.png")
        );
        std::fs::write(tmp.path.join("world-2.png"), b"x").expect("write");
        assert_eq!(
            unique_path(&tmp.path, "world", "png"),
            tmp.path.join("world-3.png")
        );
    }
}
