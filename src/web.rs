use std::cell::RefCell;
use std::path::{Path, PathBuf};

use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::io;
use crate::mapfile::{plane_file_name, strip_plane_suffix};

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
    #[wasm_bindgen(js_name = readInDirectory)]
    fn read_in_directory_js(dir: &JsValue, name: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = listDirectory)]
    fn list_directory_js(dir: &JsValue, accept: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = download)]
    fn download_js(name: &str, bytes: &[u8]);
}

pub const MAP_FILES: &str = "d6m,map";
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
    Status(String),
    Error(String),
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
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
    out
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

pub fn list_directory(ctx: egui::Context) {
    spawn_local(async move {
        let Some(dir) = io::dir() else {
            push(Event::Listed(Vec::new()), &ctx);
            return;
        };
        let names = match JsFuture::from(list_directory_js(&dir, MAP_FILES)).await {
            Ok(v) => Array::from(&v)
                .iter()
                .filter_map(|n| n.as_string())
                .collect(),
            Err(_) => Vec::new(),
        };
        push(Event::Listed(names), &ctx);
    });
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
