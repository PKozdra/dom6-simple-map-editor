use std::cell::RefCell;
use std::path::{Path, PathBuf};

use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::io;
use crate::mapfile::{plane_file_name, strip_plane_suffix, MapFile};

#[wasm_bindgen(module = "/web/fs.js")]
extern "C" {
    #[wasm_bindgen(js_name = canPickDirectory)]
    fn can_pick_directory_js() -> bool;
    #[wasm_bindgen(js_name = pickFiles)]
    fn pick_files_js(accept: &str, multiple: bool, dir: &JsValue) -> js_sys::Promise;
    #[wasm_bindgen(js_name = pickDirectory)]
    fn pick_directory_js() -> js_sys::Promise;
    #[wasm_bindgen(js_name = handleName)]
    fn handle_name_js(handle: &JsValue) -> String;
    #[wasm_bindgen(js_name = writeHandle)]
    fn write_handle_js(handle: &JsValue, bytes: &[u8]) -> js_sys::Promise;
    #[wasm_bindgen(js_name = writeInDirectory)]
    fn write_in_directory_js(dir: &JsValue, name: &str, bytes: &[u8]) -> js_sys::Promise;
    #[wasm_bindgen(js_name = removeInDirectory)]
    fn remove_in_directory_js(dir: &JsValue, name: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = readInDirectory)]
    fn read_in_directory_js(dir: &JsValue, name: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = listDirectory)]
    fn list_directory_js(dir: &JsValue, accept: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = folderSource)]
    fn folder_source_js(handle: &JsValue) -> JsValue;
    #[wasm_bindgen(js_name = pickFolderSource)]
    fn pick_folder_source_js(accept: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = sourceName)]
    fn source_name_js(source: &JsValue) -> String;
    #[wasm_bindgen(js_name = sourceRoot)]
    fn source_root_js(source: &JsValue) -> JsValue;
    #[wasm_bindgen(js_name = listTree)]
    fn list_tree_js(source: &JsValue, accept: &str, depth: u32) -> js_sys::Promise;
    #[wasm_bindgen(js_name = readFrom)]
    fn read_from_js(source: &JsValue, rel: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = download)]
    fn download_js(name: &str, bytes: &[u8]);
    #[wasm_bindgen(js_name = installDrop)]
    fn install_drop_js(cb: &Closure<dyn FnMut(JsValue)>);
}

pub const MAP_FILES: &str = "d6m,map,tga";
pub const IMAGE_FILES: &str = "png,tga";
pub const MOD_FILES: &str = "dm";

pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub handle: Option<JsValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Open,
    AddPlane,
    Blueprint { cave: bool },
    Mod,
}

pub enum Event {
    Picked(Purpose, Vec<PickedFile>),
    Directory(Option<String>),
    Listed(Vec<String>),
    Chooser(String, Vec<String>),
    Status(String),
    Error(String),
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SOURCE: RefCell<Option<JsValue>> = const { RefCell::new(None) };
    static TREE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CARDS: RefCell<Vec<(usize, crate::map_chooser::Card)>> =
        const { RefCell::new(Vec::new()) };
}

pub fn take_events() -> Vec<Event> {
    EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

fn push(event: Event, ctx: &egui::Context) {
    EVENTS.with(|e| e.borrow_mut().push(event));
    ctx.request_repaint();
}

fn describe(e: JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    Reflect::get(&e, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

fn optional(v: JsValue) -> Option<JsValue> {
    if v.is_null() || v.is_undefined() {
        None
    } else {
        Some(v)
    }
}

fn parse_file(o: &JsValue) -> Option<PickedFile> {
    let name = Reflect::get(o, &JsValue::from_str("name"))
        .ok()?
        .as_string()?;
    let bytes = Uint8Array::new(&Reflect::get(o, &JsValue::from_str("bytes")).ok()?).to_vec();
    let handle = Reflect::get(o, &JsValue::from_str("handle"))
        .ok()
        .and_then(optional);
    Some(PickedFile {
        name,
        bytes,
        handle,
    })
}

fn parse_files(v: &JsValue) -> Vec<PickedFile> {
    Array::from(v)
        .iter()
        .filter_map(|o| parse_file(&o))
        .collect()
}

pub fn can_pick_directory() -> bool {
    can_pick_directory_js()
}

pub fn virtual_root() -> PathBuf {
    PathBuf::from("/")
}

pub fn store_file(file: PickedFile) -> PathBuf {
    let path = virtual_root().join(&file.name);
    io::put(&path, file.bytes, file.handle);
    path
}

pub fn take_cards() -> Vec<(usize, crate::map_chooser::Card)> {
    CARDS.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

fn source() -> Option<JsValue> {
    SOURCE.with(|s| s.borrow().clone())
}

fn dir_of(o: &JsValue) -> Option<JsValue> {
    Reflect::get(o, &JsValue::from_str("dir"))
        .ok()
        .and_then(optional)
}

fn parent_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(i) => rel[..=i].to_string(),
        None => String::new(),
    }
}

async fn read_from_source(rel: &str) -> Option<(PickedFile, Option<JsValue>)> {
    let src = source()?;
    let v = JsFuture::from(read_from_js(&src, rel)).await.ok()?;
    let v = optional(v)?;
    let file = parse_file(&v)?;
    Some((file, dir_of(&v)))
}

fn wanted_planes(files: &[PickedFile]) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    for f in files {
        let stem = Path::new(&f.name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = strip_plane_suffix(&stem).0;
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    let mut out = Vec::new();
    for base in bases {
        for plane in 1..=9u32 {
            for ext in ["d6m", "map"] {
                out.push(plane_file_name(&base, plane, ext));
            }
        }
    }
    out
}

fn wanted_pictures<'a>(files: impl Iterator<Item = &'a PickedFile>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files {
        if !f.name.to_ascii_lowercase().ends_with(".map") {
            continue;
        }
        let text = String::from_utf8_lossy(&f.bytes);
        let map = MapFile::parse(&text, Path::new(&f.name));
        if let Some(img) = map.imagefile {
            for name in crate::imagemap::picture_names(&img) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

async fn siblings_in_source(prefix: &str, files: &[PickedFile]) -> Vec<PickedFile> {
    let mut out: Vec<PickedFile> = Vec::new();
    for name in wanted_planes(files) {
        if files.iter().any(|f| f.name == name) || out.iter().any(|f| f.name == name) {
            continue;
        }
        if let Some((f, _)) = read_from_source(&format!("{prefix}{name}")).await {
            out.push(f);
        }
    }
    for name in wanted_pictures(files.iter().chain(out.iter())) {
        if files.iter().any(|f| f.name == name) || out.iter().any(|f| f.name == name) {
            continue;
        }
        if let Some((f, _)) = read_from_source(&format!("{prefix}{name}")).await {
            out.push(f);
        }
    }
    out
}

fn tree_maps(tree: &[String]) -> Vec<String> {
    tree.iter()
        .filter(|p| {
            let lower = p.to_ascii_lowercase();
            let Some(stem) = lower.strip_suffix(".map") else {
                return false;
            };
            let stem = match stem.rfind('/') {
                Some(i) => &stem[i + 1..],
                None => stem,
            };
            strip_plane_suffix(stem).1 == 1
        })
        .cloned()
        .collect()
}

fn top_level(tree: &[String]) -> Vec<String> {
    tree.iter().filter(|p| !p.contains('/')).cloned().collect()
}

async fn use_source(source: JsValue, ctx: egui::Context) {
    let name = source_name_js(&source);
    let root = optional(source_root_js(&source));
    SOURCE.with(|s| *s.borrow_mut() = Some(source.clone()));
    match &root {
        Some(h) => {
            io::set_dir(Some((h.clone(), name.clone())));
            push(Event::Directory(Some(name.clone())), &ctx);
        }
        None => {
            io::set_dir(None);
            push(
                Event::Status(format!("{name} is open; saving downloads the files")),
                &ctx,
            );
        }
    }
    let tree: Vec<String> = match JsFuture::from(list_tree_js(&source, MAP_FILES, 3)).await {
        Ok(v) => Array::from(&v)
            .iter()
            .filter_map(|n| n.as_string())
            .collect(),
        Err(e) => {
            push(Event::Error(describe(e)), &ctx);
            return;
        }
    };
    TREE.with(|t| *t.borrow_mut() = tree.clone());
    if root.is_some() {
        push(Event::Listed(top_level(&tree)), &ctx);
    }
    let maps = tree_maps(&tree);
    match maps.len() {
        0 => push(
            Event::Error(format!(
                "{name} holds no map: a map is a .map file with its .d6m or .tga beside it"
            )),
            &ctx,
        ),
        1 => open_source(maps[0].clone(), ctx).await,
        _ => push(Event::Chooser(name, maps), &ctx),
    }
}

async fn open_source(rel: String, ctx: egui::Context) {
    let Some((first, dir)) = read_from_source(&rel).await else {
        push(Event::Error(format!("{rel} could not be read")), &ctx);
        return;
    };
    if let Some(d) = dir {
        let label = io::dir_name().unwrap_or_default();
        io::set_dir(Some((d, label)));
    }
    let mut files = vec![first];
    let more = siblings_in_source(&parent_of(&rel), &files).await;
    files.extend(more);
    push(Event::Picked(Purpose::Open, files), &ctx);
    if io::dir().is_some() {
        list_directory(ctx);
    }
}

pub fn open_from_source(rel: String, ctx: egui::Context) {
    spawn_local(async move { open_source(rel, ctx).await });
}

fn plane_count_in(tree: &[String], rel: &str) -> usize {
    let prefix = parent_of(rel);
    let stem = Path::new(rel)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = strip_plane_suffix(&stem).0;
    let mut n = 1;
    for p in 2..=9u32 {
        let want = format!("{prefix}{}", plane_file_name(&base, p, "map")).to_ascii_lowercase();
        if tree.iter().any(|t| t.to_ascii_lowercase() == want) {
            n = p as usize;
        }
    }
    n
}

async fn card_for(
    rel: &str,
    tree: &[String],
    tex: &crate::textures::TexSet,
) -> crate::map_chooser::Card {
    use crate::map_chooser::{folder_of, name_of, Kind};
    let path = PathBuf::from(format!("/{rel}"));
    let mut card = crate::map_chooser::Card {
        path: path.clone(),
        name: name_of(&path),
        folder: folder_of(Path::new("/"), &path),
        stats: None,
        error: None,
        image: None,
    };
    let Some((file, _)) = read_from_source(rel).await else {
        card.error = Some("could not be read".to_owned());
        return card;
    };
    let text = String::from_utf8_lossy(&file.bytes).into_owned();
    drop(file);
    let (stats, image) =
        crate::map_chooser::stats_from_text(&text, &path, plane_count_in(tree, rel));
    let kind = stats.kind;
    let mut size = stats.size;
    card.stats = Some(stats);
    let prefix = parent_of(rel);
    let picture = match (kind, image) {
        (Kind::Unknown, _) | (_, None) => None,
        (kind, Some(img)) => Some((kind, img)),
    };
    let preview = match picture {
        Some((kind, img)) => match read_from_source(&format!("{prefix}{img}")).await {
            Some((f, _)) => match kind {
                Kind::Picture => crate::map_chooser::picture_preview(&f.bytes),
                _ => crate::d6m::D6m::parse(&f.bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|d| {
                        if size.is_none() {
                            size = Some((d.width, d.height));
                        }
                        let map = MapFile::parse(&text, &path);
                        crate::map_chooser::rendered_preview(&d, Some(&map), tex)
                            .or_else(|_| crate::map_chooser::recipe_preview(&d))
                    }),
            },
            None => Err(format!("{img} is missing")),
        },
        None => Err(String::new()),
    };
    if let Some(s) = card.stats.as_mut() {
        s.size = size;
    }
    match preview {
        Ok(img) => {
            card.image = Some(egui::ColorImage::from_rgba_unmultiplied(
                [img.w, img.h],
                &img.rgba,
            ));
        }
        Err(e) if !e.is_empty() => card.error = Some(e),
        Err(_) => {}
    }
    card
}

pub fn spawn_cards(
    ctx: egui::Context,
    rels: Vec<String>,
    cancel: std::rc::Rc<std::cell::Cell<bool>>,
) {
    CARDS.with(|c| c.borrow_mut().clear());
    spawn_local(async move {
        let tex = crate::textures::TexSet::embedded();
        let tree = TREE.with(|t| t.borrow().clone());
        for (index, rel) in rels.into_iter().enumerate() {
            if cancel.get() {
                return;
            }
            let card = card_for(&rel, &tree, &tex).await;
            CARDS.with(|c| c.borrow_mut().push((index, card)));
            ctx.request_repaint();
        }
    });
}

async fn siblings(dir: &JsValue, files: &[PickedFile]) -> Vec<PickedFile> {
    let mut bases: Vec<String> = Vec::new();
    for f in files {
        let stem = Path::new(&f.name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let base = strip_plane_suffix(&stem).0;
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    let mut out = Vec::new();
    for base in bases {
        for plane in 1..=9u32 {
            for ext in ["d6m", "map"] {
                let name = plane_file_name(&base, plane, ext);
                if files.iter().any(|f| f.name == name)
                    || out.iter().any(|f: &PickedFile| f.name == name)
                {
                    continue;
                }
                if let Ok(v) = JsFuture::from(read_in_directory_js(dir, &name)).await {
                    if let Some(f) = optional(v).as_ref().and_then(parse_file) {
                        out.push(f);
                    }
                }
            }
        }
    }
    let mut pictures: Vec<String> = Vec::new();
    for f in files.iter().chain(out.iter()) {
        if !f.name.to_ascii_lowercase().ends_with(".map") {
            continue;
        }
        let text = String::from_utf8_lossy(&f.bytes);
        let map = MapFile::parse(&text, Path::new(&f.name));
        if let Some(img) = map.imagefile {
            for name in crate::imagemap::picture_names(&img) {
                if !pictures.contains(&name) {
                    pictures.push(name);
                }
            }
        }
    }
    for name in pictures {
        if files.iter().any(|f| f.name == name) || out.iter().any(|f| f.name == name) {
            continue;
        }
        if let Ok(v) = JsFuture::from(read_in_directory_js(dir, &name)).await {
            if let Some(f) = optional(v).as_ref().and_then(parse_file) {
                out.push(f);
            }
        }
    }
    out
}

pub fn open_folder(ctx: egui::Context) {
    spawn_local(async move {
        if can_pick_directory_js() {
            match JsFuture::from(pick_directory_js()).await {
                Ok(v) => match optional(v) {
                    Some(h) => use_source(folder_source_js(&h), ctx).await,
                    None => push(Event::Directory(None), &ctx),
                },
                Err(e) => push(Event::Error(describe(e)), &ctx),
            }
            return;
        }
        match JsFuture::from(pick_folder_source_js(MAP_FILES)).await {
            Ok(v) => match optional(v) {
                Some(s) => use_source(s, ctx).await,
                None => push(Event::Directory(None), &ctx),
            },
            Err(e) => push(Event::Error(describe(e)), &ctx),
        }
    });
}

pub fn pick(purpose: Purpose, accept: &'static str, multiple: bool, ctx: egui::Context) {
    spawn_local(async move {
        let dir = io::dir().unwrap_or(JsValue::NULL);
        match JsFuture::from(pick_files_js(accept, multiple, &dir)).await {
            Ok(v) => {
                let mut files = parse_files(&v);
                if files.is_empty() {
                    return;
                }
                if purpose == Purpose::Open && !dir.is_null() {
                    let more = siblings(&dir, &files).await;
                    files.extend(more);
                }
                push(Event::Picked(purpose, files), &ctx);
            }
            Err(e) => push(Event::Error(describe(e)), &ctx),
        }
    });
}

pub fn open_from_directory(name: String, ctx: egui::Context) {
    spawn_local(async move {
        let Some(dir) = io::dir() else {
            return;
        };
        match JsFuture::from(read_in_directory_js(&dir, &name)).await {
            Ok(v) => {
                let Some(first) = optional(v).as_ref().and_then(parse_file) else {
                    push(Event::Error(format!("{name} could not be read")), &ctx);
                    return;
                };
                let mut files = vec![first];
                let more = siblings(&dir, &files).await;
                files.extend(more);
                push(Event::Picked(Purpose::Open, files), &ctx);
            }
            Err(e) => push(Event::Error(describe(e)), &ctx),
        }
    });
}

pub fn pick_directory(ctx: egui::Context) {
    spawn_local(async move {
        match JsFuture::from(pick_directory_js()).await {
            Ok(v) => match optional(v) {
                Some(h) => {
                    let name = handle_name_js(&h);
                    io::set_dir(Some((h, name.clone())));
                    push(Event::Directory(Some(name)), &ctx);
                    list_directory(ctx);
                }
                None => push(Event::Directory(None), &ctx),
            },
            Err(e) => push(Event::Error(describe(e)), &ctx),
        }
    });
}

async fn names_in(dir: &JsValue) -> Vec<String> {
    match JsFuture::from(list_directory_js(dir, MAP_FILES)).await {
        Ok(v) => Array::from(&v)
            .iter()
            .filter_map(|n| n.as_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn list_directory(ctx: egui::Context) {
    spawn_local(async move {
        let Some(dir) = io::dir() else {
            push(Event::Listed(Vec::new()), &ctx);
            return;
        };
        let names = names_in(&dir).await;
        push(Event::Listed(names), &ctx);
    });
}

pub fn complete_maps(names: &[String]) -> Vec<String> {
    let lower: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    names
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            let Some(stem) = l.strip_suffix(".map") else {
                return false;
            };
            strip_plane_suffix(stem).1 == 1
                && (lower.contains(&format!("{stem}.d6m"))
                    || lower.contains(&format!("{stem}.tga")))
        })
        .cloned()
        .collect()
}

pub fn install_drop(ctx: egui::Context) {
    let cb = Closure::<dyn FnMut(JsValue)>::new(move |v: JsValue| {
        let files = Reflect::get(&v, &JsValue::from_str("files"))
            .map(|a| parse_files(&a))
            .unwrap_or_default();
        let dirs: Vec<JsValue> = Reflect::get(&v, &JsValue::from_str("dirs"))
            .map(|a| Array::from(&a).iter().collect())
            .unwrap_or_default();
        let ctx = ctx.clone();
        spawn_local(async move { handle_drop(files, dirs, ctx).await });
    });
    install_drop_js(&cb);
    cb.forget();
}

async fn handle_drop(mut files: Vec<PickedFile>, dirs: Vec<JsValue>, ctx: egui::Context) {
    if let Some(h) = dirs.into_iter().next() {
        if files.is_empty() {
            use_source(folder_source_js(&h), ctx).await;
            return;
        }
        let name = handle_name_js(&h);
        io::set_dir(Some((h.clone(), name.clone())));
        push(Event::Directory(Some(name)), &ctx);
        let names = names_in(&h).await;
        push(Event::Listed(names), &ctx);
    }
    if files.is_empty() {
        return;
    }
    if let Some(dir) = io::dir() {
        let more = siblings(&dir, &files).await;
        files.extend(more);
    }
    push(Event::Picked(Purpose::Open, files), &ctx);
}

pub fn forget_directory() {
    io::set_dir(None);
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn download(name: &str, bytes: &[u8]) {
    download_js(name, bytes);
}

pub fn can_write_folder() -> bool {
    io::dir().is_some()
}

pub fn persist_rename(write: Vec<PathBuf>, remove: Vec<String>, ctx: egui::Context) {
    spawn_local(async move {
        let Some(dir) = io::dir() else {
            let files: Vec<(String, Vec<u8>)> = write
                .iter()
                .filter(|p| !file_name(p).ends_with(".bak"))
                .filter_map(|p| io::read(p).ok().map(|b| (file_name(p), b)))
                .collect();
            let base = files
                .iter()
                .find(|(n, _)| n.to_ascii_lowercase().ends_with(".map"))
                .and_then(|(n, _)| {
                    Path::new(n)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                })
                .map(|stem| strip_plane_suffix(&stem).0)
                .unwrap_or_else(|| "map".to_string());
            let archive = format!("{base}.zip");
            download_js(&archive, &crate::zipfile::build(&files));
            push(
                Event::Status(format!(
                    "Downloaded {archive}: unpack it into a folder called {base} in the game's maps folder and delete the old files"
                )),
                &ctx,
            );
            return;
        };
        if let Err(e) = write_all_into(&dir, &write).await {
            push(Event::Error(e), &ctx);
            return;
        }
        let mut left = Vec::new();
        for name in &remove {
            let gone = JsFuture::from(remove_in_directory_js(&dir, name))
                .await
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !gone {
                left.push(name.clone());
            }
        }
        let text = if left.is_empty() {
            format!(
                "Renamed {} files in {}",
                remove.len(),
                io::dir_name().unwrap_or_default()
            )
        } else {
            format!(
                "Renamed the map, but these old files could not be deleted: {}",
                left.join(", ")
            )
        };
        push(Event::Status(text), &ctx);
        list_directory(ctx);
    });
}

async fn write_all_into(dir: &JsValue, paths: &[PathBuf]) -> Result<(), String> {
    for p in paths {
        let Ok(bytes) = io::read(p) else {
            continue;
        };
        let name = file_name(p);
        let h = JsFuture::from(write_in_directory_js(dir, &name, &bytes))
            .await
            .map_err(|e| format!("{name}: {}", describe(e)))?;
        io::set_handle(p, h);
    }
    Ok(())
}

pub fn export(paths: Vec<PathBuf>, ctx: egui::Context) {
    spawn_local(async move {
        let names: Vec<String> = paths.iter().map(|p| file_name(p)).collect();
        let mut pending = Vec::new();
        for p in &paths {
            let Ok(bytes) = io::read(p) else {
                continue;
            };
            if let Some(h) = io::handle_of(p) {
                if let Err(e) = JsFuture::from(write_handle_js(&h, &bytes)).await {
                    push(
                        Event::Error(format!("{}: {}", file_name(p), describe(e))),
                        &ctx,
                    );
                    return;
                }
            } else {
                pending.push(p.clone());
            }
        }
        let mut where_to = io::dir_name().unwrap_or_default();
        if !pending.is_empty() {
            let mut dir = io::dir();
            if dir.is_none() && can_pick_directory_js() {
                if let Ok(v) = JsFuture::from(pick_directory_js()).await {
                    if let Some(h) = optional(v) {
                        let name = handle_name_js(&h);
                        io::set_dir(Some((h.clone(), name.clone())));
                        where_to = name;
                        dir = Some(h);
                        push(Event::Directory(io::dir_name()), &ctx);
                    }
                }
            }
            match dir {
                Some(d) => {
                    if let Err(e) = write_all_into(&d, &pending).await {
                        push(Event::Error(e), &ctx);
                        return;
                    }
                }
                None => {
                    let files: Vec<(String, Vec<u8>)> = pending
                        .iter()
                        .filter_map(|p| io::read(p).ok().map(|b| (file_name(p), b)))
                        .collect();
                    let base = files
                        .first()
                        .and_then(|(n, _)| {
                            Path::new(n)
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                        })
                        .map(|stem| strip_plane_suffix(&stem).0)
                        .unwrap_or_else(|| "map".to_string());
                    let archive = format!("{base}.zip");
                    download_js(&archive, &crate::zipfile::build(&files));
                    push(
                        Event::Status(format!(
                            "Downloaded {archive} with {}; unpack it into the game's maps folder",
                            names.join(", ")
                        )),
                        &ctx,
                    );
                    return;
                }
            }
        }
        let text = if where_to.is_empty() {
            format!("Saved {}", names.join(", "))
        } else {
            format!("Saved {} in {where_to}", names.join(", "))
        };
        push(Event::Status(text), &ctx);
        list_directory(ctx);
    });
}
