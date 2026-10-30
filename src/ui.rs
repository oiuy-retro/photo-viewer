use std::cell::{Cell, RefCell};
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CombineRgn, CreateCompatibleBitmap, CreateCompatibleDC, CreateRectRgn,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, GetDC,
    GetDeviceCaps, GetMonitorInfoW, IntersectClipRect, InvalidateRect, MonitorFromWindow, ReleaseDC,
    ScreenToClient, SelectClipRgn, SelectObject, SetBrushOrgEx, SetDIBitsToDevice, SetStretchBltMode,
    SetWindowRgn, StretchDIBits, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HALFTONE, HBITMAP, HDC, HGDIOBJ, HRGN, LOGPIXELSX, MONITORINFO, MONITOR_DEFAULTTONEAREST, RGN_OR,
    SRCCOPY,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DELETE,
    VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileDialog, IShellItem, SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, DrawIconEx, GetClientRect,
    GetCursorPos, GetMessageW, GetSystemMetrics, IsZoomed, LoadCursorW, LoadImageW, PostQuitMessage,
    RegisterClassW, SendMessageW, SetCursor, ShowWindow, TranslateMessage, CS_DBLCLKS, CS_DROPSHADOW,
    CS_HREDRAW, CS_VREDRAW, DI_NORMAL, HMENU, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
    HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IMAGE_ICON,
    MINMAXINFO, MSG, NCCALCSIZE_PARAMS, SM_CXFRAME, SM_CXPADDEDBORDER, SM_CYFRAME, SW_MAXIMIZE,
    SW_MINIMIZE, SW_RESTORE, SW_SHOW, WINDOW_EX_STYLE, WM_ACTIVATE, WM_CAPTURECHANGED, WM_CLOSE,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_CLIPCHILDREN, WS_MAXIMIZEBOX,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
};

use crate::luna::{self, CapBtn, Fonts};
use crate::photo::{self, Decoded, Hub, Library};

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static IN_CALLBACK: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Start,
    View,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hot {
    None,
    Min,
    Max,
    Close,
    InputBrowse,
    OutputBrowse,
    Open,
    Folders,
    Image,
    Caption,
    Exif,
}

enum View {
    FitHeight,
    Free { scale: f64, ox: f64, oy: f64 },
}

impl View {
    fn zoomed(&self) -> bool {
        !matches!(self, View::FitHeight)
    }
}

enum Action {
    None,
    Pass,
    Result(isize),
    BrowseInput,
    BrowseOutput,
    Open,
    Close,
    Min,
    Max,
    DragCaption,
}

struct Layout {
    caption: RECT,
    btn_min: RECT,
    btn_max: RECT,
    btn_close: RECT,
    input_label: RECT,
    input_box: RECT,
    input_browse: RECT,
    output_label: RECT,
    output_box: RECT,
    output_browse: RECT,
    open: RECT,
    folders: RECT,
    name_bar: RECT,
    well: RECT,
    well_inner: RECT,
    exif: RECT,
    exif_header: RECT,
    exif_body: RECT,
    status: RECT,
}

struct Backbuffer {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    w: i32,
    h: i32,
}

impl Backbuffer {
    fn empty() -> Self {
        Self {
            dc: HDC(std::ptr::null_mut()),
            bmp: HBITMAP(std::ptr::null_mut()),
            old: HGDIOBJ(std::ptr::null_mut()),
            w: 0,
            h: 0,
        }
    }

    fn ensure(&mut self, hwnd: HWND, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if self.w == w && self.h == h && !self.dc.0.is_null() {
            return;
        }
        self.destroy();
        unsafe {
            let screen = GetDC(hwnd);
            let dc = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let _ = ReleaseDC(hwnd, screen);
            let old = SelectObject(dc, bmp);
            self.dc = dc;
            self.bmp = bmp;
            self.old = old;
            self.w = w;
            self.h = h;
        }
    }

    fn destroy(&mut self) {
        unsafe {
            if !self.dc.0.is_null() {
                SelectObject(self.dc, self.old);
                if !self.bmp.0.is_null() {
                    let _ = DeleteObject(self.bmp);
                }
                let _ = DeleteDC(self.dc);
            }
        }
        *self = Self::empty();
    }
}

struct App {
    hwnd: HWND,
    dpi: i32,
    fonts: Fonts,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    hub: Hub,
    lib: Library,
    screen: Screen,
    input: String,
    output: String,
    view: View,
    hot: Hot,
    press: Hot,
    dragging: bool,
    last_mouse: POINT,
    notice: String,
    active: bool,
    tracking: bool,
    exif_scroll: i32,
    shown: Option<PathBuf>,
    back: Backbuffer,
    /// The visible part of the current picture, already scaled. Repaints that
    /// do not change the picture or the view (hover, status text) just copy it.
    pic: Backbuffer,
    pic_key: Option<(Arc<Decoded>, RECT)>,
    /// When `PV_TIMING` is set: the picture switched to and when the key was pressed.
    switch_at: Option<(PathBuf, Instant)>,
    face: windows::Win32::Graphics::Gdi::HBRUSH,
}

pub fn run() -> i32 {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let module = GetModuleHandleW(None).unwrap_or_default();
        let instance = HINSTANCE(module.0);
        let face = CreateSolidBrush(luna::rgb(luna::FACE));
        register_class(instance, face);
        APP.with(|slot| *slot.borrow_mut() = Some(App::boot(instance, face)));
        let hwnd = create_window(instance);
        if hwnd.0.is_null() {
            return 1;
        }
        APP.with(|slot| {
            if let Some(app) = slot.borrow_mut().as_mut() {
                app.hwnd = hwnd;
                app.hub.set_hwnd(hwnd);
            }
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0);
            if got.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|slot| *slot.borrow_mut() = None);
        let _ = DeleteObject(face);
        let _ = CoUninitialize();
        0
    }
}

fn register_class(instance: HINSTANCE, face: windows::Win32::Graphics::Gdi::HBRUSH) {
    unsafe {
        let icon = load_icon(instance, 32);
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let class = WNDCLASSW {
            style: CS_DBLCLKS | CS_DROPSHADOW | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            hIcon: icon,
            hCursor: cursor,
            hbrBackground: face,
            lpszClassName: w!("PhotoViewerWindow"),
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
    }
}

fn load_icon(instance: HINSTANCE, px: i32) -> windows::Win32::UI::WindowsAndMessaging::HICON {
    unsafe {
        let handle = LoadImageW(instance, PCWSTR(1 as *const u16), IMAGE_ICON, px, px, Default::default());
        windows::Win32::UI::WindowsAndMessaging::HICON(handle.unwrap_or_default().0)
    }
}

fn frame_style() -> windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE {
    WS_OVERLAPPED | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME | WS_CLIPCHILDREN
}

fn create_window(instance: HINSTANCE) -> HWND {
    unsafe {
        let screen = GetDC(None);
        let sw = GetDeviceCaps(screen, windows::Win32::Graphics::Gdi::HORZRES);
        let sh = GetDeviceCaps(screen, windows::Win32::Graphics::Gdi::VERTRES);
        let _ = ReleaseDC(None, screen);
        let w = (sw * 4 / 5).max(800);
        let h = (sh * 4 / 5).max(600);
        let x = (sw - w) / 2;
        let y = (sh - h) / 2;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("PhotoViewerWindow"),
            w!("Photo Viewer"),
            frame_style(),
            x,
            y,
            w,
            h,
            HWND(std::ptr::null_mut()),
            HMENU(std::ptr::null_mut()),
            instance,
            None,
        )
        .unwrap_or_default()
    }
}

/// DPI of the monitor the window is on. The app is per-monitor DPI aware, so
/// `GetDeviceCaps(LOGPIXELSX)` would only return the main screen's DPI.
fn screen_dpi(hwnd: HWND) -> i32 {
    unsafe {
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) as i32;
        if dpi > 0 {
            return dpi;
        }
        let dc = GetDC(hwnd);
        let dpi = GetDeviceCaps(dc, LOGPIXELSX);
        let _ = ReleaseDC(hwnd, dc);
        if dpi <= 0 {
            96
        } else {
            dpi
        }
    }
}

fn scale(px: i32, dpi: i32) -> i32 {
    px * dpi / 96
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCALCSIZE {
        return nccalcsize(hwnd, wparam, lparam);
    }
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if msg == WM_GETMINMAXINFO {
        fill_minmax(hwnd, lparam);
        return LRESULT(0);
    }
    if IN_CALLBACK.get() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let action = APP.with(|slot| {
        let Ok(mut borrow) = slot.try_borrow_mut() else {
            return Action::Pass;
        };
        let Some(app) = borrow.as_mut() else {
            return Action::Pass;
        };
        app.handle(hwnd, msg, wparam, lparam)
    });
    match action {
        Action::Pass => DefWindowProcW(hwnd, msg, wparam, lparam),
        Action::Result(v) => LRESULT(v),
        Action::None => LRESULT(0),
        Action::BrowseInput => {
            browse(true);
            LRESULT(0)
        }
        Action::BrowseOutput => {
            browse(false);
            LRESULT(0)
        }
        Action::Open => {
            APP.with(|slot| {
                if let Some(app) = slot.borrow_mut().as_mut() {
                    app.open_view();
                    app.redraw();
                }
            });
            LRESULT(0)
        }
        Action::Close => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        Action::Min => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            LRESULT(0)
        }
        Action::Max => {
            unsafe {
                let cmd = if IsZoomed(hwnd).as_bool() { SW_RESTORE } else { SW_MAXIMIZE };
                let _ = ShowWindow(hwnd, cmd);
            }
            LRESULT(0)
        }
        Action::DragCaption => {
            unsafe {
                let _ = ReleaseCapture();
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let packed = LPARAM((((pt.y as u32) << 16) | (pt.x as u32 & 0xFFFF)) as isize);
                let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), packed);
            }
            LRESULT(0)
        }
    }
}

unsafe fn nccalcsize(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // WS_THICKFRAME maximize hangs the window past the monitor so the OS frame
    // sits off-screen. We drew that overshoot as client, which clipped all four
    // sides. Pull the client back onto the work area.
    if wparam.0 != 0 {
        let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
        if IsZoomed(hwnd).as_bool() || rect_covers_work(hwnd, params.rgrc[0]) {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                params.rgrc[0] = mi.rcWork;
            } else {
                let pad = GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                let pady = GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                params.rgrc[0].left += pad;
                params.rgrc[0].top += pady;
                params.rgrc[0].right -= pad;
                params.rgrc[0].bottom -= pady;
            }
        }
    }
    LRESULT(0)
}

fn rect_covers_work(hwnd: HWND, proposed: RECT) -> bool {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }
        let pw = proposed.right - proposed.left;
        let ph = proposed.bottom - proposed.top;
        let ww = mi.rcWork.right - mi.rcWork.left;
        let wh = mi.rcWork.bottom - mi.rcWork.top;
        pw >= ww && ph >= wh
    }
}

fn fill_minmax(hwnd: HWND, lparam: LPARAM) {
    unsafe {
        let info = &mut *(lparam.0 as *mut MINMAXINFO);
        let dpi = screen_dpi(hwnd);
        info.ptMinTrackSize.x = scale(640, dpi);
        info.ptMinTrackSize.y = scale(480, dpi);
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            info.ptMaxPosition.x = mi.rcWork.left - mi.rcMonitor.left;
            info.ptMaxPosition.y = mi.rcWork.top - mi.rcMonitor.top;
            info.ptMaxSize.x = mi.rcWork.right - mi.rcWork.left;
            info.ptMaxSize.y = mi.rcWork.bottom - mi.rcWork.top;
        }
    }
}

fn browse(input: bool) {
    let (hwnd, current) = APP.with(|slot| {
        let app = slot.borrow();
        let app = app.as_ref().unwrap();
        (app.hwnd, if input { app.input.clone() } else { app.output.clone() })
    });
    let title = if input { "Input folder" } else { "Output folder" };
    let picked = pick_folder(hwnd, &current, title);
    unsafe {
        let _ = SetFocus(hwnd);
    }
    let Some(path) = picked else {
        return;
    };
    APP.with(|slot| {
        if let Some(app) = slot.borrow_mut().as_mut() {
            if input {
                app.set_input(path);
            } else {
                app.set_output(path);
            }
            app.redraw();
        }
    });
}

fn pick_folder(owner: HWND, current: &str, title: &str) -> Option<String> {
    unsafe {
        let dialog: IFileDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let title_w = luna::wide(title);
        dialog.SetTitle(PCWSTR(title_w.as_ptr())).ok()?;
        if !current.is_empty() {
            let wide = luna::wide(current);
            let item: windows::core::Result<IShellItem> = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None);
            if let Ok(item) = item {
                let _ = dialog.SetFolder(&item);
            }
        }
        if dialog.Show(owner).is_err() {
            return None;
        }
        let result = dialog.GetResult().ok()?;
        let name = result.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = name.to_string().unwrap_or_default();
        CoTaskMemFree(Some(name.0 as *const _));
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

impl App {
    fn boot(instance: HINSTANCE, face: windows::Win32::Graphics::Gdi::HBRUSH) -> Self {
        let dpi = 96;
        Self {
            hwnd: luna::null_hwnd(),
            dpi,
            fonts: Fonts::new(dpi),
            icon: load_icon(instance, 16),
            hub: Hub::new(),
            lib: Library::new(),
            screen: Screen::Start,
            input: String::new(),
            output: String::new(),
            view: View::FitHeight,
            hot: Hot::None,
            press: Hot::None,
            dragging: false,
            last_mouse: POINT::default(),
            notice: String::new(),
            active: true,
            tracking: false,
            exif_scroll: 0,
            shown: None,
            back: Backbuffer::empty(),
            pic: Backbuffer::empty(),
            pic_key: None,
            switch_at: None,
            face,
        }
    }

    fn handle(&mut self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Action {
        self.hwnd = hwnd;
        match msg {
            x if x == WM_PAINT => {
                self.paint();
                Action::None
            }
            x if x == WM_SIZE => {
                if wparam.0 != 1 {
                    self.sync_dpi();
                    self.update_target();
                    self.apply_region();
                    self.redraw();
                }
                Action::None
            }
            x if x == WM_DPICHANGED => {
                // The new DPI is in the low word of wparam; lparam is the
                // window rect Windows suggests for the new monitor.
                self.set_dpi((wparam.0 & 0xFFFF) as i32);
                self.update_target();
                let rect = unsafe { &*(lparam.0 as *const RECT) };
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                        hwnd,
                        HWND(std::ptr::null_mut()),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                            | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
                    );
                }
                self.apply_region();
                self.redraw();
                Action::None
            }
            x if x == windows::Win32::UI::WindowsAndMessaging::WM_MOVE
                || x == windows::Win32::UI::WindowsAndMessaging::WM_DISPLAYCHANGE =>
            {
                self.update_target();
                Action::Pass
            }
            x if x == WM_NCHITTEST => self.hit_test(lparam),
            x if x == WM_SETCURSOR => self.set_cursor(),
            x if x == WM_ACTIVATE => {
                self.active = (wparam.0 & 0xFFFF) != 0;
                self.redraw();
                Action::None
            }
            x if x == photo::WM_PHOTO_READY => {
                if self.lib.apply(&self.hub) {
                    self.sync_view();
                    self.redraw();
                }
                Action::None
            }
            x if x == WM_KEYDOWN => self.key(wparam),
            x if x == WM_MOUSEWHEEL => self.wheel(wparam, lparam),
            x if x == WM_MOUSEMOVE => self.mouse_move(lparam),
            x if x == WM_LBUTTONDOWN => self.mouse_down(lparam),
            x if x == WM_LBUTTONUP => self.mouse_up(),
            x if x == WM_LBUTTONDBLCLK => self.mouse_dbl(lparam),
            x if x == WM_MOUSELEAVE => {
                self.tracking = false;
                self.hot = Hot::None;
                self.redraw();
                Action::None
            }
            x if x == WM_CAPTURECHANGED => {
                self.dragging = false;
                self.press = Hot::None;
                Action::None
            }
            x if x == WM_CLOSE => Action::Close,
            x if x == WM_DESTROY => {
                self.hub.set_hwnd(luna::null_hwnd());
                self.back.destroy();
                self.pic.destroy();
                self.fonts.drop_all();
                unsafe { PostQuitMessage(0) };
                Action::None
            }
            _ => Action::Pass,
        }
    }

    fn client(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut r);
        }
        r
    }

    fn maximized(&self) -> bool {
        unsafe { IsZoomed(self.hwnd).as_bool() }
    }

    fn layout(&self) -> Layout {
        let r = self.client();
        let dpi = self.dpi;
        let s = |px: i32| scale(px, dpi);
        let border = if self.maximized() { 0 } else { s(3) };
        let cap_h = s(30);
        let btn = s(21);
        let name_h = s(22);
        let status_h = s(20);
        let exif_w = s(128);
        let header_h = s(20);
        let btn_w = s(75);
        let btn_h = s(23);
        let nil = RECT { left: 0, top: 0, right: 0, bottom: 0 };

        let inner = RECT {
            left: r.left + border,
            top: r.top + border,
            right: r.right - border,
            bottom: r.bottom - border,
        };
        let caption = RECT {
            left: inner.left,
            top: inner.top,
            right: inner.right,
            bottom: inner.top + cap_h,
        };
        let by = caption.top + (cap_h - btn) / 2;
        let close = RECT {
            left: caption.right - s(6) - btn,
            top: by,
            right: caption.right - s(6),
            bottom: by + btn,
        };
        let max = RECT {
            left: close.left - s(2) - btn,
            top: by,
            right: close.left - s(2),
            bottom: by + btn,
        };
        let min = RECT {
            left: max.left - s(2) - btn,
            top: by,
            right: max.left - s(2),
            bottom: by + btn,
        };

        if self.screen == Screen::Start {
            let card_w = s(500);
            let card_h = s(172);
            let cx = (inner.left + inner.right) / 2;
            let cy = (caption.bottom + inner.bottom) / 2;
            let card = RECT {
                left: cx - card_w / 2,
                top: cy - card_h / 2,
                right: cx + card_w / 2,
                bottom: cy + card_h / 2,
            };
            let pad = s(16);
            let label_w = s(48);
            let row1 = card.top + s(44);
            let row2 = row1 + s(32);
            let browse = RECT {
                left: card.right - pad - btn_w,
                top: row1,
                right: card.right - pad,
                bottom: row1 + btn_h,
            };
            let input_label = RECT {
                left: card.left + pad,
                top: row1,
                right: card.left + pad + label_w,
                bottom: row1 + btn_h,
            };
            let input_box = RECT {
                left: input_label.right + s(4),
                top: row1,
                right: browse.left - s(6),
                bottom: row1 + btn_h,
            };
            let output_browse = RECT {
                left: browse.left,
                top: row2,
                right: browse.right,
                bottom: row2 + btn_h,
            };
            let output_label = RECT {
                left: input_label.left,
                top: row2,
                right: input_label.right,
                bottom: row2 + btn_h,
            };
            let output_box = RECT {
                left: input_box.left,
                top: row2,
                right: input_box.right,
                bottom: row2 + btn_h,
            };
            let open = RECT {
                left: card.right - pad - btn_w,
                top: card.bottom - pad - btn_h,
                right: card.right - pad,
                bottom: card.bottom - pad,
            };
            return Layout {
                caption,
                btn_min: min,
                btn_max: max,
                btn_close: close,
                input_label,
                input_box,
                input_browse: browse,
                output_label,
                output_box,
                output_browse,
                open,
                folders: nil,
                name_bar: RECT {
                    left: card.left + pad,
                    top: card.top + s(12),
                    right: card.right - pad,
                    bottom: card.top + s(36),
                },
                well: card,
                well_inner: card,
                exif: nil,
                exif_header: nil,
                exif_body: nil,
                status: nil,
            };
        }

        let status = RECT {
            left: inner.left,
            top: inner.bottom - status_h,
            right: inner.right,
            bottom: inner.bottom,
        };
        let gap = s(3);
        let body_top = caption.bottom + gap;
        let body_bot = status.top;
        let exif = RECT {
            left: inner.right - gap - exif_w,
            top: body_top,
            right: inner.right - gap,
            bottom: body_bot,
        };
        let folders = RECT {
            left: exif.left - gap - btn_w,
            top: body_top + (name_h - btn_h) / 2,
            right: exif.left - gap,
            bottom: body_top + (name_h - btn_h) / 2 + btn_h,
        };
        let name_bar = RECT {
            left: inner.left + gap,
            top: body_top,
            right: folders.left - s(4),
            bottom: body_top + name_h,
        };
        let well = RECT {
            left: inner.left + gap,
            top: name_bar.bottom + s(2),
            right: exif.left - s(4),
            bottom: body_bot,
        };
        let well_inner = luna::inset(well, 1);
        let exif_header = RECT {
            left: exif.left,
            top: exif.top,
            right: exif.right,
            bottom: exif.top + header_h,
        };
        let exif_body = RECT {
            left: exif.left,
            top: exif_header.bottom,
            right: exif.right,
            bottom: exif.bottom,
        };
        Layout {
            caption,
            btn_min: min,
            btn_max: max,
            btn_close: close,
            input_label: nil,
            input_box: nil,
            input_browse: nil,
            output_label: nil,
            output_box: nil,
            output_browse: nil,
            open: nil,
            folders,
            name_bar,
            well,
            well_inner,
            exif,
            exif_header,
            exif_body,
            status,
        }
    }

    fn sync_dpi(&mut self) {
        self.set_dpi(screen_dpi(self.hwnd));
    }

    /// Re-create the fonts and caption icon for a new DPI. Layout, the window
    /// region and the back buffer are sized from `self.dpi` on the next paint.
    fn set_dpi(&mut self, dpi: i32) {
        if dpi <= 0 || dpi == self.dpi {
            return;
        }
        self.fonts.drop_all();
        self.dpi = dpi;
        self.fonts = Fonts::new(dpi);
        unsafe {
            let module = GetModuleHandleW(None).unwrap_or_default();
            let icon = load_icon(HINSTANCE(module.0), scale(16, dpi));
            if !icon.0.is_null() {
                if !self.icon.0.is_null() {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(self.icon);
                }
                self.icon = icon;
            }
        }
    }

    fn apply_region(&self) {
        unsafe {
            let r = self.client();
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            if w <= 0 || h <= 0 {
                return;
            }
            if self.maximized() {
                SetWindowRgn(self.hwnd, HRGN(std::ptr::null_mut()), BOOL(1));
                return;
            }
            let rad = scale(16, self.dpi);
            let round = CreateRoundRectRgn(0, 0, w + 1, h + 1, rad, rad);
            let square = CreateRectRgn(0, h / 2, w + 1, h + 1);
            CombineRgn(round, round, square, RGN_OR);
            let _ = DeleteObject(square);
            SetWindowRgn(self.hwnd, round, BOOL(1));
        }
    }

    fn redraw(&self) {
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, BOOL(0));
        }
    }

    fn set_input(&mut self, path: String) {
        self.input = path;
        self.notice.clear();
    }

    fn set_output(&mut self, path: String) {
        self.output = path;
        self.notice.clear();
    }

    fn open_view(&mut self) {
        if self.input.is_empty() {
            self.notice = "Choose an input folder.".into();
            return;
        }
        self.view = View::FitHeight;
        self.exif_scroll = 0;
        self.shown = None;
        self.notice.clear();
        self.lib.open_input(&self.hub, PathBuf::from(&self.input).as_path());
        if !self.output.is_empty() {
            self.lib.set_output(PathBuf::from(&self.output).as_path());
        }
        self.screen = Screen::View;
    }

    fn back_to_start(&mut self) {
        self.screen = Screen::Start;
        self.dragging = false;
        self.press = Hot::None;
        self.hot = Hot::None;
        self.notice.clear();
    }

    fn sync_view(&mut self) {
        let path = self.lib.current_photo().map(|p| p.path.clone());
        if path != self.shown {
            self.shown = path;
            self.view = View::FitHeight;
            self.exif_scroll = 0;
        }
    }

    fn point(lparam: LPARAM) -> POINT {
        POINT {
            x: (lparam.0 as i16) as i32,
            y: ((lparam.0 >> 16) as i16) as i32,
        }
    }

    fn hot_at(&self, p: POINT) -> Hot {
        let l = self.layout();
        if luna::hit(l.btn_close, p) {
            Hot::Close
        } else if luna::hit(l.btn_max, p) {
            Hot::Max
        } else if luna::hit(l.btn_min, p) {
            Hot::Min
        } else if self.screen == Screen::Start {
            if luna::hit(l.input_browse, p) {
                Hot::InputBrowse
            } else if luna::hit(l.output_browse, p) {
                Hot::OutputBrowse
            } else if luna::hit(l.open, p) {
                Hot::Open
            } else if luna::hit(l.caption, p) {
                Hot::Caption
            } else {
                Hot::None
            }
        } else if luna::hit(l.folders, p) {
            Hot::Folders
        } else if luna::hit(l.well_inner, p) {
            Hot::Image
        } else if luna::hit(l.exif, p) {
            Hot::Exif
        } else if luna::hit(l.caption, p) {
            Hot::Caption
        } else {
            Hot::None
        }
    }

    fn hit_test(&self, lparam: LPARAM) -> Action {
        if self.maximized() {
            return Action::Result(HTCLIENT as isize);
        }
        let mut pt = POINT {
            x: (lparam.0 as i16) as i32,
            y: ((lparam.0 >> 16) as i16) as i32,
        };
        unsafe {
            let _ = ScreenToClient(self.hwnd, &mut pt);
        }
        let r = self.client();
        let edge = scale(4, self.dpi);
        let left = pt.x < r.left + edge;
        let right = pt.x >= r.right - edge;
        let top = pt.y < r.top + edge;
        let bottom = pt.y >= r.bottom - edge;
        let code = if top && left {
            HTTOPLEFT
        } else if top && right {
            HTTOPRIGHT
        } else if bottom && left {
            HTBOTTOMLEFT
        } else if bottom && right {
            HTBOTTOMRIGHT
        } else if left {
            HTLEFT
        } else if right {
            HTRIGHT
        } else if top {
            HTTOP
        } else if bottom {
            HTBOTTOM
        } else {
            HTCLIENT
        };
        Action::Result(code as isize)
    }

    fn set_cursor(&self) -> Action {
        // Plain arrow everywhere in the client area, including over the picture
        // and while click-dragging to pan. The old four-way "move" cursor was
        // distracting whenever the picture was zoomed.
        unsafe {
            if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
                SetCursor(c);
            }
        }
        Action::Result(1)
    }

    fn track(&mut self) {
        if self.tracking {
            return;
        }
        self.tracking = true;
        let mut tme = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut tme);
        }
    }

    fn mouse_move(&mut self, lparam: LPARAM) -> Action {
        self.track();
        let p = Self::point(lparam);
        if self.dragging {
            if let View::Free { ox, oy, .. } = &mut self.view {
                *ox += (p.x - self.last_mouse.x) as f64;
                *oy += (p.y - self.last_mouse.y) as f64;
            }
            self.last_mouse = p;
            self.clamp_view();
            self.redraw();
            return Action::None;
        }
        let hot = self.hot_at(p);
        if hot != self.hot {
            self.hot = hot;
            self.redraw();
        }
        Action::None
    }

    fn mouse_down(&mut self, lparam: LPARAM) -> Action {
        let p = Self::point(lparam);
        self.press = self.hot_at(p);
        self.last_mouse = p;
        match self.press {
            Hot::Caption => return Action::DragCaption,
            Hot::Image if self.pannable() => {
                self.dragging = true;
                unsafe {
                    let _ = SetCapture(self.hwnd);
                }
            }
            Hot::Min | Hot::Max | Hot::Close | Hot::InputBrowse | Hot::OutputBrowse | Hot::Open | Hot::Folders => {
                self.redraw();
            }
            _ => {}
        }
        Action::None
    }

    fn mouse_up(&mut self) -> Action {
        if self.dragging {
            self.dragging = false;
            unsafe {
                let _ = ReleaseCapture();
            }
            self.press = Hot::None;
            return Action::None;
        }
        let press = self.press;
        self.press = Hot::None;
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
            let _ = ScreenToClient(self.hwnd, &mut pt);
        }
        let hot = self.hot_at(pt);
        if press != hot {
            self.redraw();
            return Action::None;
        }
        match press {
            Hot::Close => Action::Close,
            Hot::Min => Action::Min,
            Hot::Max => Action::Max,
            Hot::InputBrowse => Action::BrowseInput,
            Hot::OutputBrowse => Action::BrowseOutput,
            Hot::Open => Action::Open,
            Hot::Folders => {
                self.back_to_start();
                self.redraw();
                Action::None
            },
            _ => {
                self.redraw();
                Action::None
            }
        }
    }

    fn mouse_dbl(&mut self, lparam: LPARAM) -> Action {
        let p = Self::point(lparam);
        let hot = self.hot_at(p);
        if hot == Hot::Caption || hot == Hot::Max {
            return Action::Max;
        }
        if hot == Hot::Image {
            self.toggle_zoom();
            self.redraw();
        }
        Action::None
    }

    fn toggle_zoom(&mut self) {
        let Some(img) = self.lib.current_decoded() else {
            return;
        };
        let l = self.layout();
        let vw = (l.well_inner.right - l.well_inner.left).max(1) as f64;
        let vh = (l.well_inner.bottom - l.well_inner.top).max(1) as f64;
        if self.view.zoomed() {
            self.view = View::FitHeight;
        } else {
            let ox = (vw - img.width as f64) / 2.0;
            let oy = (vh - img.height as f64) / 2.0;
            self.view = View::Free { scale: 1.0, ox, oy };
            self.clamp_view();
        }
    }

    /// Fit-height scale, but never above 100%: a picture shorter than the
    /// window is shown at its real size instead of being blown up.
    fn fit_scale(&self, img: &Decoded, well: RECT) -> f64 {
        let vh = (well.bottom - well.top).max(1) as f64;
        (vh / img.height.max(1) as f64).min(1.0)
    }

    fn wheel(&mut self, wparam: WPARAM, lparam: LPARAM) -> Action {
        if self.screen != Screen::View {
            return Action::None;
        }
        let delta = ((wparam.0 >> 16) as u16 as i16) as i32;
        let mut pt = POINT {
            x: (lparam.0 as i16) as i32,
            y: ((lparam.0 >> 16) as i16) as i32,
        };
        unsafe {
            let _ = ScreenToClient(self.hwnd, &mut pt);
        }
        let l = self.layout();
        if luna::hit(l.exif, pt) {
            self.exif_scroll -= delta / 4;
            self.redraw();
            return Action::None;
        }
        if !luna::hit(l.well_inner, pt) {
            return Action::None;
        }
        let Some(img) = self.lib.current_decoded() else {
            return Action::None;
        };
        let vw = (l.well_inner.right - l.well_inner.left).max(1) as f64;
        let vh = (l.well_inner.bottom - l.well_inner.top).max(1) as f64;
        let fit = self.fit_scale(&img, l.well_inner);
        let min = self.min_scale(&img, l.well_inner);
        // At fit height, zooming out only goes further for pictures wider
        // than the window (down to fit width).
        if delta <= 0 && !self.view.zoomed() && min >= fit - 0.0005 {
            return Action::None;
        }
        if matches!(self.view, View::FitHeight) {
            let dw = img.width as f64 * fit;
            self.view = View::Free {
                scale: fit,
                ox: (vw - dw) / 2.0,
                oy: (vh - img.height as f64 * fit) / 2.0,
            };
        }
        let factor = if delta > 0 { 1.2_f64 } else { 1.0 / 1.2 };
        if let View::Free { scale, ox, oy } = &mut self.view {
            let cx = pt.x as f64 - l.well_inner.left as f64;
            let cy = pt.y as f64 - l.well_inner.top as f64;
            let img_x = (cx - *ox) / *scale;
            let img_y = (cy - *oy) / *scale;
            let next = (*scale * factor).min(16.0_f64.max(fit)).max(min);
            // Stop at fit height when a zoom step reaches or passes it, in
            // either direction.
            let eps = 0.0005;
            let lands = (next - fit).abs() <= eps;
            let crosses = (*scale > fit + eps && next < fit - eps) || (*scale < fit - eps && next > fit + eps);
            if lands || crosses || (next <= fit + eps && min >= fit - eps) {
                self.view = View::FitHeight;
            } else {
                *scale = next;
                *ox = cx - img_x * *scale;
                *oy = cy - img_y * *scale;
            }
        }
        self.clamp_view();
        self.redraw();
        Action::None
    }

    fn key(&mut self, wparam: WPARAM) -> Action {
        let vk = wparam.0 as u16;
        if self.screen == Screen::Start {
            if vk == VK_RETURN.0 {
                return Action::Open;
            }
            if vk == VK_ESCAPE.0 {
                return Action::Close;
            }
            return Action::Pass;
        }
        if vk == VK_ESCAPE.0 {
            self.back_to_start();
            self.redraw();
            return Action::None;
        }
        match vk {
            x if x == VK_LEFT.0 || x == VK_RIGHT.0 || x == VK_UP.0 || x == VK_DOWN.0 => {
                if self.pannable() {
                    let l = self.layout();
                    let vw = (l.well_inner.right - l.well_inner.left).max(1) as f64;
                    let vh = (l.well_inner.bottom - l.well_inner.top).max(1) as f64;
                    let step_x = (vw * 0.12).max(80.0);
                    let step_y = (vh * 0.12).max(80.0);
                    if let View::Free { ox, oy, .. } = &mut self.view {
                        if vk == VK_LEFT.0 {
                            *ox += step_x;
                        } else if vk == VK_RIGHT.0 {
                            *ox -= step_x;
                        } else if vk == VK_UP.0 {
                            *oy += step_y;
                        } else {
                            *oy -= step_y;
                        }
                    }
                    self.clamp_view();
                    self.redraw();
                } else {
                    let delta = if vk == VK_LEFT.0 || vk == VK_UP.0 { -1 } else { 1 };
                    if self.lib.go(&self.hub, delta) {
                        if photo::timing_enabled() {
                            self.switch_at = self.lib.current_photo().map(|p| (p.path.clone(), Instant::now()));
                        }
                        self.sync_view();
                        self.notice.clear();
                        self.redraw();
                    }
                }
                Action::None
            }
            x if x == VK_SPACE.0 => {
                match self.lib.copy_current(PathBuf::from(&self.output).as_path()) {
                    Ok(msg) => self.notice = msg.to_string(),
                    Err(err) => self.notice = err,
                }
                self.redraw();
                Action::None
            }
            x if x == VK_DELETE.0 => {
                match self.lib.delete_current(&self.hub, self.hwnd) {
                    Ok(()) => {
                        self.sync_view();
                        self.notice.clear();
                    }
                    Err(err) => self.notice = err,
                }
                self.redraw();
                Action::None
            }
            _ => Action::Pass,
        }
    }

    fn clamp_view(&mut self) {
        let Some(img) = self.lib.current_decoded() else {
            return;
        };
        let l = self.layout();
        let vw = (l.well_inner.right - l.well_inner.left).max(1) as f64;
        let vh = (l.well_inner.bottom - l.well_inner.top).max(1) as f64;
        let fit = self.fit_scale(&img, l.well_inner);
        let min = self.min_scale(&img, l.well_inner);
        if let View::Free { scale, .. } = self.view {
            // Back to fit height when at it, or when below it but the picture
            // is not wider than the window (so there is nothing to zoom out to).
            if scale <= fit + 0.0005 && (scale >= fit - 0.0005 || min >= fit - 0.0005) {
                self.view = View::FitHeight;
                return;
            }
        }
        if let View::Free { scale, ox, oy } = &mut self.view {
            *scale = scale.max(min);
            let dw = img.width as f64 * *scale;
            let dh = img.height as f64 * *scale;
            if dw <= vw {
                *ox = (vw - dw) / 2.0;
            } else {
                *ox = ox.clamp(vw - dw, 0.0);
            }
            if dh <= vh {
                *oy = (vh - dh) / 2.0;
            } else {
                *oy = oy.clamp(vh - dh, 0.0);
            }
        }
    }

    /// Smallest zoom: fit height, or fit width when the picture is wider than
    /// the window at fit height.
    fn min_scale(&self, img: &Decoded, well: RECT) -> f64 {
        let vw = (well.right - well.left).max(1) as f64;
        let fit_width = vw / img.width.max(1) as f64;
        self.fit_scale(img, well).min(fit_width)
    }

    /// True when the zoomed picture is bigger than the window in some
    /// direction, so dragging and the arrow keys move it around. When the whole
    /// picture is visible the arrow keys change picture instead.
    fn pannable(&self) -> bool {
        let View::Free { scale, .. } = self.view else {
            return false;
        };
        let Some(img) = self.lib.current_decoded() else {
            return false;
        };
        let l = self.layout();
        let vw = (l.well_inner.right - l.well_inner.left).max(1) as f64;
        let vh = (l.well_inner.bottom - l.well_inner.top).max(1) as f64;
        img.width as f64 * scale > vw + 0.5 || img.height as f64 * scale > vh + 0.5
    }

    fn dest_rect(&self, img: &Decoded, well: RECT) -> (i32, i32, i32, i32, f64) {
        let vw = (well.right - well.left).max(1) as f64;
        let vh = (well.bottom - well.top).max(1) as f64;
        let (scale, ox, oy) = match self.view {
            View::FitHeight => {
                let scale = self.fit_scale(img, well);
                let dw = img.width as f64 * scale;
                let dh = img.height as f64 * scale;
                (scale, (vw - dw) / 2.0, (vh - dh) / 2.0)
            }
            View::Free { scale, ox, oy } => (scale, ox, oy),
        };
        let dw = (img.width as f64 * scale).round() as i32;
        let dh = (img.height as f64 * scale).round() as i32;
        (
            well.left + ox.round() as i32,
            well.top + oy.round() as i32,
            dw.max(1),
            dh.max(1),
            scale,
        )
    }

    /// Tell the decoder the height of the monitor the window is on, so
    /// pictures are decoded to that screen's size. Called on resize, move,
    /// DPI change and display change; when the height changes, copies made
    /// for a shorter screen are decoded again.
    fn update_target(&mut self) {
        unsafe {
            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                let h = (mi.rcMonitor.bottom - mi.rcMonitor.top).max(0) as u32;
                if self.hub.set_target_height(h) && self.screen == Screen::View {
                    self.lib.kick(&self.hub);
                }
            }
        }
    }

    /// Zoomed in past the screen-size copy: ask for every pixel. The
    /// screen-size copy is shown stretched until they arrive.
    fn refine(&mut self) {
        if self.screen != Screen::View {
            return;
        }
        let Some(img) = self.lib.current_decoded() else {
            return;
        };
        if img.full {
            return;
        }
        let l = self.layout();
        let (_, _, dw, dh, _) = self.dest_rect(&img, l.well_inner);
        if dw as f64 > img.pix_w as f64 * 1.02 || dh as f64 > img.pix_h as f64 * 1.02 {
            self.lib.want_full(&self.hub);
        }
    }

    fn paint(&mut self) {
        self.refine();
        unsafe {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = BeginPaint(self.hwnd, &mut ps);
            let r = self.client();
            let w = r.right - r.left;
            let h = r.bottom - r.top;
            self.back.ensure(self.hwnd, w, h);
            if !self.back.dc.0.is_null() {
                self.draw(self.back.dc, r);
                let _ = BitBlt(hdc, 0, 0, w, h, self.back.dc, 0, 0, SRCCOPY);
            } else {
                self.draw(hdc, r);
            }
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn draw(&mut self, hdc: HDC, r: RECT) {
        let l = self.layout();
        unsafe {
            let _ = FillRect(hdc, &r, self.face);
        }
        luna::window_frame(hdc, r, self.active);
        luna::caption_fill(hdc, l.caption, self.active);
        let icon = scale(16, self.dpi);
        let iy = l.caption.top + (l.caption.bottom - l.caption.top - icon) / 2;
        unsafe {
            let _ = DrawIconEx(
                hdc,
                l.caption.left + scale(6, self.dpi),
                iy,
                self.icon,
                icon,
                icon,
                0,
                windows::Win32::Graphics::Gdi::HBRUSH(std::ptr::null_mut()),
                DI_NORMAL,
            );
        }
        let title = RECT {
            left: l.caption.left + scale(28, self.dpi),
            top: l.caption.top,
            right: l.btn_min.left - scale(8, self.dpi),
            bottom: l.caption.bottom,
        };
        luna::text_left(
            hdc,
            self.fonts.trebuchet,
            RECT { left: title.left + 1, top: title.top + 1, right: title.right, bottom: title.bottom },
            "Photo Viewer",
            (0, 32, 96),
        );
        luna::text_left(hdc, self.fonts.trebuchet, title, "Photo Viewer", luna::WHITE);
        let max_kind = if self.maximized() { CapBtn::Restore } else { CapBtn::Max };
        luna::caption_button(hdc, l.btn_min, CapBtn::Min, self.hot == Hot::Min, self.press == Hot::Min, self.active);
        luna::caption_button(hdc, l.btn_max, max_kind, self.hot == Hot::Max, self.press == Hot::Max, self.active);
        luna::caption_button(hdc, l.btn_close, CapBtn::Close, self.hot == Hot::Close, self.press == Hot::Close, self.active);

        if self.screen == Screen::Start {
            self.draw_start(hdc, &l);
            return;
        }

        luna::fill(hdc, l.name_bar, luna::FACE_HI);
        luna::hline(hdc, l.name_bar.left, l.name_bar.right, l.name_bar.bottom - 1, (199, 197, 178));
        let name = self.lib.current_photo().map(|p| p.name.as_str()).unwrap_or("");
        let count = if self.lib.items.is_empty() {
            String::new()
        } else {
            format!("{} of {}", self.lib.current + 1, self.lib.items.len())
        };
        luna::text_left(
            hdc,
            self.fonts.tahoma11b,
            RECT { left: l.name_bar.left + 4, top: l.name_bar.top, right: l.name_bar.right - scale(120, self.dpi), bottom: l.name_bar.bottom },
            name,
            luna::BLACK,
        );
        if self.lib.is_copied() {
            luna::text_right(
                hdc,
                self.fonts.tahoma8b,
                RECT { left: l.name_bar.left, top: l.name_bar.top, right: l.name_bar.right - scale(90, self.dpi), bottom: l.name_bar.bottom },
                "Copied",
                luna::GREEN,
            );
        }
        luna::text_right(
            hdc,
            self.fonts.tahoma9,
            RECT { left: l.name_bar.left, top: l.name_bar.top, right: l.name_bar.right - 4, bottom: l.name_bar.bottom },
            &count,
            luna::GROUP,
        );
        luna::command_button(
            hdc,
            l.folders,
            "Folders",
            self.fonts.tahoma8,
            self.hot == Hot::Folders,
            self.press == Hot::Folders,
            true,
        );

        luna::sunken_well(hdc, l.well);
        self.draw_image(hdc, l.well_inner);
        self.draw_exif(hdc, &l);

        luna::fill(hdc, l.status, luna::FACE);
        luna::hline(hdc, l.status.left, l.status.right, l.status.top, luna::STATUS_SEP);
        luna::hline(hdc, l.status.left, l.status.right, l.status.top + 1, luna::WHITE);
        let status = self.status_text();
        luna::text_left(
            hdc,
            self.fonts.tahoma8,
            RECT { left: l.status.left + 8, top: l.status.top, right: l.status.right - 24, bottom: l.status.bottom },
            &status,
            luna::BLACK,
        );
        luna::gripper(hdc, l.status);
    }

    fn draw_start(&self, hdc: HDC, l: &Layout) {
        let card = l.well;
        luna::fill(hdc, card, (244, 244, 236));
        luna::frame_rect(hdc, card, luna::EDIT_BORDER);
        luna::hline(hdc, card.left, card.right, card.top, (140, 170, 230));
        luna::text_left(hdc, self.fonts.tahoma11b, l.name_bar, "Choose folders", luna::GROUP);
        luna::text_left(hdc, self.fonts.tahoma8, l.input_label, "Input", luna::GROUP);
        luna::edit_box(hdc, l.input_box, &self.input, self.fonts.tahoma8, "Pictures to look through");
        luna::command_button(
            hdc,
            l.input_browse,
            "Browse...",
            self.fonts.tahoma8,
            self.hot == Hot::InputBrowse,
            self.press == Hot::InputBrowse,
            true,
        );
        luna::text_left(hdc, self.fonts.tahoma8, l.output_label, "Output", luna::GROUP);
        luna::edit_box(hdc, l.output_box, &self.output, self.fonts.tahoma8, "Where Space copies a picture");
        luna::command_button(
            hdc,
            l.output_browse,
            "Browse...",
            self.fonts.tahoma8,
            self.hot == Hot::OutputBrowse,
            self.press == Hot::OutputBrowse,
            true,
        );
        luna::command_button(
            hdc,
            l.open,
            "Open",
            self.fonts.tahoma8,
            self.hot == Hot::Open,
            self.press == Hot::Open,
            !self.input.is_empty(),
        );
        if !self.notice.is_empty() {
            let note = RECT {
                left: card.left + scale(16, self.dpi),
                top: l.open.top,
                right: l.open.left - scale(8, self.dpi),
                bottom: l.open.bottom,
            };
            luna::text_left(hdc, self.fonts.tahoma8, note, &self.notice, luna::CLOSE_RED);
        }
    }

    fn status_text(&self) -> String {
        let mut parts = Vec::new();
        if !self.notice.is_empty() {
            parts.push(self.notice.clone());
        }
        if let Some(img) = self.lib.current_decoded() {
            let l = self.layout();
            let (_, _, _, _, scale) = self.dest_rect(&img, l.well_inner);
            let mode = if !self.view.zoomed() && (scale - 1.0).abs() >= 0.01 {
                "Fit height".to_string()
            } else if (scale - 1.0).abs() < 0.01 {
                "100%".to_string()
            } else {
                format!("{}%", (scale * 100.0).round() as i32)
            };
            parts.push(mode);
            parts.push(format!("{} × {}", img.width, img.height));
        } else if self.lib.items.is_empty() {
            if self.input.is_empty() {
                parts.push("Choose an input folder.".into());
            } else {
                parts.push("No pictures in this folder.".into());
            }
        } else if self.lib.current_error().is_some() {
            parts.push(self.lib.current_error().unwrap().to_string());
        } else {
            parts.push("Loading…".into());
        }
        if !self.lib.items.is_empty() {
            parts.push(format!(
                "{} of {} in RAM · {}",
                self.lib.ram_count(),
                self.lib.items.len(),
                photo::format_bytes(self.lib.ram_bytes())
            ));
        }
        parts.join("  ·  ")
    }

    fn draw_image(&mut self, hdc: HDC, well: RECT) {
        if let Some(err) = self.lib.current_error() {
            luna::text_left(
                hdc,
                self.fonts.tahoma9,
                luna::inset(well, 12),
                err,
                luna::WHITE,
            );
            return;
        }
        let Some(img) = self.lib.current_decoded() else {
            let msg = if self.lib.items.is_empty() {
                if self.input.is_empty() {
                    "Choose an input folder."
                } else {
                    "No pictures in this folder."
                }
            } else {
                "Loading…"
            };
            luna::text_left(hdc, self.fonts.tahoma9, luna::inset(well, 12), msg, luna::WHITE);
            return;
        };
        let (dx, dy, dw, dh, _) = self.dest_rect(&img, well);
        let dest = RECT { left: dx, top: dy, right: dx + dw, bottom: dy + dh };
        let vis = RECT {
            left: dest.left.max(well.left),
            top: dest.top.max(well.top),
            right: dest.right.min(well.right),
            bottom: dest.bottom.min(well.bottom),
        };
        let (vw, vh) = (vis.right - vis.left, vis.bottom - vis.top);
        if vw > 0 && vh > 0 {
            // Scale once per picture/view into a cached bitmap; every other
            // repaint is a plain copy.
            let at = RECT { left: dx - vis.left, top: dy - vis.top, right: dw, bottom: dh };
            let cached = |k: &Option<(Arc<Decoded>, RECT)>| {
                k.as_ref().map(|(d, r)| Arc::ptr_eq(d, &img) && *r == at).unwrap_or(false)
            };
            if !cached(&self.pic_key) || self.pic.w != vw || self.pic.h != vh {
                self.pic.ensure(self.hwnd, vw, vh);
                if !self.pic.dc.0.is_null() {
                    stretch(self.pic.dc, &img, dx - vis.left, dy - vis.top, dw, dh);
                    self.pic_key = Some((Arc::clone(&img), at));
                }
            }
            unsafe {
                if cached(&self.pic_key) {
                    let _ = BitBlt(hdc, vis.left, vis.top, vw, vh, self.pic.dc, 0, 0, SRCCOPY);
                } else {
                    IntersectClipRect(hdc, well.left, well.top, well.right, well.bottom);
                    stretch(hdc, &img, dx, dy, dw, dh);
                    SelectClipRgn(hdc, HRGN(std::ptr::null_mut()));
                }
            }
        }
        if let Some((path, at)) = &self.switch_at {
            if self.lib.current_photo().map(|p| &p.path == path).unwrap_or(false) {
                photo::timing_log(&format!(
                    "switch {} on screen {:.0} ms after the key ({}x{} {})",
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    at.elapsed().as_secs_f64() * 1000.0,
                    img.pix_w,
                    img.pix_h,
                    if img.full { "full" } else { "screen copy" }
                ));
            }
            self.switch_at = None;
        }
    }

    fn draw_exif(&mut self, hdc: HDC, l: &Layout) {
        luna::fill(hdc, l.exif, (244, 244, 236));
        luna::frame_rect(hdc, l.exif, luna::EDIT_BORDER);
        luna::group_header(hdc, l.exif_header, "Properties", self.fonts.verdana);
        let props = self
            .lib
            .current_props();
        let label_h = scale(13, self.dpi);
        let value_h = scale(15, self.dpi);
        let block = label_h + value_h + scale(6, self.dpi);
        let content = (props.len() as i32) * block;
        let body_h = (l.exif_body.bottom - l.exif_body.top).max(1);
        let max_scroll = (content - body_h).max(0);
        self.exif_scroll = self.exif_scroll.clamp(0, max_scroll);
        unsafe {
            IntersectClipRect(hdc, l.exif_body.left, l.exif_body.top, l.exif_body.right, l.exif_body.bottom);
        }
        let pad = scale(6, self.dpi);
        for (i, (key, value)) in props.iter().enumerate() {
            let y = l.exif_body.top + scale(4, self.dpi) + i as i32 * block - self.exif_scroll;
            if y + block < l.exif_body.top || y > l.exif_body.bottom {
                continue;
            }
            let kr = RECT {
                left: l.exif_body.left + pad,
                top: y,
                right: l.exif_body.right - pad,
                bottom: y + label_h,
            };
            let vr = RECT {
                left: l.exif_body.left + pad,
                top: y + label_h,
                right: l.exif_body.right - pad,
                bottom: y + label_h + value_h,
            };
            luna::text_left(hdc, self.fonts.tahoma8, kr, key, luna::GROUP);
            luna::text_left(hdc, self.fonts.tahoma8, vr, value, luna::BLACK);
        }
        if props.is_empty() {
            luna::text_left(
                hdc,
                self.fonts.tahoma8,
                luna::inset(l.exif_body, 8),
                "No properties yet.",
                luna::DISABLED,
            );
        }
        unsafe {
            SelectClipRgn(hdc, HRGN(std::ptr::null_mut()));
        }
    }
}

/// Draw `img` scaled to `dw` × `dh` at (`x`, `y`). Exact 1:1 copies skip the
/// smoothing pass.
fn stretch(hdc: HDC, img: &Decoded, x: i32, y: i32, dw: i32, dh: i32) {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: img.pix_w as i32,
            biHeight: -(img.pix_h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let bits = img.bgra.as_ptr() as *const core::ffi::c_void;
        if dw == img.pix_w as i32 && dh == img.pix_h as i32 {
            SetDIBitsToDevice(hdc, x, y, dw as u32, dh as u32, 0, 0, 0, img.pix_h, bits, &info, DIB_RGB_COLORS);
        } else {
            SetStretchBltMode(hdc, HALFTONE);
            let _ = SetBrushOrgEx(hdc, 0, 0, None);
            StretchDIBits(
                hdc,
                x,
                y,
                dw,
                dh,
                0,
                0,
                img.pix_w as i32,
                img.pix_h as i32,
                Some(bits),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}
