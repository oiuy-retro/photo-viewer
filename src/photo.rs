use std::collections::{HashMap, HashSet};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::core::{w, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{GENERIC_READ, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameDecode, IWICImagingFactory,
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
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub props: Vec<(String, String)>,
}

impl Decoded {
    pub fn bytes(&self) -> u64 {
        self.bgra.len() as u64
    }
}

pub struct Done {
    pub gen: u64,
    pub path: PathBuf,
    pub result: Result<Arc<Decoded>, String>,
}

struct Job {
    gen: u64,
    path: PathBuf,
}

pub struct Hub {
    tx: Sender<Option<Job>>,
    results: Arc<Mutex<Vec<Done>>>,
    hwnd: Arc<AtomicIsize>,
    workers: Vec<JoinHandle<()>>,
}

impl Hub {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Option<Job>>();
        let rx = Arc::new(Mutex::new(rx));
        let results = Arc::new(Mutex::new(Vec::new()));
        let hwnd = Arc::new(AtomicIsize::new(0));
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
            .clamp(2, 4);
        let mut workers = Vec::new();
        for _ in 0..n {
            let rx = Arc::clone(&rx);
            let results = Arc::clone(&results);
            let hwnd = Arc::clone(&hwnd);
            workers.push(std::thread::spawn(move || worker(rx, results, hwnd)));
        }
        Self { tx, results, hwnd, workers }
    }

    pub fn set_hwnd(&self, hwnd: HWND) {
        self.hwnd.store(hwnd.0 as isize, Ordering::SeqCst);
    }

    pub fn submit(&self, gen: u64, path: PathBuf) {
        let _ = self.tx.send(Some(Job { gen, path }));
    }

    pub fn drain(&self) -> Vec<Done> {
        let mut guard = self.results.lock().unwrap();
        std::mem::take(&mut *guard)
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.hwnd.store(0, Ordering::SeqCst);
        for _ in 0..self.workers.len() {
            let _ = self.tx.send(None);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker(rx: Arc<Mutex<Receiver<Option<Job>>>>, results: Arc<Mutex<Vec<Done>>>, hwnd: Arc<AtomicIsize>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    loop {
        let job = {
            let locked = rx.lock().unwrap();
            locked.recv()
        };
        match job {
            Ok(Some(job)) => {
                let result = decode_file(&job.path).map(Arc::new);
                results.lock().unwrap().push(Done {
                    gen: job.gen,
                    path: job.path,
                    result,
                });
                let raw = hwnd.load(Ordering::SeqCst);
                if raw != 0 {
                    let window = HWND(raw as *mut core::ffi::c_void);
                    unsafe {
                        let _ = PostMessageW(window, WM_PHOTO_READY, WPARAM(0), LPARAM(0));
                    }
                }
            }
            _ => break,
        }
    }
    unsafe {
        CoUninitialize();
    }
}

pub struct Library {
    pub gen: u64,
    pub items: Vec<Photo>,
    pub current: usize,
    decoded: HashMap<PathBuf, Arc<Decoded>>,
    failed: HashMap<PathBuf, String>,
    inflight: HashSet<PathBuf>,
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
            inflight: HashSet::new(),
            copied: HashSet::new(),
        }
    }

    pub fn open_input(&mut self, hub: &Hub, folder: &Path) {
        self.gen = self.gen.wrapping_add(1);
        self.items = scan_folder(folder);
        self.current = 0;
        self.decoded.clear();
        self.failed.clear();
        self.inflight.clear();
        self.kick(hub);
    }

    pub fn set_output(&mut self, folder: &Path) {
        self.copied = scan_names(folder);
    }

    pub fn current_photo(&self) -> Option<&Photo> {
        self.items.get(self.current)
    }

    pub fn current_decoded(&self) -> Option<Arc<Decoded>> {
        self.current_photo().and_then(|p| self.decoded.get(&p.path).cloned())
    }

    pub fn current_error(&self) -> Option<&str> {
        self.current_photo().and_then(|p| self.failed.get(&p.path).map(|s| s.as_str()))
    }

    pub fn is_copied(&self) -> bool {
        self.current_photo().map(|p| self.copied.contains(&p.name)).unwrap_or(false)
    }

    pub fn ram_count(&self) -> usize {
        self.decoded.len()
    }

    pub fn ram_bytes(&self) -> u64 {
        self.decoded.values().map(|d| d.bytes()).sum()
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
        self.current = next;
        self.kick(hub);
        true
    }

    pub fn apply(&mut self, hub: &Hub) -> bool {
        let done = hub.drain();
        if done.is_empty() {
            return false;
        }
        for item in done {
            self.inflight.remove(&item.path);
            if item.gen != self.gen {
                continue;
            }
            if !self.items.iter().any(|p| p.path == item.path) {
                continue;
            }
            match item.result {
                Ok(image) => {
                    self.decoded.insert(item.path, image);
                }
                Err(err) => {
                    self.failed.insert(item.path, err);
                }
            }
        }
        self.evict();
        self.kick(hub);
        true
    }

    pub fn kick(&mut self, hub: &Hub) {
        self.evict();
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let budget = memory_budget();
        let mut used = self.ram_bytes();
        let avg = if self.decoded.is_empty() {
            20 * 1024 * 1024
        } else {
            (used / self.decoded.len() as u64).max(1)
        };
        let mut inflight = self.inflight.len();
        let current_path = self.items[self.current].path.clone();
        for idx in priority(self.current, n) {
            if inflight >= 6 {
                break;
            }
            let path = self.items[idx].path.clone();
            if self.decoded.contains_key(&path) || self.failed.contains_key(&path) || self.inflight.contains(&path) {
                continue;
            }
            let must = path == current_path;
            if !must && used + avg > budget && !self.decoded.is_empty() {
                break;
            }
            self.inflight.insert(path.clone());
            hub.submit(self.gen, path);
            used = used.saturating_add(avg);
            inflight += 1;
        }
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
        self.inflight.remove(&photo.path);
        let index = self.current;
        self.items.remove(index);
        if self.current >= self.items.len() && self.current > 0 {
            self.current -= 1;
        }
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
            self.copied.insert(photo.name.clone());
            return Ok("Already in the output folder.");
        }
        if dest.exists() {
            self.copied.insert(photo.name.clone());
            return Ok("Already in the output folder.");
        }
        let src = path_wide(&photo.path);
        let dst = path_wide(&dest);
        unsafe {
            CopyFileW(PCWSTR(src.as_ptr()), PCWSTR(dst.as_ptr()), true).map_err(|e| e.to_string())?;
        }
        self.copied.insert(photo.name.clone());
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
            names.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXT.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

fn priority(current: usize, n: usize) -> impl Iterator<Item = usize> {
    std::iter::once(current).chain((1..n).flat_map(move |d| {
        let mut out = Vec::with_capacity(2);
        if current + d < n {
            out.push(current + d);
        }
        if current >= d {
            out.push(current - d);
        }
        out
    }))
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

pub(crate) fn decode_file(path: &Path) -> Result<Decoded, String> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let wide = path_wide(path);
        let decoder = factory
            .CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .map_err(|e| e.to_string())?;
        let frame = decoder.GetFrame(0).map_err(|e| e.to_string())?;
        let converter = factory.CreateFormatConverter().map_err(|e| e.to_string())?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None::<&IWICPalette>,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|e| e.to_string())?;

        let orient = orientation(&frame);
        let (width, height, bgra) = if let Some(options) = orient_options(orient) {
            let rotator = factory.CreateBitmapFlipRotator().map_err(|e| e.to_string())?;
            rotator.Initialize(&converter, options).map_err(|e| e.to_string())?;
            copy_bgra(&rotator)?
        } else {
            copy_bgra(&converter)?
        };

        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let mut props = vec![
            ("Dimensions".into(), format!("{} × {}", width, height)),
            ("Size".into(), format_bytes(file_size)),
        ];
        read_store(path, &mut props);
        Ok(Decoded { width, height, bgra, props })
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
    fn scans_and_decodes_jpeg() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
        let items = scan_folder(&folder);
        assert!(!items.is_empty(), "expected pictures, got {}", items.len());
        assert!(items.iter().any(|p| p.name.eq_ignore_ascii_case("sample.jpg")));
        let decoded = decode_file(&items[0].path).expect("decode");
        assert!(decoded.width > 0 && decoded.height > 0);
        assert_eq!(decoded.bgra.len(), decoded.width as usize * decoded.height as usize * 4);
        assert!(decoded.props.iter().any(|(k, _)| k == "Dimensions"));
        unsafe {
            CoUninitialize();
        }
    }
}
