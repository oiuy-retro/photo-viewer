//! `configuration.conf`: the shared plain-text settings file that sits next
//! to the exe (see README, "Configuration file").
//!
//! Format: UTF-8, `# comments`, `[section]`, `key = value`. Keys and section
//! names are case-insensitive; unknown keys and sections are ignored; a
//! missing key means "use the default".

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "configuration.conf";
const APP_DIR_NAME: &str = "Photo Viewer";

pub const TEMPLATE: &str = "\
# Photo Viewer configuration file
# Plain text, UTF-8. Lines starting with # are comments.
# Keys are not case-sensitive. Delete a line to go back to its default.

[tools]
# Outside programs the app needs, one per line: name = value
# Photo Viewer uses the picture decoders built into Windows, so it needs none.
# Values other apps in this family understand:
#   auto  = look next to the exe, then in each folder directly inside it,
#           then in that folder's bin folder, then on the PATH
#   path  = only look on the PATH
#   anything else = a file path. Relative paths start from the exe's folder.
#           %VARIABLES% such as %USERPROFILE% are expanded.
# Example:  ffmpeg = auto

[folders]
# Where each Browse button opens. The app updates these lines itself
# every time you pick a folder; you can also edit them by hand.
# input  = folder last chosen with the Input folder button
# output = folder last chosen with the Output folder button

[photo-viewer]
# Settings for Photo Viewer itself (none yet).
";

// ---------------------------------------------------------------- parsing

fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn section_of(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
        Some(norm(&t[1..t.len() - 1]))
    } else {
        None
    }
}

fn key_value(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    let k = norm(k);
    if k.is_empty() {
        return None;
    }
    Some((k, v.trim().to_string()))
}

/// Value of `key` in `[section]`, or `None` if absent. The last one wins.
pub fn get(text: &str, section: &str, key: &str) -> Option<String> {
    let (section, key) = (norm(section), norm(key));
    let mut current = String::new();
    let mut found = None;
    for line in text.lines() {
        if let Some(s) = section_of(line) {
            current = s;
        } else if current == section {
            if let Some((k, v)) = key_value(line) {
                if k == key {
                    found = Some(v);
                }
            }
        }
    }
    found
}

/// Returns `text` with `key` in `[section]` set to `value`. Only that key's
/// line changes; every other line, comment and blank line is kept as-is. A
/// missing key is added at the end of its section, and a missing section is
/// added at the end of the file.
pub fn set(text: &str, section: &str, key: &str, value: &str) -> String {
    let (section_n, key_n) = (norm(section), norm(key));
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let new_line = format!("{key} = {value}");

    let mut current = String::new();
    let mut last_hit = None;
    let mut section_last_content = None; // index after which to insert
    for (i, line) in lines.iter().enumerate() {
        if let Some(s) = section_of(line) {
            current = s;
            if current == section_n {
                section_last_content = Some(i);
            }
            continue;
        }
        if current == section_n {
            if !line.trim().is_empty() {
                section_last_content = Some(i);
            }
            if let Some((k, _)) = key_value(line) {
                if k == key_n {
                    last_hit = Some(i);
                }
            }
        }
    }
    if let Some(i) = last_hit {
        lines[i] = new_line;
    } else if let Some(i) = section_last_content {
        lines.insert(i + 1, new_line);
    } else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        lines.push(new_line);
    }
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

// ------------------------------------------------------------- %VARS%

/// Expands `%NAME%` using `lookup`. Unknown names and lone `%` are kept.
pub fn expand_vars_with(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn expand_vars(s: &str) -> String {
    expand_vars_with(s, |n| std::env::var(n).ok())
}

// --------------------------------------------------------------- [tools]

/// Finds an outside program from its `[tools]` value. `name` is the file
/// name to look for (for example `ffmpeg.exe`). An explicit path that does
/// not exist is an error naming the key and the path; there is no silent
/// fallback to searching.
#[allow(dead_code)] // Photo Viewer has no outside tools yet.
pub fn resolve_tool(
    key: &str,
    value: Option<&str>,
    name: &str,
    exe_dir: &Path,
    path_var: Option<&str>,
) -> Result<PathBuf, String> {
    let value = value.map(str::trim).filter(|v| !v.is_empty()).unwrap_or("auto");
    let on_path = || -> Option<PathBuf> {
        std::env::split_paths(path_var?)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    };
    match value.to_ascii_lowercase().as_str() {
        "auto" => {
            let mut candidates = vec![exe_dir.join(name)];
            if let Ok(entries) = fs::read_dir(exe_dir) {
                let mut subdirs: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                subdirs.sort();
                for d in &subdirs {
                    candidates.push(d.join(name));
                }
                for d in &subdirs {
                    candidates.push(d.join("bin").join(name));
                }
            }
            candidates
                .into_iter()
                .find(|p| p.is_file())
                .or_else(on_path)
                .ok_or_else(|| {
                    format!("[tools] {key} = auto: {name} was not found next to the program, in its subfolders, or on the PATH.")
                })
        }
        "path" => on_path().ok_or_else(|| format!("[tools] {key} = path: {name} was not found on the PATH.")),
        _ => {
            let expanded = PathBuf::from(expand_vars(value));
            let full = if expanded.is_absolute() { expanded } else { exe_dir.join(expanded) };
            if full.is_file() {
                Ok(full)
            } else {
                Err(format!("[tools] {key}: the file {} does not exist.", full.display()))
            }
        }
    }
}

// ------------------------------------------------------------- [folders]

/// `path` if it is a folder, else its nearest parent that is, else `None`.
pub fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut p = Some(path);
    while let Some(dir) = p {
        if !dir.as_os_str().is_empty() && dir.is_dir() {
            return Some(dir.to_path_buf());
        }
        p = dir.parent();
    }
    None
}

// ------------------------------------------------------- file on disk

struct Store {
    text: String,
    /// Where changes are saved; `None` = remember for this session only.
    path: Option<PathBuf>,
    /// The AppData copy, used when the exe's folder cannot be written to.
    fallback: Option<PathBuf>,
}

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn appdata_file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join(APP_DIR_NAME).join(FILE_NAME))
}

fn write_file(path: &Path, text: &str) -> bool {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    fs::write(path, text).is_ok()
}

fn load() -> Store {
    let exe_file = exe_dir().map(|d| d.join(FILE_NAME));
    let fallback = appdata_file();
    let read = |p: &Option<PathBuf>| p.as_ref().and_then(|p| fs::read_to_string(p).ok());
    // A UTF-8 byte order mark (Notepad adds one) is not part of the text.
    let clean = |t: String| t.trim_start_matches('\u{feff}').to_string();

    if let Some(text) = read(&exe_file).map(clean) {
        // Folders remembered in AppData (because the exe's folder was
        // read-only) still apply on top of the exe-side file.
        let mut text = text;
        if let Some(extra) = read(&fallback).map(clean) {
            for key in ["input", "output"] {
                if let Some(v) = get(&extra, "folders", key) {
                    text = set(&text, "folders", key, &v);
                }
            }
        }
        return Store { text, path: exe_file, fallback };
    }
    if let Some(text) = read(&fallback).map(clean) {
        return Store { text, path: fallback.clone(), fallback };
    }
    let text = TEMPLATE.replace('\n', "\r\n");
    let path = [&exe_file, &fallback]
        .into_iter()
        .flatten()
        .find(|p| write_file(p, &text))
        .cloned();
    Store { text, path, fallback }
}

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    STORE.with(|s| f(s.borrow_mut().get_or_insert_with(load)))
}

/// Creates the file on first run if it is missing. Never fails.
pub fn init() {
    with_store(|_| ());
}

/// Folder a Browse button should open at: its remembered folder, or that
/// folder's nearest existing parent, or `None` for the default.
pub fn picker_start(key: &str) -> Option<String> {
    let saved = with_store(|s| get(&s.text, "folders", key))?;
    if saved.is_empty() {
        return None;
    }
    nearest_existing(Path::new(&expand_vars(&saved))).map(|p| p.display().to_string())
}

/// Remembers the folder a Browse button picked, changing only that line.
pub fn remember_folder(key: &str, folder: &str) {
    with_store(|s| {
        s.text = set(&s.text, "folders", key, folder);
        let Some(path) = s.path.clone() else { return };
        if write_file(&path, &s.text) {
            return;
        }
        // The exe's folder is read-only: keep a copy in AppData instead.
        if let Some(fb) = s.fallback.clone().filter(|fb| *fb != path) {
            let fb_text = fs::read_to_string(&fb)
                .map(|t| t.trim_start_matches('\u{feff}').to_string())
                .unwrap_or_else(|_| s.text.clone());
            let fb_text = set(&fb_text, "folders", key, folder);
            if write_file(&fb, &fb_text) {
                return;
            }
        }
        s.path = None; // session only
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pv-conf-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parses_sections_keys_and_comments() {
        let t = "# c\n[Tools]\nFFmpeg = auto \n; x = 1\n[folders]\ninput=D:\\Photos\n  # input = nope\n";
        assert_eq!(get(t, "tools", "ffmpeg").as_deref(), Some("auto"));
        assert_eq!(get(t, "FOLDERS", "Input").as_deref(), Some("D:\\Photos"));
        assert_eq!(get(t, "folders", "output"), None);
        assert_eq!(get(t, "tools", "input"), None);
        assert_eq!(get(t, "tools", "x"), None);
    }

    #[test]
    fn template_parses_with_no_values() {
        assert_eq!(get(TEMPLATE, "folders", "input"), None);
        assert_eq!(get(TEMPLATE, "tools", "ffmpeg"), None);
        assert!(TEMPLATE.contains("[tools]") && TEMPLATE.contains("[photo-viewer]"));
    }

    #[test]
    fn rewrite_changes_only_that_line() {
        let t = "# top\r\n[folders]\r\n# my note\r\ninput = C:\\Old\r\noutput = E:\\Picks\r\n\r\n[other]\r\nk = v\r\n";
        let out = set(t, "folders", "input", "D:\\New");
        assert_eq!(out, t.replace("input = C:\\Old", "input = D:\\New"));
    }

    #[test]
    fn rewrite_appends_missing_key_inside_section() {
        let t = "[folders]\n# note\n\n[photo-viewer]\n# x\n";
        let out = set(t, "folders", "output", "E:\\Picks");
        assert_eq!(out, "[folders]\n# note\noutput = E:\\Picks\n\n[photo-viewer]\n# x\n");
    }

    #[test]
    fn rewrite_adds_missing_section() {
        assert_eq!(set("[tools]\n", "folders", "input", "D:\\"), "[tools]\n\n[folders]\ninput = D:\\\n");
        assert_eq!(set("", "folders", "input", "D:\\"), "[folders]\ninput = D:\\\n");
    }

    #[test]
    fn expands_vars() {
        let look = |n: &str| (n.eq_ignore_ascii_case("USERPROFILE")).then(|| "C:\\Users\\me".to_string());
        assert_eq!(expand_vars_with("%USERPROFILE%\\bin\\x.exe", look), "C:\\Users\\me\\bin\\x.exe");
        assert_eq!(expand_vars_with("%NOPE%\\a", look), "%NOPE%\\a");
        assert_eq!(expand_vars_with("100% sure", look), "100% sure");
        assert_eq!(expand_vars_with("a%%b", look), "a%%b");
    }

    #[test]
    fn auto_search_order() {
        let root = temp_dir("auto");
        let exe = root.join("app");
        let pathdir = root.join("onpath");
        for d in [&exe, &exe.join("aaa").join("bin"), &exe.join("zzz"), &pathdir] {
            fs::create_dir_all(d).unwrap();
        }
        let path_var = pathdir.display().to_string();
        let pv = Some(path_var.as_str());
        let touch = |p: PathBuf| fs::write(p, b"x").unwrap();

        assert!(resolve_tool("t", Some("auto"), "t.exe", &exe, pv).is_err());
        touch(pathdir.join("t.exe"));
        assert_eq!(resolve_tool("t", None, "t.exe", &exe, pv).unwrap(), pathdir.join("t.exe"));
        touch(exe.join("aaa").join("bin").join("t.exe"));
        assert_eq!(resolve_tool("t", None, "t.exe", &exe, pv).unwrap(), exe.join("aaa").join("bin").join("t.exe"));
        // A direct subfolder beats any subfolder's bin, even if sorted later.
        touch(exe.join("zzz").join("t.exe"));
        assert_eq!(resolve_tool("t", None, "t.exe", &exe, pv).unwrap(), exe.join("zzz").join("t.exe"));
        touch(exe.join("t.exe"));
        assert_eq!(resolve_tool("t", Some("AUTO"), "t.exe", &exe, pv).unwrap(), exe.join("t.exe"));
        // `path` ignores everything except the PATH.
        assert_eq!(resolve_tool("t", Some("path"), "t.exe", &exe, pv).unwrap(), pathdir.join("t.exe"));
        // Relative explicit path is from the exe's folder.
        assert_eq!(resolve_tool("t", Some("zzz\\t.exe"), "t.exe", &exe, pv).unwrap(), exe.join("zzz\\t.exe"));
        // Missing explicit path: error names key and path, no fallback.
        let err = resolve_tool("ffmpeg", Some("nope\\t.exe"), "t.exe", &exe, pv).unwrap_err();
        assert!(err.contains("ffmpeg") && err.contains("nope"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nearest_existing_parent() {
        let root = temp_dir("near");
        let gone = root.join("a").join("b").join("c");
        assert_eq!(nearest_existing(&gone), Some(root.clone()));
        fs::create_dir_all(root.join("a")).unwrap();
        assert_eq!(nearest_existing(&gone), Some(root.join("a")));
        assert_eq!(nearest_existing(&root), Some(root.clone()));
        assert_eq!(nearest_existing(Path::new("")), None);
        let _ = fs::remove_dir_all(&root);
    }
}
