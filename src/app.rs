use crate::settings_ui::*;
use crate::{
    cache::Cache,
    config::{Config, Provider},
    overlay::{wide, Overlay},
    translator::{
        winhttp::{AzureTranslator, DeepseekTranslator, MymemoryTranslator},
        MockTranslator, TranslateError, Translator, DEEPSEEK_MODEL, DEEPSEEK_PROMPT_VERSION,
    },
    tray::{self, Tray},
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{Controls::*, HiDpi::*, Shell::ShellExecuteW, WindowsAndMessaging::*},
    },
};

pub const HIDE: u32 = WM_APP + 2;
pub const RESULT: u32 = WM_APP + 3;
pub const STATUS: u32 = WM_APP + 4;
pub struct TranslationJob {
    pub epoch: u64,
    pub text: String,
    pub caret: Option<RECT>,
    pub pid: u32,
    pub config: Config,
}
/// A one-slot mailbox: while HTTP is busy, newer sentences replace queued old ones.
pub struct JobSender {
    latest: Arc<Mutex<Option<TranslationJob>>>,
    signal: SyncSender<()>,
}
struct JobReceiver {
    latest: Arc<Mutex<Option<TranslationJob>>>,
    signal: Receiver<()>,
}
fn job_channel() -> (JobSender, JobReceiver) {
    let latest = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::sync_channel(1);
    (
        JobSender {
            latest: latest.clone(),
            signal: tx,
        },
        JobReceiver { latest, signal: rx },
    )
}
impl JobSender {
    pub fn send(&self, job: TranslationJob) -> bool {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(job);
        !matches!(
            self.signal.try_send(()),
            Err(mpsc::TrySendError::Disconnected(_))
        )
    }
}
impl JobReceiver {
    fn recv(&self) -> Option<TranslationJob> {
        loop {
            self.signal.recv().ok()?;
            if let Some(job) = self.latest.lock().unwrap_or_else(|e| e.into_inner()).take() {
                return Some(job);
            }
        }
    }
}
struct Display {
    epoch: u64,
    text: String,
    caret: Option<RECT>,
    pid: u32,
    duration: u32,
}
/// Bounded metadata diagnostics, retained when the user opens our settings window.
#[derive(Default)]
struct DiagnosticStatus {
    current: String,
    last_authorized: Option<String>,
    events: VecDeque<String>,
}
impl DiagnosticStatus {
    fn current(&mut self, text: &str) {
        self.current = text.into();
    }
    fn authorized(&mut self, context: &str, text: &str) {
        self.current(text);
        self.last_authorized = Some(format!("{context}\r\n{text}"));
        self.event(&format!("{context}：{text}"));
    }
    fn event(&mut self, text: &str) {
        if self.events.back().is_some_and(|last| last == text) {
            return;
        }
        if self.events.len() == 24 {
            self.events.pop_front();
        }
        self.events.push_back(text.to_owned());
    }
    fn presentation(&self) -> String {
        format!(
            "当前：{}\r\n最近授权应用：{}\r\n最近事件（仅元数据，最多 24 条）：\r\n{}",
            self.current,
            self.last_authorized
                .as_deref()
                .unwrap_or("尚未检测；请切到已授权应用的输入框"),
            self.events.iter().cloned().collect::<Vec<_>>().join("\r\n")
        )
    }

    fn report(&self, cfg: &Config) -> String {
        format!(
            "英语伴学 Lite {} 诊断\r\n\
             仅包含状态和接口元数据，不包含输入原文、聊天标题、邮箱、密钥或白名单路径。\r\n\
             自动翻译={}；Mock={}；联网授权={}；翻译服务={}\r\n\
             最少汉字={}；句末停顿={}ms；无标点停顿={}ms\r\n\r\n{}\r\n\r\n\
             注：外层窗口未返回 Text/TextEdit 接口，不代表内部输入框也不支持。\r\n\
             请另附 Windows 版本和 QQ 完整版本；无需提供聊天内容。\r\n",
            env!("CARGO_PKG_VERSION"),
            cfg.enabled,
            cfg.mock_mode,
            cfg.allow_network,
            cfg.provider.name(),
            cfg.min_han,
            cfg.punctuation_delay_ms,
            cfg.idle_delay_ms,
            self.presentation()
        )
    }
}

fn write_diagnostic_report(config_path: &std::path::Path, report: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    let directory = config_path.parent().ok_or("无法定位诊断保存目录。")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "无法获取诊断文件时间。")?
        .as_nanos();
    let path = directory.join(format!("diagnostics-{stamp}.txt"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| "无法保存诊断文件。")?;
    file.write_all(format!("\u{feff}{report}").as_bytes())
        .map_err(|_| "无法写入诊断文件。")?;
    Ok(path)
}
pub struct Shared {
    config: Mutex<Config>,
    pub enabled: AtomicBool,
    pub stopping: AtomicBool,
    pub epoch: AtomicU64,
    pub reconfigure: AtomicBool,
    pub focus_dirty: AtomicBool,
    pub text_dirty: AtomicBool,
    pub selection_dirty: AtomicBool,
    signal: SyncSender<()>,
    hwnd: AtomicU64,
    status_text: Mutex<DiagnosticStatus>,
    display: Mutex<Option<Display>>,
}
impl Shared {
    pub(crate) fn metadata_event(&self, text: &str) {
        self.status_text
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .event(text);
        self.notify_status();
    }
    pub fn config(&self) -> Config {
        self.config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn wake(&self) {
        let _ = self.signal.try_send(());
    }
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        unsafe {
            let _ = PostMessageW(
                HWND(self.hwnd.load(Ordering::Acquire) as *mut _),
                HIDE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    pub fn status(&self, text: &str) {
        self.status_text
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current(text);
        self.notify_status();
    }
    pub fn authorized_status(&self, context: &str, text: &str) {
        self.status_text
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .authorized(context, text);
        self.notify_status();
    }
    fn notify_status(&self) {
        unsafe {
            let _ = PostMessageW(
                HWND(self.hwnd.load(Ordering::Acquire) as *mut _),
                STATUS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    fn update(&self, cfg: Config) {
        // Close the acquisition gate before reconfiguration; the worker removes subscriptions.
        self.enabled.store(false, Ordering::Release);
        self.invalidate();
        let enable = cfg.enabled;
        *self.config.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
        self.reconfigure.store(true, Ordering::Release);
        self.enabled.store(enable, Ordering::Release);
        self.wake();
    }
}

fn translation_worker(shared: Arc<Shared>, rx: JobReceiver) {
    translation_worker_with(shared, rx, translate_online);
}
fn translate_online(job: &TranslationJob) -> Result<String, TranslateError> {
    match job.config.provider {
        Provider::Deepseek => DeepseekTranslator.translate(&job.text),
        Provider::Azure => AzureTranslator::new(job.config.endpoint.clone()).translate(&job.text),
        Provider::Mymemory => MymemoryTranslator {
            email: job.config.mymemory_email.clone(),
        }
        .translate(&job.text),
    }
}

fn translation_worker_with(
    shared: Arc<Shared>,
    rx: JobReceiver,
    mut online: impl FnMut(&TranslationJob) -> Result<String, TranslateError>,
) {
    let mut cache = Cache::new(128);
    let mut context = String::new();
    let mut cooldown = Instant::now();
    while let Some(job) = rx.recv() {
        if shared.stopping.load(Ordering::Acquire) {
            break;
        }
        if !valid(&shared, job.epoch) {
            cache.clear();
            continue;
        }
        let tag = format!(
            "{:?}|{}|{}|{}|{}|{}|{}|{}",
            job.config.provider,
            job.config.endpoint,
            job.config.mymemory_email,
            job.config.mock_mode,
            job.config.allow_network,
            DEEPSEEK_MODEL,
            DEEPSEEK_PROMPT_VERSION,
            crate::credentials::revision()
        );
        if context != tag {
            cache.clear();
            context = tag;
            cooldown = Instant::now();
        }
        let result = if let Some(value) = cache.get(&job.text) {
            Ok(value)
        } else if job.config.mock_mode {
            MockTranslator.translate(&job.text)
        } else if !job.config.allow_network {
            shared.status("联网翻译未授权；可在设置中选择 Mock 测试模式");
            continue;
        } else if Instant::now() < cooldown {
            shared.status("翻译服务冷却中，当前句子已跳过");
            continue;
        } else {
            online(&job)
        };
        match result {
            Ok(text) => {
                if !valid(&shared, job.epoch) {
                    continue;
                }
                cache.insert(job.text, text.clone());
                *shared.display.lock().unwrap_or_else(|e| e.into_inner()) = Some(Display {
                    epoch: job.epoch,
                    text,
                    caret: job.caret,
                    pid: job.pid,
                    duration: job.config.display_ms,
                });
                unsafe {
                    let _ = PostMessageW(
                        HWND(shared.hwnd.load(Ordering::Acquire) as *mut _),
                        RESULT,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
                shared.status(if job.config.mock_mode {
                    "Mock 测试提示已生成（不是真实英文翻译）"
                } else {
                    "英文翻译已生成"
                });
            }
            Err(e) => {
                cooldown = Instant::now()
                    + Duration::from_secs(match e {
                        TranslateError::RateLimited => 60,
                        TranslateError::DailyQuotaExceeded
                        | TranslateError::InsufficientBalance => 3600,
                        TranslateError::Server | TranslateError::Network => 10,
                        _ => 0,
                    });
                if valid(&shared, job.epoch) {
                    shared.status(e.status());
                }
            }
        }
    }
}
fn valid(shared: &Shared, epoch: u64) -> bool {
    shared.enabled.load(Ordering::Acquire)
        && !shared.stopping.load(Ordering::Acquire)
        && shared.epoch.load(Ordering::Acquire) == epoch
}

struct State {
    manager: HWND,
    shared: Arc<Shared>,
    path: PathBuf,
    overlay: Box<Overlay>,
    tray: Option<Tray>,
    settings: HWND,
    settings_ui: Option<Box<SettingsUi>>,
    taskbar_message: u32,
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            self.shared.enabled.store(false, Ordering::Release);
            self.shared.stopping.store(true, Ordering::Release);
            self.shared.hwnd.store(0, Ordering::Release);
            self.shared.wake();
            SetWindowLongPtrW(self.manager, GWLP_USERDATA, 0);
            self.settings_ui.take();
            if self.settings != HWND::default() {
                SetWindowLongPtrW(self.settings, GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.settings);
            }
            self.tray.take();
            let _ = DestroyWindow(self.manager);
        }
    }
}
pub fn fatal(error: &str) {
    unsafe {
        let text = wide(error);
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("英语伴学 Lite"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s != "--check") {
        return Err("支持参数：--check（仅检查配置，不开始监听）。".into());
    }
    let path = Config::path()?;
    let first = !path.exists();
    let cfg = Config::load(&path)?;
    if args.iter().any(|s| s == "--check") {
        cfg.validate()?;
        return Ok(());
    }
    unsafe {
        run_ui(path, cfg, first).map_err(|error| {
            format!(
                "Windows 窗口、托盘或线程初始化失败（HRESULT 0x{:08X}）。",
                error.code().0
            )
        })
    }
}

unsafe fn run_ui(path: PathBuf, cfg: Config, first: bool) -> windows::core::Result<()> {
    let mutex = CreateMutexW(None, true, w!("Local\\EnglishCompanionLite.SingleInstance"))?;
    if GetLastError() == ERROR_ALREADY_EXISTS {
        let _ = CloseHandle(mutex);
        return Ok(());
    }
    struct MutexHandle(HANDLE);
    impl Drop for MutexHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _single = MutexHandle(mutex);
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(manager_proc),
        hInstance: instance.into(),
        lpszClassName: w!("ECL.Manager"),
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 {
        return Err(windows::core::Error::from_win32());
    }
    let class = WNDCLASSW {
        lpfnWndProc: Some(settings_proc),
        hInstance: instance.into(),
        lpszClassName: w!("ECL.Settings"),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: HBRUSH::default(),
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 {
        return Err(windows::core::Error::from_win32());
    }
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        w!("ECL.Manager"),
        w!("English Companion Lite Control"),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        instance,
        None,
    )?;
    let (tx, rx) = mpsc::sync_channel(1);
    let shared = Arc::new(Shared {
        config: Mutex::new(cfg.clone()),
        enabled: AtomicBool::new(cfg.enabled),
        stopping: AtomicBool::new(false),
        epoch: AtomicU64::new(1),
        reconfigure: AtomicBool::new(true),
        focus_dirty: AtomicBool::new(false),
        text_dirty: AtomicBool::new(false),
        selection_dirty: AtomicBool::new(false),
        signal: tx,
        hwnd: AtomicU64::new(hwnd.0 as u64),
        status_text: Mutex::new(DiagnosticStatus {
            current: "已暂停，首次使用请明确授权记事本与翻译模式。".into(),
            last_authorized: None,
            events: VecDeque::new(),
        }),
        display: Mutex::new(None),
    });
    let state = Box::new(RefCell::new(State {
        manager: hwnd,
        shared: shared.clone(),
        path,
        overlay: Overlay::create()?,
        tray: Some(Tray::new(hwnd)?),
        settings: HWND::default(),
        settings_ui: None,
        taskbar_message: RegisterWindowMessageW(w!("TaskbarCreated")),
    }));
    // RefCell protects against synchronous Win32 message re-entry during setters/dialogs.
    SetWindowLongPtrW(
        hwnd,
        GWLP_USERDATA,
        (&*state as *const RefCell<State>) as isize,
    );
    let (job_tx, job_rx) = job_channel();
    let worker_shared = shared.clone();
    let network = thread::Builder::new()
        .name("translation".into())
        .spawn(move || translation_worker(worker_shared, job_rx))
        .map_err(|_| windows::core::Error::from_win32())?;
    let uia_shared = shared.clone();
    let input = thread::Builder::new()
        .name("uia-mta".into())
        .spawn(move || crate::uia::run(uia_shared, rx, job_tx))
        .map_err(|_| windows::core::Error::from_win32())?;
    shared.wake();
    if first {
        show_settings(hwnd, &mut state.borrow_mut());
    }
    let mut message = MSG::default();
    loop {
        let result = GetMessageW(&mut message, None, 0, 0).0;
        if result <= 0 {
            break;
        }
        let settings = state.borrow().settings;
        if settings != HWND::default()
            && (message.hwnd == settings || IsChild(settings, message.hwnd).as_bool())
        {
            if let Some(ui) = &state.borrow().settings_ui {
                ui.input_message(message.message, message.wParam);
            }
        }
        if settings != HWND::default()
            && IsWindowVisible(settings).as_bool()
            && IsDialogMessageW(settings, &message).as_bool()
        {
            continue;
        }
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    shared.stopping.store(true, Ordering::Release);
    shared.enabled.store(false, Ordering::Release);
    shared.invalidate();
    shared.wake();
    state.borrow().overlay.hide();
    // UIA unregisters on the same MTA thread. Dropping its sender terminates the network worker.
    let input_result = input.join();
    let network_result = network.join();
    if input_result.is_err() || network_result.is_err() {
        fatal("后台线程异常退出；进程即将关闭。");
    }
    Ok(())
}

unsafe extern "system" fn manager_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>;
    if let Some(cell) = ptr.as_ref() {
        let Ok(mut borrow) = cell.try_borrow_mut() else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        let state = &mut *borrow;
        match msg {
            HIDE => {
                if state.shared.enabled.load(Ordering::Acquire)
                    && !state.shared.reconfigure.load(Ordering::Acquire)
                {
                    state.overlay.hide_unless_interacting();
                } else {
                    state.overlay.hide();
                }
                return LRESULT(0);
            }
            RESULT => {
                let display = state
                    .shared
                    .display
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take();
                if let Some(d) = display {
                    if valid(&state.shared, d.epoch) && crate::uia::foreground_pid() == d.pid {
                        state.overlay.show(d.text, d.caret, d.duration);
                    }
                }
                return LRESULT(0);
            }
            STATUS => {
                refresh_status(state);
                return LRESULT(0);
            }
            tray::TRAY_MESSAGE => {
                match lp.0 as u32 {
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        if let Some(t) = &state.tray {
                            t.menu(state.shared.enabled.load(Ordering::Acquire));
                        }
                    }
                    WM_LBUTTONUP => show_settings(hwnd, state),
                    _ => {}
                }
                return LRESULT(0);
            }
            WM_COMMAND => {
                match wp.0 & 0xffff {
                    tray::TOGGLE => {
                        let mut cfg = state.shared.config();
                        cfg.enabled = !cfg.enabled;
                        apply(state, cfg);
                    }
                    tray::SETTINGS => show_settings(hwnd, state),
                    tray::RELOAD => match Config::load(&state.path) {
                        Ok(cfg) => {
                            state.overlay.hide();
                            state.shared.update(cfg);
                            if state.settings != HWND::default() {
                                populate(state);
                            }
                        }
                        Err(e) => fatal(&e),
                    },
                    tray::EXIT => {
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                return LRESULT(0);
            }
            WM_CLOSE => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ if msg == state.taskbar_message => {
                if let Some(t) = &state.tray {
                    t.restore();
                }
                return LRESULT(0);
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
fn save_config_and_key(
    cfg: &Config,
    path: &std::path::Path,
    key: Option<&str>,
) -> Result<(), String> {
    cfg.validate()?;
    let previous = if let Some(key) = key {
        crate::credentials::validate_key(key)?;
        let previous = crate::credentials::load(path)?;
        crate::credentials::save(path, key)?;
        previous
    } else {
        None
    };
    if let Err(error) = cfg.save(path) {
        if key.is_some() {
            let restored = match previous.as_deref() {
                Some(previous) => crate::credentials::save(path, previous),
                None => crate::credentials::remove(path),
            };
            if restored.is_err() {
                return Err("配置保存失败，密钥恢复也未完成；请检查存储权限后重新保存。".into());
            }
        }
        return Err(error);
    }
    Ok(())
}
unsafe fn apply(state: &mut State, cfg: Config) {
    apply_with_key(state, cfg, None);
}
unsafe fn apply_with_key(state: &mut State, cfg: Config, key: Option<&str>) {
    if let Err(error) = save_config_and_key(&cfg, &state.path, key) {
        if let Some(ui) = &state.settings_ui {
            ui.message(&error);
        } else {
            fatal(&error);
        }
        return;
    }
    state.shared.update(cfg);
    state.overlay.hide();
    if state.settings != HWND::default() {
        populate(state);
    }
}
unsafe fn show_settings(owner: HWND, state: &mut State) {
    if state.settings == HWND::default() {
        let title = wide(&format!(
            "英语伴学 Lite {} · 设置与状态",
            env!("CARGO_PKG_VERSION")
        ));
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_COMPOSITED,
            w!("ECL.Settings"),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            936,
            689,
            owner,
            None,
            GetModuleHandleW(None).unwrap_or_default(),
            None,
        ) else {
            fatal("无法创建设置窗口。");
            return;
        };
        state.settings = hwnd;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, GetWindowLongPtrW(owner, GWLP_USERDATA));
        match SettingsUi::install(hwnd) {
            Ok(ui) => state.settings_ui = Some(ui),
            Err(_) => {
                let _ = DestroyWindow(hwnd);
                state.settings = HWND::default();
                fatal("无法创建设置控件。");
                return;
            }
        }
        // Size the client surface in logical pixels, including the current monitor's DPI.
        let dpi = GetDpiForWindow(hwnd).max(96);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 920 * dpi as i32 / 96,
            bottom: 650 * dpi as i32 / 96,
        };
        let _ = AdjustWindowRectExForDpi(
            &mut rect,
            WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
            false,
            WS_EX_CONTROLPARENT,
            dpi,
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        if let Some(tray) = &state.tray {
            tray.set_window_icon(hwnd);
        }
        populate(state);
    }
    let _ = ShowWindow(state.settings, SW_SHOWNORMAL);
    let _ = SetForegroundWindow(state.settings);
}
unsafe fn set_text(hwnd: HWND, id: i32, text: &str) {
    if get_text(hwnd, id) == text {
        return;
    }
    let txt = wide(text);
    if let Ok(child) = GetDlgItem(hwnd, id) {
        let _ = SetWindowTextW(child, PCWSTR(txt.as_ptr()));
    }
}
unsafe fn get_text(hwnd: HWND, id: i32) -> String {
    let Ok(child) = GetDlgItem(hwnd, id) else {
        return String::new();
    };
    let n = GetWindowTextLengthW(child).max(0) as usize;
    let mut value = vec![0u16; (n + 1).min(32768)];
    let got = GetWindowTextW(child, &mut value);
    String::from_utf16_lossy(&value[..got.max(0) as usize])
}
unsafe fn checked(hwnd: HWND, id: i32) -> bool {
    GetDlgItem(hwnd, id).ok().is_some_and(|h| {
        SendMessageW(h, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize
    })
}
unsafe fn selected_provider(hwnd: HWND) -> Provider {
    let index = GetDlgItem(hwnd, PROVIDER)
        .ok()
        .map(|h| SendMessageW(h, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0);
    match index {
        Some(1) => Provider::Azure,
        Some(2) => Provider::Deepseek,
        _ => Provider::Mymemory,
    }
}
unsafe fn refresh_provider_notice(hwnd: HWND) {
    let name = selected_provider(hwnd).name();
    set_text(hwnd, NETWORK, &format!("允许联网：句子将发送到 {name}"));
    if let Ok(email) = GetDlgItem(hwnd, MYMEMORY_EMAIL) {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(
            email,
            selected_provider(hwnd) == Provider::Mymemory,
        );
    }
    let _ = InvalidateRect(hwnd, None, false);
}
unsafe fn populate(state: &mut State) {
    let c = state.shared.config();
    let h = state.settings;
    if let Ok(combo) = GetDlgItem(h, PROVIDER) {
        SendMessageW(
            combo,
            CB_SETCURSEL,
            WPARAM(match c.provider {
                Provider::Mymemory => 0,
                Provider::Azure => 1,
                Provider::Deepseek => 2,
            }),
            LPARAM(0),
        );
    }
    set_text(h, MYMEMORY_EMAIL, &c.mymemory_email);
    // Do not load a saved secret into a UI control. Blank means keep the saved key.
    set_text(h, DEEPSEEK_KEY, "");
    let key_label = match crate::credentials::load(&state.path) {
        Ok(Some(_)) => "DeepSeek API Key · 已保存，留空保留",
        Ok(None) => "DeepSeek API Key",
        Err(_) => "DeepSeek API Key · 无法读取，请重新保存",
    };
    set_text(h, DEEPSEEK_KEY_LABEL, key_label);
    refresh_provider_notice(h);
    for (id, on) in [
        (ENABLED, c.enabled),
        (NOTEPAD, c.allow_notepad),
        (NETWORK, c.allow_network),
        (MOCK, c.mock_mode),
    ] {
        if let Ok(child) = GetDlgItem(h, id) {
            SendMessageW(
                child,
                BM_SETCHECK,
                WPARAM(if on { BST_CHECKED.0 as usize } else { 0 }),
                LPARAM(0),
            );
        }
    }
    for (id, n) in [
        (PUNCT, c.punctuation_delay_ms),
        (IDLE, c.idle_delay_ms),
        (DISPLAY_MS, c.display_ms as u64),
        (MIN_HAN, c.min_han as u64),
    ] {
        set_text(h, id, &n.to_string());
    }
    set_text(h, PATHS, &c.allowed_executables.join("\r\n"));
    if let Some(ui) = &state.settings_ui {
        ui.refresh_provider();
        ui.applied(c.enabled);
    }
    refresh_status(state);
}
unsafe fn refresh_status(state: &mut State) {
    let (status, details) = {
        let value = state
            .shared
            .status_text
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        (value.current.clone(), value.presentation())
    };
    let caption = wide(&format!("English Companion Lite · {status}"));
    let _ = SetWindowTextW(
        HWND(state.shared.hwnd.load(Ordering::Acquire) as *mut _),
        PCWSTR(caption.as_ptr()),
    );
    if let Some(tray) = &mut state.tray {
        tray.update(state.shared.enabled.load(Ordering::Acquire));
    }
    if state.settings != HWND::default() {
        set_text(state.settings, STATUS_LABEL, &details);
    }
}
unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>;
    if let Some(cell) = ptr.as_ref() {
        let Ok(mut borrow) = cell.try_borrow_mut() else {
            return DefWindowProcW(hwnd, msg, wp, lp);
        };
        let state = &mut *borrow;
        match msg {
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                return LRESULT(0);
            }
            WM_COMMAND => {
                match wp.0 & 0xffff {
                    2 => {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                    SAVE => {
                        let parsed = (
                            get_text(hwnd, PUNCT).parse::<u64>(),
                            get_text(hwnd, IDLE).parse::<u64>(),
                            get_text(hwnd, DISPLAY_MS).parse::<u32>(),
                            get_text(hwnd, MIN_HAN).parse::<usize>(),
                        );
                        if let (Ok(p), Ok(i), Ok(d), Ok(m)) = parsed {
                            let mut c = state.shared.config();
                            c.enabled = checked(hwnd, ENABLED);
                            c.allow_notepad = checked(hwnd, NOTEPAD);
                            c.allow_network = checked(hwnd, NETWORK);
                            c.mock_mode = checked(hwnd, MOCK);
                            c.provider = selected_provider(hwnd);
                            c.mymemory_email = get_text(hwnd, MYMEMORY_EMAIL).trim().to_owned();
                            c.punctuation_delay_ms = p;
                            c.idle_delay_ms = i;
                            c.display_ms = d;
                            c.min_han = m;
                            c.allowed_executables = get_text(hwnd, PATHS)
                                .lines()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect();
                            if let Err(error) = c.validate() {
                                if let Some(ui) = &state.settings_ui {
                                    ui.message(&error);
                                }
                            } else {
                                let key = get_text(hwnd, DEEPSEEK_KEY);
                                let key = key.trim();
                                let replacement = (c.provider == Provider::Deepseek
                                    && !key.is_empty())
                                .then_some(key);
                                apply_with_key(state, c, replacement);
                            }
                        } else {
                            if let Some(ui) = &state.settings_ui {
                                ui.message("延迟、时长和字数必须填写整数。");
                            }
                        }
                    }
                    OPEN_CONFIG => {
                        let path = wide(&format!("\"{}\"", state.path.display()));
                        ShellExecuteW(
                            hwnd,
                            w!("open"),
                            w!("notepad.exe"),
                            PCWSTR(path.as_ptr()),
                            PCWSTR::null(),
                            SW_SHOWNORMAL,
                        );
                    }
                    EXPORT_DIAGNOSTICS => {
                        let cfg = state.shared.config();
                        let report = state
                            .shared
                            .status_text
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .report(&cfg);
                        match write_diagnostic_report(&state.path, &report) {
                            Ok(path) => {
                                state
                                    .shared
                                    .status(&format!("诊断已保存：{}", path.display()));
                                let quoted = wide(&format!("\"{}\"", path.display()));
                                ShellExecuteW(
                                    hwnd,
                                    w!("open"),
                                    w!("notepad.exe"),
                                    PCWSTR(quoted.as_ptr()),
                                    PCWSTR::null(),
                                    SW_SHOWNORMAL,
                                );
                            }
                            Err(error) => fatal(&error),
                        }
                    }
                    command
                        if command == PROVIDER as usize
                            && (wp.0 >> 16) & 0xffff == CBN_SELCHANGE as usize =>
                    {
                        refresh_provider_notice(hwnd);
                    }
                    _ => {}
                }
                return LRESULT(0);
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Uses synthetic credentials in an isolated profile to check failed-config rollback"]
    fn credentials_rollback_when_config_cannot_be_written() {
        let directory = std::env::current_dir()
            .unwrap()
            .join("artifacts")
            .join(format!(
                "credential-rollback-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let blocker = directory.join("not-a-directory");
        std::fs::write(&blocker, "synthetic test fixture").unwrap();
        let path = blocker.join("config.toml");
        struct Cleanup(PathBuf, PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = crate::credentials::remove(&self.0);
                let _ = std::fs::remove_file(&self.1);
                let _ = std::fs::remove_dir(self.1.parent().unwrap());
            }
        }
        let _cleanup = Cleanup(path.clone(), blocker);
        let cfg = Config::default();
        assert!(save_config_and_key(&cfg, &path, Some("sk-synthetic-new")).is_err());
        assert!(crate::credentials::load(&path).unwrap().is_none());
        crate::credentials::save(&path, "sk-synthetic-old").unwrap();
        assert!(save_config_and_key(&cfg, &path, Some("sk-synthetic-replacement")).is_err());
        assert!(crate::credentials::load(&path).unwrap().as_deref() == Some("sk-synthetic-old"));
    }
    #[test]
    fn diagnostic_export_retains_full_reason_without_serializing_private_config() {
        let cfg = Config {
            mymemory_email: "private-contact@example.com".into(),
            allowed_executables: vec![r"C:\private-application\QQ.exe".into()],
            ..Config::default()
        };
        let mut status = DiagnosticStatus::default();
        status.authorized(
            "QQ.exe；外层焦点 Type=50032(窗口)",
            "未定位到输入框\r\n同进程内部焦点数=0\r\n原生焦点与外层相同",
        );
        status.current("本工具设置窗口不采集");
        let report = status.report(&cfg);
        assert!(report.contains("内部焦点数=0"));
        assert!(report.contains("原生焦点与外层相同"));
        assert!(report.contains(env!("CARGO_PKG_VERSION")));
        assert!(!report.contains(&cfg.mymemory_email));
        assert!(!report.contains(&cfg.allowed_executables[0]));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::current_dir()
            .unwrap()
            .join("artifacts")
            .join(format!("diagnostic-export-test-{stamp}"));
        std::fs::create_dir_all(&directory).unwrap();
        let config_path = directory.join("config.toml");
        std::fs::write(&config_path, "unchanged test config").unwrap();
        let first = write_diagnostic_report(&config_path, &report).unwrap();
        let second = write_diagnostic_report(&config_path, "another report").unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            format!("\u{feff}{report}")
        );
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            "unchanged test config"
        );
        for file in [first, second, config_path] {
            std::fs::remove_file(file).unwrap();
        }
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    #[ignore = "Sends one fixed synthetic sentence to the free MyMemory service"]
    fn mymemory_live_worker() {
        let config = Config {
            provider: Provider::Mymemory,
            enabled: true,
            allow_notepad: true,
            allow_network: true,
            ..Config::default()
        };
        let shared = shared(config.clone());
        let (tx, rx) = job_channel();
        let mut request = job(1, &config);
        request.text = "今天工作比较忙，晚上想早点休息。".into();
        tx.send(request);
        drop(tx);
        translation_worker_with(shared.clone(), rx, translate_online);
        let display = shared
            .display
            .lock()
            .unwrap()
            .take()
            .expect("live worker should produce English");
        assert!(!display.text.contains("[MOCK"));
        assert!(display.text.to_ascii_lowercase().contains("work"));
        println!("Live worker translation: {}", display.text);
    }
    #[test]
    #[ignore = "Sends one fixed synthetic sentence to the paid DeepSeek API using configured credentials"]
    fn deepseek_live_worker() {
        let config = Config {
            provider: Provider::Deepseek,
            enabled: true,
            allow_notepad: true,
            allow_network: true,
            ..Config::default()
        };
        let shared = shared(config.clone());
        let (tx, rx) = job_channel();
        let mut request = job(1, &config);
        request.text = "今天工作比较忙，晚上想早点休息。".into();
        tx.send(request);
        drop(tx);
        translation_worker_with(shared.clone(), rx, translate_online);
        let display = shared.display.lock().unwrap().take()
            .expect("DeepSeek should return a complete translation; check diagnostic status, key and balance");
        assert!(!display.text.trim().is_empty());
        assert!(!display.text.contains("[MOCK"));
        println!("DeepSeek fixed-sentence translation: {}", display.text);
    }
    #[test]
    fn settings_focus_and_unrelated_app_do_not_erase_authorized_failure() {
        let mut status = DiagnosticStatus::default();
        status.authorized(
            "QQ.exe；Text=有，Text2=无，TextEdit=有",
            "等待插入光标：UIA 选区未折叠",
        );
        status.current("本工具设置窗口不采集");
        assert!(status.presentation().contains("QQ.exe"));
        assert!(status.presentation().contains("选区未折叠"));
        status.current("跳过：当前进程未授权");
        assert!(status.presentation().contains("QQ.exe"));
        status.authorized("notepad.exe", "正在监听");
        assert!(!status
            .last_authorized
            .as_deref()
            .unwrap()
            .contains("QQ.exe"));
        assert!(status.presentation().contains("QQ.exe"));
        for index in 0..100 {
            status.event(&format!("metadata event {index}"));
        }
        assert_eq!(status.events.len(), 24);
        assert!(!status.presentation().contains("QQ.exe"));
    }
    fn shared(cfg: Config) -> Arc<Shared> {
        let (signal, _) = mpsc::sync_channel(1);
        Arc::new(Shared {
            enabled: AtomicBool::new(cfg.enabled),
            config: Mutex::new(cfg),
            stopping: AtomicBool::new(false),
            epoch: AtomicU64::new(1),
            reconfigure: AtomicBool::new(false),
            focus_dirty: AtomicBool::new(false),
            text_dirty: AtomicBool::new(false),
            selection_dirty: AtomicBool::new(false),
            signal,
            hwnd: AtomicU64::new(0),
            status_text: Mutex::new(DiagnosticStatus::default()),
            display: Mutex::new(None),
        })
    }
    fn job(epoch: u64, config: &Config) -> TranslationJob {
        TranslationJob {
            epoch,
            text: "今天晚上早点休息。".into(),
            caret: None,
            pid: 1,
            config: config.clone(),
        }
    }
    #[test]
    fn late_network_response_cannot_overwrite_a_new_sentence() {
        let config = Config {
            enabled: true,
            allow_notepad: true,
            allow_network: true,
            ..Config::default()
        };
        let shared = shared(config.clone());
        let (tx, rx) = job_channel();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        assert!(tx.send(job(1, &config)));
        let worker_shared = shared.clone();
        let worker = thread::spawn(move || {
            let mut calls = 0;
            translation_worker_with(worker_shared, rx, move |_| {
                calls += 1;
                if calls == 1 {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok("Old English".into())
                } else {
                    Ok("Current English".into())
                }
            });
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        shared.invalidate();
        let mut latest = job(2, &config);
        latest.text = "明天早上继续学习英语。".into();
        assert!(tx.send(latest));
        drop(tx);
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        let display = shared.display.lock().unwrap().take().unwrap();
        assert_eq!(display.epoch, 2);
        assert_eq!(display.text, "Current English");
    }
    #[test]
    fn offline_authorization_and_pause_prevent_online_calls() {
        for enabled in [false, true] {
            let config = Config {
                enabled,
                allow_notepad: true,
                allow_network: false,
                ..Config::default()
            };
            let shared = shared(config.clone());
            let (tx, rx) = job_channel();
            assert!(tx.send(job(1, &config)));
            drop(tx);
            translation_worker_with(shared.clone(), rx, |_| panic!("network must not be called"));
            assert!(shared.display.lock().unwrap().is_none());
        }
    }
    #[test]
    fn service_and_contact_changes_do_not_reuse_another_services_cache() {
        let config = Config {
            enabled: true,
            allow_notepad: true,
            allow_network: true,
            ..Config::default()
        };
        let shared = shared(config.clone());
        let (tx, rx) = job_channel();
        assert!(tx.send(job(1, &config)));
        let mut sender = Some(tx);
        let mut calls = 0;
        translation_worker_with(shared.clone(), rx, |current| {
            calls += 1;
            match calls {
                1 => {
                    let mut next = job(1, &current.config);
                    next.config.provider = Provider::Azure;
                    sender.as_ref().unwrap().send(next);
                }
                2 => {
                    let mut next = job(1, &current.config);
                    next.config.mymemory_email = "contact@example.com".into();
                    sender.as_ref().unwrap().send(next);
                }
                3 => {
                    sender.take();
                }
                _ => panic!("unexpected request"),
            }
            Ok(format!("translation {calls}"))
        });
        assert_eq!(calls, 3);
        assert_eq!(
            shared.display.lock().unwrap().take().unwrap().text,
            "translation 3"
        );
    }
    #[test]
    fn quota_and_balance_skip_requests_but_switching_service_clears_cooldown() {
        for (switch, error) in [
            (false, TranslateError::DailyQuotaExceeded),
            (true, TranslateError::DailyQuotaExceeded),
            (false, TranslateError::InsufficientBalance),
            (true, TranslateError::InsufficientBalance),
        ] {
            let config = Config {
                enabled: true,
                allow_notepad: true,
                allow_network: true,
                ..Config::default()
            };
            let shared = shared(config.clone());
            let (tx, rx) = job_channel();
            tx.send(job(1, &config));
            let mut sender = Some(tx);
            let mut calls = 0;
            translation_worker_with(shared.clone(), rx, |current| {
                calls += 1;
                if calls == 1 {
                    let mut next = job(1, &current.config);
                    if switch {
                        next.config.provider = Provider::Azure;
                    }
                    sender.take().unwrap().send(next);
                    Err(error.clone())
                } else {
                    Ok("Another service".into())
                }
            });
            assert_eq!(calls, if switch { 2 } else { 1 });
            assert_eq!(shared.display.lock().unwrap().is_some(), switch);
        }
    }
    #[test]
    fn queued_sentences_are_bounded_and_latest_wins() {
        let cfg = Config {
            enabled: true,
            allow_notepad: true,
            mock_mode: true,
            ..Config::default()
        };
        let shared = shared(cfg.clone());
        let (tx, rx) = job_channel();
        for i in 1..=10 {
            let mut value = job(1, &cfg);
            value.text = "中".repeat(i + 6);
            assert!(tx.send(value));
        }
        drop(tx);
        translation_worker_with(shared.clone(), rx, |_| panic!("mock cannot use network"));
        let shown = shared.display.lock().unwrap().take().unwrap();
        assert!(shown.text.contains("16 characters"));
    }
}
