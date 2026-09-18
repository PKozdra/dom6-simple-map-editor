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

export function folderSource(handle) {
  return handle ? { kind: "handle", root: handle, name: handle.name } : null;
}

export function pickFolderSource(accept) {
  const exts = extensions(accept);
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.webkitdirectory = true;
    input.style.display = "none";
    input.onchange = () => {
      const files = new Map();
      let name = "";
      for (const f of input.files) {
        const full = f.webkitRelativePath || f.name;
        const parts = full.split("/");
        if (!name && parts.length > 1) {
          name = parts[0];
        }
        const rel = parts.length > 1 ? parts.slice(1).join("/") : full;
        if (exts.some((e) => f.name.toLowerCase().endsWith(e))) {
          files.set(rel, f);
        }
      }
      input.remove();
      resolve(files.size ? { kind: "files", root: null, name, files } : null);
    };
    input.oncancel = () => {
      input.remove();
      resolve(null);
    };
    document.body.appendChild(input);
    input.click();
  });
}

export function sourceName(source) {
  return source ? source.name || "" : "";
}

export function sourceRoot(source) {
  return source && source.kind === "handle" ? source.root : null;
}

let scanStats = { folders: 0, files: 0, mods: 0, found: 0, skipped: 0, ms: 0, error: "" };

export function lastScanStats() {
  return scanStats;
}

export function logInfo(text) {
  console.info(text);
}

function isModFolder(names) {
  let dm = false;
  for (const n of names) {
    const l = n.toLowerCase();
    if (l.endsWith(".map") || l.endsWith(".d6m")) {
      return false;
    }
    if (l.endsWith(".dm")) {
      dm = true;
    }
  }
  return dm;
}

async function walkParallel(root, exts, depth, out, stats, tick) {
  const queue = [{ dir: root, prefix: "", depth }];
  let active = 0;
  return new Promise((resolve, reject) => {
    const next = () => {
      if (!queue.length && active === 0) {
        resolve();
        return;
      }
      while (queue.length && active < 16) {
        const job = queue.shift();
        active += 1;
        visit(job)
          .catch((e) => {
            if (!job.prefix) {
              throw e;
            }
            stats.skipped += 1;
            if (!stats.error) {
              stats.error = `${job.prefix}: ${e && e.message ? e.message : e}`;
            }
            console.warn(`scan skipped ${job.prefix}: ${e && e.message ? e.message : e}`);
          })
          .then(() => {
            active -= 1;
            next();
          })
          .catch(reject);
      }
    };
    const visit = async ({ dir, prefix, depth }) => {
      const files = [];
      const dirs = [];
      for await (const [name, entry] of dir.entries()) {
        if (entry.kind === "file") {
          files.push(name);
        } else {
          dirs.push([name, entry]);
        }
      }
      stats.folders += 1;
      stats.files += files.length;
      for (const name of files) {
        if (exts.some((e) => name.toLowerCase().endsWith(e))) {
          out.push(prefix ? prefix + "/" + name : name);
          if (name.toLowerCase().endsWith(".map")) {
            stats.found += 1;
          }
        }
      }
      if (depth > 0 && dirs.length) {
        if (isModFolder(files)) {
          stats.mods += 1;
        } else {
          for (const [name, entry] of dirs) {
            queue.push({ dir: entry, prefix: prefix ? prefix + "/" + name : name, depth: depth - 1 });
          }
        }
      }
      tick();
    };
    next();
  });
}

export async function listTree(source, accept, depth, progress) {
  const exts = extensions(accept);
  const out = [];
  const started = performance.now();
  const stats = { folders: 0, files: 0, mods: 0, found: 0, skipped: 0, ms: 0, error: "" };
  scanStats = stats;
  if (!source) {
    return out;
  }
  let last = 0;
  const tick = () => {
    const now = performance.now();
    if (progress && now - last > 150) {
      last = now;
      stats.ms = now - started;
      progress(stats);
    }
  };
  if (source.kind === "handle") {
    try {
      await walkParallel(source.root, exts, depth, out, stats, tick);
    } catch (e) {
      stats.ms = performance.now() - started;
      throw new Error(`cannot read ${source.name}: ${e && e.message ? e.message : e}`);
    }
  } else {
    for (const rel of source.files.keys()) {
      stats.files += 1;
      if (rel.split("/").length <= depth + 1 && exts.some((e) => rel.toLowerCase().endsWith(e))) {
        out.push(rel);
        if (rel.toLowerCase().endsWith(".map")) {
          stats.found += 1;
        }
      }
    }
  }
  out.sort();
  stats.ms = performance.now() - started;
  console.info(
    `scan ${source.name}: ${stats.folders} folders, ${stats.files} files, ${stats.found} .map, ${stats.mods} mod folders skipped, ${stats.skipped} unreadable, ${stats.ms.toFixed(0)} ms`,
  );
  return out;
}

export async function readFrom(source, relPath) {
  if (!source) {
    return null;
  }
  const parts = relPath.split("/").filter((s) => s.length);
  const name = parts.pop();
  if (!name) {
    return null;
  }
  if (source.kind === "handle") {
    try {
      let dir = source.root;
      for (const p of parts) {
        dir = await dir.getDirectoryHandle(p);
      }
      const handle = await dir.getFileHandle(name);
      const file = await handle.getFile();
      return { name, bytes: new Uint8Array(await file.arrayBuffer()), handle, dir };
    } catch (e) {
      return null;
    }
  }
  const f = source.files.get(relPath);
  if (!f) {
    return null;
  }
  return { name, bytes: new Uint8Array(await f.arrayBuffer()), handle: null, dir: null };
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

export async function removeInDirectory(dir, name) {
  try {
    await ensureWritable(dir);
    await dir.removeEntry(name);
    return true;
  } catch (e) {
    return false;
  }
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
    if (en.isFile && /\.(d6m|map|tga)$/i.test(en.name)) {
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
