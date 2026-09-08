use std::ops::Range;
use switcher_platform::ports::PlatformError;

pub(super) fn update_and_wake(
    mailbox: &std::sync::Mutex<Mailbox>,
    update: impl FnOnce(&mut Mailbox) -> bool,
    wake: impl FnOnce() -> Result<(), PlatformError>,
) -> Result<(), PlatformError> {
    // Keep revocation serialized with the whole nonblocking native post. Otherwise a
    // concurrent owner drop could join/close the thread and allow its numeric ID's reuse.
    let mut inbox = mailbox.lock().unwrap_or_else(|e| e.into_inner());
    if update(&mut inbox) { wake() } else { Ok(()) }
}

#[derive(Debug, Default)]
pub(super) struct Mailbox {
    latest: Option<Vec<i16>>,
    stopped: bool,
}

impl Mailbox {
    pub fn replace(&mut self, pcm: Vec<i16>) -> bool {
        if self.stopped {
            return false;
        }
        self.latest = Some(pcm);
        true
    }
    pub fn take(&mut self) -> Option<Vec<i16>> {
        self.latest.take()
    }
    pub fn stop(&mut self) {
        self.stopped = true;
        self.latest = None;
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

#[derive(Debug)]
pub(super) struct PcmBuffer {
    pub samples: Vec<i16>,
    sent: usize,
}

impl PcmBuffer {
    pub fn new(samples: Vec<i16>) -> Self {
        Self { samples, sent: 0 }
    }
    pub fn next(&self, capacity: u32, padding: u32) -> Result<Range<usize>, PlatformError> {
        let available = capacity.checked_sub(padding).ok_or_else(|| {
            PlatformError::new(
                "audio_invalid_padding",
                "WASAPI padding exceeds buffer size",
            )
        })?;
        Ok(self.sent..self.samples.len().min(self.sent + available as usize))
    }
    pub fn submitted(&mut self, end: usize) {
        self.sent = end;
    }
    pub fn drained(&self, padding: u32) -> bool {
        self.sent == self.samples.len() && padding == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_waits_until_an_in_flight_wake_finishes() {
        use std::sync::{Arc, Mutex, TryLockError};
        use std::time::Duration;
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let sender_mailbox = Arc::clone(&mailbox);
        let (entered, observe) = crossbeam_channel::bounded(1);
        let (resume, resumed) = crossbeam_channel::bounded(1);
        let sender = std::thread::spawn(move || {
            update_and_wake(
                &sender_mailbox,
                |inbox| inbox.replace(vec![1]),
                || {
                    entered.send(()).unwrap();
                    resumed.recv().unwrap();
                    Ok(())
                },
            )
        });
        observe.recv_timeout(Duration::from_secs(2)).unwrap();
        // Reproduce stop at the exact point where a sender is still posting its wake.
        let revoked_early = match mailbox.try_lock() {
            Ok(mut inbox) => {
                inbox.stop();
                true
            }
            Err(TryLockError::WouldBlock) => false,
            Err(TryLockError::Poisoned(_)) => panic!("unexpected poison"),
        };
        resume.send(()).unwrap();
        sender.join().unwrap().unwrap();
        assert!(
            !revoked_early,
            "stop could close the thread handle before its last wake"
        );
        mailbox.lock().unwrap().stop();
    }

    #[test]
    fn rapid_requests_keep_only_the_latest_and_stop_revokes_future_requests() {
        let mut queue = Mailbox::default();
        assert!(queue.replace(vec![1]));
        assert!(queue.replace(vec![2]));
        assert_eq!(queue.take().unwrap(), vec![2]);
        assert!(queue.take().is_none());
        queue.replace(vec![3]);
        queue.stop();
        assert!(queue.take().is_none());
        assert!(!queue.replace(vec![4]));
        assert!(queue.stopped());
    }

    #[test]
    fn submitting_the_last_frame_does_not_mean_playback_has_drained() {
        let mut buffer = PcmBuffer::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(buffer.next(3, 0).unwrap(), 0..3);
        buffer.submitted(3);
        assert_eq!(buffer.next(3, 2).unwrap(), 3..4);
        buffer.submitted(4);
        assert_eq!(buffer.next(3, 0).unwrap(), 4..5);
        buffer.submitted(5);
        assert!(!buffer.drained(1));
        assert_eq!(buffer.next(3, 1).unwrap(), 5..5);
        assert!(buffer.drained(0));
        assert!(buffer.next(3, 4).is_err());
    }
}
