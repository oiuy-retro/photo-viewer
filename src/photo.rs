use std::collections::{HashMap, HashSet, VecDeque};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use windows::core::{w, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GENERIC_READ, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGR,
    GUID_WICPixelFormat32bppBGRA, GUID_WICPixelFormat8bppGray, IWICBitmapFrameDecode, IWICBitmapSource,
    IWICBitmapSourceTransform, IWICImagingFactory, WICBitmapInterpolationModeFant, WICBitmapTransformRotate0,
    IWICPalette, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICBitmapTransformFlipHorizontal,
    WICBitmapTransformFlipVertical, WICBitmapTransformOptions, WICBitmapTransformRotate180,
    WICBitmapTransformRotate270, WICBitmapTransformRotate90, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_ApplicationName, PKEY_Author, PKEY_Comment, PKEY_Copyright, PKEY_DateModified, PKEY_GPS_Latitude,
    PKEY_GPS_Longitude, PKEY_Image_Dimensions, PKEY_ItemTypeText, PKEY_Photo_CameraManufacturer,
    PKEY_Photo_CameraModel, PKEY_Photo_DateTaken, PKEY_Photo_ExposureBias, PKEY_Photo_ExposureTime,
    PKEY_Photo_FNumber, PKEY_Photo_FlashText, PKEY_Photo_FocalLength, PKEY_Photo_ISOSpeed,
    PKEY_Photo_LensModel, PKEY_Photo_MeteringModeText, PKEY_Photo_Orientation, PKEY_Photo_WhiteBalanceText,
    PKEY_Size, PKEY_SoftwareUsed, PKEY_Title,
};
use windows::Win32::Storage::FileSystem::CopyFileW;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IPropertyStore, PSFormatForDisplayAlloc, SHGetPropertyStoreFromParsingName, GPS_DEFAULT,
    PDFF_DEFAULT, PROPERTYKEY,
};
use windows::Win32::UI::Shell::{
    SHFileOperationW, StrCmpLogicalW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    FO_DELETE, SHFILEOPSTRUCTW,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::PROPVARIANT;

use crate::luna;

pub const WM_PHOTO_READY: u32 = 0x8001;

const EXT: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "dib", "tif", "tiff", "webp", "ico", "heic",
    "heif", "wdp", "jxr", "hdp", "avif",
];

/// Camera RAW formats. Windows opens these only when a RAW codec is installed
/// (the free Microsoft "Raw Image Extension" on Windows 10/11, or a camera
/// maker's WIC codec).
const RAW_EXT: &[&str] = &[
    "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef",
    "ptx", "srw", "x3f", "3fr", "fff", "iiq", "mef", "mos", "mrw", "erf", "kdc", "dcr", "rwl",
    "raw", "gpr",
];

/// WINCODEC_ERR_COMPONENTNOTFOUND: no WIC decoder is installed for the file.
const NO_CODEC: windows::core::HRESULT = windows::core::HRESULT(0x88982F50u32 as i32);

const PROP_KEYS: &[(&str, PROPERTYKEY)] = &[
    ("Type", PKEY_ItemTypeText),
    ("Size", PKEY_Size),
    ("Dimensions", PKEY_Image_Dimensions),
    ("Date taken", PKEY_Photo_DateTaken),
    ("Date modified", PKEY_DateModified),
    ("Camera maker", PKEY_Photo_CameraManufacturer),
    ("Camera model", PKEY_Photo_CameraModel),
    ("Lens", PKEY_Photo_LensModel),
    ("F-stop", PKEY_Photo_FNumber),
    ("Exposure", PKEY_Photo_ExposureTime),
    ("ISO", PKEY_Photo_ISOSpeed),
    ("Focal length", PKEY_Photo_FocalLength),
    ("Exposure bias", PKEY_Photo_ExposureBias),
    ("Flash", PKEY_Photo_FlashText),
    ("White balance", PKEY_Photo_WhiteBalanceText),
    ("Metering", PKEY_Photo_MeteringModeText),
    ("Orientation", PKEY_Photo_Orientation),
    ("Latitude", PKEY_GPS_Latitude),
    ("Longitude", PKEY_GPS_Longitude),
    ("Software", PKEY_SoftwareUsed),
    ("Application", PKEY_ApplicationName),
    ("Copyright", PKEY_Copyright),
    ("Authors", PKEY_Author),
    ("Title", PKEY_Title),
    ("Comment", PKEY_Comment),
];

#[derive(Clone)]
pub struct Photo {
    pub path: PathBuf,
    pub name: String,
    #[allow(dead_code)]
    pub size: u64,
}

pub struct Decoded {
    /// Size of the picture itself (after EXIF rotation), even when the pixels
    /// below were decoded smaller.
    pub width: u32,
    pub height: u32,
    /// Size of `bgra`. Smaller than `width` × `height` for a screen-size decode.
    pub pix_w: u32,
    pub pix_h: u32,
    /// True when `bgra` holds every pixel of the picture.
    pub full: bool,
    pub bgra: Vec<u8>,
    pub props: Vec<(String, String)>,
}

impl Decoded {
    pub fn bytes(&self) -> u64 {
        self.bgra.len() as u64
    }
}

pub enum Payload {
    Image(Result<Arc<Decoded>, String>),
    Props(Vec<(String, String)>),
}

pub struct Done {
    pub gen: u64,
    pub path: PathBuf,
    pub full: bool,
    pub payload: Payload,
}

#[derive(Clone)]
pub struct Job {
    pub gen: u64,
    pub path: PathBuf,
    /// Decode every pixel instead of a screen-size copy.
    pub full: bool,
    /// The picture on screen. Urgent jobs jump the queue and always have a
    /// free worker; preloads never use the last worker.
    pub urgent: bool,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    running: Vec<(PathBuf, bool)>,
    /// Finished but not yet picked up by `drain`, so not scheduled again.
    pending: Vec<(PathBuf, bool)>,
    running_bg: usize,
    stop: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    results: Mutex<Vec<Done>>,
    hwnd: AtomicIsize,
    /// Height in pixels of the monitor the window is on. Preloads are decoded
    /// to this height so showing one is a cheap copy, not a full-size scale.
    target_h: AtomicU32,
    workers: usize,
}

pub struct Hub {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl Hub {
    pub fn new() -> Self {
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .clamp(2, 6);
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            results: Mutex::new(Vec::new()),
            hwnd: AtomicIsize::new(0),
            target_h: AtomicU32::new(1080),
            workers: n,
        });
        let workers = (0..n)
            .map(|_| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || worker(shared))
            })
            .collect();
        Self { shared, workers }
    }

    pub fn set_hwnd(&self, hwnd: HWND) {
        self.shared.hwnd.store(hwnd.0 as isize, Ordering::SeqCst);
    }

    pub fn set_target_height(&self, h: u32) {
        if h > 0 {
            self.shared.target_h.store(h, Ordering::SeqCst);
        }
    }

    pub fn target_height(&self) -> u32 {
        self.shared.target_h.load(Ordering::SeqCst)
    }

    /// Replace everything still waiting with `jobs` (most wanted first).
    /// Preloads that are no longer near the current picture are dropped here
    /// instead of making the picture you jumped to wait behind them.
    pub fn schedule(&self, jobs: Vec<Job>) {
        let mut q = self.shared.queue.lock().unwrap();
        let busy: Vec<(PathBuf, bool)> = q.running.iter().chain(q.pending.iter()).cloned().collect();
        q.jobs = jobs
            .into_iter()
            .filter(|j| !busy.iter().any(|(p, f)| *p == j.path && *f == j.full))
            .collect();
        drop(q);
        self.shared.wake.notify_all();
    }

    pub fn drain(&self) -> Vec<Done> {
        let done = std::mem::take(&mut *self.shared.results.lock().unwrap());
        let mut q = self.shared.queue.lock().unwrap();
        for d in &done {
            if matches!(d.payload, Payload::Image(_)) {
                if let Some(i) = q.pending.iter().position(|(p, f)| *p == d.path && *f == d.full) {
                    q.pending.remove(i);
                }
            }
        }
        done
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.shared.hwnd.store(0, Ordering::SeqCst);
        {
            let mut q = self.shared.queue.lock().unwrap();
            q.stop = true;
            q.jobs.clear();
        }
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn next_job(shared: &Shared) -> Option<Job> {
    let mut q = shared.queue.lock().unwrap();
    loop {
        if q.stop {
            return None;
        }
        let pick = q
            .jobs
            .iter()
            .position(|j| j.urgent)
            .or_else(|| (!q.jobs.is_empty() && q.running_bg + 1 < shared.workers).then_some(0));
        if let Some(i) = pick {
            let job = q.jobs.remove(i).unwrap();
            if !job.urgent {
                q.running_bg += 1;
            }
            q.running.push((job.path.clone(), job.full));
            return Some(job);
        }
        q = shared.wake.wait(q).unwrap();
    }
}

fn worker(shared: Arc<Shared>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    while let Some(job) = next_job(&shared) {
        let target = if job.full { None } else { Some(shared.target_h.load(Ordering::SeqCst)) };
        let started = Instant::now();
        let result = decode_file(&job.path, target).map(Arc::new);
        if timing_enabled() {
            let what = match &result {
                Ok(d) => format!("{} {}x{} of {}x{}", if d.full { "full" } else { "screen" }, d.pix_w, d.pix_h, d.width, d.height),
                Err(e) => format!("failed: {e}"),
            };
            timing_log(&format!(
                "decode {} {} in {:.0} ms{}",
                file_name(&job.path),
                what,
                started.elapsed().as_secs_f64() * 1000.0,
                if job.urgent { " (on screen)" } else { " (preload)" }
            ));
        }
        let base = result.as_ref().ok().map(|d| d.props.clone());
        {
            let mut q = shared.queue.lock().unwrap();
            q.pending.push((job.path.clone(), job.full));
            if !job.urgent {
                q.running_bg -= 1;
            }
            if let Some(i) = q.running.iter().position(|(p, f)| *p == job.path && *f == job.full) {
                q.running.remove(i);
            }
        }
        post(&shared, Done { gen: job.gen, path: job.path.clone(), full: job.full, payload: Payload::Image(result) });
        shared.wake.notify_all();
        // The Properties pane reads the file again through the Shell, which is
        // slow. Do it after the picture is on screen.
        if let (Some(mut props), false) = (base, job.full) {
            read_store(&job.path, &mut props);
            post(&shared, Done { gen: job.gen, path: job.path, full: false, payload: Payload::Props(props) });
        }
    }
    unsafe {
        CoUninitialize();
    }
}

fn post(shared: &Shared, done: Done) {
    shared.results.lock().unwrap().push(done);
    let raw = shared.hwnd.load(Ordering::SeqCst);
    if raw != 0 {
        let window = HWND(raw as *mut core::ffi::c_void);
        unsafe {
            let _ = PostMessageW(window, WM_PHOTO_READY, WPARAM(0), LPARAM(0));
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Set the environment variable `PV_TIMING=1` to log decode and switch times
/// to `%TEMP%\photo-viewer-timing.log`.
pub fn timing_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PV_TIMING").is_some_and(|v| v != "0"))
}

pub fn timing_log(line: &str) {
    use std::io::Write;
    static FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
    let file = FILE.get_or_init(|| {
        let path = std::env::temp_dir().join("photo-viewer-timing.log");
        Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(path).ok())
    });
    if let Ok(mut guard) = file.lock() {
        if let Some(f) = guard.as_mut() {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// How many pictures to keep decoded ahead of / behind the current one.
const AHEAD: usize = 8;
const BEHIND: usize = 3;

pub struct Library {
    pub gen: u64,
    pub items: Vec<Photo>,
    pub current: usize,
    decoded: HashMap<PathBuf, Arc<Decoded>>,
    failed: HashMap<PathBuf, String>,
    props: HashMap<PathBuf, Vec<(String, String)>>,
    /// Every pixel of the current picture, decoded only when zoomed in past
    /// the screen-size copy. Dropped when the picture changes.
    full: Option<(PathBuf, Arc<Decoded>)>,
    want_full: Option<PathBuf>,
    /// The picture whose full decode has finished (worked or not), so it is not retried.
    full_tried: Option<PathBuf>,
    /// +1 when moving forward through the folder, -1 when moving back.
    dir: i32,
    pub copied: HashSet<String>,
}

impl Library {
    pub fn new() -> Self {
        Self {
            gen: 0,
            items: Vec::new(),
            current: 0,
            decoded: HashMap::new(),
            failed: HashMap::new(),
            props: HashMap::new(),
            full: None,
            want_full: None,
            full_tried: None,
            dir: 1,
            copied: HashSet::new(),
        }
    }

    pub fn open_input(&mut self, hub: &Hub, folder: &Path) {
        self.gen = self.gen.wrapping_add(1);
        self.items = scan_folder(folder);
        self.current = 0;
        self.dir = 1;
        self.decoded.clear();
        self.failed.clear();
        self.props.clear();
        self.full = None;
        self.want_full = None;
        self.full_tried = None;
        self.kick(hub);
    }

    pub fn set_output(&mut self, folder: &Path) {
        self.copied = scan_names(folder);
    }

    pub fn current_photo(&self) -> Option<&Photo> {
        self.items.get(self.current)
    }

    /// The best copy of the current picture: all pixels if they have been
    /// decoded, otherwise the screen-size copy.
    pub fn current_decoded(&self) -> Option<Arc<Decoded>> {
        let p = self.current_photo()?;
        if let Some((path, full)) = &self.full {
            if *path == p.path {
                return Some(Arc::clone(full));
            }
        }
        self.decoded.get(&p.path).cloned()
    }

    pub fn current_props(&self) -> Vec<(String, String)> {
        let Some(p) = self.current_photo() else {
            return Vec::new();
        };
        if let Some(props) = self.props.get(&p.path) {
            return props.clone();
        }
        self.decoded.get(&p.path).map(|d| d.props.clone()).unwrap_or_default()
    }

    pub fn current_error(&self) -> Option<&str> {
        self.current_photo().and_then(|p| self.failed.get(&p.path).map(|s| s.as_str()))
    }

    pub fn is_copied(&self) -> bool {
        self.current_photo().map(|p| self.copied.contains(&name_key(&p.name))).unwrap_or(false)
    }

    pub fn ram_count(&self) -> usize {
        self.decoded.len()
    }

    pub fn ram_bytes(&self) -> u64 {
        let full = self.full.as_ref().map(|(_, d)| d.bytes()).unwrap_or(0);
        self.decoded.values().map(|d| d.bytes()).sum::<u64>() + full
    }

    pub fn go(&mut self, hub: &Hub, delta: i32) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let last = self.items.len() as i32 - 1;
        let next = (self.current as i32 + delta).clamp(0, last) as usize;
        if next == self.current {
            return false;
        }
        self.dir = if next > self.current { 1 } else { -1 };
        self.current = next;
        self.drop_full();
        self.kick(hub);
        true
    }

    fn drop_full(&mut self) {
        let current = self.current_photo().map(|p| p.path.clone());
        if self.full.as_ref().map(|(p, _)| Some(p) != current.as_ref()).unwrap_or(false) {
            self.full = None;
        }
        if self.want_full.as_ref() != current.as_ref() {
            self.want_full = None;
        }
        if self.full_tried.as_ref() != current.as_ref() {
            self.full_tried = None;
        }
    }

    /// Ask for every pixel of the current picture (zoomed in past the
    /// screen-size copy). Returns true if a decode was started.
    pub fn want_full(&mut self, hub: &Hub) -> bool {
        let Some(path) = self.current_photo().map(|p| p.path.clone()) else {
            return false;
        };
        if self.want_full.as_ref() == Some(&path) {
            return false;
        }
        if self.decoded.get(&path).map(|d| d.full).unwrap_or(false) {
            return false;
        }
        self.want_full = Some(path);
        self.kick(hub);
        true
    }

    pub fn apply(&mut self, hub: &Hub) -> bool {
        let done = hub.drain();
        if done.is_empty() {
            return false;
        }
        for item in done {
            if item.gen != self.gen {
                continue;
            }
            if !self.items.iter().any(|p| p.path == item.path) {
                continue;
            }
            match item.payload {
                Payload::Image(result) if item.full => {
                    if self.current_photo().map(|p| p.path == item.path).unwrap_or(false) {
                        self.full_tried = Some(item.path.clone());
                        if let Ok(image) = result {
                            self.full = Some((item.path, image));
                        }
                    }
                }
                Payload::Image(Ok(image)) => {
                    self.decoded.insert(item.path, image);
                }
                Payload::Image(Err(err)) => {
                    self.failed.insert(item.path, err);
                }
                Payload::Props(props) => {
                    self.props.insert(item.path, props);
                }
            }
        }
        self.kick(hub);
        true
    }

    pub fn kick(&mut self, hub: &Hub) {
        self.evict();
        let n = self.items.len();
        if n == 0 {
            hub.schedule(Vec::new());
            return;
        }
        let budget = memory_budget();
        let mut used = self.ram_bytes();
        let avg = if self.decoded.is_empty() {
            // A screen-size copy: roughly 16:9 at the target height.
            let h = hub.target_height() as u64;
            (h * h * 16 / 9 * 4).max(1)
        } else {
            (self.decoded.values().map(|d| d.bytes()).sum::<u64>() / self.decoded.len() as u64).max(1)
        };
        let current_path = self.items[self.current].path.clone();
        let mut jobs = Vec::new();
        for idx in priority(self.current, n, self.dir) {
            let path = &self.items[idx].path;
            if self.decoded.contains_key(path) || self.failed.contains_key(path) {
                continue;
            }
            let urgent = *path == current_path;
            if !urgent && used + avg > budget && !self.decoded.is_empty() {
                break;
            }
            jobs.push(Job { gen: self.gen, path: path.clone(), full: false, urgent });
            used = used.saturating_add(avg);
        }
        if self.want_full.as_ref() == Some(&current_path)
            && self.full.is_none()
            && self.full_tried.as_ref() != Some(&current_path)
            && !self.failed.contains_key(&current_path)
        {
            let at = if jobs.first().map(|j| j.urgent).unwrap_or(false) { 1 } else { 0 };
            jobs.insert(at, Job { gen: self.gen, path: current_path, full: true, urgent: true });
        }
        hub.schedule(jobs);
    }

    fn evict(&mut self) {
        let budget = memory_budget();
        let current_path = self.current_photo().map(|p| p.path.clone());
        while self.ram_bytes() > budget && self.decoded.len() > 1 {
            let cur = self.current;
            let farthest = self
                .decoded
                .keys()
                .filter(|p| Some(*p) != current_path.as_ref())
                .max_by_key(|p| {
                    let idx = self.items.iter().position(|it| &it.path == *p).unwrap_or(cur);
                    (idx as i32 - cur as i32).unsigned_abs()
                })
                .cloned();
            match farthest {
                Some(path) => {
                    self.decoded.remove(&path);
                    self.props.remove(&path);
                }
                None => break,
            }
        }
    }

    pub fn delete_current(&mut self, hub: &Hub, hwnd: HWND) -> Result<(), String> {
        let photo = self.current_photo().cloned().ok_or_else(|| "No picture.".to_string())?;
        recycle(hwnd, &photo.path)?;
        self.decoded.remove(&photo.path);
        self.failed.remove(&photo.path);
        self.props.remove(&photo.path);
        let index = self.current;
        self.items.remove(index);
        if self.current >= self.items.len() && self.current > 0 {
            self.current -= 1;
        }
        self.drop_full();
        self.kick(hub);
        Ok(())
    }

    pub fn copy_current(&mut self, output: &Path) -> Result<&'static str, String> {
        let photo = self.current_photo().ok_or_else(|| "No picture.".to_string())?;
        if output.as_os_str().is_empty() {
            return Err("Choose an output folder.".into());
        }
        let dest = output.join(&photo.name);
        if dest == photo.path {
            self.copied.insert(name_key(&photo.name));
            return Ok("Already in the output folder.");
        }
        if dest.exists() {
            self.copied.insert(name_key(&photo.name));
            return Ok("Already in the output folder.");
        }
        let src = path_wide(&photo.path);
        let dst = path_wide(&dest);
        unsafe {
            CopyFileW(PCWSTR(src.as_ptr()), PCWSTR(dst.as_ptr()), true).map_err(|e| e.to_string())?;
        }
        self.copied.insert(name_key(&photo.name));
        Ok("Copied.")
    }
}

pub fn scan_folder(folder: &Path) -> Vec<Photo> {
    let mut items = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return items;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_image(&path) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        items.push(Photo { path, name, size });
    }
    items.sort_by(|a, b| {
        let wa = luna::wide(&a.name);
        let wb = luna::wide(&b.name);
        unsafe { StrCmpLogicalW(PCWSTR(wa.as_ptr()), PCWSTR(wb.as_ptr())).cmp(&0) }
    });
    items
}

fn scan_names(folder: &Path) -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return names;
    };
    for entry in entries.flatten() {
        if entry.path().is_file() {
            names.insert(name_key(&entry.file_name().to_string_lossy()));
        }
    }
    names
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXT.iter().chain(RAW_EXT).any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXT.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

/// Current picture first, then the next few in the direction you are moving,
/// with a couple behind in case you step back.
fn priority(current: usize, n: usize, dir: i32) -> Vec<usize> {
    let step = |d: usize, forward: bool| -> Option<usize> {
        let ahead = (dir >= 0) == forward;
        if ahead {
            (current + d < n).then_some(current + d)
        } else {
            current.checked_sub(d)
        }
    };
    let mut out = vec![current];
    let (mut a, mut b) = (1, 1);
    while a <= AHEAD || b <= BEHIND {
        for _ in 0..2 {
            if a <= AHEAD {
                if let Some(i) = step(a, true) {
                    out.push(i);
                }
                a += 1;
            }
        }
        if b <= BEHIND {
            if let Some(i) = step(b, false) {
                out.push(i);
            }
            b += 1;
        }
    }
    out
}

pub fn memory_budget() -> u64 {
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_err() {
            return 512 * 1024 * 1024;
        }
        let from_avail = status.ullAvailPhys.saturating_sub(768 * 1024 * 1024);
        let from_total = status.ullTotalPhys / 2;
        from_avail.min(from_total).max(256 * 1024 * 1024)
    }
}

pub fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    if n < 1024 {
        format!("{} bytes", n)
    } else if (n as f64) < MB {
        format!("{:.1} KB", n as f64 / KB)
    } else if (n as f64) < GB {
        format!("{:.1} MB", n as f64 / MB)
    } else {
        format!("{:.2} GB", n as f64 / GB)
    }
}

/// Windows file names ignore upper/lower case, so `IMG_1.JPG` and `img_1.jpg`
/// are the same file. The "Copied" marker compares names in this form.
fn name_key(name: &str) -> String {
    name.to_lowercase()
}

fn path_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

fn recycle(hwnd: HWND, path: &Path) -> Result<(), String> {
    let mut from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        hwnd,
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_mut_ptr()),
        pTo: PCWSTR(std::ptr::null()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0 | FOF_NOERRORUI.0) as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Could not delete the picture ({code})."))
    }
}

/// Decode a picture. With `target_h`, pictures taller than that are decoded
/// straight to that height (like Windows Photo Viewer does for the screen);
/// `None` decodes every pixel.
pub(crate) fn decode_file(path: &Path, target_h: Option<u32>) -> Result<Decoded, String> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let wide = path_wide(path);
        let decoder = factory
            .CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .map_err(|e| open_error(path, &e))?;
        let frame = decoder.GetFrame(0).map_err(|e| e.to_string())?;
        let orient = orientation(&frame);
        let (mut fw, mut fh) = (0u32, 0u32);
        frame.GetSize(&mut fw, &mut fh).map_err(|e| e.to_string())?;
        if fw == 0 || fh == 0 {
            return Err("The picture has no size.".into());
        }
        let turned = orient >= 5;
        let (width, height) = if turned { (fh, fw) } else { (fw, fh) };
        // Decode size before EXIF rotation.
        let (sw, sh) = match target_h {
            Some(t) if t > 0 && height > t => {
                let s = t as f64 / height as f64;
                (((fw as f64 * s).round() as u32).max(1), ((fh as f64 * s).round() as u32).max(1))
            }
            _ => (fw, fh),
        };
        let shrink = sw != fw || sh != fh;

        // Full decode first. RAW codecs can fail here (or while copying the
        // pixels) on files they only partly understand, so fall back to the
        // full-size JPEG preview most cameras embed, then to the thumbnail.
        let mut preview = false;
        let source: IWICBitmapSource = frame.cast().map_err(|e| e.to_string())?;
        let first = if shrink {
            scaled(&factory, &frame, fw, fh, sw, sh).and_then(|s| to_bgra(&factory, &s, orient))
        } else {
            to_bgra(&factory, &source, orient)
        };
        let (pix_w, pix_h, bgra) = match first {
            Ok(px) => px,
            Err(full_err) => {
                let fallbacks = [
                    decoder.GetPreview().ok(),
                    frame.GetThumbnail().ok(),
                    decoder.GetThumbnail().ok(),
                ];
                let mut found = None;
                for small in fallbacks.into_iter().flatten() {
                    let small = fit_within(&factory, small, target_h);
                    if let Ok(px) = to_bgra(&factory, &small, orient) {
                        found = Some(px);
                        break;
                    }
                }
                match found {
                    Some(px) => {
                        preview = true;
                        px
                    }
                    None if is_raw(path) => {
                        return Err(format!(
                            "Windows could not decode this RAW file ({full_err}). Updating the Raw Image Extension from the Microsoft Store may help."
                        ))
                    }
                    None => return Err(full_err),
                }
            }
        };
        // An embedded preview is the best this file can give, so it counts as full.
        let (width, height) = if preview { (pix_w, pix_h) } else { (width, height) };
        let full = preview || !shrink;

        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mut props = vec![
            ("Dimensions".into(), format!("{} × {}", width, height)),
            ("Size".into(), format_bytes(file_size)),
        ];
        if preview {
            props.push(("Shown".into(), "Embedded preview (full RAW decode failed)".into()));
        }
        Ok(Decoded { width, height, pix_w, pix_h, full, bgra, props })
    }
}

/// Scale a decoder frame down to `w` × `h`. JPEG (and some other) decoders can
/// skip most of the work by decoding at 1/2, 1/4 or 1/8 size; use that when
/// it is still at least as big as needed, then finish with a smooth scale.
unsafe fn scaled(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
    fw: u32,
    fh: u32,
    w: u32,
    h: u32,
) -> Result<IWICBitmapSource, String> {
    let mut src: IWICBitmapSource = frame.cast().map_err(|e| e.to_string())?;
    if let Some(native) = native_downscale(factory, frame, fw, fh, w, h) {
        src = native;
    }
    let scaler = factory.CreateBitmapScaler().map_err(|e| e.to_string())?;
    scaler.Initialize(&src, w, h, WICBitmapInterpolationModeFant).map_err(|e| e.to_string())?;
    scaler.cast().map_err(|e| e.to_string())
}

unsafe fn native_downscale(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
    fw: u32,
    fh: u32,
    w: u32,
    h: u32,
) -> Option<IWICBitmapSource> {
    let st: IWICBitmapSourceTransform = frame.cast().ok()?;
    let mut size = None;
    for k in [8u32, 4, 2] {
        let (rw, rh) = (fw.div_ceil(k), fh.div_ceil(k));
        if rw < w || rh < h {
            continue;
        }
        let (mut nw, mut nh) = (rw, rh);
        if st.GetClosestSize(&mut nw, &mut nh).is_ok() && nw >= w && nh >= h && nw < fw && nh < fh {
            size = Some((nw, nh));
            break;
        }
    }
    let (nw, nh) = size?;
    let mut format = GUID_WICPixelFormat32bppBGRA;
    st.GetClosestPixelFormat(&mut format).ok()?;
    let bytes = if format == GUID_WICPixelFormat32bppBGRA || format == GUID_WICPixelFormat32bppBGR {
        4
    } else if format == GUID_WICPixelFormat24bppBGR {
        3
    } else if format == GUID_WICPixelFormat8bppGray {
        1
    } else {
        return None;
    };
    let stride = (nw.checked_mul(bytes)? + 3) & !3;
    let len = (stride as usize).checked_mul(nh as usize)?;
    let mut buf = Vec::new();
    buf.try_reserve(len).ok()?;
    buf.resize(len, 0);
    st.CopyPixels(std::ptr::null(), nw, nh, &format, WICBitmapTransformRotate0, stride, &mut buf).ok()?;
    let bitmap = factory.CreateBitmapFromMemory(nw, nh, &format, stride, &buf).ok()?;
    bitmap.cast().ok()
}

/// Shrink an embedded preview to the target height if it is taller.
unsafe fn fit_within(factory: &IWICImagingFactory, src: IWICBitmapSource, target_h: Option<u32>) -> IWICBitmapSource {
    let Some(t) = target_h.filter(|t| *t > 0) else {
        return src;
    };
    let (mut w, mut h) = (0u32, 0u32);
    if src.GetSize(&mut w, &mut h).is_err() || h <= t || w == 0 {
        return src;
    }
    let nw = ((w as f64 * t as f64 / h as f64).round() as u32).max(1);
    let Ok(scaler) = factory.CreateBitmapScaler() else {
        return src;
    };
    if scaler.Initialize(&src, nw, t, WICBitmapInterpolationModeFant).is_err() {
        return src;
    }
    scaler.cast().unwrap_or(src)
}

/// Plain-language message when Windows cannot open a file at all.
fn open_error(path: &Path, err: &windows::core::Error) -> String {
    if err.code() != NO_CODEC {
        return err.to_string();
    }
    if is_raw(path) {
        "Windows has no RAW codec for this camera file. Install the free \"Raw Image Extension\" from the Microsoft Store, then reopen the folder.".into()
    } else {
        "Windows has no decoder for this picture type.".into()
    }
}

/// Convert any WIC source to 32bpp BGRA, apply the EXIF orientation and copy the pixels out.
fn to_bgra(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
    orient: u16,
) -> Result<(u32, u32, Vec<u8>), String> {
    unsafe {
        let converter = factory.CreateFormatConverter().map_err(|e| e.to_string())?;
        converter
            .Initialize(
                source,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None::<&IWICPalette>,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|e| e.to_string())?;
        if let Some(options) = orient_options(orient) {
            let rotator = factory.CreateBitmapFlipRotator().map_err(|e| e.to_string())?;
            rotator.Initialize(&converter, options).map_err(|e| e.to_string())?;
            copy_bgra(&rotator)
        } else {
            copy_bgra(&converter)
        }
    }
}

fn copy_bgra<S: Interface>(source: &S) -> Result<(u32, u32, Vec<u8>), String> {
    let bitmap: windows::Win32::Graphics::Imaging::IWICBitmapSource =
        source.cast().map_err(|e| e.to_string())?;
    unsafe {
        let mut width = 0u32;
        let mut height = 0u32;
        bitmap.GetSize(&mut width, &mut height).map_err(|e| e.to_string())?;
        if width == 0 || height == 0 {
            return Err("The picture has no size.".into());
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| "The picture is too large.".to_string())?;
        let mut bgra = Vec::new();
        bgra.try_reserve(len).map_err(|_| "Not enough memory.".to_string())?;
        bgra.resize(len, 0);
        let stride = width.saturating_mul(4);
        bitmap
            .CopyPixels(std::ptr::null(), stride, &mut bgra)
            .map_err(|e| e.to_string())?;
        Ok((width, height, bgra))
    }
}

fn orientation(frame: &IWICBitmapFrameDecode) -> u16 {
    let Ok(reader) = (unsafe { frame.GetMetadataQueryReader() }) else {
        return 1;
    };
    for name in [w!("System.Photo.Orientation"), w!("/app1/ifd/{ushort=274}")] {
        let mut value = PROPVARIANT::new();
        if unsafe { reader.GetMetadataByName(name, &mut value) }.is_ok() {
            if let Ok(n) = u16::try_from(&value) {
                if n >= 1 && n <= 8 {
                    return n;
                }
            }
        }
    }
    1
}

fn orient_options(n: u16) -> Option<WICBitmapTransformOptions> {
    Some(match n {
        2 => WICBitmapTransformFlipHorizontal,
        3 => WICBitmapTransformRotate180,
        4 => WICBitmapTransformFlipVertical,
        5 => WICBitmapTransformOptions(WICBitmapTransformFlipHorizontal.0 | WICBitmapTransformRotate270.0),
        6 => WICBitmapTransformRotate90,
        7 => WICBitmapTransformOptions(WICBitmapTransformFlipHorizontal.0 | WICBitmapTransformRotate90.0),
        8 => WICBitmapTransformRotate270,
        _ => return None,
    })
}

fn read_store(path: &Path, props: &mut Vec<(String, String)>) {
    let wide = path_wide(path);
    let store: IPropertyStore =
        match unsafe { SHGetPropertyStoreFromParsingName(PCWSTR(wide.as_ptr()), None, GPS_DEFAULT) } {
            Ok(store) => store,
            Err(_) => return,
        };
    let mut seen: HashSet<String> = props.iter().map(|(_, v)| v.clone()).collect();
    for (label, key) in PROP_KEYS {
        let value = match unsafe { store.GetValue(key) } {
            Ok(v) if !v.is_empty() => v,
            _ => continue,
        };
        let display = match unsafe { PSFormatForDisplayAlloc(key, &value, PDFF_DEFAULT) } {
            Ok(p) => p,
            Err(_) => continue,
        };
        let text = pwstr(display);
        if text.is_empty() || !seen.insert(text.clone()) {
            continue;
        }
        if *label == "Dimensions" || *label == "Size" {
            if let Some(row) = props.iter_mut().find(|(k, _)| k == label) {
                row.1 = text;
                continue;
            }
        }
        props.push(((*label).to_string(), text));
    }
}

fn pwstr(ptr: PWSTR) -> String {
    if ptr.0.is_null() {
        return String::new();
    }
    let text = unsafe { ptr.to_string().unwrap_or_default() };
    unsafe { CoTaskMemFree(Some(ptr.0 as *const _)) };
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    #[test]
    fn lists_raw_files() {
        for name in ["a.CR2", "b.nef", "c.ARW", "d.dng", "e.cr3", "f.RAF"] {
            assert!(is_image(Path::new(name)), "{name} should be listed");
            assert!(is_raw(Path::new(name)));
        }
        assert!(!is_raw(Path::new("g.jpg")));
        assert!(!is_image(Path::new("h.txt")));
    }

    #[test]
    fn scans_and_decodes_jpeg() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let items = scan_folder(&folder);
        assert!(!items.is_empty(), "expected pictures, got {}", items.len());
        assert!(items.iter().any(|p| p.name.eq_ignore_ascii_case("sample.jpg")));
        let decoded = decode_file(&items[0].path, None).expect("decode");
        assert!(decoded.width > 0 && decoded.height > 0);
        assert_eq!(decoded.bgra.len(), decoded.width as usize * decoded.height as usize * 4);
        assert!(decoded.props.iter().any(|(k, _)| k == "Dimensions"));
        unsafe {
            CoUninitialize();
        }
    }

    #[test]
    fn preloads_follow_direction() {
        assert_eq!(priority(5, 20, 1)[..6], [5, 6, 7, 4, 8, 9]);
        assert_eq!(priority(5, 20, -1)[..6], [5, 4, 3, 6, 2, 1]);
        let all = priority(0, 3, 1);
        assert_eq!(all, vec![0, 1, 2]);
        assert!(priority(0, 1, -1) == vec![0]);
    }

    #[test]
    fn screen_copy_is_smaller_but_keeps_real_size() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("sample.jpg");
        let full = decode_file(&path, None).expect("full");
        assert!(full.full && full.pix_w == full.width);
        let small = decode_file(&path, Some((full.height / 2).max(1))).expect("small");
        assert_eq!((small.width, small.height), (full.width, full.height));
        assert_eq!(small.pix_h, (full.height / 2).max(1));
        assert!(!small.full);
        assert_eq!(small.bgra.len(), small.pix_w as usize * small.pix_h as usize * 4);
    }

    #[test]
    fn copied_names_ignore_case() {
        assert_eq!(name_key("IMG_0001.JPG"), name_key("img_0001.jpg"));
        assert_ne!(name_key("a.jpg"), name_key("b.jpg"));
    }
}

/// Timing benchmark, not run by `cargo test`. Point `PV_BENCH_DIR` at a folder
/// of large JPEGs and run `cargo test --release -- --ignored --nocapture bench`.
#[cfg(test)]
mod bench {
    use super::*;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
        SetStretchBltMode, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HALFTONE, SRCCOPY,
    };

    fn ms(t: Instant) -> f64 {
        t.elapsed().as_secs_f64() * 1000.0
    }

    /// One fit-height draw into a 1400 × 800 window, like a repaint does.
    fn paint_ms(d: &Decoded) -> f64 {
        let t = Instant::now();
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, 1400, 800);
            ReleaseDC(None, screen);
            let old = SelectObject(dc, bmp);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: d.pix_w as i32,
                    biHeight: -(d.pix_h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            SetStretchBltMode(dc, HALFTONE);
            let dw = (d.width as f64 * 800.0 / d.height as f64) as i32;
            StretchDIBits(dc, 0, 0, dw, 800, 0, 0, d.pix_w as i32, d.pix_h as i32, Some(d.bgra.as_ptr() as _), &info, DIB_RGB_COLORS, SRCCOPY);
            SelectObject(dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(dc);
        }
        ms(t)
    }

    #[test]
    #[ignore]
    fn bench() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let Some(dir) = std::env::var_os("PV_BENCH_DIR") else {
            return;
        };
        let items = scan_folder(Path::new(&dir));
        unsafe {
            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();
            let wide = path_wide(&items[0].path);
            let decoder = factory
                .CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
                .unwrap();
            let frame = decoder.GetFrame(0).unwrap();
            println!("BENCH JPEG decoder can scale while decoding: {}", frame.cast::<IWICBitmapSourceTransform>().is_ok());
        }
        for p in &items {
            let t = Instant::now();
            let full = decode_file(&p.path, None).unwrap();
            let full_ms = ms(t);
            let t = Instant::now();
            let screen = decode_file(&p.path, Some(1080)).unwrap();
            let screen_ms = ms(t);
            println!(
                "BENCH {}: full {:.0} ms + paint {:.0} ms ({:.0} MB) | screen copy {}x{} {:.0} ms + paint {:.0} ms ({:.1} MB)",
                p.name,
                full_ms,
                paint_ms(&full),
                full.bytes() as f64 / 1048576.0,
                screen.pix_w,
                screen.pix_h,
                screen_ms,
                paint_ms(&screen),
                screen.bytes() as f64 / 1048576.0
            );
        }
        // Jump to a picture while the others are still being preloaded.
        let hub = Hub::new();
        hub.set_target_height(1080);
        let job = |p: &Photo, urgent| Job { gen: 1, path: p.path.clone(), full: false, urgent };
        hub.schedule(items.iter().skip(1).map(|p| job(p, false)).collect());
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t = Instant::now();
        let mut jobs = vec![job(&items[0], true)];
        jobs.extend(items.iter().skip(1).map(|p| job(p, false)));
        hub.schedule(jobs);
        'wait: loop {
            for d in hub.drain() {
                if d.path == items[0].path && matches!(d.payload, Payload::Image(_)) {
                    break 'wait;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        println!("BENCH switch-wait {:.0} ms", ms(t));
    }
}
