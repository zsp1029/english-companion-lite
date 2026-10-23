//! All UIA objects, including registration and removal, belong to this COM MTA worker.
//! Callbacks carry no text and never call providers or do network work.
use crate::{
    app::{JobSender, Shared, TranslationJob},
    config::Config,
    sentence::Trigger,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Receiver,
        Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{implement, Interface, PWSTR},
    Win32::{
        Foundation::{CloseHandle, BOOL, ERROR_SUCCESS, HANDLE, HWND, RECT},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        Storage::Packaging::Appx::GetPackageFamilyName,
        System::{
            Com::*,
            Ole::{
                SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetLBound,
                SafeArrayGetUBound, SafeArrayGetVartype, SafeArrayUnaccessData,
            },
            Threading::*,
        },
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
};

#[implement(IUIAutomationFocusChangedEventHandler)]
struct FocusHandler {
    shared: Arc<Shared>,
}
impl IUIAutomationFocusChangedEventHandler_Impl for FocusHandler_Impl {
    fn HandleFocusChangedEvent(
        &self,
        _: Option<&IUIAutomationElement>,
    ) -> windows::core::Result<()> {
        if self.shared.enabled.load(Ordering::Acquire) {
            self.shared.focus_dirty.store(true, Ordering::Release);
            self.shared.invalidate();
            self.shared.wake();
        }
        Ok(())
    }
}
#[implement(IUIAutomationEventHandler)]
struct TextHandler {
    shared: Arc<Shared>,
    alive: Arc<AtomicBool>,
}
impl IUIAutomationEventHandler_Impl for TextHandler_Impl {
    fn HandleAutomationEvent(
        &self,
        _: Option<&IUIAutomationElement>,
        event: UIA_EVENT_ID,
    ) -> windows::core::Result<()> {
        if self.alive.load(Ordering::Acquire) && self.shared.enabled.load(Ordering::Acquire) {
            if event == UIA_Text_TextChangedEventId {
                self.shared.text_dirty.store(true, Ordering::Release);
            } else {
                self.shared.selection_dirty.store(true, Ordering::Release);
            }
            self.shared.invalidate();
            self.shared.wake();
        }
        Ok(())
    }
}

#[implement(IUIAutomationTextEditTextChangedEventHandler)]
struct CompositionHandler {
    shared: Arc<Shared>,
    alive: Arc<AtomicBool>,
    composition: Arc<CompositionState>,
}
impl IUIAutomationTextEditTextChangedEventHandler_Impl for CompositionHandler_Impl {
    fn HandleTextEditTextChangedEvent(
        &self,
        _: Option<&IUIAutomationElement>,
        change: TextEditChangeType,
        _: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        // Ignore the event's string payload entirely. Re-read only the authorized
        // editor's bounded, committed range on the MTA worker after checking its IME state.
        if self.alive.load(Ordering::Acquire) && self.shared.enabled.load(Ordering::Acquire) {
            self.composition.record(change);
            self.shared.metadata_event(&format!(
                "IME 事件={}，版本={}",
                change.0,
                self.composition.stamp()
            ));
            self.shared.text_dirty.store(true, Ordering::Release);
            self.shared.invalidate();
            self.shared.wake();
        }
        Ok(())
    }
}

#[derive(Default)]
struct CompositionState(AtomicU64);
impl CompositionState {
    fn record(&self, change: TextEditChangeType) {
        if change != TextEditChangeType_Composition
            && change != TextEditChangeType_CompositionFinalized
        {
            return;
        }
        // Keep the event order and a revision in one atomic value. A new composition
        // immediately revokes the prior finalization, including during a range read.
        let mut old = self.stamp();
        loop {
            let next = (old.wrapping_add(4) & !3) | change.0 as u64;
            match self
                .0
                .compare_exchange_weak(old, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(current) => old = current,
            }
        }
    }
    fn stamp(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
    fn composing(provider_active: bool, stamp: u64) -> bool {
        // QQ can retain the previous nonempty composition range after Finalized.
        // Only an explicit event on THIS subscribed editor may override that range.
        provider_active && stamp & 3 != TextEditChangeType_CompositionFinalized.0 as u64
    }
}

#[derive(Default)]
struct CompositionRetry {
    until: Option<Instant>,
    next: Option<Instant>,
}
impl CompositionRetry {
    fn arm(&mut self, now: Instant) {
        self.until = Some(now + Duration::from_secs(2));
        self.next = Some(now + Duration::from_millis(100));
    }
    fn cancel(&mut self) {
        self.until = None;
        self.next = None;
    }
    fn take_due(&mut self, now: Instant) -> bool {
        if self.until.is_some_and(|end| now >= end) {
            self.cancel();
            return false;
        }
        if self.next.is_some_and(|next| now >= next) {
            self.next = Some(now + Duration::from_millis(100));
            true
        } else {
            false
        }
    }
}

/// A focus notification may precede the provider's editor tree. Retry metadata
/// attachment only; never restart this budget from a retry or poll indefinitely.
#[derive(Default)]
struct FocusRetry {
    pid: u32,
    next: Option<Instant>,
    attempt: usize,
}
impl FocusRetry {
    const DELAYS_MS: [u64; 4] = [150, 350, 750, 1500];

    fn arm(&mut self, pid: u32, now: Instant) {
        self.pid = pid;
        self.attempt = 0;
        self.next = Some(now + Duration::from_millis(Self::DELAYS_MS[0]));
    }
    fn cancel(&mut self) {
        self.next = None;
    }
    fn take_due(&mut self, now: Instant, foreground: u32) -> bool {
        if self.pid != foreground {
            self.cancel();
        }
        if !self.next.is_some_and(|at| now >= at) {
            return false;
        }
        self.attempt += 1;
        self.next = Self::DELAYS_MS
            .get(self.attempt)
            .map(|delay| now + Duration::from_millis(*delay));
        true
    }
}

struct Target {
    context: String,
    element: IUIAutomationElement,
    focus_anchor: IUIAutomationElement,
    automation: IUIAutomation,
    custom: bool,
    composition: Arc<CompositionState>,
    pattern: IUIAutomationTextPattern,
    pattern2: Option<IUIAutomationTextPattern2>,
    edit: IUIAutomationTextEditPattern,
    handler: IUIAutomationEventHandler,
    composition_handlers: Vec<IUIAutomationTextEditTextChangedEventHandler>,
    alive: Arc<AtomicBool>,
    pid: u32,
}
struct Snapshot {
    text: String,
    composing: bool,
    caret: Option<RECT>,
}

pub fn run(shared: Arc<Shared>, rx: Receiver<()>, jobs: JobSender) {
    // COM lifetime must outlive every UIA interface, including callbacks.
    let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if init.is_err() {
        shared.status("UIA COM 初始化失败");
        return;
    }
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _com = Com;
    let result = unsafe { worker(shared.clone(), rx, jobs) };
    if result.is_err() {
        shared.status("UIA 初始化或监听失败，已安全停止采集");
    }
}

unsafe fn worker(
    shared: Arc<Shared>,
    rx: Receiver<()>,
    jobs: JobSender,
) -> windows::core::Result<()> {
    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
    struct Subscriptions(IUIAutomation);
    impl Drop for Subscriptions {
        fn drop(&mut self) {
            unsafe {
                let _ = self.0.RemoveAllEventHandlers();
            }
        }
    }
    // Also remove every handler if an intermediate registration or removal call fails.
    let _subscriptions = Subscriptions(automation.clone());
    // Providers that support IUIAutomation2 get bounded cross-process call timeouts.
    if let Ok(a2) = automation.cast::<IUIAutomation2>() {
        let _ = a2.SetConnectionTimeout(1500);
        let _ = a2.SetTransactionTimeout(1500);
    }
    let focus: IUIAutomationFocusChangedEventHandler = FocusHandler {
        shared: shared.clone(),
    }
    .into();
    let mut subscribed = false;
    let mut target: Option<Target> = None;
    let mut trigger = Trigger::default();
    let mut was_composing = false;
    let mut composition_retry = CompositionRetry::default();
    let mut focus_retry = FocusRetry::default();
    let mut cfg = shared.config();
    loop {
        if shared.stopping.load(Ordering::Acquire) {
            break;
        }
        if shared.reconfigure.swap(false, Ordering::AcqRel) {
            composition_retry.cancel();
            focus_retry.cancel();
            detach(&automation, &mut target);
            trigger = Trigger::default();
            if subscribed {
                automation.RemoveFocusChangedEventHandler(&focus)?;
                subscribed = false;
            }
            cfg = shared.config();
            if cfg.enabled {
                automation.AddFocusChangedEventHandler(None, &focus)?;
                subscribed = true;
                shared.focus_dirty.store(true, Ordering::Release);
            } else {
                shared.status("已暂停：没有注册 UIA 焦点或文本监听");
            }
        }
        let focus_changed = shared.focus_dirty.swap(false, Ordering::AcqRel);
        if focus_changed {
            focus_retry.cancel();
        }
        let recovering_focus =
            cfg.enabled && focus_retry.take_due(Instant::now(), foreground_pid());
        if cfg.enabled
            && (focus_changed || recovering_focus)
            && !target
                .as_ref()
                .is_some_and(|t| still_focused(&automation, t, &cfg))
        {
            if focus_changed {
                let pid = foreground_pid();
                if pid != GetCurrentProcessId() && allowed_process(pid, &cfg) {
                    focus_retry.arm(pid, Instant::now());
                }
            } else if recovering_focus {
                shared.metadata_event(&format!(
                    "焦点恢复尝试 {}/{}（仅检查授权前台控件）",
                    focus_retry.attempt,
                    FocusRetry::DELAYS_MS.len()
                ));
            }
            if let Some(old) = target.as_ref() {
                let focused = automation.GetFocusedElement().ok();
                shared.metadata_event(&format!(
                    "重新附加：旧 PID={}，IME 版本={}；新焦点 PID={:?}，Type={:?}，前台 PID={}",
                    old.pid,
                    old.composition.stamp(),
                    focused.as_ref().and_then(|e| e.CurrentProcessId().ok()),
                    focused
                        .as_ref()
                        .and_then(|e| e.CurrentControlType().ok())
                        .map(|v| v.0),
                    foreground_pid()
                ));
            }
            detach(&automation, &mut target);
            trigger = Trigger::default();
            was_composing = false;
            composition_retry.cancel();
            shared.text_dirty.store(false, Ordering::Release);
            shared.selection_dirty.store(false, Ordering::Release);
            match attach(&automation, &shared, &cfg) {
                Ok(t) => {
                    focus_retry.cancel();
                    match snapshot(&t, &shared) {
                        Ok(s) => {
                            trigger.baseline(&s.text);
                            was_composing = s.composing;
                            shared.authorized_status(
                                &t.context,
                                if s.composing {
                                    "正在监听：检测到拼音组合，等待提交后再读取"
                                } else {
                                    "正在监听授权编辑控件（TextPattern / TextEditPattern）"
                                },
                            );
                            target = Some(t);
                        }
                        Err(reason) => {
                            // Do not detach an authorized control for a transient selection/caret
                            // failure. Providers may publish focus before the caret has settled.
                            shared.authorized_status(&t.context, reason);
                            composition_retry.arm(Instant::now());
                            target = Some(t);
                        }
                    }
                }
                Err(reason) => shared.status(reason),
            }
        }
        let changed = shared.text_dirty.swap(false, Ordering::AcqRel);
        let moved = shared.selection_dirty.swap(false, Ordering::AcqRel);
        let retry = composition_retry.take_due(Instant::now());
        if let Some(t) = target.as_ref() {
            if changed || moved || retry {
                if !still_focused(&automation, t, &cfg) {
                    composition_retry.cancel();
                    shared.focus_dirty.store(true, Ordering::Release);
                    trigger.cancel();
                } else {
                    if (changed || moved) && was_composing {
                        composition_retry.arm(Instant::now());
                    }
                    match snapshot(t, &shared) {
                        Ok(s) => {
                            if !s.composing && (changed || retry) {
                                shared.metadata_event(&format!(
                                    "局部已提交范围：字符数={}，汉字数={}，IME 版本={}",
                                    s.text.chars().count(),
                                    s.text
                                        .chars()
                                        .filter(|c| crate::sentence::is_han(*c))
                                        .count(),
                                    t.composition.stamp()
                                ));
                            }
                            if s.composing {
                                if changed || moved {
                                    composition_retry.arm(Instant::now());
                                }
                            } else {
                                composition_retry.cancel();
                            }
                            let initialized = trigger.establish_if_missing(&s.text);
                            if initialized {
                                shared.authorized_status(
                                    &t.context,
                                    "输入基线已建立；请继续输入新句子",
                                );
                            } else if changed || was_composing {
                                if was_composing && !s.composing {
                                    trigger.composition_finished();
                                }
                                trigger.observe(&s.text, s.composing, Instant::now(), &cfg);
                            } else {
                                // Moving the caret must not translate pre-existing text.
                                // Do not overwrite the baseline: some providers deliver SelectionChanged
                                // before TextChanged for the same edit. Preserve that later text delta.
                                trigger.selection_changed(&s.text);
                            }
                            if (changed || retry) && !initialized {
                                shared.authorized_status(
                                    &t.context,
                                    if s.composing {
                                        "检测到拼音组合，等待文字提交后再读取"
                                    } else if trigger.deadline().is_some() {
                                        "已检测到提交文本，等待稳定触发"
                                    } else {
                                        "已检测到变化：组合输入、字数不足或重复句子"
                                    },
                                );
                            }
                            was_composing = s.composing;
                        }
                        Err(reason) => {
                            trigger.cancel();
                            shared.authorized_status(&t.context, reason);
                        }
                    }
                }
            }
            if trigger.deadline().is_some_and(|at| Instant::now() >= at) {
                let epoch = shared.epoch.load(Ordering::Acquire);
                if still_focused(&automation, t, &cfg) {
                    match snapshot(t, &shared) {
                        Ok(s) if !s.composing => {
                            // Revalidate text at expiry: delayed UIA events must not send a stale sentence.
                            trigger.observe(&s.text, false, Instant::now(), &cfg);
                            if let Some(text) = trigger.take_due(Instant::now()) {
                                if epoch == shared.epoch.load(Ordering::Acquire)
                                    && shared.enabled.load(Ordering::Acquire)
                                {
                                    let job = TranslationJob {
                                        epoch,
                                        text,
                                        caret: s.caret,
                                        pid: t.pid,
                                        config: cfg.clone(),
                                    };
                                    if !jobs.send(job) {
                                        shared.status("翻译线程不可用：请退出后重启");
                                    }
                                }
                            }
                        }
                        Ok(_) => trigger.cancel(),
                        Err(reason) => {
                            trigger.cancel();
                            shared.authorized_status(&t.context, reason);
                        }
                    }
                } else {
                    trigger.cancel();
                    shared.focus_dirty.store(true, Ordering::Release);
                }
            }
        }
        if shared.focus_dirty.load(Ordering::Acquire) && cfg.enabled {
            continue;
        }
        let wait = trigger
            .deadline()
            .into_iter()
            .chain(composition_retry.next)
            .chain(focus_retry.next)
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()));
        match wait {
            Some(d) => {
                let _ = rx.recv_timeout(d.max(Duration::from_millis(1)));
            }
            None => {
                if rx.recv().is_err() {
                    break;
                }
            }
        }
    }
    detach(&automation, &mut target);
    if subscribed {
        automation.RemoveFocusChangedEventHandler(&focus)?;
    }
    automation.RemoveAllEventHandlers()?;
    shared.status("UIA 监听器已释放");
    Ok(())
}

unsafe fn detach(a: &IUIAutomation, target: &mut Option<Target>) {
    if let Some(t) = target.take() {
        t.alive.store(false, Ordering::Release);
        if !t.composition_handlers.is_empty() {
            if let Ok(a3) = a.cast::<IUIAutomation3>() {
                for handler in &t.composition_handlers {
                    let _ = a3.RemoveTextEditTextChangedEventHandler(&t.element, handler);
                }
            }
        }
        let _ = a.RemoveAutomationEventHandler(UIA_Text_TextChangedEventId, &t.element, &t.handler);
        let _ = a.RemoveAutomationEventHandler(
            UIA_Text_TextSelectionChangedEventId,
            &t.element,
            &t.handler,
        );
    }
}

unsafe fn attach(
    a: &IUIAutomation,
    shared: &Arc<Shared>,
    cfg: &Config,
) -> Result<Target, &'static str> {
    let anchor = a
        .GetFocusedElement()
        .map_err(|_| "当前没有可访问的焦点控件")?;
    let pid = anchor
        .CurrentProcessId()
        .map_err(|_| "跳过：进程不可识别")? as u32;
    if pid == GetCurrentProcessId() {
        return Err("本工具设置窗口不采集；请查看最近授权应用");
    }
    if !allowed_process(pid, cfg) {
        return Err("跳过：当前进程未授权");
    }
    let label = process_label(pid);
    if foreground_pid() != pid {
        shared.authorized_status(&label, "等待授权应用进入前台");
        return Err("等待授权应用进入前台");
    }
    if let Err(reason) = safe_focus(&anchor, cfg) {
        shared.authorized_status(&label, reason);
        return Err(reason);
    }
    let mut trace = Vec::new();
    let resolution = resolve_focused_editor(a, &anchor, pid, &mut trace);
    let (e, source) = resolution.inspect_err(|reason| {
        shared.authorized_status(
            &format!(
                "{label}；未定位到输入框，当前焦点对象 {}",
                editor_metadata(&anchor)
            ),
            &format!("{reason}\r\n{}", trace.join("\r\n")),
        );
    })?;
    for event in trace {
        shared.metadata_event(&event);
    }
    safe_focus(&e, cfg).inspect_err(|reason| {
        shared.authorized_status(&label, reason);
    })?;
    let kind = e.CurrentControlType().map_err(|_| "跳过：控件类型未知")?;
    let p: windows::core::Result<IUIAutomationTextPattern> =
        e.GetCurrentPatternAs(UIA_TextPatternId);
    // If composition state cannot be established, do not guess that visible text is committed.
    let edit: windows::core::Result<IUIAutomationTextEditPattern> =
        e.GetCurrentPatternAs(UIA_TextEditPatternId);
    let pattern2: Option<IUIAutomationTextPattern2> =
        e.GetCurrentPatternAs(UIA_TextPattern2Id).ok();
    let context = format!(
        "{label}；定位={source}；Type={}({})，Text={}，Text2={}，TextEdit={}",
        kind.0,
        control_type_name(kind),
        pattern_support(&p),
        if pattern2.is_some() { "有" } else { "无" },
        pattern_support(&edit)
    );
    let report = |reason: &'static str| {
        shared.authorized_status(&context, reason);
        reason
    };
    let p = p.map_err(|_| report("跳过：控件没有 TextPattern；当前应用版本可能未暴露文本接口"))?;
    let edit = edit.map_err(|_| {
        report("跳过：没有 TextEditPattern，无法可靠判断拼音组合状态；仅延长停顿不能解决")
    })?;
    let custom = kind == UIA_GroupControlTypeId || kind == UIA_CustomControlTypeId;
    if custom
        && !e
            .CurrentIsKeyboardFocusable()
            .map_err(|_| report("跳过：自定义编辑器焦点能力未知"))?
            .as_bool()
    {
        return Err(report("跳过：自定义控件不可接收键盘焦点"));
    }
    let alive = Arc::new(AtomicBool::new(true));
    let composition = Arc::new(CompositionState::default());
    let a3 = if custom {
        Some(
            a.cast::<IUIAutomation3>()
                .map_err(|_| report("无法监听自定义编辑器的组合输入事件"))?,
        )
    } else {
        None
    };
    let handler: IUIAutomationEventHandler = TextHandler {
        shared: shared.clone(),
        alive: alive.clone(),
    }
    .into();
    a.AddAutomationEventHandler(
        UIA_Text_TextChangedEventId,
        &e,
        TreeScope_Element,
        None,
        &handler,
    )
    .map_err(|_| report("无法注册文本变化监听"))?;
    if a.AddAutomationEventHandler(
        UIA_Text_TextSelectionChangedEventId,
        &e,
        TreeScope_Element,
        None,
        &handler,
    )
    .is_err()
    {
        alive.store(false, Ordering::Release);
        let _ = a.RemoveAutomationEventHandler(UIA_Text_TextChangedEventId, &e, &handler);
        return Err(report("无法注册光标变化监听"));
    }
    let mut composition_handlers = Vec::new();
    if let Some(a3) = a3 {
        for change in [
            TextEditChangeType_Composition,
            TextEditChangeType_CompositionFinalized,
        ] {
            let h: IUIAutomationTextEditTextChangedEventHandler = CompositionHandler {
                shared: shared.clone(),
                alive: alive.clone(),
                composition: composition.clone(),
            }
            .into();
            if a3
                .AddTextEditTextChangedEventHandler(&e, TreeScope_Element, change, None, &h)
                .is_err()
            {
                alive.store(false, Ordering::Release);
                for registered in &composition_handlers {
                    let _ = a3.RemoveTextEditTextChangedEventHandler(&e, registered);
                }
                let _ = a.RemoveAutomationEventHandler(UIA_Text_TextChangedEventId, &e, &handler);
                let _ = a.RemoveAutomationEventHandler(
                    UIA_Text_TextSelectionChangedEventId,
                    &e,
                    &handler,
                );
                return Err(report("无法注册自定义编辑器的拼音组合/提交事件"));
            }
            composition_handlers.push(h);
        }
    }
    Ok(Target {
        context,
        element: e,
        focus_anchor: anchor,
        automation: a.clone(),
        custom,
        composition,
        pattern: p,
        pattern2,
        edit,
        handler,
        composition_handlers,
        alive,
        pid,
    })
}

fn text_role(kind: UIA_CONTROLTYPE_ID) -> bool {
    [
        UIA_EditControlTypeId,
        UIA_DocumentControlTypeId,
        UIA_GroupControlTypeId,
        UIA_CustomControlTypeId,
    ]
    .contains(&kind)
}

fn control_type_name(kind: UIA_CONTROLTYPE_ID) -> &'static str {
    [
        (UIA_ButtonControlTypeId, "按钮"),
        (UIA_EditControlTypeId, "编辑框"),
        (UIA_DocumentControlTypeId, "文档"),
        (UIA_GroupControlTypeId, "分组"),
        (UIA_CustomControlTypeId, "自定义"),
        (UIA_WindowControlTypeId, "窗口"),
        (UIA_PaneControlTypeId, "面板"),
    ]
    .into_iter()
    .find_map(|(id, name)| (kind == id).then_some(name))
    .unwrap_or("其他控件")
}

fn pattern_support<T>(value: &windows::core::Result<T>) -> String {
    match value {
        Ok(_) => "有".into(),
        // windows-core turns a successful COM call with a null interface into
        // Error::empty(). Zero is not an HRESULT failure or an access denial.
        Err(error) if error.code().is_ok() => "无（未返回接口）".into(),
        Err(error) => format!("无(0x{:08X})", error.code().0 as u32),
    }
}

/// Metadata only. Do not query Name, Value or text to explain a rejected focus.
unsafe fn editor_metadata(e: &IUIAutomationElement) -> String {
    let kind = e.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(-1));
    let text: windows::core::Result<IUIAutomationTextPattern> =
        e.GetCurrentPatternAs(UIA_TextPatternId);
    let edit: windows::core::Result<IUIAutomationTextEditPattern> =
        e.GetCurrentPatternAs(UIA_TextEditPatternId);
    let bounded = |value: windows::core::Result<windows::core::BSTR>| {
        value
            .map(|s| {
                s.to_string()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(64)
                    .collect::<String>()
            })
            .unwrap_or_else(|_| "未知".into())
    };
    format!(
        "Type={}({})，Class={}，Framework={}，Text={}，TextEdit={}",
        kind.0,
        control_type_name(kind),
        bounded(e.CurrentClassName()),
        bounded(e.CurrentFrameworkId()),
        pattern_support(&text),
        pattern_support(&edit)
    )
}

#[derive(Debug, PartialEq, Eq)]
enum EditorResolutionError {
    Missing(&'static str),
    Rejected(&'static str),
}
impl EditorResolutionError {
    fn reason(&self) -> &'static str {
        match self {
            Self::Missing(reason) | Self::Rejected(reason) => reason,
        }
    }
    fn permits_native_lookup(&self) -> bool {
        matches!(self, Self::Missing(_))
    }
}

fn unique_text_role(roles: &[UIA_CONTROLTYPE_ID]) -> Result<usize, EditorResolutionError> {
    let mut matching = roles
        .iter()
        .enumerate()
        .filter(|(_, kind)| text_role(**kind));
    let first = matching
        .next()
        .map(|(i, _)| i)
        .ok_or(EditorResolutionError::Missing(
            "应用仅报告外层窗口焦点，没有可确认的内部文本编辑器",
        ))?;
    if matching.next().is_some() {
        return Err(EditorResolutionError::Rejected(
            "跳过：多个内部编辑器报告焦点，无法安全选择",
        ));
    }
    Ok(first)
}

/// Some Chromium hosts report the native Window as global focus, while its single
/// logical editor reports HasKeyboardFocus. Query only metadata for that focus.
unsafe fn resolve_focused_editor(
    a: &IUIAutomation,
    anchor: &IUIAutomationElement,
    pid: u32,
    trace: &mut Vec<String>,
) -> Result<(IUIAutomationElement, &'static str), &'static str> {
    resolve_focused_editor_with_native(a, anchor, pid, trace, || native_focus_element(a, pid))
}

unsafe fn resolve_focused_editor_with_native(
    a: &IUIAutomation,
    anchor: &IUIAutomationElement,
    pid: u32,
    trace: &mut Vec<String>,
    native_lookup: impl FnOnce() -> Option<IUIAutomationElement>,
) -> Result<(IUIAutomationElement, &'static str), &'static str> {
    let failure = match focused_editor_in(a, anchor, pid, trace) {
        Ok(editor) => return Ok((editor, "UIA")),
        Err(error) => error,
    };
    trace.push(format!("UIA 定位：{}", failure.reason()));
    if !failure.permits_native_lookup() {
        return Err(failure.reason());
    }
    // A stale logical Button is never treated as an editor. Independently check
    // the native keyboard-focus HWND after an empty Window/Pane lookup too.
    let Some(native) = native_lookup() else {
        trace.push("原生定位：没有通过同进程、同窗口和密码属性校验的原生焦点".into());
        return Err(failure.reason());
    };
    trace.push(format!("原生焦点对象 {}", editor_metadata(&native)));
    editor_from_native_focus(a, anchor, &native, pid, trace)
}

unsafe fn editor_from_native_focus(
    a: &IUIAutomation,
    anchor: &IUIAutomationElement,
    native: &IUIAutomationElement,
    pid: u32,
    trace: &mut Vec<String>,
) -> Result<(IUIAutomationElement, &'static str), &'static str> {
    if a.CompareElements(anchor, native)
        .map_err(|_| "跳过：无法校验原生焦点")?
        .as_bool()
    {
        return Err(
            "跳过：原生焦点与外层 UIA 焦点相同，仍未定位到文本输入框；请单击聊天输入框后重试",
        );
    }
    focused_editor_in(a, native, pid, trace)
        .map(|editor| (editor, "UIA+Win32焦点"))
        .map_err(|error| error.reason())
}

unsafe fn native_focus_element(a: &IUIAutomation, pid: u32) -> Option<IUIAutomationElement> {
    native_focus_element_in(a, pid, GetForegroundWindow())
}

/// Production always supplies the current foreground HWND. An explicit HWND
/// also lets the native lookup be tested on a window we own without taking over
/// the user's foreground application.
unsafe fn native_focus_element_in(
    a: &IUIAutomation,
    pid: u32,
    foreground: HWND,
) -> Option<IUIAutomationElement> {
    let mut foreground_process = 0;
    let tid = GetWindowThreadProcessId(foreground, Some(&mut foreground_process));
    if tid == 0 || foreground_process != pid {
        return None;
    }
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    GetGUIThreadInfo(tid, &mut info).ok()?;
    let mut focus_process = 0;
    GetWindowThreadProcessId(info.hwndFocus, Some(&mut focus_process));
    if info.hwndFocus == HWND::default()
        || focus_process != pid
        || GetAncestor(info.hwndFocus, GA_ROOT) != GetAncestor(foreground, GA_ROOT)
    {
        return None;
    }
    let native = a.ElementFromHandle(info.hwndFocus).ok()?;
    if native.CurrentProcessId().ok() != Some(pid as i32)
        || native.CurrentIsPassword().ok()?.as_bool()
    {
        return None;
    }
    Some(native)
}

unsafe fn focused_editor_in(
    a: &IUIAutomation,
    anchor: &IUIAutomationElement,
    pid: u32,
    trace: &mut Vec<String>,
) -> Result<IUIAutomationElement, EditorResolutionError> {
    use EditorResolutionError::{Missing, Rejected};
    let kind = anchor
        .CurrentControlType()
        .map_err(|_| Rejected("跳过：控件类型未知"))?;
    if text_role(kind) {
        return Ok(anchor.clone());
    }
    if kind != UIA_WindowControlTypeId && kind != UIA_PaneControlTypeId {
        return Err(Missing(
            "跳过：当前焦点不是受支持的文本编辑控件；请单击聊天输入框后输入新句子",
        ));
    }
    let focused = a
        .CreatePropertyCondition(
            UIA_HasKeyboardFocusPropertyId,
            &windows::core::VARIANT::from(true),
        )
        .map_err(|_| Rejected("无法检查内部输入焦点"))?;
    let same_process = a
        .CreatePropertyCondition(
            UIA_ProcessIdPropertyId,
            &windows::core::VARIANT::from(pid as i32),
        )
        .map_err(|_| Rejected("无法检查内部进程身份"))?;
    let condition = a
        .CreateAndCondition(&focused, &same_process)
        .map_err(|_| Rejected("无法检查内部输入焦点"))?;
    let found = anchor
        .FindAll(TreeScope_Descendants, &condition)
        .map_err(|_| Missing("应用未提供内部输入焦点信息"))?;
    let n = found.Length().map_err(|_| Rejected("内部焦点数量未知"))?;
    trace.push(format!(
        "{}({}) 的同进程内部焦点数={n}",
        kind.0,
        control_type_name(kind)
    ));
    if n > 8 {
        return Err(Rejected("跳过：应用报告了过多焦点，无法确定输入框"));
    }
    let mut candidates = Vec::new();
    let mut roles = Vec::new();
    for i in 0..n {
        let child = found
            .GetElement(i)
            .map_err(|_| Rejected("内部焦点不可访问"))?;
        if child
            .CurrentIsPassword()
            .map_err(|_| Rejected("跳过：内部控件密码属性未知"))?
            .as_bool()
        {
            return Err(Rejected("跳过：密码控件"));
        }
        let role = child
            .CurrentControlType()
            .map_err(|_| Rejected("内部控件类型未知"))?;
        if child.CurrentProcessId().ok() != Some(pid as i32)
            || !child
                .CurrentHasKeyboardFocus()
                .map_err(|_| Rejected("内部输入焦点状态未知"))?
                .as_bool()
        {
            continue;
        }
        roles.push(role);
        candidates.push(child);
    }
    trace.push(format!(
        "内部焦点类型={:?}；文本候选数={}",
        roles.iter().map(|id| id.0).collect::<Vec<_>>(),
        roles.iter().filter(|id| text_role(**id)).count()
    ));
    let index = unique_text_role(&roles)?;
    Ok(candidates.swap_remove(index))
}

unsafe fn process_label(pid: u32) -> String {
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return format!("授权进程 PID {pid}");
    };
    let mut path = [0u16; 32768];
    let mut n = path.len() as u32;
    let result = QueryFullProcessImageNameW(
        process,
        PROCESS_NAME_WIN32,
        PWSTR(path.as_mut_ptr()),
        &mut n,
    );
    let elevation = |value: Option<bool>| match value {
        Some(true) => "是",
        Some(false) => "否",
        None => "未知",
    };
    let target_elevation = process_elevated(process);
    let tool_elevation = process_elevated(GetCurrentProcess());
    let _ = CloseHandle(process);
    let name = if result.is_ok() {
        String::from_utf16_lossy(&path[..n as usize])
            .rsplit('\\')
            .next()
            .unwrap_or("授权进程")
            .to_owned()
    } else {
        "授权进程".into()
    };
    format!(
        "{name}（PID {pid}；进程提升={}；工具提升={}）",
        elevation(target_elevation),
        elevation(tool_elevation)
    )
}

unsafe fn process_elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut size = 0;
    let result = GetTokenInformation(
        token,
        TokenElevation,
        Some(&mut elevation as *mut _ as *mut _),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut size,
    );
    let _ = CloseHandle(token);
    result.ok()?;
    Some(elevation.TokenIsElevated != 0)
}

unsafe fn safe_focus(e: &IUIAutomationElement, cfg: &Config) -> Result<u32, &'static str> {
    let pid = e.CurrentProcessId().map_err(|_| "跳过：进程不可识别")? as u32;
    if !allowed_process(pid, cfg) {
        return Err("跳过：当前进程未授权");
    }
    if e.CurrentIsPassword()
        .map_err(|_| "跳过：密码属性未知")?
        .as_bool()
    {
        return Err("跳过：密码控件");
    }
    if !e
        .CurrentHasKeyboardFocus()
        .map_err(|_| "跳过：焦点属性未知")?
        .as_bool()
        || !e
            .CurrentIsEnabled()
            .map_err(|_| "跳过：控件状态未知")?
            .as_bool()
    {
        return Err("跳过：控件未获得输入焦点");
    }
    Ok(pid)
}
unsafe fn still_focused(a: &IUIAutomation, t: &Target, cfg: &Config) -> bool {
    if foreground_pid() != t.pid {
        return false;
    }
    a.GetFocusedElement().ok().is_some_and(|e| {
        let matches = a
            .CompareElements(&e, &t.element)
            .ok()
            .is_some_and(|same| same.as_bool())
            || a.CompareElements(&e, &t.focus_anchor)
                .ok()
                .is_some_and(|same| same.as_bool());
        matches && safe_focus(&t.element, cfg).is_ok()
    })
}

/// # Safety
/// The returned PID is transient; revalidate it before accessing another process.
pub unsafe fn foreground_pid() -> u32 {
    let mut pid = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
    pid
}

/// # Safety
/// Treat the result as transient authorization; process identity can disappear after return.
pub unsafe fn allowed_process(pid: u32, cfg: &Config) -> bool {
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return false;
    };
    struct Process(windows::Win32::Foundation::HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let process = Process(process);
    let mut path = [0u16; 32768];
    let mut len = path.len() as u32;
    if QueryFullProcessImageNameW(
        process.0,
        PROCESS_NAME_WIN32,
        PWSTR(path.as_mut_ptr()),
        &mut len,
    )
    .is_err()
    {
        return false;
    }
    let path = String::from_utf16_lossy(&path[..len as usize]);
    if cfg
        .allowed_executables
        .iter()
        .any(|p| p.eq_ignore_ascii_case(&path))
    {
        return true;
    }
    if !cfg.allow_notepad || !path.to_ascii_lowercase().ends_with("\\notepad.exe") {
        return false;
    }
    let mut system = [0u16; 32768];
    let nsystem = windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut system));
    if nsystem > 0 && (nsystem as usize) < system.len() {
        let expected =
            std::path::PathBuf::from(String::from_utf16_lossy(&system[..nsystem as usize]))
                .join("notepad.exe");
        if path.eq_ignore_ascii_case(&expected.to_string_lossy()) {
            return true;
        }
    }
    // Package family identity is supplied by Windows, not by a spoofable window title.
    let mut family = [0u16; 256];
    let mut n = family.len() as u32;
    GetPackageFamilyName(process.0, &mut n, PWSTR(family.as_mut_ptr())) == ERROR_SUCCESS
        && n > 0
        && String::from_utf16_lossy(&family[..n as usize - 1])
            == "Microsoft.WindowsNotepad_8wekyb3d8bbwe"
}

unsafe fn snapshot(t: &Target, shared: &Shared) -> Result<Snapshot, &'static str> {
    let composition_stamp = t.composition.stamp();
    if !shared.enabled.load(Ordering::Acquire)
        || shared.stopping.load(Ordering::Acquire)
        || shared.reconfigure.load(Ordering::Acquire)
    {
        return Err("已暂停或正在重新配置，跳过文本读取");
    }
    if foreground_pid() != t.pid {
        return Err("焦点已切换，跳过文本读取");
    }
    if t.element
        .CurrentIsPassword()
        .map_err(|_| "跳过：密码状态不可读取")?
        .as_bool()
    {
        return Err("跳过：密码控件");
    }
    // The projected GetActiveComposition treats S_OK + null (no IME composition) as an error.
    // Inspect the documented nullable COM out pointer while preserving HRESULT failures.
    let mut raw = std::ptr::null_mut();
    (Interface::vtable(&t.edit).GetActiveComposition)(Interface::as_raw(&t.edit), &mut raw)
        .ok()
        .map_err(|_| "跳过：拼音组合状态不可读取")?;
    let provider_active = !raw.is_null();
    if provider_active {
        drop(IUIAutomationTextRange::from_raw(raw));
    }
    if CompositionState::composing(provider_active, composition_stamp) {
        return Ok(Snapshot {
            text: String::new(),
            composing: true,
            caret: None,
        });
    }
    let caret = if let Some(p2) = &t.pattern2 {
        let mut active = BOOL(0);
        let range = p2
            .GetCaretRange(&mut active)
            .map_err(|_| "跳过：光标范围不可读取")?;
        if !active.as_bool() {
            return Err("跳过：光标不活动");
        }
        range
    } else {
        let selected = t
            .pattern
            .GetSelection()
            .map_err(|_| "跳过：没有光标或选区信息")?;
        let count = selected.Length().map_err(|_| "跳过：选区未知")?;
        let r = match count {
            1 => selected.GetElement(0).map_err(|_| "跳过：选区不可访问")?,
            0 if t.custom => {
                // Chromium may expose a visible MSAA caret without a UIA selection.
                // Never infer it from the mouse, the document end, or a selected range.
                let rect = accessible_caret(t.pid)
                    .ok_or("跳过：UIA 没有选区，且系统未提供可验证的插入光标")?;
                let bounds = t
                    .element
                    .CurrentBoundingRectangle()
                    .map_err(|_| "跳过：无法验证光标所在的输入框")?;
                if !caret_inside_editor(&rect, &bounds) {
                    return Err("跳过：系统光标不在当前输入框内");
                }
                t.pattern
                    .RangeFromPoint(windows::Win32::Foundation::POINT {
                        x: rect.left,
                        y: ((i64::from(rect.top) + i64::from(rect.bottom)) / 2) as i32,
                    })
                    .map_err(|_| "跳过：输入框无法将系统光标映射为文本范围")?
            }
            0 => return Err("跳过：UIA 没有返回光标或选区"),
            _ => return Err("跳过：多重选区"),
        };
        if r.CompareEndpoints(
            TextPatternRangeEndpoint_Start,
            &r,
            TextPatternRangeEndpoint_End,
        )
        .map_err(|_| "跳过：选区未知")?
            != 0
        {
            return Err("等待插入光标：UIA 选区未折叠。请单击输入框末尾取消选中，再输入新句子");
        }
        r
    };
    let attr = caret
        .GetAttributeValue(UIA_IsReadOnlyAttributeId)
        .map_err(|_| "跳过：只读属性未知")?;
    if bool::try_from(&attr).map_err(|_| "跳过：只读属性不确定")? {
        return Err("跳过：只读文本");
    }
    if t.custom && !range_is_within_editor(&t.automation, &t.element, t.pid, &caret) {
        return Err("跳过：自定义控件的光标范围不属于当前输入框");
    }
    let anchor = caret_rect(&caret).or_else(|| system_caret());
    let range = caret.Clone().map_err(|_| "跳过：无法复制光标范围")?;
    range
        .MoveEndpointByRange(
            TextPatternRangeEndpoint_End,
            &caret,
            TextPatternRangeEndpoint_Start,
        )
        .map_err(|_| "跳过：无法定位光标")?;
    range
        .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -302)
        .map_err(|_| "跳过：控件不支持局部文本访问")?;
    if t.custom {
        // DocumentRange is a range descriptor, NOT a body read. A web editor's
        // character navigation may cross node boundaries; clamp to its own subtree.
        let bounds = t
            .pattern
            .DocumentRange()
            .map_err(|_| "跳过：自定义输入框没有独立文本边界")?;
        if !range_is_within_editor(&t.automation, &t.element, t.pid, &bounds) {
            return Err("跳过：自定义输入框的文本边界包含外部页面");
        }
        if range
            .CompareEndpoints(
                TextPatternRangeEndpoint_End,
                &bounds,
                TextPatternRangeEndpoint_End,
            )
            .map_err(|_| "跳过：无法确认输入范围")?
            > 0
        {
            // The caret was already verified as belonging to this editor. Some
            // providers expose its trailing insertion position past DocumentRange.
            // Read only the intersection with the verified editor bounds; never
            // expand bounds or read the outside portion to compensate.
            range
                .MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &bounds,
                    TextPatternRangeEndpoint_End,
                )
                .map_err(|_| "跳过：无法限制局部范围终点")?;
        }
        if range
            .CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &bounds,
                TextPatternRangeEndpoint_Start,
            )
            .map_err(|_| "跳过：无法确认输入范围")?
            < 0
        {
            range
                .MoveEndpointByRange(
                    TextPatternRangeEndpoint_Start,
                    &bounds,
                    TextPatternRangeEndpoint_Start,
                )
                .map_err(|_| "跳过：无法限制局部输入范围")?;
        }
        if !range_is_within_editor(&t.automation, &t.element, t.pid, &range) {
            return Err("跳过：局部范围越过自定义输入框");
        }
    }
    // One sentinel beyond the accepted 604 UTF-16 units detects truncation safely.
    // Never use DocumentRange/ValuePattern.Value to fetch a document body.
    if !shared.enabled.load(Ordering::Acquire)
        || shared.stopping.load(Ordering::Acquire)
        || shared.reconfigure.load(Ordering::Acquire)
    {
        return Err("已暂停或正在重新配置，跳过文本读取");
    }
    if foreground_pid() != t.pid {
        return Err("焦点已切换，跳过文本读取");
    }
    if t.composition.stamp() != composition_stamp {
        return Err("拼音状态正在变化，等待输入稳定");
    }
    let text = range
        .GetText(605)
        .map_err(|_| "跳过：局部文本不可访问")?
        .to_string();
    if t.composition.stamp() != composition_stamp {
        return Err("拼音状态已变化，丢弃本次局部读取");
    }
    if text.encode_utf16().count() > 604 {
        return Err("跳过：局部文本范围超过安全上限");
    }
    Ok(Snapshot {
        text,
        composing: false,
        caret: anchor,
    })
}

/// Validate range ownership without reading its text.
/// # Safety
/// All COM objects must belong to the caller's initialized MTA; `pid` must be the editor's authorized process.
pub unsafe fn range_is_within_editor(
    automation: &IUIAutomation,
    editor: &IUIAutomationElement,
    pid: u32,
    range: &IUIAutomationTextRange,
) -> bool {
    let Ok(mut owner) = range.GetEnclosingElement() else {
        return false;
    };
    let Ok(walker) = automation.RawViewWalker() else {
        return false;
    };
    for _ in 0..=8 {
        if automation
            .CompareElements(&owner, editor)
            .ok()
            .is_some_and(|same| same.as_bool())
        {
            return true;
        }
        if owner.CurrentProcessId().ok() != Some(pid as i32) {
            return false;
        }
        let Ok(parent) = walker.GetParentElement(&owner) else {
            return false;
        };
        owner = parent;
    }
    false
}

unsafe fn caret_rect(range: &IUIAutomationTextRange) -> Option<RECT> {
    let array = range.GetBoundingRectangles().ok()?;
    if array.is_null() {
        return None;
    }
    struct Array(*mut SAFEARRAY);
    impl Drop for Array {
        fn drop(&mut self) {
            unsafe {
                let _ = SafeArrayDestroy(self.0);
            }
        }
    }
    let _owned = Array(array);
    if SafeArrayGetDim(array) != 1
        || SafeArrayGetVartype(array).ok()? != windows::Win32::System::Variant::VT_R8
    {
        return None;
    }
    let lo = SafeArrayGetLBound(array, 1).ok()?;
    let hi = SafeArrayGetUBound(array, 1).ok()?;
    if hi - lo + 1 < 4 {
        return None;
    }
    let mut ptr = std::ptr::null_mut();
    SafeArrayAccessData(array, &mut ptr).ok()?;
    let values = std::slice::from_raw_parts(ptr as *const f64, 4);
    let rect = if values.iter().all(|n| n.is_finite() && n.abs() < 1_000_000.) && values[3] > 0. {
        Some(RECT {
            left: values[0] as i32,
            top: values[1] as i32,
            right: (values[0] + values[2]).max(values[0] + 1.) as i32,
            bottom: (values[1] + values[3]) as i32,
        })
    } else {
        None
    };
    let _ = SafeArrayUnaccessData(array);
    rect
}
unsafe fn system_caret() -> Option<RECT> {
    let tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    GetGUIThreadInfo(tid, &mut info).ok()?;
    if info.hwndCaret == HWND::default() {
        return None;
    }
    let mut points = [
        windows::Win32::Foundation::POINT {
            x: info.rcCaret.left,
            y: info.rcCaret.top,
        },
        windows::Win32::Foundation::POINT {
            x: info.rcCaret.right,
            y: info.rcCaret.bottom,
        },
    ];
    // MapWindowPoints supports rectangles and physical screen coordinates in the PMv2 process.
    windows::Win32::Graphics::Gdi::MapWindowPoints(info.hwndCaret, HWND::default(), &mut points);
    Some(RECT {
        left: points[0].x,
        top: points[0].y,
        right: points[1].x,
        bottom: points[1].y,
    })
}

fn caret_inside_editor(caret: &RECT, editor: &RECT) -> bool {
    let width = i64::from(caret.right) - i64::from(caret.left);
    let height = i64::from(caret.bottom) - i64::from(caret.top);
    (1..=16).contains(&width)
        && (1..=256).contains(&height)
        && caret.left >= editor.left
        && caret.right <= editor.right
        && caret.top >= editor.top
        && caret.bottom <= editor.bottom
}

/// Query only the public MSAA caret's role, state and geometry, never Name/Value.
unsafe fn accessible_caret(pid: u32) -> Option<RECT> {
    let mut actual_pid = 0;
    let tid = GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut actual_pid));
    if actual_pid != pid || tid == 0 {
        return None;
    }
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    GetGUIThreadInfo(tid, &mut info).ok()?;
    GetWindowThreadProcessId(info.hwndFocus, Some(&mut actual_pid));
    if actual_pid != pid || info.hwndFocus == HWND::default() {
        return None;
    }
    let mut raw = std::ptr::null_mut();
    AccessibleObjectFromWindow(
        info.hwndFocus,
        OBJID_CARET.0 as u32,
        &IAccessible::IID,
        &mut raw,
    )
    .ok()?;
    if raw.is_null() {
        return None;
    }
    let caret = IAccessible::from_raw(raw);
    let child = windows::core::VARIANT::from(0i32);
    if i32::try_from(&caret.get_accRole(&child).ok()?).ok()? != ROLE_SYSTEM_CARET as i32 {
        return None;
    }
    let state = i32::try_from(&caret.get_accState(&child).ok()?).ok()? as u32;
    use windows::Win32::UI::Controls::{STATE_SYSTEM_INVISIBLE, STATE_SYSTEM_OFFSCREEN};
    if state & (STATE_SYSTEM_INVISIBLE.0 | STATE_SYSTEM_OFFSCREEN.0) != 0 {
        return None;
    }
    let (mut left, mut top, mut width, mut height) = (0, 0, 0, 0);
    caret
        .accLocation(&mut left, &mut top, &mut width, &mut height, &child)
        .ok()?;
    Some(RECT {
        left,
        top,
        right: left.checked_add(width)?,
        bottom: top.checked_add(height)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Exercise the real Win32/UIA boundary in our own empty EDIT/BUTTON window.
    /// A manufactured stale Button anchor must resolve to the native EDIT, while
    /// an actually focused Button must still be rejected. Never access user apps.
    #[test]
    #[ignore = "Creates a temporary desktop test window; queries only that window"]
    fn native_focus_lookup_distinguishes_stale_and_real_button_focus() {
        use windows::{
            core::w,
            Win32::{
                Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
                UI::Input::KeyboardAndMouse::SetFocus,
            },
        };
        const FOCUS_CONTROL: u32 = WM_APP + 49;
        unsafe extern "system" fn window_proc(
            hwnd: HWND,
            message: u32,
            wp: WPARAM,
            lp: LPARAM,
        ) -> LRESULT {
            match message {
                FOCUS_CONTROL => {
                    let _ = SetFocus(HWND(wp.0 as *mut _));
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, message, wp, lp),
            }
        }
        struct Com;
        impl Drop for Com {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        struct TestWindow(HWND, Option<std::thread::JoinHandle<()>>);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe {
                    let _ = PostMessageW(self.0, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                self.1.take().unwrap().join().unwrap();
            }
        }
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let _com = Com;
            let (tx, rx) = std::sync::mpsc::channel();
            let ui = std::thread::spawn(move || {
                let instance = HINSTANCE::default();
                let class = w!("ECL.FocusRecoveryTest");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: class,
                    ..Default::default()
                };
                assert_ne!(RegisterClassW(&wc), 0);
                let root = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("English Companion focus test"),
                    WS_OVERLAPPEDWINDOW,
                    100,
                    100,
                    420,
                    180,
                    None,
                    None,
                    instance,
                    None,
                )
                .unwrap();
                let empty_root = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("Empty focus container"),
                    WS_OVERLAPPEDWINDOW,
                    100,
                    100,
                    200,
                    100,
                    None,
                    None,
                    instance,
                    None,
                )
                .unwrap();
                let editor = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("EDIT"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    20,
                    20,
                    340,
                    30,
                    root,
                    None,
                    instance,
                    None,
                )
                .unwrap();
                let button = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("BUTTON"),
                    w!("Test button"),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    20,
                    70,
                    120,
                    30,
                    root,
                    None,
                    instance,
                    None,
                )
                .unwrap();
                let _ = ShowWindow(root, SW_SHOWNOACTIVATE);
                let _ = SetFocus(editor);
                tx.send((
                    root.0 as usize,
                    editor.0 as usize,
                    button.0 as usize,
                    empty_root.0 as usize,
                ))
                .unwrap();
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                let _ = DestroyWindow(empty_root);
                let _ = UnregisterClassW(class, instance);
            });
            let (root, editor, button, empty_root) =
                rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let root = HWND(root as *mut _);
            let _window = TestWindow(root, Some(ui));
            let pid = GetCurrentProcessId();
            let a: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).unwrap();
            let anchor = a.ElementFromHandle(HWND(button as *mut _)).unwrap();
            assert_eq!(
                anchor.CurrentControlType().unwrap(),
                UIA_ButtonControlTypeId
            );
            let expected = a.ElementFromHandle(HWND(editor as *mut _)).unwrap();
            let mut trace = Vec::new();
            let (resolved, source) =
                resolve_focused_editor_with_native(&a, &anchor, pid, &mut trace, || {
                    native_focus_element_in(&a, pid, root)
                })
                .unwrap();
            assert_eq!(source, "UIA+Win32焦点");
            assert!(a.CompareElements(&resolved, &expected).unwrap().as_bool());
            assert!(native_focus_element_in(&a, pid.wrapping_add(1), root).is_none());
            // Regression: Window/Pane with zero focused descendants must reach
            // native lookup as well, instead of returning the first failure.
            let container = a.ElementFromHandle(HWND(empty_root as *mut _)).unwrap();
            assert_eq!(
                container.CurrentControlType().unwrap(),
                UIA_WindowControlTypeId
            );
            let mut trace = Vec::new();
            let (resolved, source) =
                resolve_focused_editor_with_native(&a, &container, pid, &mut trace, || {
                    native_focus_element_in(&a, pid, root)
                })
                .unwrap();
            assert_eq!(source, "UIA+Win32焦点");
            assert!(a.CompareElements(&resolved, &expected).unwrap().as_bool());
            assert!(trace.iter().any(|line| line.contains("内部焦点数=0")));
            SendMessageW(root, FOCUS_CONTROL, WPARAM(button), LPARAM(0));
            let native = native_focus_element_in(&a, pid, root).unwrap();
            assert!(editor_from_native_focus(&a, &anchor, &native, pid, &mut Vec::new()).is_err());
        }
    }

    #[test]
    fn delayed_focus_recovers_without_another_focus_event_and_then_stops() {
        let mut retry = FocusRetry::default();
        let start = Instant::now();
        retry.arm(42, start);
        assert!(!retry.take_due(start + Duration::from_millis(149), 42));
        // A provider that remains unavailable cannot keep extending this budget.
        for elapsed in [150, 500, 1250, 2750] {
            let now = start + Duration::from_millis(elapsed);
            assert!(retry.take_due(now, 42));
            assert!(!retry.take_due(now, 42));
        }
        assert_eq!(retry.attempt, 4);
        assert!(retry.next.is_none());
        assert!(!retry.take_due(start + Duration::from_secs(60), 42));
    }
    #[test]
    fn focus_recovery_cannot_follow_an_old_process_or_survive_cancellation() {
        let mut retry = FocusRetry::default();
        let start = Instant::now();
        retry.arm(42, start);
        assert!(!retry.take_due(start + Duration::from_millis(150), 43));
        assert!(!retry.take_due(start + Duration::from_millis(500), 42));
        retry.arm(43, start);
        assert!(retry.take_due(start + Duration::from_millis(150), 43));
        // Success, pause and a fresh external focus all revoke pending attempts.
        retry.cancel();
        assert!(!retry.take_due(start + Duration::from_secs(1), 43));
        retry.arm(44, start);
        assert_eq!(retry.attempt, 0);
        assert!(retry.take_due(start + Duration::from_millis(150), 44));
    }
    #[test]
    fn missing_pattern_diagnostics_preserve_hresult_without_provider_message() {
        let missing: windows::core::Result<()> = Err(windows::core::Error::from_hresult(
            windows::core::HRESULT(0x80040204u32 as i32),
        ));
        assert_eq!(pattern_support(&missing), "无(0x80040204)");
        let absent: windows::core::Result<()> = Err(windows::core::Error::empty());
        assert_eq!(pattern_support(&absent), "无（未返回接口）");
        assert_eq!(pattern_support(&Ok(())), "有");
        assert_eq!(control_type_name(UIA_ButtonControlTypeId), "按钮");
    }
    #[test]
    fn explicit_finalized_event_resolves_stale_range_until_next_composition() {
        let state = CompositionState::default();
        assert!(CompositionState::composing(true, state.stamp()));
        assert!(!CompositionState::composing(false, state.stamp()));
        state.record(TextEditChangeType_Composition);
        let active = state.stamp();
        assert!(CompositionState::composing(true, active));
        state.record(TextEditChangeType_CompositionFinalized);
        let finalized = state.stamp();
        assert_ne!(active, finalized);
        assert!(!CompositionState::composing(true, finalized));
        state.record(TextEditChangeType_AutoCorrect);
        assert_eq!(finalized, state.stamp());
        state.record(TextEditChangeType_Composition);
        assert_ne!(finalized, state.stamp());
        assert!(CompositionState::composing(true, state.stamp()));
        // Focus attachment starts a fresh state; an old editor's finalization cannot leak.
        assert!(CompositionState::composing(
            true,
            CompositionState::default().stamp()
        ));
    }
    #[test]
    fn composition_retry_is_bounded_and_cancels_without_permanent_polling() {
        let mut retry = CompositionRetry::default();
        let now = Instant::now();
        assert!(!retry.take_due(now));
        retry.arm(now);
        assert!(!retry.take_due(now + Duration::from_millis(99)));
        assert!(retry.take_due(now + Duration::from_millis(100)));
        assert!(!retry.take_due(now + Duration::from_secs(2)));
        assert!(retry.next.is_none());
        retry.arm(now);
        retry.cancel();
        assert!(!retry.take_due(now + Duration::from_millis(100)));
    }
    #[test]
    fn custom_group_editor_is_not_filtered_as_a_nontext_control() {
        assert!(text_role(UIA_GroupControlTypeId));
        assert!(text_role(UIA_CustomControlTypeId));
        assert!(text_role(UIA_EditControlTypeId));
        assert!(text_role(UIA_DocumentControlTypeId));
        assert!(!text_role(UIA_WindowControlTypeId));
        assert!(!text_role(UIA_PaneControlTypeId));
        assert!(!text_role(UIA_ButtonControlTypeId));
    }
    #[test]
    fn focused_child_resolution_never_picks_an_arbitrary_editor() {
        assert_eq!(
            unique_text_role(&[UIA_ButtonControlTypeId, UIA_GroupControlTypeId]),
            Ok(1)
        );
        assert!(unique_text_role(&[]).is_err());
        assert!(unique_text_role(&[UIA_PaneControlTypeId]).is_err());
        assert!(unique_text_role(&[UIA_EditControlTypeId, UIA_GroupControlTypeId]).is_err());
        assert!(unique_text_role(&[]).unwrap_err().permits_native_lookup());
        assert!(
            !unique_text_role(&[UIA_EditControlTypeId, UIA_GroupControlTypeId])
                .unwrap_err()
                .permits_native_lookup()
        );
        assert!(!EditorResolutionError::Rejected("密码或属性不确定").permits_native_lookup());
    }
    #[test]
    fn accessible_caret_must_be_a_narrow_visible_rect_inside_this_editor() {
        let editor = RECT {
            left: 100,
            top: 100,
            right: 500,
            bottom: 300,
        };
        let caret = RECT {
            left: 120,
            top: 120,
            right: 121,
            bottom: 140,
        };
        assert!(caret_inside_editor(&caret, &editor));
        for invalid in [
            RECT {
                right: 120,
                ..caret
            },
            RECT {
                right: 200,
                ..caret
            },
            RECT { top: 99, ..caret },
            RECT {
                bottom: 301,
                ..caret
            },
            RECT {
                left: i32::MIN,
                right: i32::MAX,
                ..caret
            },
        ] {
            assert!(!caret_inside_editor(&invalid, &editor));
        }
    }
}
