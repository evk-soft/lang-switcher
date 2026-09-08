//! Bounded listening experiment; uses the production PCM/stream source unchanged.
//! A uses the production worker. B/C/D/E use the stream without its mailbox/worker prewarm.
//! Endpoint peak measures all output, not a recording or proof of physical audibility.
use std::{
    fs::File,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
use switcher_platform::{
    events::{CapabilityState, PlatformEvent},
    ports::SoundCue,
};
use switcher_windows::{audio::SAMPLE_RATE, com};
use windows::Win32::{
    Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Media::Audio::{
        Endpoints::IAudioMeterInformation, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole,
        eRender,
    },
    System::Com::{CLSCTX_ALL, CoCreateInstance},
    UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
        PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
    },
};

// Reuse exact production internals without adding a long-tone mode to the product.
#[path = "../src/audio/diagnostics.rs"]
mod diagnostics;
#[allow(dead_code)] // This experiment only uses PcmBuffer, not the mailbox helpers.
#[path = "../src/audio/queue.rs"]
mod queue;
#[allow(dead_code)] // Only the production pure synthesizer is needed by this example.
#[path = "../../switcher-app/src/sound.rs"]
mod sound;
#[allow(dead_code)] // Production probe() is not needed by the direct B/C/D cases.
#[path = "../src/audio/stream.rs"]
mod stream;

fn samples(case: &str) -> Result<Vec<i16>, Box<dyn std::error::Error>> {
    let cue = sound::pcm_tone(SoundCue::Ru, 0.4).ok_or("no PCM")?;
    Ok(match case {
        "A" | "E" => cue,
        "B" => {
            let mut data = cue;
            data.resize(data.len() + SAMPLE_RATE as usize, 0);
            data
        }
        "C" => {
            let mut data = vec![0; SAMPLE_RATE as usize];
            data.extend(cue);
            data.resize(data.len() + SAMPLE_RATE as usize, 0);
            data
        }
        "D" => (0..SAMPLE_RATE as usize * 2)
            .map(|i| {
                let phase = std::f32::consts::TAU * 660.0 * i as f32 / SAMPLE_RATE as f32;
                let fade = (i as f32 / 220.0).min(1.0)
                    * ((SAMPLE_RATE as usize * 2 - 1 - i) as f32 / 220.0).min(1.0);
                (phase.sin() * fade * 0.4 * i16::MAX as f32).round() as i16
            })
            .collect(),
        _ => return Err("case must be A, B, C, D or E".into()),
    })
}

fn write_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    let mut file = File::create_new(path)?;
    let bytes = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&SAMPLE_RATE.to_le_bytes())?;
    file.write_all(&(SAMPLE_RATE * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

fn meter() -> windows::core::Result<IAudioMeterInformation> {
    // SAFETY: caller's STA outlives enumerator/device/meter; standard render selectors.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        device.Activate(CLSCTX_ALL, None)
    }
}

fn peak(meter: &IAudioMeterInformation) -> windows::core::Result<f32> {
    // SAFETY: live STA-owned interface, read-only normalized peak getter.
    unsafe { meter.GetPeakValue() }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let case = args
        .next()
        .ok_or("usage: audio_compare A|B|C|D|E NEW_WAV [--prepare-only]")?;
    let path = args.next().ok_or("missing NEW_WAV path")?;
    let prepare = match args.next().as_deref() {
        None => false,
        Some("--prepare-only") => true,
        Some(_) => return Err("unknown argument".into()),
    };
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let samples = samples(&case)?;
    write_wav(Path::new(&path), &samples)?;
    println!(
        "case={case} frames={} peak={} wav={path}",
        samples.len(),
        samples.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0)
    );
    if prepare {
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let _sta = stream::Apartment::new()?;
    let meter = meter()?;
    let mut before_peak = 0.0f32;
    for _ in 0..25 {
        before_peak = before_peak.max(peak(&meter)?);
        std::thread::sleep(Duration::from_millis(10));
    }
    let began = Instant::now();
    let mut during_peak = 0.0f32;
    if case == "A" {
        let (events, rx) = crossbeam_channel::unbounded();
        let (device, sender) = sound::SoundDevice::open(events)?;
        sender.play(samples);
        while device.stats().completed == 0 {
            for event in rx.try_iter() {
                if let PlatformEvent::CapabilityChanged(report) = event {
                    if report.state == CapabilityState::Off {
                        return Err(
                            format!("audio failed: {} {}", report.code, report.detail).into()
                        );
                    }
                }
            }
            if began.elapsed() > Duration::from_secs(5) {
                return Err("production cue timeout".into());
            }
            during_peak = during_peak.max(peak(&meter)?);
            std::thread::sleep(Duration::from_millis(5));
        }
        println!("production stats={:?}", device.stats());
    } else {
        let mut burst = stream::Burst::start(samples)?;
        loop {
            if burst.advance()? {
                break;
            }
            loop {
                during_peak = during_peak.max(peak(&meter)?);
                if began.elapsed() > Duration::from_secs(5) {
                    return Err("direct stream timeout".into());
                }
                // SAFETY: event belongs to live burst on this STA. The bounded timeout
                // samples only the meter; it does not drive production buffer submission.
                let waited = unsafe {
                    MsgWaitForMultipleObjectsEx(
                        Some(&[burst.event()]),
                        5,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    )
                };
                if waited == WAIT_FAILED {
                    return Err(windows::core::Error::from_thread().into());
                }
                if waited == WAIT_TIMEOUT {
                    continue;
                }
                if waited != WAIT_OBJECT_0 {
                    let mut message = MSG::default();
                    // SAFETY: this thread's initialized messages; dispatch without alteration.
                    unsafe {
                        for _ in 0..128 {
                            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                                break;
                            }
                            if message.message == WM_QUIT {
                                return Err("audio comparison interrupted by WM_QUIT".into());
                            }
                            let _ = TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                }
                break;
            }
        }
    }
    let elapsed_ms = began.elapsed().as_millis();
    let mut after_peak = 0.0f32;
    for _ in 0..30 {
        after_peak = after_peak.max(peak(&meter)?);
        std::thread::sleep(Duration::from_millis(10));
    }
    println!(
        "case={case} elapsed_ms={elapsed_ms} endpoint_peak_before={before_peak:.6} during={during_peak:.6} after={after_peak:.6}"
    );
    Ok(())
}
