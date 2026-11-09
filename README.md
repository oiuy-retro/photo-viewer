# Photo Viewer

Native Win32 image viewer with Windows XP Luna chrome: custom caption, minimize / maximize / close buttons, and an Explorer folder picker.

A built copy is [`Photo.Viewer.exe`](https://github.com/oiuy-retro/photo-viewer/releases/latest/download/Photo.Viewer.exe) on the [latest release](https://github.com/oiuy-retro/photo-viewer/releases/latest). It is a single file. Windows Imaging Component does the decode.

## What it does

- **Start:** pick an input folder (pictures, name order via `StrCmpLogicalW`) and an output folder, then Open.
- **View:** fit-to-height by default; pictures shorter than the window show at their real size (100%) instead of being enlarged. Mouse wheel zooms in from fit. Double-click toggles 100% and fit. Click-drag pans when zoomed. Arrow keys change picture when unzoomed, pan when zoomed.
- Space copies the current file to the output folder (skips if it is already there).
- Delete sends the file to the Recycle Bin.
- Wide pictures that do not fit across the window at fit height can be zoomed out further with the mouse wheel, down to fit width.
- Thin name bar with the filename and a Folders button. Esc or Folders returns to Start without unloading the browsed paths.
- EXIF / property pane (camera, dates, GPS, and the rest that WIC exposes).
- Switching is meant to feel instant. Worker threads decode the next pictures in the direction you are moving (8 ahead, 3 behind) at the monitor's height, so the next picture is usually already in RAM and is just copied to the screen. The picture you jump to always goes first. Full resolution is decoded only when you zoom in past screen size.

Formats are whatever WIC on that machine can open: JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, HEIC/HEIF (if the codecs are installed), JPEG XR, AVIF, and similar.

Camera RAW files (DNG, CR2, CR3, NEF, ARW, ORF, RW2, RAF, PEF, SRW and others) are listed too. Windows needs a RAW codec to open them: install the free [Raw Image Extension](https://apps.microsoft.com/detail/9NCTDW2W1BH8) from the Microsoft Store. If the full RAW decode fails, the viewer shows the JPEG preview embedded in the file instead.

Maximize fills the monitor work area. The rounded window region is cleared while maximized.

## Run

Start `Photo Viewer.exe`. Windows 10 or later. No extra files.

## Open pictures from Explorer

`Photo Viewer.exe "C:\Photos\IMG_0001.jpg"` opens that picture and lets the arrow keys browse the rest of its folder, in the same name order. A folder path opens that folder. Each double-click opens its own window, like Windows Photo Viewer.

Windows does not let the program set itself as the default. Put `Photo Viewer.exe` in a folder where it will stay, then run `scripts\set-default-viewer.ps1` once. It asks for that folder (or take `-Directory`) and makes Photo Viewer the current user's default for every picture type the program opens, including RAW. No administrator rights.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\set-default-viewer.ps1 -Directory "C:\Programs\Photo Viewer"
```

That remembers the previous default for each type in `%AppData%\Photo Viewer\default-viewer.json`. Put them back with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\undo-default-viewer.ps1
```

Settings, Apps, Default apps can change one type at a time. It has no single step that undoes every type the set script changed. If you move the exe, run the set script again with the new folder.

To set one type by hand instead (once per type, for example .jpg, .png, .heic, .cr2):

1. In Explorer, right-click a picture of that type, then Open with > Choose another app.
2. Scroll down and pick More apps > Look for another app on this PC, and select `Photo Viewer.exe`.
3. Tick "Always use this app to open .jpg files" (Windows 10) or choose Always (Windows 11).

After the first time, Photo Viewer is listed under Open with for other types too.

## Build

This crate targets `x86_64-pc-windows-gnu` (`rust-toolchain.toml`).

```bat
cargo build --release
copy target\release\photo-viewer.exe dist\"Photo Viewer.exe"
```

```bat
cargo test
```

Tests decode `tests/fixtures/sample.jpg`.

`cargo test --release -- --ignored --nocapture bench` with `PV_BENCH_DIR` set to a folder of large JPEGs prints decode and draw times. Run the app with `PV_TIMING=1` to log decode times and how long each switch took to `%TEMP%\photo-viewer-timing.log`.

## License

[MIT](LICENSE).

## Attributions

| Project | License | Use |
| --- | --- | --- |
| [windows-rs](https://github.com/microsoft/windows-rs) | MIT OR Apache-2.0 | Win32 UI, WIC, Explorer dialogs, Recycle Bin |
| [winres](https://github.com/mxre/winres) | MIT | Icon and manifest at build time |
| [Rust](https://www.rust-lang.org/) / LLVM | MIT / Apache-2.0 / NCSA | Compiler |
| Windows Imaging Component, Shell, Common Controls | Microsoft | Decode, properties, folder picker. Not redistributed. |

Luna colors and caption layout follow the Microsoft Windows XP Visual Guidelines (2001). This is not an official Microsoft product.
