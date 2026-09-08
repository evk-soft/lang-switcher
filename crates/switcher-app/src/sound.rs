//! Pure PCM cue synthesis and a demand-driven Windows player (ADR-0016).

use switcher_platform::ports::{SoundCue, SoundPlayer};

#[cfg(windows)]
pub use switcher_windows::audio::PcmDevice as SoundDevice;

#[cfg(windows)]
#[derive(Debug)]
pub struct WasapiSoundPlayer(pub switcher_windows::audio::PcmSender);

#[cfg(windows)]
impl SoundPlayer for WasapiSoundPlayer {
    fn play(&self, cue: SoundCue, volume: f32) {
        if let Some(samples) = pcm_tone(cue, volume) {
            self.0.play(samples);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullSoundPlayer;
impl SoundPlayer for NullSoundPlayer {
    fn play(&self, _: SoundCue, _: f32) {}
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

/// 90ms mono PCM16 at 44100Hz; volume is part of the samples, never global device state.
pub fn pcm_tone(cue: SoundCue, volume: f32) -> Option<Vec<i16>> {
    let gain = effective_gain(volume)?;
    let length = 3969;
    let attack = 220.0; // Five milliseconds, avoiding an abrupt onset.
    Some(
        (0..length)
            .map(|i| {
                let phase = std::f32::consts::TAU * cue_freq_hz(cue) * i as f32 / 44100.0;
                let envelope =
                    (i as f32 / attack).min(1.0) * (length - 1 - i) as f32 / (length - 1) as f32;
                (phase.sin() * envelope * gain * i16::MAX as f32).round() as i16
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_has_exact_duration_silent_edges_and_distinct_frequencies() {
        let ru = pcm_tone(SoundCue::Ru, 0.4).unwrap();
        let en = pcm_tone(SoundCue::En, 0.4).unwrap();
        assert_eq!(ru.len(), 3969);
        assert_eq!(ru[0], 0);
        assert_eq!(*ru.last().unwrap(), 0);
        assert!(ru.iter().any(|v| v.abs() > 8000));
        assert!(ru.iter().all(|v| v.abs() <= 13107));
        let crossings =
            |samples: &[i16]| samples.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
        assert!((58..=60).contains(&crossings(&ru)));
        assert!((78..=80).contains(&crossings(&en)));
        assert!(pcm_tone(SoundCue::Ru, 0.0).is_none());
        assert!(pcm_tone(SoundCue::Ru, f32::NAN).is_none());
    }

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
        let samples: Vec<f32> = pcm_tone(SoundCue::Ru, 0.4)
            .unwrap()
            .into_iter()
            .map(|v| v as f32 / i16::MAX as f32)
            .collect();
        assert!((3500..5000).contains(&samples.len()));
        assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 0.4));
        let peak = |part: &[f32]| part.iter().map(|v| v.abs()).fold(0.0, f32::max);
        assert!(peak(&samples[..500]) > 0.2);
        assert!(peak(&samples[samples.len() - 100..]) < 0.02);
    }
}
