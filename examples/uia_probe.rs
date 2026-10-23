//! Diagnostic tool: metadata only, explicitly limited to one PID. No text is fetched.
#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use windows::{
        core::Interface,
        Win32::{System::Com::*, UI::Accessibility::*},
    };
    let pid: i32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: uia_probe <authorized PID>");
    let focusable = std::env::args().any(|s| s == "--focusable");
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        {
            let a: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            if let Ok(a2) = a.cast::<IUIAutomation2>() {
                a2.SetConnectionTimeout(1500)?;
                a2.SetTransactionTimeout(1500)?;
            }
            if std::env::args().any(|s| s == "--watch-caret") {
                use windows::core::VARIANT;
                use windows::Win32::UI::WindowsAndMessaging::*;
                for _ in 0..40 {
                    let foreground = GetForegroundWindow();
                    let mut actual_pid = 0;
                    let tid = GetWindowThreadProcessId(foreground, Some(&mut actual_pid));
                    if actual_pid == pid as u32 {
                        if let Ok(e) = a.GetFocusedElement() {
                            if e.CurrentProcessId().ok() == Some(pid)
                                && e.CurrentIsPassword().ok().is_some_and(|v| !v.as_bool())
                            {
                                let selection = e
                                    .GetCurrentPatternAs::<IUIAutomationTextPattern>(
                                        UIA_TextPatternId,
                                    )
                                    .and_then(|p| p.GetSelection())
                                    .and_then(|s| s.Length());
                                let mut raw = std::ptr::null_mut();
                                if let Ok(edit) = e
                                    .GetCurrentPatternAs::<IUIAutomationTextEditPattern>(
                                        UIA_TextEditPatternId,
                                    )
                                {
                                    let _ = (Interface::vtable(&edit).GetActiveComposition)(
                                        Interface::as_raw(&edit),
                                        &mut raw,
                                    );
                                }
                                let composing = !raw.is_null();
                                if composing {
                                    drop(IUIAutomationTextRange::from_raw(raw));
                                }
                                println!("focused_type={:?}, selection_count={selection:?}, composing={composing}",e.CurrentControlType());
                            }
                        }
                        let mut info = GUITHREADINFO {
                            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                            ..Default::default()
                        };
                        if GetGUIThreadInfo(tid, &mut info).is_ok() {
                            println!(
                                "native_caret={:?}, native_rect={:?}, focus_hwnd={:?}",
                                info.hwndCaret, info.rcCaret, info.hwndFocus
                            );
                            let mut raw = std::ptr::null_mut();
                            if AccessibleObjectFromWindow(
                                info.hwndFocus,
                                OBJID_CARET.0 as u32,
                                &IAccessible::IID,
                                &mut raw,
                            )
                            .is_ok()
                                && !raw.is_null()
                            {
                                let caret = IAccessible::from_raw(raw);
                                let child = VARIANT::from(0i32);
                                let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
                                let location =
                                    caret.accLocation(&mut x, &mut y, &mut w, &mut h, &child);
                                println!("msaa_caret_location={location:?}, rect=({x},{y},{w},{h}), role={:?}, state={:?}",caret.get_accRole(&child).and_then(|v|i32::try_from(&v)),caret.get_accState(&child).and_then(|v|i32::try_from(&v)));
                            } else {
                                println!("msaa_caret_unavailable");
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                CoUninitialize();
                return Ok(());
            }
            let focused = a.GetFocusedElement()?;
            if focused.CurrentProcessId()? == pid {
                println!(
                    "global_focus: type={}, class={}",
                    focused.CurrentControlType()?.0,
                    focused.CurrentClassName()?
                );
            } else {
                println!("global_focus: outside authorized PID");
            }
            let root = a.GetRootElement()?;
            let condition = a.CreatePropertyCondition(
                UIA_ProcessIdPropertyId,
                &windows::core::VARIANT::from(pid),
            )?;
            let windows = root.FindAll(TreeScope_Children, &condition)?;
            println!("authorized_pid={pid}, windows={}", windows.Length()?);
            for i in 0..windows.Length()? {
                let window = windows.GetElement(i)?;
                let all = window.FindAll(TreeScope_Descendants, &a.CreateTrueCondition()?)?;
                for j in -1..all.Length()? {
                    let e = if j < 0 {
                        window.clone()
                    } else {
                        all.GetElement(j)?
                    };
                    if e.CurrentProcessId()? != pid {
                        continue;
                    }
                    let kind = e.CurrentControlType()?;
                    if kind != UIA_EditControlTypeId
                        && kind != UIA_DocumentControlTypeId
                        && !(focusable
                            && (e
                                .CurrentIsKeyboardFocusable()
                                .ok()
                                .is_some_and(|v| v.as_bool())
                                || e.CurrentHasKeyboardFocus()
                                    .ok()
                                    .is_some_and(|v| v.as_bool())))
                    {
                        continue;
                    }
                    if e.CurrentIsPassword()?.as_bool() {
                        println!("type={}, password=true (skipped)", kind.0);
                        continue;
                    }
                    let p = e.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId);
                    let p2 = e.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id);
                    let edit = e
                        .GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId);
                    if kind != UIA_EditControlTypeId
                        && kind != UIA_DocumentControlTypeId
                        && p.is_err()
                        && edit.is_err()
                        && !e.CurrentHasKeyboardFocus()?.as_bool()
                    {
                        continue;
                    }
                    println!(
                        "type={}, class={}, password={:?}, text={}, text2={}, textedit={}",
                        kind.0,
                        e.CurrentClassName()?,
                        e.CurrentIsPassword()?,
                        p.is_ok(),
                        p2.is_ok(),
                        edit.is_ok()
                    );
                    println!(
                        "focused={:?}, focusable={:?}, enabled={:?}, native_hwnd={:?}, rect={:?}",
                        e.CurrentHasKeyboardFocus(),
                        e.CurrentIsKeyboardFocusable(),
                        e.CurrentIsEnabled(),
                        e.CurrentNativeWindowHandle(),
                        e.CurrentBoundingRectangle()
                    );
                    if let Ok(p) = &p {
                        if let Ok(bounds) = p.DocumentRange() {
                            println!(
                                "bounds_within_editor={}",
                                english_companion_lite::uia::range_is_within_editor(
                                    &a, &e, pid as u32, &bounds
                                )
                            );
                            if let Ok(owner) = bounds.GetEnclosingElement() {
                                println!("bounds_owner_type={:?}, bounds_same_owner={:?}, bounds_empty={:?}",owner.CurrentControlType(),a.CompareElements(&owner,&e),bounds.CompareEndpoints(TextPatternRangeEndpoint_Start,&bounds,TextPatternRangeEndpoint_End).map(|n|n==0));
                            }
                        }
                        match p.GetSelection() {
                            Ok(selections) => {
                                println!("selection_count={:?}", selections.Length());
                                if selections.Length().ok() == Some(1) {
                                    let r = selections.GetElement(0)?;
                                    println!(
                                        "selection_within_editor={}",
                                        english_companion_lite::uia::range_is_within_editor(
                                            &a, &e, pid as u32, &r
                                        )
                                    );
                                    if let Ok(owner) = r.GetEnclosingElement() {
                                        println!(
                                            "selection_owner_type={:?}, selection_same_owner={:?}",
                                            owner.CurrentControlType(),
                                            a.CompareElements(&owner, &e)
                                        );
                                    }
                                    println!(
                                        "collapsed={:?}, readonly={:?}",
                                        r.CompareEndpoints(
                                            TextPatternRangeEndpoint_Start,
                                            &r,
                                            TextPatternRangeEndpoint_End
                                        )
                                        .map(|n| n == 0),
                                        r.GetAttributeValue(UIA_IsReadOnlyAttributeId)
                                            .and_then(|v| bool::try_from(&v))
                                    );
                                }
                            }
                            Err(e) => println!("selection_error={:?}", e.code()),
                        }
                    }
                    if let Ok(edit) = edit {
                        let mut raw = std::ptr::null_mut();
                        let hr = (Interface::vtable(&edit).GetActiveComposition)(
                            Interface::as_raw(&edit),
                            &mut raw,
                        );
                        println!("composition_hr={:?}, active={}", hr, !raw.is_null());
                        if !raw.is_null() {
                            let range = IUIAutomationTextRange::from_raw(raw);
                            println!(
                                "composition_empty={:?}",
                                range
                                    .CompareEndpoints(
                                        TextPatternRangeEndpoint_Start,
                                        &range,
                                        TextPatternRangeEndpoint_End
                                    )
                                    .map(|n| n == 0)
                            );
                        }
                    }
                }
            }
        }
        CoUninitialize();
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
