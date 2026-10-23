//! A lightweight, DPI-aware settings surface. Native controls keep keyboard and UIA semantics.
use crate::{
    config::Provider,
    overlay::wide,
    ui_graphics::{PaintBuffer, VectorRenderer},
};
use std::cell::{Cell, RefCell};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::*,
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};

pub const SAVE: usize = 2101;
pub const OPEN_CONFIG: usize = 2102;
pub const EXPORT_DIAGNOSTICS: usize = 2103;
pub const ENABLED: i32 = 2201;
pub const NOTEPAD: i32 = 2202;
pub const NETWORK: i32 = 2203;
pub const MOCK: i32 = 2204;
pub const PUNCT: i32 = 2205;
pub const IDLE: i32 = 2206;
pub const DISPLAY_MS: i32 = 2207;
pub const MIN_HAN: i32 = 2208;
pub const PATHS: i32 = 2209;
pub const STATUS_LABEL: i32 = 2210;
pub const PROVIDER: i32 = 2211;
pub const MYMEMORY_EMAIL: i32 = 2212;
const FEEDBACK: i32 = 2250;
const FEEDBACK_DETAIL: i32 = 2251;
const NAV: i32 = 2300;
const ALL: usize = 4;
const BG: COLORREF = rgb(246, 248, 252);
const WHITE: COLORREF = rgb(255, 255, 255);
const INK: COLORREF = rgb(25, 39, 62);
const MUTED: COLORREF = rgb(91, 106, 128);
const BLUE: COLORREF = rgb(37, 99, 235);
const TINT: COLORREF = rgb(232, 240, 255);
const BORDER: COLORREF = rgb(220, 227, 237);
const NAV_SELECTED: COLORREF = rgb(239, 243, 248);
const NAV_HOVER: COLORREF = rgb(246, 248, 251);
const NAV_TEXT: COLORREF = rgb(73, 87, 107);
const fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | g << 8 | b << 16)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FeedbackKind {
    Saved,
    Pending,
    Error,
}
impl FeedbackKind {
    fn background(self) -> COLORREF {
        match self {
            Self::Saved => WHITE,
            Self::Pending => rgb(255, 247, 230),
            Self::Error => rgb(255, 241, 240),
        }
    }
    fn foreground(self) -> COLORREF {
        match self {
            Self::Saved => INK,
            Self::Pending => rgb(146, 64, 14),
            Self::Error => rgb(180, 35, 24),
        }
    }
    fn border(self) -> COLORREF {
        match self {
            Self::Saved => WHITE,
            Self::Pending => rgb(244, 212, 165),
            Self::Error => rgb(249, 200, 195),
        }
    }
}
struct Child {
    hwnd: HWND,
    rect: [i32; 4],
    page: usize,
    font: usize,
}
pub struct SettingsUi {
    hwnd: HWND,
    page: Cell<usize>,
    dpi: Cell<u32>,
    fonts: RefCell<Vec<HFONT>>,
    children: RefCell<Vec<Child>>,
    white: HBRUSH,
    background: HBRUSH,
    enabled: Cell<bool>,
    vectors: VectorRenderer,
    hovered: Cell<HWND>,
    keyboard_focus: Cell<bool>,
    feedback_kind: Cell<FeedbackKind>,
    pending_brush: HBRUSH,
    error_brush: HBRUSH,
}
impl Drop for SettingsUi {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(surface_proc), 1);
            for child in self.children.get_mut().iter() {
                let _ = RemoveWindowSubclass(child.hwnd, Some(button_proc), 1);
                let _ = RemoveWindowSubclass(child.hwnd, Some(combo_proc), 2);
                let _ = RemoveWindowSubclass(child.hwnd, Some(edit_proc), 3);
            }
            for font in self.fonts.get_mut().drain(..) {
                let _ = DeleteObject(font);
            }
            let _ = DeleteObject(self.white);
            let _ = DeleteObject(self.background);
            let _ = DeleteObject(self.pending_brush);
            let _ = DeleteObject(self.error_brush);
        }
    }
}
impl SettingsUi {
    /// # Safety
    /// The window must belong to the calling UI thread and outlive this object.
    pub unsafe fn install(hwnd: HWND) -> windows::core::Result<Box<Self>> {
        // Keep the accessible window title and standard caption interactions,
        // but suppress the duplicated visual icon/name in the non-client area.
        let options = WTA_OPTIONS {
            dwFlags: WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON,
            dwMask: WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON,
        };
        let _ = SetWindowThemeAttribute(
            hwnd,
            WTA_NONCLIENT,
            (&options as *const WTA_OPTIONS).cast(),
            std::mem::size_of::<WTA_OPTIONS>() as u32,
        );
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CAPTION_COLOR,
            (&BG as *const COLORREF).cast(),
            std::mem::size_of::<COLORREF>() as u32,
        );
        let vectors = VectorRenderer::new()?;
        let ui = Box::new(Self {
            hwnd,
            page: Cell::new(0),
            dpi: Cell::new(GetDpiForWindow(hwnd).max(96)),
            fonts: RefCell::new(vec![]),
            children: RefCell::new(vec![]),
            white: CreateSolidBrush(WHITE),
            background: CreateSolidBrush(BG),
            enabled: Cell::new(false),
            vectors,
            hovered: Cell::new(HWND::default()),
            keyboard_focus: Cell::new(false),
            feedback_kind: Cell::new(FeedbackKind::Saved),
            pending_brush: CreateSolidBrush(FeedbackKind::Pending.background()),
            error_brush: CreateSolidBrush(FeedbackKind::Error.background()),
        });
        ui.make_fonts();
        SetWindowSubclass(hwnd, Some(surface_proc), 1, (&*ui as *const Self) as usize).ok()?;
        for (i, label) in ["常规设置", "应用授权", "触发与显示", "运行诊断"]
            .iter()
            .enumerate()
        {
            ui.add(
                w!("BUTTON"),
                label,
                WS_TABSTOP,
                NAV + i as i32,
                [16, 154 + i as i32 * 50, 158, 42],
                ALL,
                0,
            )?;
        }
        ui.add(
            w!("BUTTON"),
            "启用自动翻译",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            ENABLED,
            [262, 148, 580, 42],
            0,
            0,
        )?;
        ui.add(
            w!("BUTTON"),
            "Mock 测试模式",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            MOCK,
            [262, 216, 580, 42],
            0,
            0,
        )?;
        ui.add(
            w!("COMBOBOX"),
            "",
            WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            PROVIDER,
            [266, 344, 568, 180],
            0,
            0,
        )?;
        if let Ok(combo) = GetDlgItem(hwnd, PROVIDER) {
            for provider in [Provider::Mymemory, Provider::Azure] {
                let s = wide(provider.name());
                SendMessageW(combo, CB_ADDSTRING, WPARAM(0), LPARAM(s.as_ptr() as isize));
            }
            SendMessageW(
                combo,
                CB_SETITEMHEIGHT,
                WPARAM(usize::MAX),
                LPARAM(ui.s(30) as isize),
            );
        }
        ui.add(
            w!("BUTTON"),
            "允许联网翻译",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            NETWORK,
            [262, 390, 580, 42],
            0,
            0,
        )?;
        ui.add(
            w!("STATIC"),
            "MyMemory 联系邮箱 · 选填",
            WINDOW_STYLE(0),
            0,
            [266, 464, 400, 22],
            0,
            1,
        )?;
        ui.edit(MYMEMORY_EMAIL, [266, 488, 568, 36], 0, false)?;
        ui.add(
            w!("BUTTON"),
            "授权微软记事本",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            NOTEPAD,
            [262, 148, 580, 42],
            1,
            0,
        )?;
        ui.edit(PATHS, [266, 310, 568, 138], 1, true)?;
        for (id, label, hint, x, y) in [
            (PUNCT, "句末停顿", "200–5000 毫秒", 266, 174),
            (IDLE, "无标点停顿", "500–15000 毫秒", 564, 174),
            (DISPLAY_MS, "提示显示时长", "500–30000 毫秒", 266, 302),
            (MIN_HAN, "最少汉字数", "1–300 个汉字", 564, 302),
        ] {
            ui.add(
                w!("STATIC"),
                label,
                WINDOW_STYLE(0),
                0,
                [x, y, 242, 24],
                2,
                0,
            )?;
            ui.edit(id, [x, y + 36, 242, 34], 2, false)?;
            ui.add(
                w!("STATIC"),
                hint,
                WINDOW_STYLE(0),
                0,
                [x, y + 80, 242, 22],
                2,
                1,
            )?;
        }
        ui.add(
            w!("EDIT"),
            "",
            WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
            STATUS_LABEL,
            [266, 178, 568, 292],
            3,
            0,
        )?;
        ui.add(
            w!("BUTTON"),
            "打开配置文件",
            WS_TABSTOP,
            OPEN_CONFIG as i32,
            [266, 510, 168, 38],
            3,
            0,
        )?;
        ui.add(
            w!("BUTTON"),
            "导出诊断",
            WS_TABSTOP,
            EXPORT_DIAGNOSTICS as i32,
            [450, 510, 144, 38],
            3,
            0,
        )?;
        ui.add(
            w!("STATIC"),
            "设置已保存",
            WINDOW_STYLE(0x0200), /* SS_CENTERIMAGE: vertically center static text. */
            FEEDBACK,
            [264, 590, 412, 25],
            ALL,
            3,
        )?;
        ui.add(
            w!("STATIC"),
            "所有修改已生效",
            WINDOW_STYLE(0x0200), /* SS_CENTERIMAGE: vertically center static text. */
            FEEDBACK_DETAIL,
            [264, 615, 412, 21],
            ALL,
            6,
        )?;
        ui.add(
            w!("BUTTON"),
            "保存并应用",
            WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            SAVE as i32,
            [718, 594, 164, 42],
            ALL,
            0,
        )?;
        ui.layout();
        ui.select_page(0, false);
        Ok(ui)
    }
    unsafe fn make_fonts(&self) {
        let mut fonts = self.fonts.borrow_mut();
        for f in fonts.drain(..) {
            let _ = DeleteObject(f);
        }
        for (size, weight, name) in [
            (15, 400, w!("Microsoft YaHei UI")),
            (12, 400, w!("Microsoft YaHei UI")),
            (28, 600, w!("Microsoft YaHei UI")),
            (17, 600, w!("Microsoft YaHei UI")),
            (13, 500, w!("Segoe UI")),
            (15, 600, w!("Microsoft YaHei UI")),
            (13, 400, w!("Microsoft YaHei UI")),
        ] {
            fonts.push(CreateFontW(
                -self.s(size),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32,
                name,
            ));
        }
    }
    fn s(&self, value: i32) -> i32 {
        (value * self.dpi.get() as i32 + 48) / 96
    }
    // Keep the Win32 creation parameters explicit beside each control's layout.
    #[allow(clippy::too_many_arguments)]
    unsafe fn add(
        &self,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        id: i32,
        rect: [i32; 4],
        page: usize,
        font: usize,
    ) -> windows::core::Result<()> {
        let txt = wide(text);
        let child = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(txt.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            self.hwnd,
            HMENU(id as *mut _),
            GetModuleHandleW(None)?,
            None,
        )?;
        if class == w!("BUTTON") {
            SetWindowSubclass(child, Some(button_proc), 1, (self as *const Self) as usize).ok()?;
        }
        if class == w!("COMBOBOX") {
            SetWindowSubclass(child, Some(combo_proc), 2, (self as *const Self) as usize).ok()?;
        }
        if class == w!("EDIT") {
            SetWindowSubclass(child, Some(edit_proc), 3, (self as *const Self) as usize).ok()?;
        }
        self.children.borrow_mut().push(Child {
            hwnd: child,
            rect,
            page,
            font,
        });
        Ok(())
    }
    unsafe fn edit(
        &self,
        id: i32,
        rect: [i32; 4],
        page: usize,
        multi: bool,
    ) -> windows::core::Result<()> {
        let style = if multi {
            WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32)
        } else if [PUNCT, IDLE, DISPLAY_MS, MIN_HAN].contains(&id) {
            WINDOW_STYLE(ES_NUMBER as u32)
        } else {
            WINDOW_STYLE(ES_AUTOHSCROLL as u32)
        };
        self.add(w!("EDIT"), "", WS_TABSTOP | style, id, rect, page, 0)?;
        if let Ok(edit) = GetDlgItem(self.hwnd, id) {
            SendMessageW(
                edit,
                EM_SETMARGINS,
                WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize),
                LPARAM((self.s(10) | (self.s(10) << 16)) as isize),
            );
        }
        Ok(())
    }
    unsafe fn layout(&self) {
        for c in self.children.borrow().iter() {
            let index = if GetDlgCtrlID(c.hwnd) == FEEDBACK
                && self.feedback_kind.get() == FeedbackKind::Error
            {
                0
            } else {
                c.font
            };
            let font = self.fonts.borrow()[index];
            SendMessageW(c.hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
            let (mut top, mut height) = (self.s(c.rect[1]), self.s(c.rect[3]));
            let id = GetDlgCtrlID(c.hwnd);
            if [MYMEMORY_EMAIL, PUNCT, IDLE, DISPLAY_MS, MIN_HAN].contains(&id) {
                let dc = GetDC(c.hwnd);
                let old = SelectObject(dc, font);
                let mut metrics = TEXTMETRICW::default();
                if GetTextMetricsW(dc, &mut metrics).as_bool() {
                    let content_height = (metrics.tmHeight + 2).min(height);
                    top += (height - content_height) / 2;
                    height = content_height;
                }
                SelectObject(dc, old);
                ReleaseDC(c.hwnd, dc);
                let margin = self.s(10);
                SendMessageW(
                    c.hwnd,
                    EM_SETMARGINS,
                    WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize),
                    LPARAM((margin | (margin << 16)) as isize),
                );
            }
            let _ = MoveWindow(
                c.hwnd,
                self.s(c.rect[0]),
                top,
                self.s(c.rect[2]),
                height,
                true,
            );
            if GetDlgCtrlID(c.hwnd) == PROVIDER {
                SendMessageW(
                    c.hwnd,
                    CB_SETITEMHEIGHT,
                    WPARAM(usize::MAX),
                    LPARAM(self.s(32) as isize),
                );
            }
        }
    }
    unsafe fn select_page(&self, page: usize, focus: bool) {
        let changed = self.page.replace(page) != page;
        let visible = IsWindowVisible(self.hwnd).as_bool();
        if visible && changed {
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(0), LPARAM(0));
        }
        for c in self.children.borrow().iter() {
            if c.page == ALL {
                continue;
            }
            let show = c.page == page;
            let currently_visible = GetWindowLongW(c.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
            if show != currently_visible {
                let _ = ShowWindow(c.hwnd, if show { SW_SHOW } else { SW_HIDE });
            }
        }
        if focus {
            if let Ok(button) = GetDlgItem(self.hwnd, NAV + page as i32) {
                let _ = SetFocus(button);
            }
        }
        if visible && changed {
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(1), LPARAM(0));
        }
        if changed {
            let _ = RedrawWindow(self.hwnd, None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
        }
    }
    /// # Safety
    /// Call from the UI thread before dialog keyboard handling.
    pub unsafe fn input_message(&self, message: u32, key: WPARAM) {
        let keyboard = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN
                if matches!(key.0 as u16, 0x09 | 0x0d | 0x20 | 0x23..=0x28 | 0x75) =>
            {
                true
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_POINTERDOWN => false,
            _ => return,
        };
        if self.keyboard_focus.replace(keyboard) != keyboard {
            let focus = GetFocus();
            if IsChild(self.hwnd, focus).as_bool() {
                let _ = InvalidateRect(focus, None, false);
            }
        }
    }
    unsafe fn paint_buffered(&self, target: HDC) {
        let mut rect = RECT::default();
        let _ = GetClientRect(self.hwnd, &mut rect);
        let buffer = PaintBuffer::new(target, rect.right, rect.bottom);
        self.paint(buffer.as_ref().map_or(target, PaintBuffer::dc));
        if let Some(buffer) = buffer {
            buffer.present();
        }
    }
    /// # Safety
    /// Call on the UI thread.
    pub unsafe fn applied(&self, enabled: bool) {
        self.enabled.set(enabled);
        self.feedback(FeedbackKind::Saved, "设置已保存", "所有修改已生效");
        let _ = InvalidateRect(self.hwnd, None, false);
    }
    /// # Safety
    /// Call on the UI thread. Validation errors remain readable until corrected.
    pub unsafe fn message(&self, message: &str) {
        self.feedback(FeedbackKind::Error, message, "请修正设置后重新保存");
    }
    unsafe fn feedback(&self, kind: FeedbackKind, title: &str, detail: &str) {
        let mut changed = self.feedback_kind.replace(kind) != kind;
        for (id, text) in [(FEEDBACK, title), (FEEDBACK_DETAIL, detail)] {
            if let Ok(child) = GetDlgItem(self.hwnd, id) {
                let t = wide(text);
                let mut current = vec![0u16; t.len() + 1];
                let n = GetWindowTextW(child, &mut current) as usize;
                if n != t.len() - 1 || current[..n] != t[..n] {
                    let _ = SetWindowTextW(child, PCWSTR(t.as_ptr()));
                    changed = true;
                }
                if id == FEEDBACK {
                    let font = self.fonts.borrow()[if kind == FeedbackKind::Error { 0 } else { 3 }];
                    if SendMessageW(child, WM_GETFONT, WPARAM(0), LPARAM(0)).0 != font.0 as isize {
                        SendMessageW(child, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                        changed = true;
                    }
                }
            }
        }
        if changed {
            let r = RECT {
                left: self.s(222),
                top: self.s(588),
                right: self.s(694),
                bottom: self.s(642),
            };
            let _ = InvalidateRect(self.hwnd, Some(&r), false);
            for id in [FEEDBACK, FEEDBACK_DETAIL] {
                if let Ok(h) = GetDlgItem(self.hwnd, id) {
                    let _ = InvalidateRect(h, None, false);
                }
            }
        }
    }
    unsafe fn text(&self, dc: HDC, text: &str, rect: [i32; 4], font: usize, color: COLORREF) {
        let r = RECT {
            left: self.s(rect[0]),
            top: self.s(rect[1]),
            right: self.s(rect[0] + rect[2]),
            bottom: self.s(rect[1] + rect[3]),
        };
        self.draw_text(dc, text, r, font, color, DT_LEFT);
    }
    unsafe fn draw_text(
        &self,
        dc: HDC,
        text: &str,
        mut rect: RECT,
        font: usize,
        color: COLORREF,
        align: DRAW_TEXT_FORMAT,
    ) {
        let old = SelectObject(dc, self.fonts.borrow()[font]);
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color);
        let mut t = wide(text);
        let len = t.len() - 1;
        DrawTextW(
            dc,
            &mut t[..len],
            &mut rect,
            align | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        SelectObject(dc, old);
    }
    unsafe fn box_at(&self, dc: HDC, r: [i32; 4], fill: COLORREF, border: COLORREF, diameter: i32) {
        let scale = self.dpi.get() as f32 / 96.0;
        self.vectors.rounded(
            dc,
            r.map(|v| v as f32 * scale),
            diameter as f32 * scale,
            Some(fill),
            if fill == border { None } else { Some(border) },
        );
    }
    unsafe fn field_box(&self, dc: HDC, id: i32, r: [i32; 4], fill: COLORREF) {
        let field = GetDlgItem(self.hwnd, id).ok();
        let focus = field.is_some_and(|h| GetFocus() == h);
        let disabled = field.is_some_and(|h| !IsWindowEnabled(h).as_bool());
        self.box_at(
            dc,
            r,
            if disabled { BG } else { fill },
            if focus { BLUE } else { BORDER },
            10,
        );
    }
    unsafe fn paint(&self, dc: HDC) {
        let mut client = RECT::default();
        let _ = GetClientRect(self.hwnd, &mut client);
        FillRect(dc, &client, self.background);
        let sidebar = RECT {
            left: 0,
            top: 0,
            right: self.s(190),
            bottom: client.bottom,
        };
        FillRect(dc, &sidebar, self.white);
        let scale = self.dpi.get() as f32 / 96.0;
        self.vectors
            // Align the bubble body (excluding its tail) with the name's center.
            .brand_mark(dc, [20.5 * scale, 29.0 * scale], 36.0 * scale, BLUE);
        self.text(dc, "英语伴学", [68, 34, 108, 24], 3, INK);
        self.text(dc, "Lite · 中文 → 英文", [24, 80, 148, 22], 1, MUTED);
        self.text(dc, "偏好设置", [24, 124, 140, 20], 1, MUTED);
        self.box_at(dc, [16, 438, 158, 110], BG, BG, 12);
        self.text(dc, "让每次表达", [30, 452, 130, 24], 0, INK);
        self.text(dc, "多一点英语。", [30, 478, 130, 24], 0, INK);
        self.text(dc, "原文不变 · 随写随学", [30, 516, 132, 20], 1, MUTED);
        self.text(
            dc,
            &format!("VERSION {}", env!("CARGO_PKG_VERSION")),
            [24, 602, 150, 22],
            4,
            MUTED,
        );
        let titles = [
            ("常规设置", "选择翻译方式，让伴学融入你的日常输入。"),
            ("应用授权", "由你决定，在哪些应用中开启英语伴学。"),
            ("触发与显示", "找到适合你的输入节奏和阅读时长。"),
            ("运行诊断", "查看当前状态，快速定位输入与翻译问题。"),
        ];
        let (title, desc) = titles[self.page.get()];
        self.text(dc, "WORKSPACE / PREFERENCES", [222, 22, 410, 20], 4, MUTED);
        self.text(dc, title, [222, 50, 450, 44], 2, INK);
        self.text(dc, desc, [224, 100, 630, 24], 0, MUTED);
        self.box_at(
            dc,
            [768, 30, 114, 30],
            if self.enabled.get() { TINT } else { WHITE },
            BORDER,
            16,
        );
        self.draw_text(
            dc,
            if self.enabled.get() {
                "运行中"
            } else {
                "已暂停"
            },
            RECT {
                left: self.s(768),
                top: self.s(30),
                right: self.s(882),
                bottom: self.s(60),
            },
            1,
            if self.enabled.get() { BLUE } else { MUTED },
            DT_CENTER,
        );
        match self.page.get() {
            0 => {
                self.box_at(dc, [222, 140, 660, 146], WHITE, BORDER, 16);
                self.text(
                    dc,
                    "仅监听已授权应用；新输入稳定后显示英语提示",
                    [266, 188, 566, 22],
                    1,
                    MUTED,
                );
                self.text(
                    dc,
                    "仅验证输入检测与提示显示，不请求真实翻译",
                    [266, 256, 566, 22],
                    1,
                    MUTED,
                );
                self.box_at(dc, [222, 304, 660, 248], WHITE, BORDER, 16);
                self.text(dc, "翻译服务", [266, 312, 566, 28], 3, INK);
                let azure = GetDlgItem(self.hwnd, PROVIDER)
                    .ok()
                    .is_some_and(|h| SendMessageW(h, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 == 1);
                self.text(
                    dc,
                    if azure {
                        "Azure Key / Region 从环境变量读取；此处不存储服务密钥。"
                    } else {
                        "MyMemory 为共享服务，请勿发送敏感内容；单句限 500 UTF-8 字节。"
                    },
                    [266, 436, 568, 22],
                    1,
                    MUTED,
                );
                self.field_box(dc, MYMEMORY_EMAIL, [264, 486, 572, 40], WHITE);
            }
            1 => {
                self.box_at(dc, [222, 140, 660, 100], WHITE, BORDER, 16);
                self.text(
                    dc,
                    "按系统路径或微软官方包身份验证",
                    [266, 194, 566, 22],
                    1,
                    MUTED,
                );
                self.box_at(dc, [222, 258, 660, 232], WHITE, BORDER, 16);
                self.text(dc, "其他应用", [266, 268, 566, 28], 3, INK);
                self.field_box(dc, PATHS, [264, 300, 572, 158], BG);
                self.text(
                    dc,
                    "每行填写一个 EXE 完整路径；保存前请确认该应用中的输入适合翻译。",
                    [266, 464, 568, 22],
                    1,
                    MUTED,
                );
                self.box_at(dc, [222, 508, 660, 44], TINT, TINT, 10);
                self.text(
                    dc,
                    "只在明确授权的编辑控件中工作，不修改或替换原文。",
                    [242, 518, 618, 24],
                    1,
                    BLUE,
                );
            }
            2 => {
                self.box_at(dc, [222, 140, 660, 302], WHITE, BORDER, 16);
                for (id, x, y) in [
                    (PUNCT, 264, 208),
                    (IDLE, 562, 208),
                    (DISPLAY_MS, 264, 336),
                    (MIN_HAN, 562, 336),
                ] {
                    self.field_box(dc, id, [x, y, 246, 38], BG);
                }
                self.box_at(dc, [222, 462, 660, 90], TINT, TINT, 12);
                self.text(dc, "按你的节奏出现", [244, 474, 610, 28], 3, BLUE);
                self.text(
                    dc,
                    "句号、问号、叹号使用句末停顿；其他输入使用无标点停顿。",
                    [244, 510, 610, 24],
                    1,
                    MUTED,
                );
            }
            _ => {
                self.box_at(dc, [222, 140, 660, 350], WHITE, BORDER, 16);
                self.text(dc, "实时状态与最近事件", [266, 146, 568, 28], 3, INK);
            }
        }
        // The fixed action bar applies changes on every page.
        let footer = RECT {
            left: self.s(190),
            top: self.s(578),
            right: client.right,
            bottom: client.bottom,
        };
        FillRect(dc, &footer, self.white);
        let kind = self.feedback_kind.get();
        self.box_at(
            dc,
            [222, 588, 472, 54],
            kind.background(),
            kind.border(),
            12,
        );
        let scale = self.dpi.get() as f32 / 96.0;
        if kind == FeedbackKind::Saved {
            use windows::Win32::Graphics::GdiPlus::PointF;
            self.vectors.chevron(
                dc,
                [
                    PointF {
                        X: 237.0 * scale,
                        Y: 615.0 * scale,
                    },
                    PointF {
                        X: 242.0 * scale,
                        Y: 620.0 * scale,
                    },
                    PointF {
                        X: 251.0 * scale,
                        Y: 610.0 * scale,
                    },
                ],
                2.0 * scale,
                rgb(22, 101, 52),
            );
        } else {
            self.vectors.rounded(
                dc,
                [234.0 * scale, 605.0 * scale, 20.0 * scale, 20.0 * scale],
                20.0 * scale,
                None,
                Some(kind.foreground()),
            );
            self.draw_text(
                dc,
                "!",
                RECT {
                    left: self.s(234),
                    top: self.s(604),
                    right: self.s(254),
                    bottom: self.s(626),
                },
                3,
                kind.foreground(),
                DT_CENTER,
            );
        }
    }
}
unsafe extern "system" fn surface_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let ui = &*(data as *const SettingsUi);
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            ui.paint_buffered(dc);
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        WM_PRINTCLIENT => {
            ui.paint_buffered(HDC(wp.0 as *mut _));
            return LRESULT(0);
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let dc = HDC(wp.0 as *mut _);
            let child = HWND(lp.0 as *mut _);
            let id = GetDlgCtrlID(child);
            let color = if [PATHS, PUNCT, IDLE, DISPLAY_MS, MIN_HAN].contains(&id) {
                BG
            } else {
                WHITE
            };
            if [FEEDBACK, FEEDBACK_DETAIL].contains(&id) {
                let kind = ui.feedback_kind.get();
                SetBkColor(dc, kind.background());
                SetTextColor(
                    dc,
                    if id == FEEDBACK_DETAIL && kind == FeedbackKind::Saved {
                        MUTED
                    } else {
                        kind.foreground()
                    },
                );
                return LRESULT(
                    match kind {
                        FeedbackKind::Saved => ui.white,
                        FeedbackKind::Pending => ui.pending_brush,
                        FeedbackKind::Error => ui.error_brush,
                    }
                    .0 as isize,
                );
            }
            let secondary = (msg == WM_CTLCOLORSTATIC
                && ui
                    .children
                    .borrow()
                    .iter()
                    .any(|c| c.hwnd == child && c.font == 1))
                || !IsWindowEnabled(child).as_bool();
            SetTextColor(dc, if secondary { MUTED } else { INK });
            SetBkColor(dc, color);
            return LRESULT(if color == BG {
                ui.background.0
            } else {
                ui.white.0
            } as isize);
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xffff) as i32;
            let code = ((wp.0 >> 16) & 0xffff) as u32;
            if (NAV..NAV + 4).contains(&id) {
                ui.select_page((id - NAV) as usize, true);
                return LRESULT(0);
            }
            if (code == EN_CHANGE || code == BN_CLICKED || code == CBN_SELCHANGE)
                && (ENABLED..=MYMEMORY_EMAIL).contains(&id)
            {
                ui.feedback(
                    FeedbackKind::Pending,
                    "有未保存的修改",
                    "点击右侧「保存并应用」后生效",
                );
            }
        }
        WM_DPICHANGED => {
            ui.dpi.set((wp.0 & 0xffff) as u32);
            ui.make_fonts();
            ui.layout();
            let r = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(hwnd, None, true);
            return LRESULT(0);
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(surface_proc), 1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let ui = &*(data as *const SettingsUi);
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = if msg == WM_PAINT {
                BeginPaint(hwnd, &mut ps)
            } else {
                HDC(wp.0 as *mut _)
            };
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            let buffer = PaintBuffer::new(dc, r.right, r.bottom);
            let dc = buffer.as_ref().map_or(dc, PaintBuffer::dc);
            let id = GetDlgCtrlID(hwnd);
            let nav = (NAV..NAV + 4).contains(&id);
            let toggle = [ENABLED, MOCK, NETWORK, NOTEPAD].contains(&id);
            let active = nav && ui.page.get() == (id - NAV) as usize;
            let state = SendMessageW(hwnd, BM_GETSTATE, WPARAM(0), LPARAM(0)).0 as u32;
            let pressed = state & BST_PUSHED != 0;
            let hover = ui.hovered.get() == hwnd;
            let keyboard_focus = GetFocus() == hwnd && ui.keyboard_focus.get();
            let fill = if nav {
                if pressed {
                    rgb(230, 236, 244)
                } else if active {
                    NAV_SELECTED
                } else if hover || keyboard_focus {
                    NAV_HOVER
                } else {
                    WHITE
                }
            } else if id == SAVE as i32 {
                if pressed {
                    rgb(29, 78, 216)
                } else if hover {
                    rgb(49, 112, 245)
                } else {
                    BLUE
                }
            } else if active || pressed || (hover && !toggle) {
                TINT
            } else {
                WHITE
            };
            FillRect(dc, &r, ui.white);
            let width = r.right * 96 / ui.dpi.get() as i32;
            let height = r.bottom * 96 / ui.dpi.get() as i32;
            if !toggle {
                ui.vectors.rounded(
                    dc,
                    [0.0, 0.0, r.right as f32, r.bottom as f32],
                    10.0 * ui.dpi.get() as f32 / 96.0,
                    Some(fill),
                    if nav || id == SAVE as i32 {
                        None
                    } else {
                        Some(BORDER)
                    },
                );
            }
            let mut t = vec![0u16; 512];
            let n = GetWindowTextW(hwnd, &mut t) as usize;
            let text = String::from_utf16_lossy(&t[..n]);
            let color = if id == SAVE as i32 {
                WHITE
            } else if nav && !active {
                NAV_TEXT
            } else {
                INK
            };
            if active {
                let scale = ui.dpi.get() as f32 / 96.0;
                ui.vectors.rounded(
                    dc,
                    [
                        8.0 * scale,
                        (r.bottom as f32 - 14.0 * scale) / 2.0,
                        3.0 * scale,
                        14.0 * scale,
                    ],
                    3.0 * scale,
                    Some(BLUE),
                    None,
                );
            }
            if nav || toggle {
                ui.text(
                    dc,
                    &text,
                    [
                        if nav { 18 } else { 4 },
                        0,
                        if toggle { width - 72 } else { width - 24 },
                        height,
                    ],
                    if active { 5 } else { 0 },
                    color,
                );
            } else {
                let mut text_rect = r;
                text_rect.left += ui.s(8);
                text_rect.right -= ui.s(8);
                ui.draw_text(dc, &text, text_rect, 0, color, DT_CENTER);
            }
            if toggle {
                let on = SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0
                    == BST_CHECKED.0 as isize;
                let scale = ui.dpi.get() as f32 / 96.0;
                let x = r.right as f32 - 48.0 * scale;
                let y = (r.bottom as f32 - 24.0 * scale) / 2.0;
                let track = if on {
                    if hover || pressed {
                        rgb(29, 78, 216)
                    } else {
                        BLUE
                    }
                } else if hover || pressed {
                    rgb(113, 130, 154)
                } else {
                    rgb(146, 158, 178)
                };
                ui.vectors.rounded(
                    dc,
                    [x, y, 44.0 * scale, 24.0 * scale],
                    24.0 * scale,
                    Some(track),
                    None,
                );
                ui.vectors.rounded(
                    dc,
                    [
                        x + (if on { 23.0 } else { 3.0 }) * scale,
                        y + 3.0 * scale,
                        18.0 * scale,
                        18.0 * scale,
                    ],
                    18.0 * scale,
                    Some(WHITE),
                    None,
                );
            }
            if keyboard_focus {
                if nav {
                    let scale = ui.dpi.get() as f32 / 96.0;
                    ui.vectors.rounded(
                        dc,
                        [
                            r.right as f32 - 18.0 * scale,
                            r.bottom as f32 / 2.0 - 3.0 * scale,
                            6.0 * scale,
                            6.0 * scale,
                        ],
                        6.0 * scale,
                        Some(NAV_TEXT),
                        None,
                    );
                } else {
                    if toggle {
                        r.left = r.right - ui.s(52);
                        r.top = (r.bottom - ui.s(32)) / 2;
                        r.bottom = r.top + ui.s(32);
                    } else {
                        r.left += ui.s(3);
                        r.top += ui.s(3);
                        r.right -= ui.s(3);
                        r.bottom -= ui.s(3);
                    }
                    ui.vectors.rounded(
                        dc,
                        [
                            r.left as f32,
                            r.top as f32,
                            (r.right - r.left) as f32,
                            (r.bottom - r.top) as f32,
                        ],
                        ui.s(6) as f32,
                        None,
                        Some(if id == SAVE as i32 { WHITE } else { BLUE }),
                    );
                }
            }
            if let Some(buffer) = buffer {
                buffer.present();
            }
            if msg == WM_PAINT {
                let _ = EndPaint(hwnd, &ps);
            }
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_MOUSEMOVE => {
            let previous = ui.hovered.replace(hwnd);
            if previous != hwnd {
                if previous != HWND::default() {
                    let _ = InvalidateRect(previous, None, false);
                }
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    ..Default::default()
                };
                let _ = TrackMouseEvent(&mut track);
                let _ = InvalidateRect(hwnd, None, false);
            }
            // Native drag cancellation still runs while the button owns capture.
            if GetCapture() == hwnd {
                return native_state(hwnd, msg, wp, lp);
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            if ui.hovered.get() == hwnd {
                ui.hovered.set(HWND::default());
                let _ = InvalidateRect(hwnd, None, false);
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            ui.input_message(WM_LBUTTONDOWN, WPARAM(0));
            return DefSubclassProc(hwnd, msg, wp, lp);
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            ui.input_message(msg, wp);
            return DefSubclassProc(hwnd, msg, wp, lp);
        }
        BM_SETSTATE | BM_SETCHECK | WM_SETFOCUS | WM_KILLFOCUS | WM_UPDATEUISTATE | WM_ENABLE
        | WM_CAPTURECHANGED | WM_CANCELMODE => {
            let result = native_state(hwnd, msg, wp, lp);
            let _ = InvalidateRect(hwnd, None, false);
            return result;
        }
        WM_LBUTTONUP => {
            let result = DefSubclassProc(hwnd, msg, wp, lp);
            let _ = InvalidateRect(hwnd, None, false);
            return result;
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(button_proc), 1);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let ui = &*(data as *const SettingsUi);
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = if msg == WM_PAINT {
                BeginPaint(hwnd, &mut ps)
            } else {
                HDC(wp.0 as *mut _)
            };
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            let buffer = PaintBuffer::new(dc, r.right, r.bottom);
            let dc = buffer.as_ref().map_or(dc, PaintBuffer::dc);
            let width = r.right * 96 / ui.dpi.get() as i32;
            let height = r.bottom * 96 / ui.dpi.get() as i32;
            let focus = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd;
            FillRect(dc, &r, ui.white);
            ui.vectors.rounded(
                dc,
                [0.0, 0.0, r.right as f32, r.bottom as f32],
                8.0 * ui.dpi.get() as f32 / 96.0,
                Some(WHITE),
                Some(if focus { BLUE } else { BORDER }),
            );
            let mut text = vec![0u16; 256];
            let n = GetWindowTextW(hwnd, &mut text) as usize;
            ui.text(
                dc,
                &String::from_utf16_lossy(&text[..n]),
                [12, 0, width - 48, height],
                0,
                INK,
            );
            let scale = ui.dpi.get() as f32 / 96.0;
            let x = r.right as f32 - 23.0 * scale;
            let y = r.bottom as f32 / 2.0;
            ui.vectors.chevron(
                dc,
                [
                    windows::Win32::Graphics::GdiPlus::PointF {
                        X: x - 4.0 * scale,
                        Y: y - 2.0 * scale,
                    },
                    windows::Win32::Graphics::GdiPlus::PointF {
                        X: x,
                        Y: y + 2.0 * scale,
                    },
                    windows::Win32::Graphics::GdiPlus::PointF {
                        X: x + 4.0 * scale,
                        Y: y - 2.0 * scale,
                    },
                ],
                1.5 * scale,
                MUTED,
            );
            if let Some(buffer) = buffer {
                buffer.present();
            }
            if msg == WM_PAINT {
                let _ = EndPaint(hwnd, &ps);
            }
            return LRESULT(0);
        }
        WM_SETFOCUS | WM_KILLFOCUS | CB_SETCURSEL => {
            let r = DefSubclassProc(hwnd, msg, wp, lp);
            let _ = InvalidateRect(hwnd, None, false);
            return r;
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(combo_proc), 2);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let ui = &*(data as *const SettingsUi);
    match msg {
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            let result = DefSubclassProc(hwnd, msg, wp, lp);
            let _ = InvalidateRect(ui.hwnd, None, false);
            return result;
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(edit_proc), 3);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe fn native_state(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let visible = GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
    if visible {
        DefSubclassProc(hwnd, WM_SETREDRAW, WPARAM(0), LPARAM(0));
    }
    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if visible {
        DefSubclassProc(hwnd, WM_SETREDRAW, WPARAM(1), LPARAM(0));
    }
    result
}
