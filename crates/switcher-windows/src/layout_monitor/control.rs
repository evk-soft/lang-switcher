//! Latest preference plus a wake target revoked before its thread can exit.

use crate::win_util::PumpWaker;
use std::sync::Mutex;
use switcher_platform::ports::PlatformError;

#[derive(Debug)]
struct State {
    enabled: bool,
    waker: Option<PumpWaker>,
}

#[derive(Debug)]
pub(super) struct Control(Mutex<State>);

impl Control {
    pub fn new(enabled: bool) -> Self {
        Self(Mutex::new(State {
            enabled,
            waker: None,
        }))
    }

    pub fn enabled(&self) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).enabled
    }

    pub fn attach(&self, waker: PumpWaker) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).waker = Some(waker);
    }

    pub fn detach(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).waker = None;
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        self.set_with(enabled, |waker| waker.wake())
    }

    fn set_with(
        &self,
        enabled: bool,
        wake: impl FnOnce(PumpWaker) -> Result<(), PlatformError>,
    ) -> Result<(), PlatformError> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.enabled = enabled;
        // The lock covers the nonblocking native post as well as the state update.
        // Detach cannot finish (and permit TID reuse) halfway through this operation.
        // During setup/backoff there is no target; the next attempt reads this bool.
        match state.waker {
            Some(waker) => wake(waker),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, TryLockError};
    use std::time::Duration;

    #[test]
    fn latest_preference_survives_detach_and_a_failed_post() {
        let control = Control::new(true);
        control.attach(PumpWaker::new(7));
        assert!(
            control
                .set_with(false, |_| Err(PlatformError::new("full", "test")))
                .is_err()
        );
        assert!(!control.enabled());
        control.detach();
        control
            .set_with(true, |_| panic!("revoked target must not be posted"))
            .unwrap();
        control.attach(PumpWaker::new(8));
        assert!(control.enabled());
    }

    #[test]
    fn revocation_waits_for_an_in_flight_preference_wake() {
        let control = Arc::new(Control::new(true));
        control.attach(PumpWaker::new(7));
        let sender_control = Arc::clone(&control);
        let (entered, seen) = crossbeam_channel::bounded(1);
        let (resume, resumed) = crossbeam_channel::bounded(1);
        let sender = std::thread::spawn(move || {
            sender_control.set_with(false, |_| {
                entered.send(()).unwrap();
                resumed.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            })
        });
        seen.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            control.0.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        resume.send(()).unwrap();
        sender.join().unwrap().unwrap();
        control.detach();
        assert!(!control.enabled());
        control
            .set_with(true, |_| panic!("post after detach"))
            .unwrap();
    }
}
