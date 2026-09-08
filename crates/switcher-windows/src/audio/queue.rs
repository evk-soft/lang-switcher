use std::ops::Range;
use switcher_platform::ports::PlatformError;

pub(super) fn render_buffer(
    mut samples: Vec<i16>,
    sample_rate: u32,
    latency_100ns: u64,
) -> PcmBuffer {
    if !samples.is_empty() {
        // Realtek dropped a complete 90ms cue at zero reported latency. Keep a
        // bounded 100ms release tail, or the greater validated device latency.
        // This is measured compatibility policy, not an API guarantee (ADR-0020).
        let release_100ns = latency_100ns.max(1_000_000);
        let tail = (release_100ns * sample_rate as u64).div_ceil(10_000_000) as usize;
        samples.resize(samples.len() + tail, 0);
    }
    PcmBuffer::new(samples)
}

pub(super) fn playback_reached(
    frames: usize,
    position: u64,
    frequency: u64,
    sample_rate: u32,
) -> bool {
    frequency != 0
        && sample_rate != 0
        && position as u128 * sample_rate as u128 >= frames as u128 * frequency as u128
}

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
    fn a_short_cue_keeps_a_silent_release_tail_even_when_latency_is_zero() {
        let cue: Vec<i16> = (0..3969).map(|i| (i % 123) as i16).collect();
        let mut buffer = render_buffer(cue.clone(), 44100, 0);
        assert_eq!(&buffer.samples[..cue.len()], cue.as_slice());
        buffer.submitted(cue.len());
        assert!(
            !buffer.drained(0),
            "the 90ms clock endpoint lost the entire cue in the Realtek control"
        );
        assert_eq!(buffer.samples.len(), 8379);
        assert!(
            buffer.samples[cue.len()..]
                .iter()
                .all(|&sample| sample == 0)
        );
        assert!(!playback_reached(buffer.samples.len(), 7938, 88200, 44100));
        buffer.submitted(buffer.samples.len());
        assert!(buffer.drained(0));
        assert!(playback_reached(buffer.samples.len(), 16758, 88200, 44100));
    }

    #[test]
    fn release_tail_honors_longer_latency_and_does_not_turn_a_probe_into_audio() {
        let reported = render_buffer(vec![7], 44100, 2_000_001);
        assert_eq!(reported.samples.len(), 1 + 8821);
        let minimum = render_buffer(vec![7], 48000, 0);
        assert_eq!(minimum.samples.len(), 1 + 4800);
        let probe = render_buffer(Vec::new(), 44100, 5_000_000);
        assert!(probe.samples.is_empty());
    }

    #[test]
    fn an_empty_client_buffer_does_not_finish_a_cue_still_at_the_device() {
        let mut buffer = PcmBuffer::new(vec![1; 3969]);
        buffer.submitted(3969);
        assert!(buffer.drained(0));
        // Realtek observations: client queue is empty at only 64-70ms of a 90ms cue.
        assert!(!playback_reached(3969, 5666, 88200, 44100));
        assert!(!playback_reached(3969, 6200, 88200, 44100));
        assert!(!playback_reached(3969, 7937, 88200, 44100));
        assert!(playback_reached(3969, 7938, 88200, 44100));
        assert!(playback_reached(3969, 8820, 88200, 44100));
    }

    #[test]
    fn playback_clock_uses_its_own_units_and_rejects_zero_frequency() {
        assert!(!playback_reached(3969, 899999, 10000000, 44100));
        assert!(playback_reached(3969, 900000, 10000000, 44100));
        assert!(!playback_reached(3969, u64::MAX, 0, 44100));
        assert!(!playback_reached(3969, u64::MAX, 88200, 0));
        assert!(playback_reached(1, u64::MAX, u64::MAX, 44100));
    }

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
