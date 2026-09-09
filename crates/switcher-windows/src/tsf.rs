//! TSF notifications on a supervised STA thread (ADR-0013).

use crate::layout_monitor::{current, language_for};
use crate::supervise::{StopToken, SupervisedThread, guard_callback, spawn_supervised};
use crate::win_util::{PumpHandler, PumpVerdict, pump_with_handler};
use crossbeam_channel::Sender;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use switcher_platform::events::{
    Capability, CapabilityReport, CapabilityState, LayoutId, LayoutSource, PlatformEvent,
};
use switcher_platform::ports::PlatformError;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::{
    CLSID_TF_ThreadMgr, ITfInputProcessorProfileActivationSink,
    ITfInputProcessorProfileActivationSink_Impl, ITfSource, ITfThreadMgr, TF_IPSINK_FLAG_ACTIVE,
    TF_PROFILETYPE_KEYBOARDLAYOUT,
};
use windows::core::{GUID, HRESULT, Interface, implement};

fn validate_sta(result: HRESULT) -> Result<(), PlatformError> {
    result
        .ok()
        .map_err(|error| PlatformError::new("com_init_failed", error.message()))
}

struct Sta(PhantomData<Rc<()>>);

impl Sta {
    fn new() -> Result<Self, PlatformError> {
        // SAFETY: called on the dedicated TSF thread. NULL reserved argument and
        // explicit STA. Both S_OK and S_FALSE create one balanced owner; failures do not.
        validate_sta(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) })?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Sta {
    fn drop(&mut self) {
        // SAFETY: !Send guard drops on its creating thread, once per successful init.
        // The subscription borrows this guard, so its COM pointers have already dropped.
        unsafe { CoUninitialize() };
    }
}

struct ActiveManager {
    interface: ITfThreadMgr,
    active: bool,
}

impl ActiveManager {
    fn deactivate(&mut self) {
        if self.active {
            self.active = false;
            // SAFETY: same STA and interface as our successful Activate, balanced once.
            if let Err(error) = unsafe { self.interface.Deactivate() } {
                tracing::warn!(%error, "TSF Deactivate failed");
            }
        }
    }
}

impl Drop for ActiveManager {
    fn drop(&mut self) {
        self.deactivate();
    }
}

struct Subscription<'a> {
    source: ITfSource,
    cookie: u32,
    manager: ActiveManager,
    _sink: ITfInputProcessorProfileActivationSink,
    observed: Rc<Cell<bool>>,
    _sta: PhantomData<&'a Sta>,
}

impl<'a> Subscription<'a> {
    fn new(_sta: &'a Sta, events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        // SAFETY: live STA guard; static class IID, in-process TSF manager, no aggregation.
        let interface: ITfThreadMgr =
            unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| PlatformError::new("tsf_create_failed", error.message()))?;
        // SAFETY: COM initialized on this thread and interface came from successful creation.
        unsafe { interface.Activate() }
            .map_err(|error| PlatformError::new("tsf_activate_failed", error.message()))?;
        let manager = ActiveManager {
            interface,
            active: true,
        };
        let source: ITfSource = manager
            .interface
            .cast()
            .map_err(|error| PlatformError::new("tsf_query_source_failed", error.message()))?;
        let observed = Rc::new(Cell::new(false));
        let sink: ITfInputProcessorProfileActivationSink = ActivationSink {
            events,
            observed: Rc::clone(&observed),
        }
        .into();
        // SAFETY: static sink IID, valid COM source and sink on this STA. Successful
        // subscription owns a reference until UnadviseSink; we keep our own reference too.
        let cookie =
            unsafe { source.AdviseSink(&ITfInputProcessorProfileActivationSink::IID, &sink) }
                .map_err(|error| PlatformError::new("tsf_advise_failed", error.message()))?;
        Ok(Self {
            source,
            cookie,
            manager,
            _sink: sink,
            observed,
            _sta: PhantomData,
        })
    }
}

impl Drop for Subscription<'_> {
    fn drop(&mut self) {
        // SAFETY: cookie belongs to this source on this STA, both still live. This
        // executes before Deactivate and before any interface or the STA guard is released.
        if let Err(error) = unsafe { self.source.UnadviseSink(self.cookie) } {
            tracing::warn!(%error, "TSF UnadviseSink failed");
        }
        self.manager.deactivate();
    }
}

// windows-implement defaults to agile=true, which would advertise cross-apartment
// calls despite our STA-owned Rc<Cell> and thread-local callback failure handling.
#[implement(ITfInputProcessorProfileActivationSink, Agile = false)]
struct ActivationSink {
    events: Sender<PlatformEvent>,
    observed: Rc<Cell<bool>>,
}

impl ITfInputProcessorProfileActivationSink_Impl for ActivationSink_Impl {
    // The method name and argument list are imposed by the COM interface.
    #[allow(non_snake_case)]
    fn OnActivated(
        &self,
        profile_type: u32,
        _langid: u16,
        _clsid: *const GUID,
        _catid: *const GUID,
        _profile: *const GUID,
        hkl: HKL,
        flags: u32,
    ) -> windows::core::Result<()> {
        guard_callback(
            Capability::LayoutTsf,
            || Ok(()),
            || {
                // Keep callback arrival distinct from accepted activation events.
                tracing::trace!(target: "switcher_windows::tsf", profile_type, flags, ?hkl, "TSF callback received");
                if flags & TF_IPSINK_FLAG_ACTIVE == 0 {
                    return Ok(());
                }
                let value = if profile_type == TF_PROFILETYPE_KEYBOARDLAYOUT && !hkl.is_invalid() {
                    let layout = LayoutId(hkl.0 as usize as u64);
                    Ok((layout, language_for(layout)))
                } else {
                    current()
                };
                if let Ok((layout, lang)) = value {
                    if !self.observed.replace(true) {
                        report(
                            &self.events,
                            CapabilityState::Ok,
                            "tsf_activation_observed",
                            "TSF activation callback received",
                        );
                    }
                    tracing::trace!(target: "switcher_windows::tsf", profile_type, ?layout, lang = lang.as_str(), "TSF activation");
                    let _ = self.events.send(PlatformEvent::LayoutChanged {
                        layout,
                        lang,
                        source: LayoutSource::Tsf,
                    });
                }
                Ok(())
            },
        )
    }
}

#[derive(Debug)]
pub struct TsfSource {
    _worker: SupervisedThread,
}

impl TsfSource {
    pub fn new(events: Sender<PlatformEvent>) -> Result<Self, PlatformError> {
        Ok(Self {
            _worker: spawn_supervised(&[Capability::LayoutTsf], events, run)?,
        })
    }
}

struct Waiting<'a>(&'a StopToken);
impl PumpHandler for Waiting<'_> {
    fn should_stop(&self) -> bool {
        self.0.is_stopped()
    }
    fn on_thread_message(&mut self, _: u32, _: WPARAM, _: LPARAM) -> PumpVerdict {
        PumpVerdict::Continue
    }
}

fn run(events: &Sender<PlatformEvent>, stop: &StopToken) -> Result<(), PlatformError> {
    let sta = Sta::new()?;
    let subscription = Subscription::new(&sta, events.clone())?;
    if !subscription.observed.get() {
        report(
            events,
            CapabilityState::Degraded,
            "tsf_delivery_unverified",
            "subscribed; awaiting an activation callback",
        );
    }
    pump_with_handler("tsf", &mut Waiting(stop))
}

fn report(
    events: &Sender<PlatformEvent>,
    state: CapabilityState,
    code: &'static str,
    detail: &str,
) {
    let _ = events.send(PlatformEvent::CapabilityChanged(CapabilityReport {
        capability: Capability::LayoutTsf,
        state,
        code,
        detail: detail.into(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{E_FAIL, RPC_E_CHANGED_MODE, S_FALSE, S_OK};

    #[test]
    fn apartment_bound_sink_does_not_advertise_agility() {
        let (events, _) = crossbeam_channel::unbounded();
        let sink: ITfInputProcessorProfileActivationSink = ActivationSink {
            events,
            observed: Rc::new(Cell::new(false)),
        }
        .into();
        assert!(
            sink.cast::<windows::Win32::System::Com::IAgileObject>()
                .is_err(),
            "callbacks and Rc<Cell> state belong to this STA"
        );
    }

    #[test]
    fn sta_accepts_both_balanced_successes_and_rejects_a_different_apartment() {
        assert!(validate_sta(S_OK).is_ok());
        assert!(validate_sta(S_FALSE).is_ok());
        assert_eq!(
            validate_sta(RPC_E_CHANGED_MODE).unwrap_err().code,
            "com_init_failed"
        );
        assert_eq!(validate_sta(E_FAIL).unwrap_err().code, "com_init_failed");
    }

    #[test]
    fn native_subscription_can_be_created_and_released_repeatedly() {
        std::thread::spawn(|| {
            for _ in 0..3 {
                let sta = Sta::new().expect("initialize STA");
                let nested = Sta::new().expect("S_FALSE is still a balanced init");
                let (events, _) = crossbeam_channel::unbounded();
                let subscription = Subscription::new(&sta, events).expect("subscribe TSF");
                drop(subscription);
                drop(nested);
                drop(sta);
            }
        })
        .join()
        .expect("STA lifecycle worker exits");
    }

    #[test]
    fn deactivation_is_ignored_and_keyboard_activation_keeps_its_hkl() {
        let (events, received) = crossbeam_channel::unbounded();
        let sink: ITfInputProcessorProfileActivationSink = ActivationSink {
            events,
            observed: Rc::new(Cell::new(false)),
        }
        .into();
        let null_guid = GUID::zeroed();
        let hkl = HKL(0x4190419usize as *mut _);
        // SAFETY: invoke our own COM implementation on the creating thread. All GUID
        // arguments point to a live value and the opaque HKL is not dereferenced.
        unsafe {
            sink.OnActivated(
                TF_PROFILETYPE_KEYBOARDLAYOUT,
                0x419,
                &null_guid,
                &null_guid,
                &null_guid,
                hkl,
                0,
            )
        }
        .unwrap();
        assert!(received.try_recv().is_err());
        // SAFETY: the same valid call frame, now marking activation instead of deactivation.
        unsafe {
            sink.OnActivated(
                TF_PROFILETYPE_KEYBOARDLAYOUT,
                0x419,
                &null_guid,
                &null_guid,
                &null_guid,
                hkl,
                TF_IPSINK_FLAG_ACTIVE,
            )
        }
        .unwrap();
        assert!(matches!(
            received.recv().unwrap(),
            PlatformEvent::CapabilityChanged(_)
        ));
        assert!(
            matches!(received.recv().unwrap(), PlatformEvent::LayoutChanged { layout: LayoutId(0x4190419), lang, source: LayoutSource::Tsf } if lang.as_str() == "ru-RU")
        );
    }
}
