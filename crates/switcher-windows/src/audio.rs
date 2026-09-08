//! Demand-driven shared WASAPI output (ADR-0016).
mod diagnostics;
mod queue;
mod stream;

use crate::win_util::{PumpWaker, current_thread_waker};
use crossbeam_channel::{Sender, bounded};
use queue::Mailbox;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use switcher_platform::{
    events::{Capability, CapabilityReport, CapabilityState, PlatformEvent},
    ports::PlatformError,
};
use windows::Win32::{
    Foundation::{HANDLE, WAIT_FAILED, WAIT_TIMEOUT},
    System::Threading::INFINITE,
    UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
        PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
    },
};

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, Default)]
struct Counters {
    active: AtomicBool,
    started: AtomicUsize,
    completed: AtomicUsize,
}

#[derive(Debug, Clone, Copy)]
pub struct AudioStats {
    pub active: bool,
    pub started: usize,
    pub completed: usize,
}

#[derive(Debug)]
pub struct PcmDevice {
    mailbox: Arc<Mutex<Mailbox>>,
    waker: PumpWaker,
    worker: Option<JoinHandle<()>>,
    counters: Arc<Counters>,
}

#[derive(Debug, Clone)]
pub struct PcmSender {
    mailbox: Arc<Mutex<Mailbox>>,
    waker: PumpWaker,
}

impl PcmDevice {
    pub fn open(events: Sender<PlatformEvent>) -> Result<(Self, PcmSender), PlatformError> {
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let counters = Arc::new(Counters::default());
        let inbox = Arc::clone(&mailbox);
        let stats = Arc::clone(&counters);
        let (ready, receive) = bounded(1);
        let worker = std::thread::Builder::new()
            .name("switcher-audio".into())
            .spawn(move || {
                let waker = current_thread_waker();
                if ready.send(waker).is_err() {
                    return;
                }
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&inbox, &stats, &events)
                }));
                let error = match outcome {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some(PlatformError::new(
                        "audio_thread_panicked",
                        "Audio thread panicked",
                    )),
                };
                inbox.lock().unwrap_or_else(|e| e.into_inner()).stop();
                stats.active.store(false, Ordering::Release);
                if let Some(error) = error {
                    report(&events, Err(error));
                }
            })
            .map_err(|e| PlatformError::new("audio_thread_failed", e.to_string()))?;
        let waker = match receive.recv() {
            Ok(waker) => waker,
            Err(error) => {
                let _ = worker.join();
                return Err(PlatformError::new("audio_thread_failed", error.to_string()));
            }
        };
        let sender = PcmSender {
            mailbox: Arc::clone(&mailbox),
            waker,
        };
        Ok((
            Self {
                mailbox,
                waker,
                worker: Some(worker),
                counters,
            },
            sender,
        ))
    }

    pub fn stats(&self) -> AudioStats {
        AudioStats {
            active: self.counters.active.load(Ordering::Acquire),
            started: self.counters.started.load(Ordering::Acquire),
            completed: self.counters.completed.load(Ordering::Acquire),
        }
    }
}

impl Drop for PcmDevice {
    fn drop(&mut self) {
        let stopped = queue::update_and_wake(
            &self.mailbox,
            |inbox| {
                if inbox.stopped() {
                    return false;
                }
                inbox.stop();
                true
            },
            || self.waker.wake(),
        );
        // The mailbox guard must be released before logging or joining its worker.
        if let Err(error) = stopped {
            tracing::warn!(%error, "audio stop wake failed");
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl PcmSender {
    /// Bounded PCM16 mono44100, at most 200ms. Replaces an older waiting cue.
    pub fn play(&self, pcm: Vec<i16>) {
        if pcm.is_empty() || pcm.len() > SAMPLE_RATE as usize / 5 {
            return;
        }
        if let Err(error) = queue::update_and_wake(
            &self.mailbox,
            |inbox| inbox.replace(pcm),
            || self.waker.wake(),
        ) {
            tracing::warn!(%error, "audio cue wake failed");
        }
    }
}

fn report(events: &Sender<PlatformEvent>, result: Result<(), PlatformError>) {
    let (state, code, detail) = match result {
        Ok(()) => (
            CapabilityState::Ok,
            "audio_ready",
            "Default audio endpoint is available".into(),
        ),
        Err(error) => (CapabilityState::Off, error.code, error.detail),
    };
    let _ = events.send(PlatformEvent::CapabilityChanged(CapabilityReport {
        capability: Capability::Sound,
        state,
        code,
        detail,
    }));
}

fn run(
    mailbox: &Mutex<Mailbox>,
    counters: &Counters,
    events: &Sender<PlatformEvent>,
) -> Result<(), PlatformError> {
    let _apartment = stream::Apartment::new()?;
    let initial = stream::probe();
    let mut failed = initial.is_err();
    report(events, initial);
    let mut active: Option<stream::Burst> = None;
    let mut deadline = Instant::now();
    loop {
        if !drain_messages()? {
            return Ok(());
        }
        let request = {
            let mut inbox = mailbox.lock().unwrap_or_else(|e| e.into_inner());
            if inbox.stopped() {
                return Ok(());
            }
            if active.is_none() { inbox.take() } else { None }
        };
        // Check the absolute deadline even when native messages are continuously ready.
        if active.is_some() && Instant::now() >= deadline {
            active = None;
            counters.active.store(false, Ordering::Release);
            report(
                events,
                Err(PlatformError::new(
                    "audio_stream_timeout",
                    "Audio stream did not drain within three seconds",
                )),
            );
            failed = true;
            continue;
        }
        if let Some(pcm) = request {
            match stream::Burst::start(pcm) {
                Ok(burst) => {
                    active = Some(burst);
                    counters.active.store(true, Ordering::Release);
                    counters.started.fetch_add(1, Ordering::Release);
                    deadline = Instant::now() + Duration::from_secs(3);
                    if failed {
                        report(events, Ok(()));
                        failed = false;
                    }
                    tracing::debug!("audio stream started");
                }
                Err(error) => {
                    report(events, Err(error));
                    failed = true;
                    // Loop once before waiting: a newer cue may already be queued.
                    continue;
                }
            }
        }
        if let Some(burst) = &mut active {
            match burst.advance() {
                Ok(true) => {
                    active = None; // Release stream before publishing idle/completion.
                    counters.active.store(false, Ordering::Release);
                    counters.completed.fetch_add(1, Ordering::Release);
                    tracing::debug!("audio stream drained and released");
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    active = None;
                    counters.active.store(false, Ordering::Release);
                    report(events, Err(error));
                    failed = true;
                    continue;
                }
            }
        }
        let handles: Vec<HANDLE> = active.iter().map(|burst| burst.event()).collect();
        let timeout = if active.is_some() {
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(u32::MAX as u128) as u32
        } else {
            INFINITE
        };
        // SAFETY: this thread owns every event for the whole wait; MWMO_INPUTAVAILABLE
        // keeps COM/posted input dispatchable, including messages seen by PeekMessage.
        let wait = unsafe {
            MsgWaitForMultipleObjectsEx(Some(&handles), timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if wait == WAIT_FAILED {
            return Err(PlatformError::new(
                "audio_wait_failed",
                windows::core::Error::from_thread().to_string(),
            ));
        }
        if wait == WAIT_TIMEOUT {
            active = None;
            counters.active.store(false, Ordering::Release);
            report(
                events,
                Err(PlatformError::new(
                    "audio_stream_timeout",
                    "Audio stream did not drain within three seconds",
                )),
            );
            failed = true;
        }
    }
}

fn drain_messages() -> Result<bool, PlatformError> {
    // A bounded drain prevents COM/native input from starving audio or shutdown.
    for _ in 0..128 {
        let mut message = MSG::default();
        // SAFETY: live MSG, only the calling thread's queue is accessed.
        if !unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            break;
        }
        if message.message == WM_QUIT {
            return Ok(false);
        }
        // SAFETY: unmodified message from this thread; window callbacks are system-owned.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(true)
}
