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

## Configuration file

Photo Viewer keeps its settings in a plain text file called `configuration.conf` in the same folder as `Photo Viewer.exe`. If the file is missing, the app writes one with comments explaining every line the first time it starts. You can open it in Notepad and edit it while the app is closed. Lines that start with `#` are comments, `[name]` starts a section, and every setting is a `key = value` line. Key names are not case-sensitive, lines the app does not recognise are ignored, and deleting a line puts that setting back to its default.

The `[folders]` section remembers where each Browse button last left off. `input` is the folder last picked with the Input folder button and `output` is the folder last picked with the Output folder button, for example `input = D:\Photos` and `output = E:\Picks`. Each button opens at its own remembered folder. If that folder no longer exists, for example because a drive is unplugged, the button opens at the nearest folder above it that still exists, or at the normal default, without showing an error. After each pick the app changes only that one line, so your own comments and edits stay as they are.

The `[tools]` section is the same in all of these apps and says where to find outside programs. A value of `auto` looks next to the exe, then in each folder directly inside the exe's folder, then in a `bin` folder inside each of those, and then on the PATH. A value of `path` looks only on the PATH. Anything else is a path to the program; a relative path starts from the exe's folder, and variables such as `%USERPROFILE%` are filled in. If a path you typed does not exist, the app says which line is wrong instead of guessing. Photo Viewer uses the picture decoders built into Windows, so it needs no outside programs and this section is empty apart from comments. The `[photo-viewer]` section is for settings that only apply to this app and is empty for now.

If the exe's folder cannot be written to, for example inside Program Files, the app uses `%APPDATA%\Photo Viewer\configuration.conf` instead. A `configuration.conf` next to the exe is still read first if it exists, and folders saved in the AppData copy are applied on top of it. If neither place can be written to, the app still works and remembers folders until it is closed.

## Run

Start `Photo Viewer.exe`. Windows 10 or later. No extra files.

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
