function extensions(accept) {
  return accept.split(",").map((s) => "." + s.trim()).filter((s) => s.length > 1);
}

function hasFilePicker() {
  return typeof window.showOpenFilePicker === "function";
}

async function fromHandle(handle) {
  const file = await handle.getFile();
  return { name: file.name, bytes: new Uint8Array(await file.arrayBuffer()), handle };
}

export function canPickDirectory() {
  return typeof window.showDirectoryPicker === "function";
}

export async function pickFiles(accept, multiple, dir) {
  const exts = extensions(accept);
  if (hasFilePicker()) {
    const opts = {
      multiple,
      types: [{ description: "Map files", accept: { "application/octet-stream": exts } }],
    };
    if (dir) {
      opts.startIn = dir;
    }
    let handles;
    try {
      handles = await window.showOpenFilePicker(opts);
    } catch (e) {
      return [];
    }
    const out = [];
    for (const h of handles) {
      out.push(await fromHandle(h));
    }
    return out;
  }
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = multiple;
    input.accept = exts.join(",");
    input.style.display = "none";
    input.onchange = async () => {
      const out = [];
      for (const f of input.files) {
        out.push({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()), handle: null });
      }
      input.remove();
      resolve(out);
    };
    input.oncancel = () => {
      input.remove();
      resolve([]);
    };
    document.body.appendChild(input);
    input.click();
  });
}

export async function pickDirectory() {
  if (!canPickDirectory()) {
    return null;
  }
  try {
    return await window.showDirectoryPicker({ mode: "readwrite" });
  } catch (e) {
    return null;
  }
}

export function handleName(handle) {
  return handle ? handle.name : "";
}

async function ensureWritable(handle) {
  const mode = { mode: "readwrite" };
  if ((await handle.queryPermission(mode)) === "granted") {
    return;
  }
  if ((await handle.requestPermission(mode)) !== "granted") {
    throw new Error("write permission was not granted");
  }
}

export async function writeHandle(handle, bytes) {
  const copy = bytes.slice();
  await ensureWritable(handle);
  const stream = await handle.createWritable();
  await stream.write(copy);
  await stream.close();
}

export async function writeInDirectory(dir, name, bytes) {
  const copy = bytes.slice();
  await ensureWritable(dir);
  const handle = await dir.getFileHandle(name, { create: true });
  const stream = await handle.createWritable();
  await stream.write(copy);
  await stream.close();
  return handle;
}

export async function readInDirectory(dir, name) {
  try {
    const handle = await dir.getFileHandle(name);
    return await fromHandle(handle);
  } catch (e) {
    return null;
  }
}

export async function listDirectory(dir, accept) {
  const exts = extensions(accept);
  const names = [];
  try {
    for await (const [name, entry] of dir.entries()) {
      if (entry.kind === "file" && exts.some((e) => name.toLowerCase().endsWith(e))) {
        names.push(name);
      }
    }
  } catch (e) {
    return names;
  }
  names.sort();
  return names;
}

export function download(name, bytes) {
  const blob = new Blob([bytes.slice()], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}

function fromEntry(entry) {
  return new Promise((resolve) => {
    entry.file(
      async (f) => resolve({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()), handle: null }),
      () => resolve(null),
    );
  });
}

async function readEntryDirectory(dir) {
  const reader = dir.createReader();
  const all = [];
  for (;;) {
    const batch = await new Promise((resolve) => reader.readEntries(resolve, () => resolve([])));
    if (!batch.length) {
      break;
    }
    all.push(...batch);
  }
  const out = [];
  for (const en of all) {
    if (en.isFile && /\.(d6m|map)$/i.test(en.name)) {
      const f = await fromEntry(en);
      if (f) {
        out.push(f);
      }
    }
  }
  return out;
}

export function installDrop(cb) {
  window.addEventListener(
    "dragover",
    (e) => {
      e.preventDefault();
    },
    true,
  );
  window.addEventListener(
    "drop",
    (e) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      const items = e.dataTransfer ? Array.from(e.dataTransfer.items || []) : [];
      const handlePromises = [];
      const entries = [];
      const plain = [];
      for (const it of items) {
        if (it.kind !== "file") {
          continue;
        }
        if (typeof it.getAsFileSystemHandle === "function") {
          handlePromises.push(it.getAsFileSystemHandle());
        } else if (typeof it.webkitGetAsEntry === "function" && it.webkitGetAsEntry()) {
          entries.push(it.webkitGetAsEntry());
        } else {
          const f = it.getAsFile();
          if (f) {
            plain.push(f);
          }
        }
      }
      (async () => {
        const files = [];
        const dirs = [];
        for (const p of handlePromises) {
          let h = null;
          try {
            h = await p;
          } catch (err) {
            h = null;
          }
          if (!h) {
            continue;
          }
          if (h.kind === "directory") {
            dirs.push(h);
          } else {
            files.push(await fromHandle(h));
          }
        }
        for (const en of entries) {
          if (en.isDirectory) {
            files.push(...(await readEntryDirectory(en)));
          } else if (en.isFile) {
            const f = await fromEntry(en);
            if (f) {
              files.push(f);
            }
          }
        }
        for (const f of plain) {
          files.push({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()), handle: null });
        }
        cb({ files, dirs });
      })();
    },
    true,
  );
}
