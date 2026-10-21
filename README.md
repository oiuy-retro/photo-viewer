# Photo Viewer

Native Win32 image viewer with Windows XP Luna chrome: custom caption, minimize / maximize / close buttons, and an Explorer folder picker.

A built copy is in [`dist/Photo Viewer.exe`](dist/Photo%20Viewer.exe). It is a single file. Windows Imaging Component does the decode.

## What it does

- **Start:** pick an input folder (pictures, name order via `StrCmpLogicalW`) and an output folder, then Open.
- **View:** fit-to-height by default. Mouse wheel zooms in from fit. Double-click toggles 100% and fit. Click-drag pans when zoomed. Arrow keys change picture when unzoomed, pan when zoomed.
- Space copies the current file to the output folder (skips if it is already there).
- Delete sends the file to the Recycle Bin.
- Thin name bar with the filename and a Folders button. Esc or Folders returns to Start without unloading the browsed paths.
- EXIF / property pane (camera, dates, GPS, and the rest that WIC exposes).
- Decoded BGRA stays in RAM around the current index; the whole folder if it fits. Decode uses a few worker threads.

Formats are whatever WIC on that machine can open: JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, HEIC/HEIF (if the codecs are installed), JPEG XR, AVIF, and similar.

Maximize fills the monitor work area. The rounded window region is cleared while maximized.

## Run

Start `dist/Photo Viewer.exe`. Windows 10 or later. No extra files.

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
