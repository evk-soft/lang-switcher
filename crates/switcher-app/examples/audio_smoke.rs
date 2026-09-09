//! Bounded device test: real WASAPI buffers, optional audible cues, idle before/after.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    use switcher_app::sound::{SoundDevice, WasapiSoundPlayer};
    use switcher_platform::{
        events::{CapabilityState, PlatformEvent},
        ports::{SoundCue, SoundPlayer},
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let audible = std::env::args().any(|arg| arg == "--audible");
    let (events, rx) = crossbeam_channel::unbounded();
    let (device, pcm) = SoundDevice::open(events)?;
    let player = WasapiSoundPlayer(pcm.clone());
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        device.stats().started,
        0,
        "opening the owner must not start playback"
    );
    for index in 0..20 {
        let before = device.stats().completed;
        let began = Instant::now();
        if audible {
            println!(
                "LISTEN {}/20: {}",
                index + 1,
                if index % 2 == 0 {
                    "RU (lower tone)"
                } else {
                    "EN (higher tone)"
                }
            );
            player.play(
                if index % 2 == 0 {
                    SoundCue::Ru
                } else {
                    SoundCue::En
                },
                0.4,
            );
        } else {
            pcm.play(vec![0; 3969]);
        }
        while device.stats().completed == before {
            if began.elapsed() > Duration::from_secs(4) {
                anyhow::bail!("cue did not complete: {:?}", device.stats());
            }
            for event in rx.try_iter() {
                if let PlatformEvent::CapabilityChanged(report) = event
                    && report.state == CapabilityState::Off
                {
                    anyhow::bail!("audio unavailable: {} {}", report.code, report.detail);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        println!(
            "cue={index} elapsed_ms={} stats={:?}",
            began.elapsed().as_millis(),
            device.stats()
        );
        if audible {
            // Listening is a separate diagnostic mode, not a latency benchmark.
            // Leave enough silence to distinguish two adjacent 90ms cues by ear.
            std::thread::sleep(Duration::from_millis(900));
        }
    }
    std::thread::sleep(Duration::from_secs(2));
    assert!(!device.stats().active);
    let before = Instant::now();
    pcm.play(vec![0; 3969]);
    let start_deadline = Instant::now() + Duration::from_secs(2);
    while !device.stats().active && Instant::now() < start_deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let was_active = device.stats().active;
    drop(device); // Sender is still alive, so channel disconnection cannot be the stop condition.
    assert!(was_active, "shutdown regression did not start a stream");
    assert!(
        before.elapsed() < Duration::from_secs(2),
        "shutdown waited for a live sender"
    );
    pcm.play(vec![0; 3969]); // Revoked sender cannot restart the worker.
    println!(
        "shutdown_ms={} live_sender=true",
        before.elapsed().as_millis()
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
