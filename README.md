# Dominions 6 Simple Map Editor (& Generator)

Edits `.d6m` maps for Dominions 6. The game's editor won't touch the height data, so this will have to do.

Also allows to generate vanilla random game maps.

![Editor](docs/screenshot.png)

## Run

In the browser: https://pkozdra.github.io/dom6-simple-map-editor/

On the desktop, grab the exe from releases. Open a `.d6m` or `.map`, or drop one on the window, or:

```
dom6-simple-map-editor.exe path\to\map.d6m
```

Maps will be saved in `%APPDATA%\Dominions6\maps`. Press F1 if you're lost.
CTRL + S to save the map.

## Build

```
cargo build --release
```

The web version needs `trunk` and the `wasm32-unknown-unknown` target:

```
trunk build --release
```

It lands in `dist/web`; `trunk serve` runs it at http://127.0.0.1:8790/.

## Credits

Dominions 6: Rise of the Pantokrator is made by [Illwinter Game Design](https://www.illwinter.com/) (Johan Karlsson and Kristoffer Osterman)
