//! Synthesized cues. The output device is owned by the main thread (ADR-0009).

use std::{
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::Sender;
use rodio::{DeviceSinkBuilder, Source, source::SineWave};
use switcher_platform::{
    events::{Capability, CapabilityReport, CapabilityState, PlatformEvent},
    ports::{SoundCue, SoundPlayer},
};

/// Construct and drop on the main, message-pumping thread: cpal initializes COM STA.
#[derive(Debug)]
pub struct SoundDevice {
    _sink: rodio::MixerDeviceSink,
    available: Arc<AtomicBool>,
    _main_thread: PhantomData<Rc<()>>,
}

pub struct RodioSoundPlayer {
    mixer: rodio::mixer::Mixer,
    available: Arc<AtomicBool>,
}

impl std::fmt::Debug for RodioSoundPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RodioSoundPlayer")
            .field("available", &self.available.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl SoundDevice {
    /// The caller reports startup success/failure. Later stream failure reports Off
    /// once and stops accepting tones, without changing the user's sound preference.
    pub fn open(
        tx: Sender<PlatformEvent>,
    ) -> Result<(Self, RodioSoundPlayer), rodio::DeviceSinkError> {
        let available = Arc::new(AtomicBool::new(true));
        let on_error = Arc::clone(&available);
        // Keep output on the selected default device; try its supported formats.
        // The stock open_default_sink helper has no custom error callback.
        let mut sink = DeviceSinkBuilder::from_default_device()?
            .with_error_callback(move |error| {
                if on_error.swap(false, Ordering::AcqRel) {
                    let _ = tx.send(PlatformEvent::CapabilityChanged(CapabilityReport {
                        capability: Capability::Sound,
                        state: CapabilityState::Off,
                        code: "audio_stream_failed",
                        detail: error.to_string(),
                    }));
                }
            })
            .open_sink_or_fallback()?;
        sink.log_on_drop(false);
        let player = RodioSoundPlayer {
            mixer: sink.mixer().clone(),
            available: Arc::clone(&available),
        };
        Ok((
            Self {
                _sink: sink,
                available,
                _main_thread: PhantomData,
            },
            player,
        ))
    }
}

impl Drop for SoundDevice {
    fn drop(&mut self) {
        self.available.store(false, Ordering::Release);
    }
}

impl SoundPlayer for RodioSoundPlayer {
    fn play(&self, cue: SoundCue, volume: f32) {
        if !self.available.load(Ordering::Acquire) {
            return;
        }
        if let Some(gain) = effective_gain(volume) {
            self.mixer.add(tone(cue, gain));
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullSoundPlayer;

impl SoundPlayer for NullSoundPlayer {
    fn play(&self, _cue: SoundCue, _volume: f32) {}
}

fn cue_freq_hz(cue: SoundCue) -> f32 {
    match cue {
        SoundCue::Ru => 660.0,
        SoundCue::En => 880.0,
        SoundCue::Neutral => 520.0,
    }
}

fn effective_gain(volume: f32) -> Option<f32> {
    (volume.is_finite() && volume > 0.0).then(|| volume.min(1.0))
}

fn tone(cue: SoundCue, gain: f32) -> impl Source<Item = f32> + Send + 'static {
    let duration = Duration::from_millis(90);
    // rodio's fade_out begins at source start, so use the entire tone duration.
    SineWave::new(cue_freq_hz(cue))
        .take_duration(duration)
        .fade_out(duration)
        .amplify(gain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_have_distinct_audible_frequencies() {
        let frequencies = [
            cue_freq_hz(SoundCue::Ru),
            cue_freq_hz(SoundCue::En),
            cue_freq_hz(SoundCue::Neutral),
        ];
        assert!(frequencies.iter().all(|f| (200.0..2000.0).contains(f)));
        assert_ne!(frequencies[0], frequencies[1]);
        assert_ne!(frequencies[1], frequencies[2]);
        assert_ne!(frequencies[0], frequencies[2]);
    }

    #[test]
    fn gain_rejects_silence_and_nonfinite_values_and_clamps_the_rest() {
        for v in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(effective_gain(v), None);
        }
        assert_eq!(effective_gain(2.0), Some(1.0));
        assert_eq!(effective_gain(0.4), Some(0.4));
        NullSoundPlayer.play(SoundCue::Ru, f32::NAN);
    }

    #[test]
    fn tone_has_a_finite_duration_and_fades_to_silence() {
        let samples: Vec<_> = tone(SoundCue::Ru, 0.4).collect();
        assert!((3500..5000).contains(&samples.len()));
        assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 0.4));
        let peak = |part: &[f32]| part.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(peak(&samples[..500]) > 0.2);
        assert!(peak(&samples[samples.len() - 100..]) < 0.02);
    }
}
