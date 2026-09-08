//! Single-threaded effect execution over ports, with monotonic hide deadlines.

use crate::{
    capability::{CapabilityMap, compose_status, compose_tooltip},
    config_io::ConfigStore,
    menu::MenuCommand,
    render::BadgeCache,
    tray::{Checks, TrayCommand},
};
use crossbeam_channel::{Receiver, Sender};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};
use switcher_core::{
    config::{BadgeMode, Config},
    content::BadgeContent,
    engine::{Effect, Engine, Event},
};
use switcher_platform::{
    events::{
        Capability, CapabilityReport, CapabilityState, LayoutSource, PlatformEvent, ResolvedAnchor,
    },
    ports::*,
};

pub struct Ports {
    pub layout_monitor: Box<dyn LayoutMonitor>,
    pub overlay: Box<dyn OverlayWindow>,
    pub pointer: Box<dyn PointerTracker>,
    pub caret: Box<dyn CaretLocator>,
    pub sound: Box<dyn SoundPlayer>,
    pub autostart: Box<dyn Autostart>,
}

impl std::fmt::Debug for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ports").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct TraySender {
    tx: Sender<TrayCommand>,
    #[cfg(windows)]
    waker: Option<switcher_windows::win_util::PumpWaker>,
    #[cfg(windows)]
    quit: Option<switcher_windows::quit_signal::QuitSignal>,
}

impl TraySender {
    pub fn new(tx: Sender<TrayCommand>) -> Self {
        Self {
            tx,
            #[cfg(windows)]
            waker: None,
            #[cfg(windows)]
            quit: None,
        }
    }
    #[cfg(windows)]
    pub fn with_waker(mut self, waker: switcher_windows::win_util::PumpWaker) -> Self {
        self.waker = Some(waker);
        self
    }
    pub fn send(&self, command: TrayCommand) {
        #[cfg(windows)]
        let shutting_down = matches!(command, TrayCommand::Shutdown);
        if self.tx.send(command).is_err() {
            return;
        }
        #[cfg(windows)]
        if shutting_down {
            if let Some(quit) = &self.quit {
                if let Err(error) = quit.request() {
                    tracing::warn!(%error, "could not request native tray shutdown");
                }
            }
        }
        #[cfg(windows)]
        if let Some(waker) = &self.waker {
            if let Err(error) = waker.wake() {
                tracing::warn!(%error, "could not wake tray");
            }
        }
    }

    #[cfg(windows)]
    pub fn with_quit_signal(mut self, quit: switcher_windows::quit_signal::QuitSignal) -> Self {
        self.quit = Some(quit);
        self
    }
}

#[derive(Debug, Clone)]
struct Shown {
    content: BadgeContent,
    anchor: ResolvedAnchor,
    dpi: u32,
}

#[derive(Debug)]
pub struct Runtime {
    engine: Engine,
    ports: Ports,
    cache: BadgeCache,
    store: ConfigStore,
    tray: TraySender,
    caps: CapabilityMap,
    warnings: BTreeMap<&'static str, String>,
    label: String,
    last: Option<Shown>,
    overlay_visible: bool,
    hide_deadline_ms: Option<u64>,
    initialized: bool,
    layout_initialized: bool,
    quitting: bool,
}

impl Runtime {
    pub fn new(
        config: Config,
        ports: Ports,
        cache: BadgeCache,
        store: ConfigStore,
        tray: TraySender,
        caps: CapabilityMap,
    ) -> Self {
        Self {
            engine: Engine::new(config),
            ports,
            cache,
            store,
            tray,
            caps,
            warnings: BTreeMap::new(),
            label: "??".into(),
            last: None,
            overlay_visible: false,
            hide_deadline_ms: None,
            initialized: false,
            layout_initialized: false,
            quitting: false,
        }
    }

    /// Must run before consuming queued source notifications (ADR-0011).
    pub fn initialize(&mut self, now_ms: u64) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.apply_layout_fallback(self.engine.config().layout.fallback_enabled);
        self.reconcile_autostart(now_ms);
        self.read_layout(LayoutSource::Initial, now_ms);
        self.sync_checks();
        self.publish_status();
    }

    pub fn add_warning(&mut self, key: &'static str, detail: String) {
        if self.warnings.get(key) == Some(&detail) {
            return;
        }
        tracing::warn!(code = key, %detail, "application service unavailable");
        self.warnings.insert(key, detail);
        self.publish_status();
    }

    fn clear_warning(&mut self, key: &'static str) {
        if self.warnings.remove(key).is_some() {
            self.publish_status();
        }
    }

    fn read_layout(&mut self, source: LayoutSource, now_ms: u64) {
        match self.ports.layout_monitor.current() {
            Ok((layout, lang)) => {
                self.clear_warning("layout_read_failed");
                let source = if self.layout_initialized {
                    source
                } else {
                    LayoutSource::Initial
                };
                self.layout_initialized = true;
                tracing::debug!(
                    ?layout,
                    lang = lang.as_str(),
                    ?source,
                    "current foreground layout"
                );
                self.event(
                    Event::Layout {
                        layout,
                        lang,
                        source,
                    },
                    now_ms,
                );
            }
            // Opening our tray menu is an expected pause, not an adapter failure.
            Err(error) if error.code == "foreground_is_own" => {}
            Err(error) => self.add_warning("layout_read_failed", error.to_string()),
        }
    }

    pub fn handle_platform(&mut self, event: PlatformEvent, now_ms: u64) {
        match event {
            PlatformEvent::LayoutChanged {
                layout,
                lang,
                source,
            } => {
                if source == LayoutSource::ForegroundPoll
                    && !self.engine.config().layout.fallback_enabled
                {
                    tracing::trace!("ignoring queued disabled layout fallback notification");
                    return;
                }
                tracing::trace!(
                    ?layout,
                    lang = lang.as_str(),
                    ?source,
                    "layout notification payload"
                );
                self.read_layout(source, now_ms);
            }
            PlatformEvent::PointerMoved { pos: _ } => {
                // A queued sample can belong to the previous visible interval. Resolve
                // the cursor again so it cannot move a newly shown badge backwards.
                if self.overlay_visible {
                    if let Some(pos) = self.ports.pointer.cursor_pos() {
                        if self
                            .last
                            .as_ref()
                            .is_some_and(|last| last.anchor != ResolvedAnchor::Cursor(pos))
                        {
                            self.event(Event::Pointer { pos }, now_ms);
                        }
                    }
                }
            }
            PlatformEvent::OverlayScaleChanged { dpi: _ } => {
                if !self.overlay_visible {
                    return;
                }
                if let Some(last) = self.last.clone() {
                    // Like a layout payload, a queued DPI hint can already be stale.
                    let dpi = self.ports.overlay.dpi_for(last.anchor);
                    if dpi != last.dpi {
                        self.show(last.content, last.anchor, dpi);
                    }
                }
            }
            PlatformEvent::CapabilityChanged(report) => self.report(report),
        }
    }

    pub fn handle_menu(&mut self, command: MenuCommand, now_ms: u64) {
        let config = self.engine.config();
        let event = match command {
            MenuCommand::ToggleFollow => {
                Event::SetMode(if config.badge.mode == BadgeMode::Follow {
                    BadgeMode::Transient
                } else {
                    BadgeMode::Follow
                })
            }
            MenuCommand::ToggleLayoutFallback => {
                Event::SetLayoutFallbackEnabled(!config.layout.fallback_enabled)
            }
            MenuCommand::ToggleSound => Event::SetSoundEnabled(!config.sound.enabled),
            MenuCommand::ToggleAutostart => Event::SetAutostart(!config.autostart),
            MenuCommand::Quit => {
                self.quitting = true;
                self.tray.send(TrayCommand::Shutdown);
                return;
            }
        };
        self.event(event, now_ms);
    }

    pub fn reconcile_autostart(&mut self, now_ms: u64) {
        match self.ports.autostart.is_enabled() {
            Ok(actual) => {
                self.report(CapabilityReport {
                    capability: Capability::Autostart,
                    state: CapabilityState::Ok,
                    code: "registry_state_read",
                    detail: "Autostart state is known".into(),
                });
                self.event(
                    Event::AutostartApplied {
                        requested: actual,
                        ok: true,
                    },
                    now_ms,
                );
            }
            Err(error) => self.report_error(Capability::Autostart, error),
        }
    }

    pub fn timeout(&self, now_ms: u64) -> Option<Duration> {
        self.hide_deadline_ms
            .map(|deadline| Duration::from_millis(deadline.saturating_sub(now_ms)))
    }

    pub fn fire_hide_timer(&mut self, now_ms: u64) {
        if self
            .hide_deadline_ms
            .is_some_and(|deadline| now_ms >= deadline)
        {
            self.hide_deadline_ms = None;
            self.event(Event::HideTimerFired, now_ms);
        }
    }

    fn event(&mut self, event: Event, now_ms: u64) {
        let effects = self.engine.handle(event, now_ms);
        self.dispatch(effects, now_ms);
    }

    fn dispatch(&mut self, effects: Vec<Effect>, now_ms: u64) {
        let mut queue: VecDeque<_> = effects.into();
        let mut steps = 0;
        while let Some(effect) = queue.pop_front() {
            steps += 1;
            if steps > 64 {
                self.add_warning("effect_loop", "Effect dispatch did not converge".into());
                self.quitting = true;
                break;
            }
            match effect {
                Effect::QueryAnchor => {
                    // Answer synchronously in this pass: never leave the core awaiting.
                    let caret = self.ports.caret.caret_point();
                    let cursor = self.ports.pointer.cursor_pos();
                    queue.extend(
                        self.engine
                            .handle(Event::AnchorResolved { caret, cursor }, now_ms),
                    );
                }
                Effect::ShowBadge { content, anchor } => {
                    let dpi = self.ports.overlay.dpi_for(anchor);
                    self.show(content, anchor, dpi);
                }
                Effect::MoveBadge { anchor } => {
                    if !self.overlay_visible {
                        continue;
                    }
                    if let Some(last) = &mut self.last {
                        last.anchor = anchor;
                        self.ports.overlay.move_to(anchor);
                    }
                }
                Effect::HideBadge => {
                    self.ports.overlay.hide();
                    self.last = None;
                    self.overlay_visible = false;
                }
                Effect::ArmHideTimer { after_ms } => {
                    self.hide_deadline_ms = Some(now_ms.saturating_add(after_ms))
                }
                Effect::CancelHideTimer => self.hide_deadline_ms = None,
                Effect::SetPointerTracking(active) => self.ports.pointer.set_active(
                    active
                        && self.overlay_visible
                        && self.caps.state(Capability::Pointer) != CapabilityState::Off,
                ),
                Effect::PlaySound { cue, volume } => self.ports.sound.play(cue, volume),
                Effect::UpdateTray { label, lang } => {
                    self.label = label;
                    let config = self.engine.config();
                    let content =
                        BadgeContent::for_lang(&lang, config.badge.style, &config.badge.colors);
                    match self.cache.tray_rgba(&content, 16) {
                        Ok(rgba_straight) => {
                            self.tray.send(TrayCommand::SetIcon {
                                rgba_straight,
                                size: 16,
                            });
                            self.clear_warning("tray_render_failed");
                        }
                        Err(error) => self.add_warning("tray_render_failed", error.to_string()),
                    }
                    self.publish_status();
                }
                Effect::SetLayoutFallbackEnabled(enabled) => {
                    self.apply_layout_fallback(enabled);
                }
                Effect::ApplyAutostart(want) => {
                    let actual = match self.ports.autostart.set_enabled(want) {
                        Ok(()) => {
                            self.report(CapabilityReport {
                                capability: Capability::Autostart,
                                state: CapabilityState::Ok,
                                code: "registry_write_succeeded",
                                detail: "Autostart updated".into(),
                            });
                            Some(want)
                        }
                        Err(error) => {
                            self.report_error(Capability::Autostart, error);
                            // Failure says nothing about external drift. Re-read OS truth,
                            // and keep the write failure visible even if reading succeeds.
                            match self.ports.autostart.is_enabled() {
                                Ok(actual) => Some(actual),
                                Err(error) => {
                                    self.report_error(Capability::Autostart, error);
                                    None
                                }
                            }
                        }
                    };
                    queue.extend(self.engine.handle(
                        Event::AutostartApplied {
                            requested: actual.unwrap_or(want),
                            ok: actual.is_some(),
                        },
                        now_ms,
                    ));
                    // Even when the confirmed value equals config, snap the library's
                    // optimistic checkbox back to that value.
                    queue.push_back(Effect::SyncTrayMenu);
                }
                Effect::PersistConfig => {
                    match self.store.save(self.engine.config()) {
                        Ok(()) => self.clear_warning("config_write_failed"),
                        Err(error) => self.add_warning("config_write_failed", error.to_string()),
                    }
                    self.cache.clear();
                    self.sync_checks();
                }
                Effect::SyncTrayMenu => self.sync_checks(),
            }
        }
    }

    fn show(&mut self, content: BadgeContent, anchor: ResolvedAnchor, dpi: u32) {
        if self.caps.state(Capability::Overlay) == CapabilityState::Off {
            return;
        }
        match self.cache.image(&content, dpi) {
            Ok(image) => {
                self.ports.overlay.show(image, anchor);
                self.overlay_visible = self.caps.state(Capability::Overlay) == CapabilityState::Ok;
                self.last = Some(Shown {
                    content,
                    anchor,
                    dpi,
                });
                self.clear_warning("badge_render_failed");
            }
            Err(error) => {
                self.ports.overlay.hide();
                self.ports.pointer.set_active(false);
                self.last = None;
                self.overlay_visible = false;
                self.add_warning("badge_render_failed", error.to_string());
            }
        }
    }

    fn apply_layout_fallback(&mut self, enabled: bool) {
        match self.ports.layout_monitor.set_fallback_enabled(enabled) {
            Ok(()) => self.clear_warning("layout_fallback_update_failed"),
            Err(error) => self.add_warning("layout_fallback_update_failed", error.to_string()),
        }
    }

    fn report_error(&mut self, capability: Capability, error: PlatformError) {
        self.report(CapabilityReport {
            capability,
            state: CapabilityState::Off,
            code: error.code,
            detail: error.detail,
        });
    }

    fn report(&mut self, report: CapabilityReport) {
        if self.caps.apply(report.clone()) {
            if report.capability == Capability::Overlay {
                self.overlay_visible = report.state == CapabilityState::Ok && self.last.is_some();
                if self.overlay_visible {
                    if let Some(last) = self.last.clone() {
                        // The native show can emit a DPI hint before its recovery Ok.
                        // Re-check here because hints were ignored while unavailable.
                        let dpi = self.ports.overlay.dpi_for(last.anchor);
                        if dpi != last.dpi {
                            self.show(last.content, last.anchor, dpi);
                        }
                    }
                }
                if report.state == CapabilityState::Off {
                    self.last = None;
                }
                let active = self.overlay_visible
                    && self
                        .last
                        .as_ref()
                        .is_some_and(|last| matches!(last.anchor, ResolvedAnchor::Cursor(_)))
                    && self.caps.state(Capability::Pointer) != CapabilityState::Off;
                self.ports.pointer.set_active(active);
            }
            if report.state == CapabilityState::Ok {
                tracing::info!(
                    cap = report.capability.key(),
                    code = report.code,
                    detail = report.detail,
                    "capability ready"
                );
            } else {
                tracing::warn!(cap = report.capability.key(), state = ?report.state, code = report.code, detail = report.detail, "capability limited");
            }
            self.publish_status();
        }
    }

    fn sync_checks(&self) {
        let config = self.engine.config();
        self.tray.send(TrayCommand::SyncChecks(Checks {
            follow: config.badge.mode == BadgeMode::Follow,
            layout_fallback: config.layout.fallback_enabled,
            sound: config.sound.enabled,
            autostart: config.autostart,
        }));
    }

    fn publish_status(&self) {
        let mut rows = compose_status(&self.caps);
        if !self.warnings.is_empty() && self.caps.degraded().next().is_none() {
            rows.clear();
        }
        rows.extend(
            self.warnings
                .iter()
                .map(|(key, value)| format!("{key}: {}", value.replace(['\r', '\n', '\t'], " "))),
        );
        let tooltip = compose_tooltip(&self.label, &self.caps);
        let tooltip = if self.warnings.is_empty() {
            tooltip
        } else {
            format!("{tooltip}\n⚠ Подробности в «Состояние»")
        };
        self.tray
            .send(TrayCommand::SetTooltip(crate::capability::truncate_utf16(
                &tooltip, 126,
            )));
        self.tray.send(TrayCommand::SetStatus {
            rows,
            autostart_available: self.caps.state(Capability::Autostart) != CapabilityState::Off,
        });
    }
}

/// No periodic timer: block on channels, with a timeout only for a visible transient
/// badge. Check the absolute deadline before *every* select, including pointer floods.
pub fn run(
    runtime: Runtime,
    platform: Receiver<PlatformEvent>,
    menu: Receiver<MenuCommand>,
    stop: Receiver<()>,
) {
    let origin = Instant::now();
    let now = || origin.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    run_with_clock(runtime, platform, menu, stop, now);
}

fn run_with_clock(
    mut runtime: Runtime,
    platform: Receiver<PlatformEvent>,
    menu: Receiver<MenuCommand>,
    stop: Receiver<()>,
    mut now: impl FnMut() -> u64,
) {
    runtime.initialize(now());
    while !runtime.quitting {
        if !matches!(stop.try_recv(), Err(crossbeam_channel::TryRecvError::Empty)) {
            break;
        }
        runtime.fire_hide_timer(now());
        let mut select = crossbeam_channel::Select::new();
        let stop_index = select.recv(&stop);
        let platform_index = select.recv(&platform);
        let menu_index = select.recv(&menu);
        let selected = match runtime.timeout(now()) {
            Some(timeout) => select.select_timeout(timeout).ok(),
            None => Some(select.select()),
        };
        let Some(operation) = selected else {
            continue;
        };
        // Complete the selected receive before invoking ports. Dropping a reserved
        // crossbeam operation during an unrelated unwind would itself panic.
        let incoming = match operation.index() {
            index if index == stop_index => {
                let _ = operation.recv(&stop);
                break;
            }
            index if index == platform_index => match operation.recv(&platform) {
                Ok(event) => Ok(event),
                Err(_) => break,
            },
            index if index == menu_index => match operation.recv(&menu) {
                Ok(command) => Err(command),
                Err(_) => break,
            },
            _ => unreachable!("only three receivers are registered"),
        };
        let at = now();
        runtime.fire_hide_timer(at);
        match incoming {
            Ok(event) => runtime.handle_platform(event, at),
            Err(command) => runtime.handle_menu(command, at),
        }
    }
    runtime.ports.pointer.set_active(false);
    runtime.ports.overlay.hide();
    runtime.tray.send(TrayCommand::Shutdown);
    tracing::info!("core loop stopped");
    // All port owners drop here; their native threads join before run returns.
}

#[cfg(test)]
mod tests;
