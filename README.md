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
- Decoded BGRA stays in RAM around the current index; the whole folder if it fits. Decode uses a few worker threads.

Formats are whatever WIC on that machine can open: JPEG, PNG, GIF, BMP, TIFF, WebP, ICO, HEIC/HEIF (if the codecs are installed), JPEG XR, AVIF, and similar.

Camera RAW files (DNG, CR2, CR3, NEF, ARW, ORF, RW2, RAF, PEF, SRW and others) are listed too. Windows needs a RAW codec to open them: install the free [Raw Image Extension](https://apps.microsoft.com/detail/9NCTDW2W1BH8) from the Microsoft Store. If the full RAW decode fails, the viewer shows the JPEG preview embedded in the file instead.

Maximize fills the monitor work area. The rounded window region is cleared while maximized.

## Run

Start `Photo Viewer.exe`. Windows 10 or later. No extra files.

## Open pictures from Explorer

`Photo Viewer.exe "C:\Photos\IMG_0001.jpg"` opens that picture and lets the arrow keys browse the rest of its folder, in the same name order. A folder path opens that folder. Each double-click opens its own window, like Windows Photo Viewer.

To make it the default for a picture type (do this once per type, for example .jpg, .png, .heic, .cr2):

1. Put `Photo Viewer.exe` where it will stay, for example `C:\Programs\Photo Viewer\`.
2. In Explorer, right-click a picture of that type, then Open with > Choose another app.
3. Scroll down and pick More apps > Look for another app on this PC, and select `Photo Viewer.exe`.
4. Tick "Always use this app to open .jpg files" (Windows 10) or choose Always (Windows 11).

Windows does not let an app make itself the default. After the first time, Photo Viewer is listed under Open with for other types too. If you move the exe, repeat these steps.

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
