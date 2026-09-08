# Dominions 6 Simple Map Editor (& Generator)

Edits `.d6m` maps for Dominions 6. The game's editor won't touch the height data, so this will have to do.

Also allows to generate vanilla random game maps.

![Editor](docs/screenshot.png)

## Run

Grab the exe from releases. Open a `.d6m` or `.map`, or drop one on the window, or:

```
dom6-simple-map-editor.exe path\to\map.d6m
```

Maps will be saved in `%APPDATA%\Dominions6\maps`. Press F1 if you're lost.
CTRL + S to save the map.

## Build

```
cargo build --release
```
