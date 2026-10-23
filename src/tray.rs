use crate::overlay::wide;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};
pub const TRAY_MESSAGE: u32 = WM_APP + 1;
pub const TOGGLE: usize = 1001;
pub const SETTINGS: usize = 1002;
pub const RELOAD: usize = 1003;
pub const EXIT: usize = 1004;

pub struct Tray {
    data: NOTIFYICONDATAW,
}
impl Tray {
    /// # Safety
    /// `hwnd` must be a live top-level window owned by the calling UI thread.
    pub unsafe fn new(hwnd: HWND) -> windows::core::Result<Self> {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: brand_icon()?,
            ..Default::default()
        };
        let tip = wide("英语伴学 Lite · 默认暂停");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            let error = windows::core::Error::from_win32();
            let _ = DestroyIcon(data.hIcon);
            return Err(error);
        }
        Ok(Self { data })
    }
    /// # Safety
    /// Call on the owning UI thread.
    pub unsafe fn set_window_icon(&self, hwnd: HWND) {
        SendMessageW(
            hwnd,
            WM_SETICON,
            WPARAM(ICON_SMALL as usize),
            LPARAM(self.data.hIcon.0 as isize),
        );
        SendMessageW(
            hwnd,
            WM_SETICON,
            WPARAM(ICON_BIG as usize),
            LPARAM(self.data.hIcon.0 as isize),
        );
    }
    /// # Safety
    /// Call on the owning UI thread.
    pub unsafe fn update(&mut self, enabled: bool) {
        let text = wide(if enabled {
            "英语伴学 Lite · 运行中 · 点击打开设置"
        } else {
            "英语伴学 Lite · 已暂停 · 点击打开设置"
        });
        self.data.szTip.fill(0);
        self.data.szTip[..text.len()].copy_from_slice(&text);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &self.data);
    }
    /// # Safety
    /// Call on the owning UI thread while the notification window is alive.
    pub unsafe fn restore(&self) {
        let _ = Shell_NotifyIconW(NIM_ADD, &self.data);
    }
    /// # Safety
    /// Call on the owning UI thread while the notification window is alive.
    pub unsafe fn menu(&self, enabled: bool) {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let label = wide(if enabled {
            "暂停自动翻译"
        } else {
            "启用自动翻译"
        });
        let _ = AppendMenuW(menu, MF_STRING, TOGGLE, PCWSTR(label.as_ptr()));
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            SETTINGS,
            windows::core::w!("设置 / 诊断状态"),
        );
        let _ = AppendMenuW(menu, MF_STRING, RELOAD, windows::core::w!("重新加载配置"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, EXIT, windows::core::w!("退出"));
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let _ = SetForegroundWindow(self.data.hWnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            p.x,
            p.y,
            0,
            self.data.hWnd,
            None,
        );
        let _ = DestroyMenu(menu);
        if selected.0 != 0 {
            let _ = PostMessageW(
                self.data.hWnd,
                WM_COMMAND,
                WPARAM(selected.0 as usize),
                LPARAM(0),
            );
        }
        let _ = PostMessageW(self.data.hWnd, WM_NULL, WPARAM(0), LPARAM(0));
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
            let _ = DestroyIcon(self.data.hIcon);
        }
    }
}

// ICO directory entry #2 is the 32px mark; the resource is embedded in the single EXE.
unsafe fn brand_icon() -> windows::core::Result<HICON> {
    let ico = include_bytes!("../assets/companion.ico");
    let entry = 6 + 16;
    let size = u32::from_le_bytes(ico[entry + 8..entry + 12].try_into().unwrap()) as usize;
    let start = u32::from_le_bytes(ico[entry + 12..entry + 16].try_into().unwrap()) as usize;
    CreateIconFromResourceEx(
        &ico[start..start + size],
        true,
        0x00030000,
        32,
        32,
        LR_DEFAULTCOLOR,
    )
}
