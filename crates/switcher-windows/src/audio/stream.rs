use super::{
    SAMPLE_RATE,
    queue::{PcmBuffer, playback_reached},
};
use switcher_platform::ports::PlatformError;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Media::Audio::{
            AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            IAudioClient, IAudioClock, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator,
            MMDeviceEnumerator, WAVEFORMATEX, eConsole, eRender,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Threading::CreateEventW,
        },
    },
    core::{Owned, PCWSTR},
};

fn api(error: windows::core::Error) -> PlatformError {
    PlatformError::new("audio_stream_failed", error.to_string())
}

pub(super) use crate::com::StaApartment as Apartment;

fn endpoint() -> Result<IMMDevice, PlatformError> {
    // SAFETY: caller owns a live STA guard; returned interfaces remain on this thread.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(api)?;
    // SAFETY: live enumerator and documented rendering/default-console selectors.
    unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|e| PlatformError::new("no_output_device", e.to_string()))
}

pub(super) fn probe() -> Result<(), PlatformError> {
    Burst::prepare(Vec::new()).map(drop)
}

pub(super) struct Burst {
    // Drop order is significant: services, client, event. Event outlives WASAPI.
    render: IAudioRenderClient,
    clock: IAudioClock,
    client: IAudioClient,
    event: Owned<HANDLE>,
    buffer: PcmBuffer,
    capacity: u32,
    frequency: u64,
}

impl Burst {
    pub fn start(samples: Vec<i16>) -> Result<Self, PlatformError> {
        let mut burst = Self::prepare(samples)?;
        burst.advance()?;
        // SAFETY: initialized/prefilled client with SetEventHandle completed.
        unsafe { burst.client.Start() }.map_err(api)?;
        Ok(burst)
    }

    fn prepare(mut samples: Vec<i16>) -> Result<Self, PlatformError> {
        // SAFETY: unnamed auto-reset event, initially nonsignalled; no security descriptor.
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.map_err(api)?;
        // SAFETY: newly created event is uniquely owned and closed after all COM clients.
        let event = unsafe { Owned::new(handle) };
        let device = endpoint()?;
        // SAFETY: this STA owns device; no activation parameters, requested IAudioClient IID.
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(api)?;
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 1,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        // SAFETY: fully specified PCM descriptor lives through Initialize. Shared event
        // mode requires zero duration/periodicity; Windows converts channels/sample rate.
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                0,
                0,
                &format,
                None,
            )
        }
        .map_err(api)?;
        // SAFETY: initialized event-driven client; valid event remains owned past its release.
        unsafe { client.SetEventHandle(*event) }.map_err(api)?;
        // SAFETY: successful Initialize; all queries and buffer operations stay on this STA.
        let (capacity, latency, render): (u32, i64, IAudioRenderClient) = unsafe {
            (
                client.GetBufferSize().map_err(api)?,
                client.GetStreamLatency().map_err(api)?,
                client.GetService().map_err(api)?,
            )
        };
        // SAFETY: initialized client on this STA; the clock service is released before
        // this client on the same STA. Its frequency defines GetPosition units (ADR-0018).
        let clock: IAudioClock = unsafe { client.GetService() }.map_err(api)?;
        // SAFETY: live clock service; scalar output, frequency is constant for this stream.
        let frequency = unsafe { clock.GetFrequency() }.map_err(api)?;
        if frequency == 0 {
            return Err(PlatformError::new(
                "audio_invalid_clock",
                "Audio clock frequency is zero",
            ));
        }
        if capacity == 0 || !(0..=5_000_000).contains(&latency) {
            return Err(PlatformError::new(
                "audio_invalid_buffer",
                "Audio buffer or latency is outside the supported bounds",
            ));
        }
        super::diagnostics::log_output(&device, &client);
        if tracing::enabled!(tracing::Level::DEBUG) {
            tracing::debug!(
                frames = samples.len(),
                peak = samples
                    .iter()
                    .map(|sample| sample.unsigned_abs())
                    .max()
                    .unwrap_or(0),
                capacity,
                latency_100ns = latency,
                "audio source buffer"
            );
        }
        // Include the reported device latency as a silent tail, not a wall-clock guess.
        let tail = (latency as u64 * SAMPLE_RATE as u64).div_ceil(10_000_000) as usize;
        samples.resize(samples.len() + tail, 0);
        let burst = Self {
            render,
            clock,
            client,
            event,
            buffer: PcmBuffer::new(samples),
            capacity,
            frequency,
        };
        Ok(burst)
    }

    pub fn event(&self) -> HANDLE {
        *self.event
    }

    pub fn advance(&mut self) -> Result<bool, PlatformError> {
        // SAFETY: this initialized client and render service belong to the calling STA.
        let padding = unsafe { self.client.GetCurrentPadding() }.map_err(api)?;
        if self.buffer.drained(padding) {
            let mut position = 0;
            // SAFETY: live clock service on this STA; initialized writable output.
            // Padding describes the client queue, not the device's playback position.
            unsafe { self.clock.GetPosition(&mut position, None) }.map_err(api)?;
            let complete = playback_reached(
                self.buffer.samples.len(),
                position,
                self.frequency,
                SAMPLE_RATE,
            );
            tracing::trace!(
                position,
                frequency = self.frequency,
                complete,
                "audio client buffer empty"
            );
            if complete {
                tracing::debug!(
                    frames = self.buffer.samples.len(),
                    position,
                    frequency = self.frequency,
                    "audio device playback complete"
                );
            }
            return Ok(complete);
        }
        let range = self.buffer.next(self.capacity, padding)?;
        let count = range.len() as u32;
        if count != 0 {
            // SAFETY: request fits capacity-padding; no earlier GetBuffer remains outstanding.
            let target = unsafe { self.render.GetBuffer(count) }.map_err(api)?;
            if target.is_null() {
                return Err(PlatformError::new(
                    "audio_null_buffer",
                    "WASAPI returned a null nonempty buffer",
                ));
            }
            // SAFETY: GetBuffer supplied count mono PCM16 frames (count*2 bytes); source
            // range is in-bounds and disjoint. Byte copy needs no target i16 alignment.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.buffer.samples[range.clone()].as_ptr().cast::<u8>(),
                    target,
                    count as usize * 2,
                );
            }
            // SAFETY: matches the previous successful GetBuffer on this same thread.
            unsafe { self.render.ReleaseBuffer(count, 0) }.map_err(api)?;
            self.buffer.submitted(range.end);
        }
        Ok(false)
    }
}

impl Drop for Burst {
    fn drop(&mut self) {
        // SAFETY: client stays alive through Stop and all service releases follow on
        // this STA. Stop is also valid for an already stopped initialized client.
        if let Err(error) = unsafe { self.client.Stop() } {
            tracing::warn!(%error, "could not stop audio stream");
        }
    }
}
