//! Windows XP Luna look, from Microsoft's Windows XP Visual Guidelines (August 2001).
//! Fonts: Tahoma 8/9/11, Trebuchet MS Bold 10 for title bars, Verdana Bold 8 for task boxes.
//! Command buttons 75×23 with a 1px indent curve. Group-box titles R0 G70 B213.

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect,
    GetTextExtentPoint32W, LineTo, MoveToEx, SelectObject, SetBkMode, SetTextColor, DEFAULT_QUALITY,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HBRUSH,
    HDC, HFONT, PS_SOLID, TRANSPARENT,
};

pub const FACE: (u8, u8, u8) = (230, 234, 216); // #E6EAD8
pub const FACE_HI: (u8, u8, u8) = (236, 233, 216);
pub const WHITE: (u8, u8, u8) = (255, 255, 255);
pub const BLACK: (u8, u8, u8) = (0, 0, 0);
pub const GROUP: (u8, u8, u8) = (0, 70, 213);
pub const DISABLED: (u8, u8, u8) = (161, 161, 146);
pub const EDIT_BORDER: (u8, u8, u8) = (127, 157, 185);
pub const WELL: (u8, u8, u8) = (64, 64, 64);
pub const WELL_TOP: (u8, u8, u8) = (128, 128, 128);
pub const STATUS_SEP: (u8, u8, u8) = (199, 197, 178);
pub const GRIP: (u8, u8, u8) = (184, 180, 163);
pub const FRAME: (u8, u8, u8) = (0, 82, 217);
pub const FRAME_DARK: (u8, u8, u8) = (0, 48, 160);
pub const BTN_BORDER: (u8, u8, u8) = (0, 60, 165);
pub const HOVER_ORANGE: (u8, u8, u8) = (255, 204, 0);
pub const HOVER_ORANGE2: (u8, u8, u8) = (230, 139, 44);
pub const GREEN: (u8, u8, u8) = (19, 146, 13);
pub const CLOSE_RED: (u8, u8, u8) = (227, 92, 47);
#[allow(dead_code)]
pub const TASK_BLUE: (u8, u8, u8) = (1, 72, 178);

pub fn rgb(c: (u8, u8, u8)) -> COLORREF {
    COLORREF(c.0 as u32 | ((c.1 as u32) << 8) | ((c.2 as u32) << 16))
}

pub fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t).round() as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t).round() as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t).round() as u8,
    )
}

pub fn fill(hdc: HDC, r: RECT, c: (u8, u8, u8)) {
    unsafe {
        let br = CreateSolidBrush(rgb(c));
        let _ = FillRect(hdc, &r, br);
        let _ = DeleteObject(br);
    }
}

pub fn vline(hdc: HDC, x: i32, y0: i32, y1: i32, c: (u8, u8, u8)) {
    fill(hdc, RECT { left: x, top: y0, right: x + 1, bottom: y1 }, c);
}

pub fn hline(hdc: HDC, x0: i32, x1: i32, y: i32, c: (u8, u8, u8)) {
    fill(hdc, RECT { left: x0, top: y, right: x1, bottom: y + 1 }, c);
}

pub fn frame_rect(hdc: HDC, r: RECT, c: (u8, u8, u8)) {
    hline(hdc, r.left, r.right, r.top, c);
    hline(hdc, r.left, r.right, r.bottom - 1, c);
    vline(hdc, r.left, r.top, r.bottom, c);
    vline(hdc, r.right - 1, r.top, r.bottom, c);
}

pub fn inset(r: RECT, n: i32) -> RECT {
    RECT {
        left: r.left + n,
        top: r.top + n,
        right: r.right - n,
        bottom: r.bottom - n,
    }
}

pub fn hit(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn make_font(face: windows::core::PCWSTR, pt: i32, dpi: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -((pt * dpi + 36) / 72),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            DEFAULT_QUALITY.0 as u32,
            0,
            face,
        )
    }
}

pub struct Fonts {
    pub tahoma8: HFONT,
    pub tahoma8b: HFONT,
    pub tahoma9: HFONT,
    pub tahoma11: HFONT,
    pub tahoma11b: HFONT,
    pub trebuchet: HFONT,
    pub verdana: HFONT,
}

impl Fonts {
    pub fn new(dpi: i32) -> Self {
        Self {
            tahoma8: make_font(w!("Tahoma"), 8, dpi, 400),
            tahoma8b: make_font(w!("Tahoma"), 8, dpi, 700),
            tahoma9: make_font(w!("Tahoma"), 9, dpi, 400),
            tahoma11: make_font(w!("Tahoma"), 11, dpi, 400),
            tahoma11b: make_font(w!("Tahoma"), 11, dpi, 700),
            trebuchet: make_font(w!("Trebuchet MS"), 10, dpi, 700),
            verdana: make_font(w!("Verdana"), 8, dpi, 700),
        }
    }

    pub fn drop_all(&self) {
        unsafe {
            let _ = DeleteObject(self.tahoma8);
            let _ = DeleteObject(self.tahoma8b);
            let _ = DeleteObject(self.tahoma9);
            let _ = DeleteObject(self.tahoma11);
            let _ = DeleteObject(self.tahoma11b);
            let _ = DeleteObject(self.trebuchet);
            let _ = DeleteObject(self.verdana);
        }
    }
}

pub fn text(hdc: HDC, font: HFONT, r: RECT, s: &str, color: (u8, u8, u8), flags: u32) {
    if s.is_empty() {
        return;
    }
    let mut w: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, rgb(color));
        let _ = DrawTextW(hdc, &mut w, &mut { r }, windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags));
        SelectObject(hdc, old);
    }
}

pub fn text_left(hdc: HDC, font: HFONT, r: RECT, s: &str, color: (u8, u8, u8)) {
    text(
        hdc,
        font,
        r,
        s,
        color,
        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0 | DT_END_ELLIPSIS.0,
    );
}

pub fn text_right(hdc: HDC, font: HFONT, r: RECT, s: &str, color: (u8, u8, u8)) {
    text(
        hdc,
        font,
        r,
        s,
        color,
        DT_RIGHT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0 | DT_END_ELLIPSIS.0,
    );
}

#[allow(dead_code)]
pub fn measure(hdc: HDC, font: HFONT, s: &str) -> i32 {
    let w = wide(s);
    let mut sz = windows::Win32::Foundation::SIZE::default();
    unsafe {
        let old = SelectObject(hdc, font);
        let _ = GetTextExtentPoint32W(hdc, &w[..w.len().saturating_sub(1)], &mut sz);
        SelectObject(hdc, old);
    }
    sz.cx
}

/// Luna active caption gradient stops from the published Windows XP style.
const CAP_ON: [(f32, (u8, u8, u8)); 9] = [
    (0.00, (0x00, 0x58, 0xee)),
    (0.06, (0x28, 0x8e, 0xff)),
    (0.14, (0x02, 0x62, 0xee)),
    (0.24, (0x00, 0x54, 0xe3)),
    (0.56, (0x00, 0x55, 0xeb)),
    (0.76, (0x02, 0x6a, 0xfe)),
    (0.86, (0x00, 0x62, 0xef)),
    (0.94, (0x00, 0x40, 0xab)),
    (1.00, (0x00, 0x30, 0x92)),
];

const CAP_OFF: [(f32, (u8, u8, u8)); 6] = [
    (0.00, (0x76, 0x97, 0xe7)),
    (0.08, (0x97, 0xb4, 0xe9)),
    (0.25, (0x79, 0x96, 0xde)),
    (0.56, (0x7b, 0x99, 0xe1)),
    (0.89, (0x80, 0xa5, 0xe7)),
    (1.00, (0xab, 0xba, 0xe3)),
];

fn sample(stops: &[(f32, (u8, u8, u8))], t: f32) -> (u8, u8, u8) {
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        if t <= w[1].0 {
            let span = (w[1].0 - w[0].0).max(0.0001);
            return lerp(w[0].1, w[1].1, (t - w[0].0) / span);
        }
    }
    stops[stops.len() - 1].1
}

pub fn caption_fill(hdc: HDC, r: RECT, active: bool) {
    let h = (r.bottom - r.top).max(1);
    let stops = if active { CAP_ON.as_slice() } else { CAP_OFF.as_slice() };
    for y in r.top..r.bottom {
        let t = (y - r.top) as f32 / h as f32;
        hline(hdc, r.left, r.right, y, sample(stops, t));
    }
}

pub fn vgrad(hdc: HDC, r: RECT, top: (u8, u8, u8), bot: (u8, u8, u8)) {
    let h = (r.bottom - r.top).max(1);
    for y in r.top..r.bottom {
        let t = (y - r.top) as f32 / h as f32;
        hline(hdc, r.left, r.right, y, lerp(top, bot, t));
    }
}

/// Command button: 75×23 typical, 1px indent curve, orange hover (Visual Guidelines).
pub fn command_button(hdc: HDC, r: RECT, label: &str, font: HFONT, hover: bool, press: bool, enabled: bool) {
    let fill_top = if press { (200, 198, 186) } else { (255, 255, 255) };
    let fill_bot = if press { (220, 218, 204) } else { (236, 235, 229) };
    vgrad(hdc, r, fill_top, fill_bot);

    let border = if !enabled {
        DISABLED
    } else if hover || press {
        HOVER_ORANGE2
    } else {
        BTN_BORDER
    };
    frame_rect(hdc, r, border);
    if hover && enabled {
        frame_rect(hdc, inset(r, 1), HOVER_ORANGE);
    } else {
        hline(hdc, r.left + 1, r.right - 1, r.top + 1, WHITE);
        vline(hdc, r.left + 1, r.top + 1, r.bottom - 1, WHITE);
    }

    // 1px indent curve
    fill(hdc, RECT { left: r.left, top: r.top, right: r.left + 1, bottom: r.top + 1 }, FACE);
    fill(hdc, RECT { left: r.right - 1, top: r.top, right: r.right, bottom: r.top + 1 }, FACE);
    fill(hdc, RECT { left: r.left, top: r.bottom - 1, right: r.left + 1, bottom: r.bottom }, FACE);
    fill(hdc, RECT { left: r.right - 1, top: r.bottom - 1, right: r.right, bottom: r.bottom }, FACE);

    let mut tr = inset(r, 1);
    if press {
        tr.left += 1;
        tr.top += 1;
    }
    let color = if enabled { BLACK } else { DISABLED };
    text(
        hdc,
        font,
        tr,
        label,
        color,
        DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_NOPREFIX.0,
    );
}

pub fn edit_box(hdc: HDC, r: RECT, value: &str, font: HFONT, empty: &str) {
    fill(hdc, r, WHITE);
    frame_rect(hdc, r, EDIT_BORDER);
    let inner = RECT {
        left: r.left + 4,
        top: r.top,
        right: r.right - 4,
        bottom: r.bottom,
    };
    if value.is_empty() {
        text_left(hdc, font, inner, empty, DISABLED);
    } else {
        text_left(hdc, font, inner, value, BLACK);
    }
}

pub fn sunken_well(hdc: HDC, r: RECT) {
    fill(hdc, r, WELL);
    hline(hdc, r.left, r.right, r.top, WELL_TOP);
    vline(hdc, r.left, r.top, r.bottom, WELL_TOP);
    hline(hdc, r.left, r.right, r.bottom - 1, WHITE);
    vline(hdc, r.right - 1, r.top, r.bottom, WHITE);
}

pub fn group_header(hdc: HDC, r: RECT, title: &str, font: HFONT) {
    vgrad(hdc, r, (40, 91, 197), (1, 72, 178));
    hline(hdc, r.left, r.right, r.top, (140, 170, 230));
    text_left(
        hdc,
        font,
        RECT { left: r.left + 8, top: r.top, right: r.right - 8, bottom: r.bottom },
        title,
        WHITE,
    );
}

pub fn caption_button(hdc: HDC, r: RECT, kind: CapBtn, hover: bool, press: bool, active: bool) {
    let (top, bot) = match kind {
        CapBtn::Close => {
            if press {
                ((180, 40, 20), (140, 20, 10))
            } else if hover {
                ((255, 120, 80), (200, 50, 30))
            } else {
                ((238, 110, 80), (185, 50, 30))
            }
        }
        _ => {
            if press {
                ((20, 60, 170), (10, 40, 130))
            } else if hover {
                ((120, 180, 255), (40, 90, 210))
            } else if active {
                ((90, 150, 245), (20, 70, 190))
            } else {
                ((160, 180, 220), (100, 130, 190))
            }
        }
    };
    vgrad(hdc, r, top, bot);
    frame_rect(hdc, r, WHITE);
    frame_rect(hdc, inset(r, 1), (0, 40, 120));
    fill(hdc, RECT { left: r.left, top: r.top, right: r.left + 1, bottom: r.top + 1 }, FACE);
    fill(hdc, RECT { left: r.right - 1, top: r.top, right: r.right, bottom: r.top + 1 }, FACE);
    fill(hdc, RECT { left: r.left, top: r.bottom - 1, right: r.left + 1, bottom: r.bottom }, FACE);
    fill(hdc, RECT { left: r.right - 1, top: r.bottom - 1, right: r.right, bottom: r.bottom }, FACE);

    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    let ink = WHITE;
    match kind {
        CapBtn::Min => {
            hline(hdc, cx - 4, cx + 5, cy + 3, ink);
            hline(hdc, cx - 4, cx + 5, cy + 4, ink);
        }
        CapBtn::Max => {
            frame_rect(hdc, RECT { left: cx - 4, top: cy - 4, right: cx + 5, bottom: cy + 4 }, ink);
            hline(hdc, cx - 4, cx + 5, cy - 3, ink);
        }
        CapBtn::Restore => {
            frame_rect(hdc, RECT { left: cx - 2, top: cy - 5, right: cx + 5, bottom: cy + 1 }, ink);
            frame_rect(hdc, RECT { left: cx - 5, top: cy - 2, right: cx + 2, bottom: cy + 5 }, ink);
            fill(hdc, RECT { left: cx - 4, top: cy - 1, right: cx + 1, bottom: cy + 4 }, top);
        }
        CapBtn::Close => {
            for i in -3..=3 {
                fill(hdc, RECT { left: cx + i, top: cy + i, right: cx + i + 1, bottom: cy + i + 1 }, ink);
                fill(hdc, RECT { left: cx + i, top: cy - i, right: cx + i + 1, bottom: cy - i + 1 }, ink);
                fill(hdc, RECT { left: cx + i - 1, top: cy + i, right: cx + i, bottom: cy + i + 1 }, ink);
                fill(hdc, RECT { left: cx + i - 1, top: cy - i, right: cx + i, bottom: cy - i + 1 }, ink);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CapBtn {
    Min,
    Max,
    Restore,
    Close,
}

pub fn window_frame(hdc: HDC, r: RECT, active: bool) {
    let c = if active { FRAME } else { (120, 150, 200) };
    let d = if active { FRAME_DARK } else { (90, 110, 160) };
    frame_rect(hdc, r, d);
    frame_rect(hdc, inset(r, 1), c);
    frame_rect(hdc, inset(r, 2), if active { (80, 140, 230) } else { (160, 180, 210) });
}

pub fn gripper(hdc: HDC, r: RECT) {
    let x = r.right - 12;
    let y = r.bottom - 12;
    for row in 0..3 {
        for col in 0..3 {
            if col + row < 2 {
                continue;
            }
            let px = x + col * 3;
            let py = y + row * 3;
            fill(hdc, RECT { left: px + 1, top: py + 1, right: px + 3, bottom: py + 3 }, WHITE);
            fill(hdc, RECT { left: px, top: py, right: px + 2, bottom: py + 2 }, GRIP);
        }
    }
}

#[allow(dead_code)]
pub fn status_sep(hdc: HDC, x: i32, y0: i32, y1: i32) {
    vline(hdc, x, y0, y1, STATUS_SEP);
    vline(hdc, x + 1, y0, y1, WHITE);
}

#[allow(dead_code)]
pub fn line(hdc: HDC, x0: i32, y0: i32, x1: i32, y1: i32, c: (u8, u8, u8)) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, rgb(c));
        let old = SelectObject(hdc, pen);
        let _ = MoveToEx(hdc, x0, y0, None);
        let _ = LineTo(hdc, x1, y1);
        SelectObject(hdc, old);
        let _ = DeleteObject(pen);
    }
}

pub fn null_hwnd() -> HWND {
    HWND(std::ptr::null_mut())
}

#[allow(dead_code)]
pub fn null_brush() -> HBRUSH {
    HBRUSH(std::ptr::null_mut())
}
