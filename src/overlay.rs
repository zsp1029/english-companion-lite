use std::{
    cell::RefCell,
    time::{Duration, Instant},
};
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
const BODY: usize = 3101;
const COPY: usize = 3102;
const CLOSE: usize = 3103;
const BACKGROUND: COLORREF = COLORREF(0x00F5FAFC);
const FOREGROUND: COLORREF = COLORREF(0x00392A20);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Content {
    edit: HWND,
    copy: HWND,
    close: HWND,
    font: HFONT,
    brush: HBRUSH,
    padding: i32,
    remaining: Duration,
    last_tick: Instant,
}
impl Drop for Content {
    fn drop(&mut self) {
        unsafe {
            if self.font != HFONT::default() {
                let _ = DeleteObject(self.font);
            }
            let _ = DeleteObject(self.brush);
        }
    }
}
pub struct Overlay {
    pub hwnd: HWND,
    // Win32 can re-enter during control updates. The callback uses an independent,
    // stable allocation with checked interior borrowing.
    content: Box<RefCell<Content>>,
}
impl Overlay {
    /// # Safety
    /// Call once on the application's Win32 UI thread, with its message loop alive.
    pub unsafe fn create() -> windows::core::Result<Box<Self>> {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: w!("ECL.Overlay"),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }
        let mut value = Box::new(Self {
            hwnd: HWND::default(),
            content: Box::new(RefCell::new(Content {
                edit: HWND::default(),
                copy: HWND::default(),
                close: HWND::default(),
                font: HFONT::default(),
                brush: CreateSolidBrush(BACKGROUND),
                padding: 16,
                remaining: Duration::ZERO,
                last_tick: Instant::now(),
            })),
        });
        value.hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ECL.Overlay"),
            w!("English Companion Lite"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        )?;
        SetWindowLongPtrW(
            value.hwnd,
            GWLP_USERDATA,
            (&*value.content as *const RefCell<Content>) as isize,
        );
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!(""),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | ES_NOHIDESEL) as u32),
            0,
            0,
            0,
            0,
            value.hwnd,
            HMENU(BODY as *mut _),
            instance,
            None,
        )?;
        if !SetWindowSubclass(edit, Some(text_proc), 1, 0).as_bool() {
            return Err(windows::core::Error::from_win32());
        }
        let copy = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("复制全部"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            0,
            0,
            0,
            value.hwnd,
            HMENU(COPY as *mut _),
            instance,
            None,
        )?;
        let close = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("关闭"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0,
            0,
            0,
            0,
            value.hwnd,
            HMENU(CLOSE as *mut _),
            instance,
            None,
        )?;
        let mut c = value.content.borrow_mut();
        c.edit = edit;
        c.copy = copy;
        c.close = close;
        drop(c);
        Ok(value)
    }
    /// # Safety
    /// Must run on the owning UI thread, before destruction.
    pub unsafe fn hide(&self) {
        hide_window(self.hwnd);
    }
    /// # Safety
    /// Must run on the owning UI thread. Configuration changes must call hide instead.
    pub unsafe fn hide_unless_interacting(&self) {
        let capture = GetCapture();
        if GetForegroundWindow() != self.hwnd
            && capture != self.hwnd
            && !IsChild(self.hwnd, capture).as_bool()
        {
            self.hide();
        }
    }
    /// # Safety
    /// Must run on the owning UI thread; caret coordinates must be physical screen pixels.
    pub unsafe fn show(&mut self, text: String, caret: Option<RECT>, duration: u32) {
        let mut c = self.content.borrow_mut();
        let caption = wide(&text);
        let _ = SetWindowTextW(self.hwnd, PCWSTR(caption.as_ptr()));
        let _ = SetWindowTextW(c.edit, PCWSTR(caption.as_ptr()));
        SendMessageW(c.edit, EM_SETSEL, WPARAM(0), LPARAM(0));
        let monitor = if let Some(r) = caret {
            MonitorFromPoint(
                POINT {
                    x: r.left,
                    y: r.bottom,
                },
                MONITOR_DEFAULTTONEAREST,
            )
        } else {
            MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST)
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let (mut dx, mut dy) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let scale = |v: i32| (v * dx as i32 / 96).max(1);
        c.padding = scale(16);
        let font = CreateFontW(
            -scale(16),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        for child in [c.edit, c.copy, c.close] {
            SendMessageW(child, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        if c.font != HFONT::default() {
            let _ = DeleteObject(c.font);
        }
        c.font = font;
        let work = info.rcWork;
        let width = scale(430).min((work.right - work.left - scale(24)).max(scale(100)));
        let dc = GetDC(self.hwnd);
        if dc == HDC::default() {
            return;
        }
        let previous = SelectObject(dc, c.font);
        let mut bounds = RECT {
            right: (width - c.padding * 2 - scale(20)).max(1),
            ..Default::default()
        };
        let mut body = wide(&text);
        DrawTextW(
            dc,
            &mut body[..text.encode_utf16().count()],
            &mut bounds,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
        );
        SelectObject(dc, previous);
        ReleaseDC(self.hwnd, dc);
        let height =
            (bounds.bottom.max(scale(50)) + scale(110)).min(work.bottom - work.top - scale(24));
        let (x, y) = match caret {
            Some(r) if r.top - height - scale(24) >= work.top => {
                (r.left, r.top - height - scale(24))
            }
            _ => (
                work.right - width - scale(24),
                work.bottom - height - scale(80),
            ),
        };
        let x = x.clamp(work.left, (work.right - width).max(work.left));
        let y = y.clamp(work.top, (work.bottom - height).max(work.top));
        let footer = height - c.padding - scale(30);
        let flags = SWP_NOACTIVATE | SWP_NOZORDER;
        let _ = SetWindowPos(
            c.edit,
            None,
            c.padding,
            c.padding + scale(30),
            (width - c.padding * 2).max(1),
            (footer - c.padding - scale(38)).max(1),
            flags,
        );
        let _ = SetWindowPos(
            c.copy,
            None,
            (width - c.padding - scale(164)).max(0),
            footer,
            scale(92),
            scale(30),
            flags,
        );
        let _ = SetWindowPos(
            c.close,
            None,
            (width - c.padding - scale(64)).max(0),
            footer,
            scale(64),
            scale(30),
            flags,
        );
        c.remaining = Duration::from_millis(duration as u64);
        c.last_tick = Instant::now();
        let _ = SetWindowPos(
            self.hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(self.hwnd, None, true);
        SetTimer(self.hwnd, 1, 100, None);
    }
}
impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
unsafe fn hide_window(hwnd: HWND) {
    let _ = KillTimer(hwnd, 1);
    let _ = ShowWindow(hwnd, SW_HIDE);
}
unsafe fn hovered(hwnd: HWND) -> bool {
    let mut point = POINT::default();
    let mut rect = RECT::default();
    GetCursorPos(&mut point).is_ok()
        && GetWindowRect(hwnd, &mut rect).is_ok()
        && point.x >= rect.left
        && point.x < rect.right
        && point.y >= rect.top
        && point.y < rect.bottom
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Automatic display is nonactivating. An intentional click may focus the editor.
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_ACTIVATE as isize);
    }
    if msg == WM_CLOSE {
        hide_window(hwnd);
        return LRESULT(0);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Content>;
    if let Some(cell) = ptr.as_ref() {
        if let Ok(mut c) = cell.try_borrow_mut() {
            match msg {
                WM_TIMER if wp.0 == 1 => {
                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(c.last_tick);
                    c.last_tick = now;
                    if !hovered(hwnd) && GetForegroundWindow() != hwnd {
                        c.remaining = c.remaining.saturating_sub(elapsed);
                        if c.remaining.is_zero() {
                            hide_window(hwnd);
                        }
                    }
                    return LRESULT(0);
                }
                WM_COMMAND => {
                    match wp.0 & 0xffff {
                        COPY => {
                            let (mut start, mut end) = (0u32, 0u32);
                            SendMessageW(
                                c.edit,
                                EM_GETSEL,
                                WPARAM(&mut start as *mut _ as usize),
                                LPARAM(&mut end as *mut _ as isize),
                            );
                            SendMessageW(c.edit, EM_SETSEL, WPARAM(0), LPARAM(-1));
                            // Only this explicit action copies the displayed English/test text.
                            SendMessageW(c.edit, WM_COPY, WPARAM(0), LPARAM(0));
                            SendMessageW(
                                c.edit,
                                EM_SETSEL,
                                WPARAM(start as usize),
                                LPARAM(end as isize),
                            );
                            return LRESULT(0);
                        }
                        CLOSE => {
                            hide_window(hwnd);
                            return LRESULT(0);
                        }
                        _ => {}
                    }
                }
                WM_CTLCOLORSTATIC if lp.0 == c.edit.0 as isize => {
                    let dc = HDC(wp.0 as *mut _);
                    SetTextColor(dc, FOREGROUND);
                    SetBkColor(dc, BACKGROUND);
                    return LRESULT(c.brush.0 as isize);
                }
                WM_ERASEBKGND => return LRESULT(1),
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    FillRect(dc, &rect, c.brush);
                    let previous = SelectObject(dc, c.font);
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, FOREGROUND);
                    rect.left += c.padding;
                    rect.top += c.padding;
                    rect.right -= c.padding;
                    let mut title = wide("EN · 英语伴学 Lite");
                    let n = title.len() - 1;
                    DrawTextW(dc, &mut title[..n], &mut rect, DT_SINGLELINE | DT_NOPREFIX);
                    SelectObject(dc, previous);
                    let _ = EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }
                _ => {}
            }
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
unsafe extern "system" fn text_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_KEYDOWN {
        if wp.0 == VK_ESCAPE.0 as usize {
            if let Ok(parent) = GetParent(hwnd) {
                let _ = PostMessageW(parent, WM_COMMAND, WPARAM(CLOSE), LPARAM(0));
            }
            return LRESULT(0);
        }
        if wp.0 == b'A' as usize && GetKeyState(VK_CONTROL.0 as i32) < 0 {
            SendMessageW(hwnd, EM_SETSEL, WPARAM(0), LPARAM(-1));
            return LRESULT(0);
        }
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(text_proc), 1);
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
