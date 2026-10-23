//! Own-window integration checks. WM_COPY is intercepted in this process, so the
//! user's clipboard is never read or overwritten by this test.
use english_companion_lite::overlay::Overlay;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::*,
            Input::KeyboardAndMouse::SetFocus,
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};
static COPY_COUNT: AtomicU32 = AtomicU32::new(0);
static COPY_START: AtomicU32 = AtomicU32::new(0);
static COPY_END: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn copy_spy(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_COPY {
        let (mut start, mut end) = (0u32, 0u32);
        SendMessageW(
            hwnd,
            EM_GETSEL,
            WPARAM(&mut start as *mut _ as usize),
            LPARAM(&mut end as *mut _ as isize),
        );
        COPY_START.store(start, Ordering::SeqCst);
        COPY_END.store(end, Ordering::SeqCst);
        COPY_COUNT.fetch_add(1, Ordering::SeqCst);
        return LRESULT(0);
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
unsafe fn pump(ms: u64) {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn pass(ok: bool, label: &str) {
    assert!(ok, "{label}");
    println!("PASS: {label}");
}
unsafe fn body_text(edit: HWND) -> String {
    let mut text = [0u16; 2048];
    let n = GetWindowTextW(edit, &mut text);
    String::from_utf16_lossy(&text[..n as usize])
}
unsafe fn away_from_mouse(hwnd: HWND) {
    let mut point = POINT::default();
    GetCursorPos(&mut point).unwrap();
    let mut rect = RECT::default();
    GetWindowRect(hwnd, &mut rect).unwrap();
    // Move only our own fixture, never the pointer or another application's window.
    let x = if point.x >= rect.left && point.x < rect.right {
        point.x + 40
    } else {
        rect.left
    };
    SetWindowPos(
        hwnd,
        None,
        x,
        rect.top,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
    )
    .unwrap();
}
fn main() -> windows::core::Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        unsafe extern "system" fn source_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            DefWindowProcW(h, m, w, l)
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(source_proc),
            hInstance: instance.into(),
            lpszClassName: w!("ECL.OverlayFixture"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        let source = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("ECL.OverlayFixture"),
            w!("English Companion own-window test"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            40,
            40,
            460,
            220,
            None,
            None,
            instance,
            None,
        )?;
        struct Source(HWND);
        impl Drop for Source {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let _source = Source(source);
        let _ = ShowWindow(source, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(source);
        if GetForegroundWindow() != source {
            use windows::Win32::{System::Com::*, UI::Accessibility::*};
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let a: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let focus_result = a.ElementFromHandle(source)?.SetFocus();
            drop(a);
            CoUninitialize();
            focus_result?;
        }
        pump(100);
        pass(
            GetForegroundWindow() == source,
            "fixture source is foreground before popup",
        );
        let mut overlay = Overlay::create()?;
        let text = "Work was busy today.\r\nI want to get some rest early tonight. 🙂";
        overlay.show(text.into(), None, 500);
        pump(50);
        let edit = GetDlgItem(overlay.hwnd, 3101)?;
        pass(
            body_text(edit) == text,
            "native body contains the complete displayed English",
        );
        pass(
            GetWindowLongPtrW(edit, GWL_STYLE) as u32 & ES_READONLY as u32 != 0,
            "English body is read-only",
        );
        pass(
            GetDlgItem(overlay.hwnd, 3102).is_ok() && GetDlgItem(overlay.hwnd, 3103).is_ok(),
            "copy-all and close buttons exist",
        );
        pass(
            GetForegroundWindow() == source,
            "automatic popup does not take foreground focus",
        );
        pass(
            SendMessageW(overlay.hwnd, WM_NCHITTEST, WPARAM(0), LPARAM(0)).0
                != HTTRANSPARENT as isize,
            "popup accepts mouse interaction",
        );
        pass(
            SendMessageW(overlay.hwnd, WM_MOUSEACTIVATE, WPARAM(0), LPARAM(0)).0
                == MA_ACTIVATE as isize,
            "intentional mouse click can activate selection",
        );
        pass(
            SetWindowSubclass(edit, Some(copy_spy), 77, 0).as_bool(),
            "copy observer installed in this test's own process",
        );
        SendMessageW(edit, EM_SETSEL, WPARAM(2), LPARAM(7));
        SendMessageW(edit, WM_COPY, WPARAM(0), LPARAM(0));
        pass(
            COPY_START.load(Ordering::SeqCst) == 2 && COPY_END.load(Ordering::SeqCst) == 7,
            "partial selection routes to native copy",
        );
        SendMessageW(overlay.hwnd, WM_COMMAND, WPARAM(3102), LPARAM(0));
        pass(
            COPY_COUNT.load(Ordering::SeqCst) == 2
                && COPY_START.load(Ordering::SeqCst) == 0
                && COPY_END.load(Ordering::SeqCst) == text.encode_utf16().count() as u32,
            "copy-all routes the full Unicode body to native copy",
        );
        let (mut start, mut end) = (0u32, 0u32);
        SendMessageW(
            edit,
            EM_GETSEL,
            WPARAM(&mut start as *mut _ as usize),
            LPARAM(&mut end as *mut _ as isize),
        );
        pass(
            start == 2 && end == 7,
            "copy-all preserves the existing partial selection",
        );
        SendMessageW(edit, WM_CHAR, WPARAM(b'X' as usize), LPARAM(0));
        pass(
            body_text(edit) == text,
            "typing cannot alter the displayed translation",
        );
        let _ = SetForegroundWindow(overlay.hwnd);
        let _ = SetFocus(edit);
        pump(50);
        pass(
            GetForegroundWindow() == overlay.hwnd,
            "explicit selection can focus the popup",
        );
        overlay.hide_unless_interacting();
        pump(650);
        pass(
            IsWindowVisible(overlay.hwnd).as_bool(),
            "focus-change dismissal and timer preserve active selection",
        );
        SendMessageW(overlay.hwnd, WM_COMMAND, WPARAM(3103), LPARAM(0));
        pass(
            !IsWindowVisible(overlay.hwnd).as_bool(),
            "close button dismisses the popup",
        );
        let _ = SetForegroundWindow(source);
        overlay.show(text.into(), None, 500);
        let mut point = POINT::default();
        GetCursorPos(&mut point)?;
        SetWindowPos(
            overlay.hwnd,
            None,
            point.x - 20,
            point.y - 20,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
        )?;
        pump(650);
        pass(
            IsWindowVisible(overlay.hwnd).as_bool(),
            "hover pauses expiry without taking source focus",
        );
        pass(
            GetForegroundWindow() == source,
            "hover does not take keyboard focus",
        );
        away_from_mouse(overlay.hwnd);
        pump(650);
        pass(
            !IsWindowVisible(overlay.hwnd).as_bool(),
            "expiry resumes after pointer leaves",
        );
        overlay.show(text.into(), None, 5000);
        away_from_mouse(overlay.hwnd);
        overlay.hide_unless_interacting();
        pass(
            !IsWindowVisible(overlay.hwnd).as_bool(),
            "new source input can dismiss an unselected popup",
        );
        overlay.show(text.into(), None, 5000);
        let _ = SetForegroundWindow(overlay.hwnd);
        let _ = SetFocus(edit);
        pump(30);
        SendMessageW(edit, WM_KEYDOWN, WPARAM(0x1b), LPARAM(0));
        pump(50);
        pass(
            !IsWindowVisible(overlay.hwnd).as_bool(),
            "Escape dismisses a focused translation",
        );
        if std::env::args().any(|v| v == "--preview") {
            let _ = SetForegroundWindow(source);
            overlay.show(text.into(), None, 30000);
            println!("PREVIEW_READY");
            pump(60000);
        }
    }
    Ok(())
}
