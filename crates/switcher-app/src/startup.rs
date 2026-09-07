//! Windows assembly. Native objects and the audio device live on the main thread.

use crate::{
    capability::{CapabilityMap, compose_status},
    config_io::ConfigStore,
    menu::MenuCommand,
    paths::{self, AppPaths},
    render::{BadgeCache, BadgeMetrics, FONT},
    runtime::{self, Ports, Runtime, TraySender},
    sound::{NullSoundPlayer, SoundDevice},
    tray::{self, Checks, TrayCommand, TrayInit},
};
use anyhow::{Context, bail};
use crossbeam_channel::{Sender, bounded, unbounded};
use std::{path::PathBuf, time::Duration};
use switcher_core::content::{BadgeContent, BadgeStyle};
use switcher_platform::{
    events::{
        BadgeImage, Capability, CapabilityReport, CapabilityState, LangTag, PlatformEvent, Point,
        ResolvedAnchor,
    },
    ports::*,
};
use switcher_windows::{
    autostart::RegistryAutostart, dpi, layout_monitor::LayoutHooks, overlay::Overlay,
    pointer::Pointer, tsf::TsfSource, win_util::current_thread_waker,
};

#[derive(Debug, Default)]
pub struct Options {
    /// Explicit data directory for portable diagnostics; normal launches use AppData.
    pub data_dir: Option<PathBuf>,
    /// Optional bounded diagnostic run. No periodic timer is created otherwise.
    pub run_for: Option<Duration>,
}

impl Options {
    pub fn from_args() -> anyhow::Result<Self> {
        let mut options = Self::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--data-dir") => {
                    options.data_dir = Some(PathBuf::from(
                        args.next().context("--data-dir requires a path")?,
                    ))
                }
                Some("--run-for") => {
                    let arg = args.next().context("--run-for requires seconds")?;
                    let seconds: u64 = arg
                        .to_str()
                        .context("invalid duration")?
                        .parse()
                        .context("invalid duration")?;
                    if !(1..=3600).contains(&seconds) {
                        bail!("--run-for must be 1..3600 seconds");
                    }
                    options.run_for = Some(Duration::from_secs(seconds));
                }
                _ => bail!("usage: lang-switcher [--data-dir PATH] [--run-for SECONDS]"),
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
struct NullOverlay;
impl OverlayWindow for NullOverlay {
    fn show(&self, _: &BadgeImage, _: ResolvedAnchor) {}
    fn move_to(&self, _: ResolvedAnchor) {}
    fn hide(&self) {}
    fn dpi_for(&self, _: ResolvedAnchor) -> u32 {
        96
    }
}

#[derive(Debug)]
struct NullPointer;
impl PointerTracker for NullPointer {
    fn set_active(&self, _: bool) {}
    fn cursor_pos(&self) -> Option<Point> {
        None
    }
}

fn unavailable(caps: &mut CapabilityMap, capability: Capability, error: PlatformError) {
    tracing::warn!(cap = capability.key(), state = ?CapabilityState::Off, code = error.code,
        detail = error.detail, "capability unavailable at startup");
    caps.apply(CapabilityReport {
        capability,
        state: CapabilityState::Off,
        code: error.code,
        detail: error.detail,
    });
}

pub fn run(options: Options) -> anyhow::Result<()> {
    // Handler installation must precede the first event (OnceCell in both libraries).
    let (events_tx, events_rx) = unbounded::<PlatformEvent>();
    let (menu_tx, menu_rx) = unbounded::<MenuCommand>();
    tray::install_event_handlers(menu_tx);
    let dpi_report = dpi::ensure_per_monitor_v2(); // Before any HWND.
    let waker = current_thread_waker(); // Queue exists before producers can wake it.
    let paths = match options.data_dir {
        Some(dir) => AppPaths {
            config_file: dir.join("config.toml"),
            log_dir: dir.join("logs"),
        },
        None => paths::resolve()?,
    };
    let loaded = ConfigStore::load(paths.config_file);
    let mut warnings: Vec<(&'static str, String)> = loaded
        .warnings
        .iter()
        .map(|w| ("config_load", w.clone()))
        .collect();
    // Named guard remains alive until all service and thread owners have dropped.
    let _log_guard = match crate::logging::init(&paths.log_dir, &loaded.config.log_level) {
        Ok(guard) => Some(guard),
        Err(error) => {
            warnings.push(("logging_unavailable", error.to_string()));
            // Useful for debug/diagnostic launches; the tray also displays this failure.
            let _ = tracing_subscriber::fmt().with_ansi(false).try_init();
            None
        }
    };
    tracing::info!(?dpi_report, config = %loaded.store.path().display(), "lang-switcher starting");
    if !dpi_report.per_monitor_v2 {
        warnings.push((
            "dpi_awareness",
            "Per-monitor DPI awareness is unavailable".into(),
        ));
    }
    let mut caps = CapabilityMap::default();
    let quit_window = switcher_windows::quit_signal::QuitWindow::new()?;
    // Startup Sound=Ok lives in caps before processing any asynchronous stream Off.
    let (sound_device, sound): (Option<SoundDevice>, Box<dyn SoundPlayer>) =
        match SoundDevice::open(events_tx.clone()) {
            Ok((device, player)) => (Some(device), Box::new(player)),
            Err(error) => {
                unavailable(
                    &mut caps,
                    Capability::Sound,
                    PlatformError::new("no_output_device", error.to_string()),
                );
                (None, Box::new(NullSoundPlayer))
            }
        };
    let overlay: Box<dyn OverlayWindow> = match Overlay::new(events_tx.clone()) {
        Ok(overlay) => Box::new(overlay),
        Err(error) => {
            unavailable(&mut caps, Capability::Overlay, error);
            Box::new(NullOverlay)
        }
    };
    let pointer: Box<dyn PointerTracker> = match Pointer::new(events_tx.clone()) {
        Ok(pointer) => Box::new(pointer),
        Err(error) => {
            unavailable(&mut caps, Capability::Pointer, error);
            Box::new(NullPointer)
        }
    };
    let layout_monitor: Box<dyn LayoutMonitor> = match LayoutHooks::new(events_tx.clone()) {
        Ok(monitor) => Box::new(monitor),
        Err(error) => {
            unavailable(&mut caps, Capability::LayoutShellHook, error.clone());
            unavailable(&mut caps, Capability::LayoutForegroundHook, error);
            Box::new(LayoutHooks::reader_only())
        }
    };
    let tsf = match TsfSource::new(events_tx.clone()) {
        Ok(source) => Some(source),
        Err(error) => {
            unavailable(&mut caps, Capability::LayoutTsf, error);
            None
        }
    };
    unavailable(
        &mut caps,
        Capability::Caret,
        PlatformError::new(
            "caret_not_implemented",
            "Caret anchoring is planned for M2; using cursor or fixed position",
        ),
    );
    let cache = BadgeCache::new(FONT, BadgeMetrics::default())?;
    let unknown = BadgeContent::for_lang(
        &LangTag::new(""),
        BadgeStyle::Text,
        &loaded.config.badge.colors,
    );
    let init = TrayInit {
        rgba: cache.tray_rgba(&unknown, 16)?,
        size: 16,
        tooltip: "lang-switcher · запуск".into(),
        status: compose_status(&caps),
        checks: Checks {
            follow: loaded.config.badge.mode == switcher_core::config::BadgeMode::Follow,
            sound: loaded.config.sound.enabled,
            autostart: loaded.config.autostart,
        },
        autostart_available: true,
    };
    let ports = Ports {
        layout_monitor,
        overlay,
        pointer,
        caret: Box::new(NullCaretLocator),
        sound,
        autostart: Box::new(RegistryAutostart::new()),
    };
    let (tray_tx, tray_rx) = unbounded();
    let tray_sender = TraySender::new(tray_tx)
        .with_waker(waker)
        .with_quit_signal(quit_window.signal());
    let mut runtime = Runtime::new(
        loaded.config,
        ports,
        cache,
        loaded.store,
        tray_sender.clone(),
        caps,
    );
    for (key, detail) in warnings {
        runtime.add_warning(key, detail);
    }
    // Startup state is applied synchronously before the source queue is consumed.
    runtime.initialize(0);
    let (stop_tx, stop_rx) = bounded(1);
    let (done_tx, done_rx) = bounded(1);
    // Finish all fallible helper creation before transferring native owners to core.
    let diagnostic = DiagnosticStop::new(options.run_for, stop_tx.clone())?;
    let worker = std::thread::Builder::new()
        .name("core".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _tsf_source = tsf;
                runtime::run(runtime, events_rx, menu_rx, stop_rx);
            }));
            if outcome.is_err() {
                tracing::error!("core thread panicked; shutting down");
            }
            tray_sender.send(TrayCommand::Shutdown);
            // Sent only after all native port/TSF owners dropped and joined their pumps.
            let _ = done_tx.send(outcome.is_ok());
        })
        .context("could not create the core thread")?;
    let tray_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tray::run_tray(init, tray_rx)
    }))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("tray thread panicked")));
    // Cloned adapter senders do not keep shutdown waiting for channel disconnection.
    let _ = stop_tx.try_send(());
    drop(diagnostic);
    let clean = match done_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(clean) => clean,
        Err(error) => {
            tracing::error!(%error, "native threads did not acknowledge shutdown within 5 seconds");
            return Err(error).context("native shutdown timed out or failed");
        }
    };
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("core completion thread panicked"))?;
    drop(sound_device);
    drop(events_tx);
    tracing::info!(clean, "lang-switcher stopped");
    tray_result?;
    if !clean {
        bail!("core thread failed; see log");
    }
    Ok(())
}

struct DiagnosticStop {
    cancel: Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DiagnosticStop {
    fn new(duration: Option<Duration>, stop: Sender<()>) -> anyhow::Result<Self> {
        let (cancel, cancellation) = bounded(1);
        let thread = duration
            .map(|duration| {
                std::thread::Builder::new()
                    .name("diagnostic-stop".into())
                    .spawn(move || {
                        if cancellation.recv_timeout(duration)
                            == Err(crossbeam_channel::RecvTimeoutError::Timeout)
                        {
                            let _ = stop.try_send(());
                        }
                    })
            })
            .transpose()?;
        Ok(Self { cancel, thread })
    }
}

impl Drop for DiagnosticStop {
    fn drop(&mut self) {
        let _ = self.cancel.try_send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
