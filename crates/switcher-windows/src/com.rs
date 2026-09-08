//! Thread-bound STA guard. The process's first STA must be the last to close.
use std::{marker::PhantomData, rc::Rc};
use switcher_platform::ports::PlatformError;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

#[derive(Debug)]
pub struct StaApartment(PhantomData<Rc<()>>);

impl StaApartment {
    pub fn new() -> Result<Self, PlatformError> {
        // SAFETY: initializes only the caller; changed-mode failure creates no guard.
        // S_OK and S_FALSE both require one matching CoUninitialize on this thread.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|e| PlatformError::new("com_sta_failed", e.to_string()))?;
        Ok(Self(PhantomData))
    }
}

impl Drop for StaApartment {
    fn drop(&mut self) {
        // SAFETY: !Send keeps Drop on the initializing thread; owns one successful init.
        unsafe {
            CoUninitialize();
        }
    }
}
