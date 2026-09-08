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
