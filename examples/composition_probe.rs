//! Observe event types and nullable range metadata only. Never fetch text or event payloads.
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{implement, Interface},
    Win32::{System::Com::*, UI::Accessibility::*},
};

#[implement(IUIAutomationTextEditTextChangedEventHandler)]
struct Handler {
    phase: Arc<AtomicU32>,
}
impl IUIAutomationTextEditTextChangedEventHandler_Impl for Handler_Impl {
    fn HandleTextEditTextChangedEvent(
        &self,
        _: Option<&IUIAutomationElement>,
        change: TextEditChangeType,
        _: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        self.phase.store(change.0 as u32, Ordering::Release);
        println!("textedit_event_type={}", change.0);
        Ok(())
    }
}
fn main() -> windows::core::Result<()> {
    let pid: i32 = std::env::args()
        .nth(1)
        .expect("authorized PID")
        .parse()
        .expect("PID");
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let a: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
        if let Ok(a2) = a.cast::<IUIAutomation2>() {
            a2.SetConnectionTimeout(1500)?;
            a2.SetTransactionTimeout(1500)?;
        }
        let a3 = a.cast::<IUIAutomation3>()?;
        let phase = Arc::new(AtomicU32::new(0));
        let mut target: Option<IUIAutomationElement> = None;
        let mut handlers = Vec::new();
        let end = Instant::now() + Duration::from_secs(55);
        let mut previous = None;
        while Instant::now() < end {
            if target.is_none() {
                if let Ok(e) = a.GetFocusedElement() {
                    if e.CurrentProcessId().ok() == Some(pid)
                        && e.CurrentControlType().ok() == Some(UIA_GroupControlTypeId)
                        && e.CurrentIsPassword().ok().is_some_and(|p| !p.as_bool())
                    {
                        for change in [
                            TextEditChangeType_Composition,
                            TextEditChangeType_CompositionFinalized,
                        ] {
                            let h: IUIAutomationTextEditTextChangedEventHandler = Handler {
                                phase: phase.clone(),
                            }
                            .into();
                            a3.AddTextEditTextChangedEventHandler(
                                &e,
                                TreeScope_Element,
                                change,
                                None,
                                &h,
                            )?;
                            handlers.push(h);
                        }
                        println!("subscribed_authorized_group");
                        target = Some(e);
                    }
                }
            }
            if let Some(e) = &target {
                if let Ok(edit) =
                    e.GetCurrentPatternAs::<IUIAutomationTextEditPattern>(UIA_TextEditPatternId)
                {
                    let mut raw = std::ptr::null_mut();
                    let hr = (Interface::vtable(&edit).GetActiveComposition)(
                        Interface::as_raw(&edit),
                        &mut raw,
                    );
                    let active = !raw.is_null();
                    let empty = if active {
                        let r = IUIAutomationTextRange::from_raw(raw);
                        r.CompareEndpoints(
                            TextPatternRangeEndpoint_Start,
                            &r,
                            TextPatternRangeEndpoint_End,
                        )
                        .ok()
                        .map(|n| n == 0)
                    } else {
                        None
                    };
                    let focused = e
                        .CurrentHasKeyboardFocus()
                        .ok()
                        .is_some_and(|b| b.as_bool());
                    let state = (hr.0, active, empty, focused, phase.load(Ordering::Acquire));
                    if previous != Some(state) {
                        println!("composition_metadata={state:?}");
                        previous = Some(state);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Some(e) = target {
            for h in handlers {
                a3.RemoveTextEditTextChangedEventHandler(&e, &h)?;
            }
        }
        drop(a3);
        drop(a);
        CoUninitialize();
    }
    Ok(())
}
