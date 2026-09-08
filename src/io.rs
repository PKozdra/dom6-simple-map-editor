use std::path::Path;

pub const IS_WEB: bool = cfg!(target_arch = "wasm32");

fn shown(path: &Path) -> String {
    crate::settings::shown(path)
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    pub fn read(path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    pub fn read_to_string(path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
        let tmp = path.with_extension("tmp_write");
        std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", shown(&tmp)))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("replace {}: {e}", shown(path)))?;
        Ok(())
    }

    pub fn write_plain(path: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", shown(path)))
    }

    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn is_dir(path: &Path) -> bool {
        path.is_dir()
    }

    pub fn copy(from: &Path, to: &Path) -> Result<(), String> {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("copy {}: {e}", shown(to)))
    }

    pub fn rename(from: &Path, to: &Path) -> Result<(), String> {
        std::fs::rename(from, to).map_err(|e| format!("move {}: {e}", shown(from)))
    }

    pub fn create_dir_all(dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", shown(dir)))
    }

    pub fn remove_file(path: &Path) -> Result<(), String> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", shown(path))),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use wasm_bindgen::JsValue;

    pub struct Entry {
        pub bytes: Vec<u8>,
        pub handle: Option<JsValue>,
    }

    thread_local! {
        static STORE: RefCell<HashMap<PathBuf, Entry>> = RefCell::new(HashMap::new());
        static DIR: RefCell<Option<(JsValue, String)>> = const { RefCell::new(None) };
    }

    fn key(path: &Path) -> PathBuf {
        PathBuf::from(path.to_string_lossy().replace('\\', "/"))
    }

    fn not_found(path: &Path) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not open in the browser", shown(path)),
        )
    }

    pub fn read(path: &Path) -> std::io::Result<Vec<u8>> {
        STORE.with(|s| {
            s.borrow()
                .get(&key(path))
                .map(|e| e.bytes.clone())
                .ok_or_else(|| not_found(path))
        })
    }

    pub fn read_to_string(path: &Path) -> std::io::Result<String> {
        read(path).map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
        STORE.with(|s| {
            let mut s = s.borrow_mut();
            let k = key(path);
            let handle = s.get(&k).and_then(|e| e.handle.clone());
            s.insert(
                k,
                Entry {
                    bytes: bytes.to_vec(),
                    handle,
                },
            );
        });
        Ok(())
    }

    pub fn write_plain(path: &Path, bytes: &[u8]) -> Result<(), String> {
        write(path, bytes)
    }

    pub fn put(path: &Path, bytes: Vec<u8>, handle: Option<JsValue>) {
        STORE.with(|s| {
            s.borrow_mut().insert(key(path), Entry { bytes, handle });
        });
    }

    pub fn handle_of(path: &Path) -> Option<JsValue> {
        STORE.with(|s| s.borrow().get(&key(path)).and_then(|e| e.handle.clone()))
    }

    pub fn set_handle(path: &Path, handle: JsValue) {
        STORE.with(|s| {
            if let Some(e) = s.borrow_mut().get_mut(&key(path)) {
                e.handle = Some(handle);
            }
        });
    }

    pub fn exists(path: &Path) -> bool {
        STORE.with(|s| s.borrow().contains_key(&key(path)))
    }

    pub fn is_dir(_path: &Path) -> bool {
        false
    }

    pub fn copy(from: &Path, to: &Path) -> Result<(), String> {
        let bytes = read(from).map_err(|e| e.to_string())?;
        put(to, bytes, None);
        Ok(())
    }

    pub fn rename(from: &Path, to: &Path) -> Result<(), String> {
        STORE.with(|s| {
            let mut s = s.borrow_mut();
            match s.remove(&key(from)) {
                Some(e) => {
                    s.insert(
                        key(to),
                        Entry {
                            bytes: e.bytes,
                            handle: None,
                        },
                    );
                    Ok(())
                }
                None => Err(format!("{} is not open in the browser", shown(from))),
            }
        })
    }

    pub fn create_dir_all(_dir: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn remove_file(path: &Path) -> Result<(), String> {
        STORE.with(|s| {
            s.borrow_mut().remove(&key(path));
        });
        Ok(())
    }

    pub fn set_dir(handle: Option<(JsValue, String)>) {
        DIR.with(|d| *d.borrow_mut() = handle);
    }

    pub fn dir() -> Option<JsValue> {
        DIR.with(|d| d.borrow().as_ref().map(|(h, _)| h.clone()))
    }

    pub fn dir_name() -> Option<String> {
        DIR.with(|d| d.borrow().as_ref().map(|(_, n)| n.clone()))
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::*;
