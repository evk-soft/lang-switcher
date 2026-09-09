use super::*;
use crate::{
    config_io::tests::TempDir,
    render::{BadgeMetrics, FONT},
};
use std::sync::{Arc, Mutex};
use switcher_platform::events::{BadgeImage, LangTag, LayoutId, Point};

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Show(u32, ResolvedAnchor),
    Move(ResolvedAnchor),
    Hide,
    Track(bool),
    Sound,
}

#[derive(Debug)]
struct State {
    snapshot: Result<(LayoutId, LangTag), PlatformError>,
    layout_reads: usize,
    fallback_switches: Vec<bool>,
    fallback_error: Option<PlatformError>,
    cursor: Option<Point>,
    dpi: u32,
    actual_autostart: Result<bool, PlatformError>,
    write_error: Option<PlatformError>,
    ui_languages: Result<Vec<String>, PlatformError>,
    ui_language_reads: usize,
    calls: Vec<Call>,
}

#[derive(Debug, Clone)]
struct Mock(Arc<Mutex<State>>);
impl LayoutMonitor for Mock {
    fn current(&self) -> Result<(LayoutId, LangTag), PlatformError> {
        let mut state = self.0.lock().unwrap();
        state.layout_reads += 1;
        state.snapshot.clone()
    }

    fn set_fallback_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        let mut state = self.0.lock().unwrap();
        state.fallback_switches.push(enabled);
        match &state.fallback_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}
impl PointerTracker for Mock {
    fn set_active(&self, active: bool) {
        self.0.lock().unwrap().calls.push(Call::Track(active));
    }
    fn cursor_pos(&self) -> Option<Point> {
        self.0.lock().unwrap().cursor
    }
}
impl CaretLocator for Mock {
    fn caret_point(&self) -> Option<Point> {
        None
    }
}
impl OverlayWindow for Mock {
    fn show(&self, image: &BadgeImage, anchor: ResolvedAnchor) {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(Call::Show(image.dpi, anchor));
    }
    fn move_to(&self, anchor: ResolvedAnchor) {
        self.0.lock().unwrap().calls.push(Call::Move(anchor));
    }
    fn hide(&self) {
        self.0.lock().unwrap().calls.push(Call::Hide);
    }
    fn dpi_for(&self, _: ResolvedAnchor) -> u32 {
        self.0.lock().unwrap().dpi
    }
}
impl SoundPlayer for Mock {
    fn play(&self, _: SoundCue, _: f32) {
        self.0.lock().unwrap().calls.push(Call::Sound);
    }
}
impl UiLanguages for Mock {
    fn preferred(&self) -> Result<Vec<String>, PlatformError> {
        let mut state = self.0.lock().unwrap();
        state.ui_language_reads += 1;
        state.ui_languages.clone()
    }
}
impl Autostart for Mock {
    fn is_enabled(&self) -> Result<bool, PlatformError> {
        self.0.lock().unwrap().actual_autostart.clone()
    }
    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError> {
        let mut state = self.0.lock().unwrap();
        if let Some(error) = &state.write_error {
            return Err(error.clone());
        }
        state.actual_autostart = Ok(enabled);
        Ok(())
    }
}

fn fixture(config: Config) -> (Runtime, Mock, Receiver<TrayCommand>, TempDir) {
    let mock = Mock(Arc::new(Mutex::new(State {
        snapshot: Ok((LayoutId(1), LangTag::new("ru-RU"))),
        layout_reads: 0,
        fallback_switches: vec![],
        fallback_error: None,
        cursor: Some(Point { x: 10, y: 20 }),
        dpi: 144,
        actual_autostart: Ok(false),
        write_error: None,
        ui_languages: Ok(vec!["de-DE".to_owned()]),
        ui_language_reads: 0,
        calls: vec![],
    })));
    let ports = Ports {
        layout_monitor: Box::new(mock.clone()),
        overlay: Box::new(mock.clone()),
        pointer: Box::new(mock.clone()),
        caret: Box::new(mock.clone()),
        sound: Box::new(mock.clone()),
        autostart: Box::new(mock.clone()),
        ui_languages: Box::new(mock.clone()),
    };
    let dir = TempDir::new();
    let loaded = ConfigStore::load(dir.0.join("config.toml"));
    let (tx, rx) = crossbeam_channel::unbounded();
    let translator = Translator::for_config(&config.ui_language, &[]);
    (
        Runtime::new(
            config,
            ports,
            BadgeCache::new(FONT, BadgeMetrics::default()).unwrap(),
            loaded.store,
            TraySender::new(tx),
            CapabilityMap::default(),
            translator,
        ),
        mock,
        rx,
        dir,
    )
}

fn notice() -> PlatformEvent {
    PlatformEvent::LayoutChanged {
        layout: LayoutId(999),
        lang: LangTag::new("stale"),
        source: LayoutSource::ShellHook,
    }
}

#[test]
fn queued_layout_read_at_our_menu_keeps_the_badge_without_a_warning_or_sound() {
    let (mut runtime, mock, _rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    mock.0.lock().unwrap().snapshot = Err(PlatformError::new("foreground_is_own", "own menu"));
    runtime.handle_platform(
        PlatformEvent::LayoutChanged {
            layout: LayoutId(2),
            lang: LangTag::new("en"),
            source: LayoutSource::ForegroundPoll,
        },
        100,
    );
    assert_eq!(runtime.label, "RU");
    assert!(calls(&mock).is_empty());
    assert!(!runtime.warnings.contains_key("layout_read_failed"));
    runtime.handle_platform(notice(), 101);
    assert!(!runtime.warnings.contains_key("layout_read_failed"));
}

fn set_layout(mock: &Mock, id: u64, lang: &str) {
    mock.0.lock().unwrap().snapshot = Ok((LayoutId(id), LangTag::new(lang)));
}

fn calls(mock: &Mock) -> Vec<Call> {
    mock.0.lock().unwrap().calls.clone()
}

fn tooltip_texts(rx: &Receiver<TrayCommand>) -> Vec<String> {
    rx.try_iter()
        .filter_map(|command| match command {
            TrayCommand::SetTooltip(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn choosing_a_language_relabels_the_tray_and_survives_a_restart() {
    let (mut runtime, mock, rx, dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);

    runtime.handle_menu(MenuCommand::SetUiLanguage("ru".into()), 10);

    assert_eq!(runtime.engine.config().ui_language, "ru");
    assert_eq!(runtime.tr.locale().tag, "ru");
    let commands: Vec<TrayCommand> = rx.try_iter().collect();
    let relabelled = commands.iter().find_map(|command| match command {
        TrayCommand::Localize(labels) => Some(labels.clone()),
        _ => None,
    });
    assert_eq!(
        relabelled.expect("the tray is relabelled").quit,
        "Выход",
        "the menu must be redrawn in the chosen language"
    );
    assert!(commands.iter().any(|command| matches!(
        command,
        TrayCommand::SyncChecks(Checks { ui_language, .. }) if ui_language == "ru"
    )));
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, TrayCommand::SetTooltip(_))),
        "the tooltip is user-visible text and must be redrawn too"
    );

    // Nothing about a menu language is an input event.
    assert!(!calls(&mock).contains(&Call::Sound));
    assert!(
        !calls(&mock)
            .iter()
            .any(|call| matches!(call, Call::Show(..))),
        "picking a language must not pop up a badge"
    );

    assert_eq!(
        ConfigStore::load(dir.0.join("config.toml"))
            .config
            .ui_language,
        "ru",
        "the choice must outlive the process"
    );
}

/// The menu library unticks a check item itself before it hands us the click, so a click
/// on the already-selected language must still produce a `SyncChecks`. Without one the
/// language submenu would sit with nothing ticked, indefinitely, while the config is
/// unchanged — it is the only place the active interface language is shown.
#[test]
fn re_selecting_the_current_language_still_re_asserts_the_check_marks() {
    let (mut runtime, _mock, rx, dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);
    assert_eq!(runtime.engine.config().ui_language, "auto");

    runtime.handle_menu(MenuCommand::SetUiLanguage("auto".into()), 10);

    let commands: Vec<TrayCommand> = rx.try_iter().collect();
    assert!(
        commands.iter().any(|command| matches!(
            command,
            TrayCommand::SyncChecks(Checks { ui_language, .. }) if ui_language == "auto"
        )),
        "the menu must be re-synced even though nothing changed: {commands:?}"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, TrayCommand::Localize(_))),
        "nothing changed, so there is nothing to relabel"
    );
    assert!(
        !dir.0.join("config.toml").exists(),
        "a no-op click must not write the config file"
    );

    // The same must hold for an explicitly chosen language, not just the default.
    runtime.handle_menu(MenuCommand::SetUiLanguage("de".into()), 20);
    rx.try_iter().for_each(drop);
    runtime.handle_menu(MenuCommand::SetUiLanguage("de".into()), 30);
    assert!(rx.try_iter().any(|command| matches!(
        command,
        TrayCommand::SyncChecks(Checks { ui_language, .. }) if ui_language == "de"
    )));
    assert_eq!(runtime.tr.locale().tag, "de");
}

/// "auto" is the only value that consults the OS, and it must not be consulted on any
/// other path — a layout switch must never turn into a display-language query.
#[test]
fn auto_reads_the_system_preference_and_an_explicit_choice_does_not() {
    let (mut runtime, mock, rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);
    let reads_after_startup = mock.0.lock().unwrap().ui_language_reads;

    set_layout(&mock, 2, "en-US");
    runtime.handle_platform(notice(), 10);
    assert_eq!(
        mock.0.lock().unwrap().ui_language_reads,
        reads_after_startup,
        "a layout change must not query the display language"
    );

    runtime.handle_menu(MenuCommand::SetUiLanguage("fr".into()), 20);
    assert_eq!(runtime.tr.locale().tag, "fr");
    assert_eq!(
        mock.0.lock().unwrap().ui_language_reads,
        reads_after_startup,
        "an explicit language must not query the OS"
    );

    runtime.handle_menu(MenuCommand::SetUiLanguage("auto".into()), 30);
    assert_eq!(
        mock.0.lock().unwrap().ui_language_reads,
        reads_after_startup + 1
    );
    assert_eq!(
        runtime.tr.locale().tag,
        "de",
        "the mock reports de-DE as the display language"
    );
}

/// The port is allowed to fail. English is a working menu; an empty one is not.
#[test]
fn a_failing_display_language_query_falls_back_to_english() {
    let (mut runtime, mock, rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    mock.0.lock().unwrap().ui_languages = Err(PlatformError::new("test_failure", "no answer"));
    rx.try_iter().for_each(drop);

    runtime.handle_menu(MenuCommand::SetUiLanguage("ru".into()), 10);
    runtime.handle_menu(MenuCommand::SetUiLanguage("auto".into()), 20);

    assert_eq!(runtime.tr.locale().tag, "en");
    assert_eq!(runtime.engine.config().ui_language, "auto");
    assert!(
        runtime.warnings.is_empty(),
        "a display-language read is not a capability failure (ADR-0022)"
    );
}

#[test]
fn the_tooltip_follows_the_chosen_language() {
    let (mut runtime, mock, rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    set_layout(&mock, 2, "ru-RU");
    runtime.handle_platform(notice(), 10);
    rx.try_iter().for_each(drop);

    runtime.handle_menu(MenuCommand::SetUiLanguage("ru".into()), 20);
    let russian = tooltip_texts(&rx);
    assert!(
        russian.iter().any(|text| text.contains("RU")),
        "the badge label stays in the tooltip: {russian:?}"
    );

    // Degrade something so the tooltip has a translated word in it, not just the label.
    runtime.handle_platform(
        PlatformEvent::CapabilityChanged(CapabilityReport {
            capability: Capability::Sound,
            state: CapabilityState::Off,
            code: "test_failure",
            detail: "Unavailable".into(),
        }),
        30,
    );
    assert!(
        tooltip_texts(&rx).iter().any(|text| text.contains("звук")),
        "the warning line must be in the chosen language"
    );

    runtime.handle_menu(MenuCommand::SetUiLanguage("de".into()), 40);
    assert!(
        tooltip_texts(&rx).iter().any(|text| text.contains("Ton")),
        "switching language must redraw the warning line"
    );
}

#[test]
fn startup_applies_disabled_fallback_and_syncs_its_checkmark() {
    let mut config = Config::default();
    config.layout.fallback_enabled = false;
    let (mut runtime, mock, rx, _dir) = fixture(config);
    runtime.initialize(0);
    assert_eq!(mock.0.lock().unwrap().fallback_switches, [false]);
    assert!(rx.try_iter().any(|command| matches!(
        command,
        TrayCommand::SyncChecks(Checks {
            layout_fallback: false,
            ..
        })
    )));
}

#[test]
fn layout_fallback_toggle_updates_port_checkmark_and_persisted_preference() {
    let (mut runtime, mock, rx, dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);

    runtime.handle_menu(MenuCommand::ToggleLayoutFallback, 10);
    assert!(!runtime.engine.config().layout.fallback_enabled);
    assert!(
        !ConfigStore::load(dir.0.join("config.toml"))
            .config
            .layout
            .fallback_enabled
    );
    assert!(rx.try_iter().any(|command| matches!(
        command,
        TrayCommand::SyncChecks(Checks {
            layout_fallback: false,
            ..
        })
    )));

    runtime.handle_menu(MenuCommand::ToggleLayoutFallback, 20);
    assert!(runtime.engine.config().layout.fallback_enabled);
    assert!(
        ConfigStore::load(dir.0.join("config.toml"))
            .config
            .layout
            .fallback_enabled
    );
    assert_eq!(
        mock.0.lock().unwrap().fallback_switches,
        [true, false, true]
    );
}

#[test]
fn fallback_port_error_keeps_and_persists_user_preference() {
    let (mut runtime, mock, rx, dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);
    mock.0.lock().unwrap().fallback_error = Some(PlatformError::new(
        "fallback_update_failed",
        "worker disconnected",
    ));

    runtime.handle_menu(MenuCommand::ToggleLayoutFallback, 10);

    assert!(!runtime.engine.config().layout.fallback_enabled);
    assert!(
        !ConfigStore::load(dir.0.join("config.toml"))
            .config
            .layout
            .fallback_enabled
    );
    assert!(
        runtime
            .warnings
            .contains_key("layout_fallback_update_failed")
    );
    assert!(rx.try_iter().any(|command| matches!(
        command,
        TrayCommand::SyncChecks(Checks {
            layout_fallback: false,
            ..
        })
    )));
}

#[test]
fn disabled_fallback_ignores_queued_poll_but_keeps_other_layout_sources() {
    let mut config = Config::default();
    config.layout.fallback_enabled = false;
    let (mut runtime, mock, _rx, _dir) = fixture(config);
    runtime.initialize(0);
    assert_eq!(mock.0.lock().unwrap().layout_reads, 1);
    set_layout(&mock, 2, "en-US");

    runtime.handle_platform(
        PlatformEvent::LayoutChanged {
            layout: LayoutId(2),
            lang: LangTag::new("en-US"),
            source: LayoutSource::ForegroundPoll,
        },
        10,
    );
    assert_eq!(runtime.label, "RU");
    assert_eq!(mock.0.lock().unwrap().layout_reads, 1);

    runtime.handle_platform(notice(), 20);
    assert_eq!(runtime.label, "EN");
    assert_eq!(mock.0.lock().unwrap().layout_reads, 2);
}

#[test]
fn bootstrap_is_silent_and_notifications_read_current_before_synchronous_anchor_resolution() {
    let (mut runtime, mock, rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    assert_eq!(runtime.label, "RU");
    assert!(calls(&mock).is_empty());
    set_layout(&mock, 2, "en-US");
    runtime.handle_platform(notice(), 100);
    assert_eq!(runtime.label, "EN");
    assert_eq!(
        calls(&mock),
        [
            Call::Sound,
            Call::Show(144, ResolvedAnchor::Cursor(Point { x: 10, y: 20 })),
            Call::Track(true)
        ]
    );
    assert_eq!(runtime.timeout(100), Some(Duration::from_millis(1500)));
    assert!(
        rx.try_iter()
            .any(|c| matches!(c, TrayCommand::SetTooltip(s) if s.contains("EN")))
    );
    // Stale payloads never revert state, and duplicate notifications have no effects.
    runtime.handle_platform(notice(), 110);
    assert_eq!(calls(&mock).len(), 3);
    // A real rapid return is accepted.
    set_layout(&mock, 1, "ru-RU");
    runtime.handle_platform(notice(), 120);
    assert_eq!(runtime.label, "RU");
    assert_eq!(calls(&mock).len(), 6);
}

#[test]
fn failed_initial_snapshot_retries_as_initial_and_preserves_follow_without_sound() {
    let mut config = Config::default();
    config.badge.mode = BadgeMode::Follow;
    let (mut runtime, mock, _, _dir) = fixture(config);
    mock.0.lock().unwrap().snapshot = Err(PlatformError::new("missing", "no foreground"));
    runtime.initialize(0);
    assert!(!runtime.layout_initialized);
    set_layout(&mock, 2, "en-US");
    mock.0.lock().unwrap().cursor = None;
    runtime.handle_platform(notice(), 10);
    assert_eq!(
        calls(&mock),
        [Call::Show(144, ResolvedAnchor::Fixed), Call::Track(false)]
    );
    assert_eq!(runtime.timeout(10), None);
    mock.0.lock().unwrap().snapshot = Err(PlatformError::new("race", "foreground moved"));
    runtime.handle_platform(notice(), 20);
    assert_eq!(runtime.label, "EN");
    assert_eq!(calls(&mock).len(), 2);
}

#[test]
fn dpi_rerender_uses_current_anchor_dpi_and_never_resurrects_a_hidden_badge() {
    let (mut runtime, mock, _, _dir) = fixture(Config::default());
    runtime.initialize(0);
    set_layout(&mock, 2, "en");
    runtime.handle_platform(notice(), 10);
    let p = Point { x: 30, y: 40 };
    mock.0.lock().unwrap().cursor = Some(p);
    runtime.handle_platform(PlatformEvent::PointerMoved { pos: p }, 11);
    mock.0.lock().unwrap().dpi = 192;
    runtime.handle_platform(PlatformEvent::OverlayScaleChanged { dpi: 192 }, 12);
    assert_eq!(
        calls(&mock).last(),
        Some(&Call::Show(192, ResolvedAnchor::Cursor(p)))
    );
    let count = calls(&mock).len();
    runtime.handle_platform(PlatformEvent::OverlayScaleChanged { dpi: 144 }, 13);
    assert_eq!(
        calls(&mock).len(),
        count,
        "late DPI payload must not rerender for the old monitor"
    );
    runtime.fire_hide_timer(1510);
    assert_eq!(runtime.timeout(1510), None);
    assert!(calls(&mock).ends_with(&[Call::Hide, Call::Track(false)]));
    let count = calls(&mock).len();
    runtime.handle_platform(PlatformEvent::OverlayScaleChanged { dpi: 288 }, 1600);
    assert_eq!(calls(&mock).len(), count);
}

#[test]
fn autostart_failure_rechecks_truth_and_only_persists_actual_external_drift() {
    let (mut runtime, mock, rx, dir) = fixture(Config::default());
    runtime.initialize(0);
    rx.try_iter().for_each(drop);
    mock.0.lock().unwrap().write_error = Some(PlatformError::new("denied", "write denied"));
    runtime.handle_menu(MenuCommand::ToggleAutostart, 10);
    assert!(!runtime.engine.config().autostart);
    assert!(!dir.0.join("config.toml").exists());
    assert!(rx.try_iter().any(|c| matches!(
        c,
        TrayCommand::SyncChecks(Checks {
            autostart: false,
            ..
        })
    )));
    mock.0.lock().unwrap().actual_autostart = Ok(true);
    runtime.handle_menu(MenuCommand::ToggleAutostart, 20);
    assert!(runtime.engine.config().autostart);
    assert!(
        ConfigStore::load(dir.0.join("config.toml"))
            .config
            .autostart
    );
    assert_eq!(
        runtime.caps.state(Capability::Autostart),
        CapabilityState::Off
    );
    mock.0.lock().unwrap().actual_autostart = Err(PlatformError::new("unreadable", "unknown"));
    runtime.handle_menu(MenuCommand::ToggleAutostart, 30);
    assert!(rx.try_iter().any(|c| matches!(
        c,
        TrayCommand::SetStatus {
            autostart_available: false,
            ..
        }
    )));
}

#[test]
fn autostart_startup_and_success_sync_checks_and_persist_registry_truth() {
    let (mut runtime, mock, rx, dir) = fixture(Config::default());
    mock.0.lock().unwrap().actual_autostart = Ok(true);
    runtime.initialize(0);
    assert!(runtime.engine.config().autostart);
    assert!(dir.0.join("config.toml").exists());
    runtime.handle_menu(MenuCommand::ToggleAutostart, 10);
    assert!(!runtime.engine.config().autostart);
    assert!(
        !ConfigStore::load(dir.0.join("config.toml"))
            .config
            .autostart
    );
    assert!(rx.try_iter().any(|c| matches!(
        c,
        TrayCommand::SyncChecks(Checks {
            autostart: false,
            ..
        })
    )));
    assert!(runtime.cache.is_empty());
}

#[test]
fn sound_failure_does_not_change_preference_and_quit_requests_graceful_shutdown() {
    let (mut runtime, mock, rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    runtime.handle_platform(
        PlatformEvent::CapabilityChanged(CapabilityReport {
            capability: Capability::Sound,
            state: CapabilityState::Off,
            code: "no_output_device",
            detail: "No output device".into(),
        }),
        1,
    );
    set_layout(&mock, 2, "en");
    runtime.handle_platform(notice(), 2);
    assert!(calls(&mock).contains(&Call::Sound));
    assert!(runtime.engine.config().sound.enabled);
    runtime.handle_menu(MenuCommand::ToggleFollow, 3);
    assert!(runtime.timeout(9999).is_none());
    runtime.handle_menu(MenuCommand::Quit, 4);
    assert!(runtime.quitting);
    assert!(rx.try_iter().any(|c| c == TrayCommand::Shutdown));
}

#[test]
fn stop_channel_ends_a_blocked_loop_even_with_live_platform_senders() {
    let (runtime, _mock, _rx, _dir) = fixture(Config::default());
    let (_platform_tx, platform_rx) = crossbeam_channel::unbounded();
    let (_menu_tx, menu_rx) = crossbeam_channel::unbounded();
    let (stop_tx, stop_rx) = crossbeam_channel::bounded(1);
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let worker = std::thread::spawn(move || {
        run(runtime, platform_rx, menu_rx, stop_rx);
        done_tx.send(()).unwrap();
    });
    stop_tx.send(()).unwrap();
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
}

#[test]
fn continuously_ready_pointer_channel_cannot_starve_the_absolute_hide_deadline() {
    let mut config = Config::default();
    config.badge.show_ms = 200;
    let (mut runtime, mock, _rx, _dir) = fixture(config);
    runtime.initialize(0);
    set_layout(&mock, 2, "en");
    let (platform_tx, platform_rx) = crossbeam_channel::unbounded();
    platform_tx.send(notice()).unwrap();
    for n in 0..1000 {
        platform_tx
            .send(PlatformEvent::PointerMoved {
                pos: Point { x: n, y: 0 },
            })
            .unwrap();
    }
    let (_menu_tx, menu_rx) = crossbeam_channel::unbounded();
    let (stop_tx, stop_rx) = crossbeam_channel::bounded(1);
    let mut time = 0;
    run_with_clock(runtime, platform_rx, menu_rx, stop_rx, || {
        time += 1;
        if time >= 250 {
            let _ = stop_tx.try_send(());
        }
        time
    });
    let observed = calls(&mock);
    assert_eq!(
        observed.iter().filter(|c| **c == Call::Hide).count(),
        2,
        "one hide at the deadline, another at shutdown, while the event queue is still ready"
    );
    let first_hide = observed.iter().position(|c| *c == Call::Hide).unwrap();
    assert!(
        !observed[first_hide..]
            .iter()
            .any(|c| matches!(c, Call::Move(_)))
    );
}

#[test]
fn initial_follow_precedes_already_queued_hook_events() {
    let mut config = Config::default();
    config.badge.mode = BadgeMode::Follow;
    let (runtime, mock, _rx, _dir) = fixture(config);
    let (platform_tx, platform_rx) = crossbeam_channel::unbounded();
    for _ in 0..20 {
        platform_tx.send(notice()).unwrap();
    }
    let (_menu_tx, menu_rx) = crossbeam_channel::unbounded();
    let (stop_tx, stop_rx) = crossbeam_channel::bounded(1);
    let mut time = 0;
    run_with_clock(runtime, platform_rx, menu_rx, stop_rx, || {
        time += 1;
        if time >= 20 {
            let _ = stop_tx.try_send(());
        }
        time
    });
    let observed = calls(&mock);
    assert_eq!(
        observed
            .iter()
            .filter(|c| matches!(c, Call::Show(_, _)))
            .count(),
        1
    );
    assert!(!observed.contains(&Call::Sound));
}

#[test]
fn raster_failure_hides_old_image_and_does_not_arm_tracking() {
    let (mut runtime, mock, _rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    set_layout(&mock, 2, "en");
    mock.0.lock().unwrap().dpi = 0;
    runtime.handle_platform(notice(), 10);
    assert!(runtime.last.is_none());
    assert!(calls(&mock).contains(&Call::Hide));
    assert!(!calls(&mock).contains(&Call::Track(true)));
    assert!(runtime.warnings.contains_key("badge_render_failed"));
}

fn overlay_report(state: CapabilityState) -> PlatformEvent {
    PlatformEvent::CapabilityChanged(CapabilityReport {
        capability: Capability::Overlay,
        state,
        code: "test_overlay",
        detail: "Test overlay state".into(),
    })
}

#[test]
fn permanently_unavailable_overlay_never_arms_pointer_even_in_follow() {
    for fail_at_startup in [true, false] {
        let mut config = Config::default();
        config.badge.mode = BadgeMode::Follow;
        let (mut runtime, mock, _rx, _dir) = fixture(config);
        if fail_at_startup {
            runtime.handle_platform(overlay_report(CapabilityState::Off), 0);
        }
        runtime.initialize(0);
        runtime.handle_platform(overlay_report(CapabilityState::Off), 1);
        mock.0.lock().unwrap().calls.clear();
        runtime.handle_platform(
            PlatformEvent::PointerMoved {
                pos: Point { x: 33, y: 44 },
            },
            2,
        );
        set_layout(&mock, 2, "en");
        runtime.handle_platform(notice(), 3);
        assert!(
            !calls(&mock)
                .iter()
                .any(|c| matches!(c, Call::Move(_) | Call::Show(_, _) | Call::Track(true)))
        );
    }
}

#[test]
fn overlay_degradation_disarms_until_successful_recovery_and_late_ok_cannot_undo_hide() {
    let mut config = Config::default();
    config.badge.mode = BadgeMode::Follow;
    let (mut runtime, mock, _rx, _dir) = fixture(config);
    runtime.initialize(0);
    mock.0.lock().unwrap().calls.clear();
    runtime.handle_platform(overlay_report(CapabilityState::Degraded), 1);
    assert_eq!(calls(&mock), [Call::Track(false)]);
    runtime.handle_platform(
        PlatformEvent::PointerMoved {
            pos: Point { x: 33, y: 44 },
        },
        2,
    );
    assert_eq!(calls(&mock).len(), 1);
    set_layout(&mock, 2, "en");
    runtime.handle_platform(notice(), 3);
    assert!(!calls(&mock).contains(&Call::Track(true)));
    runtime.handle_platform(overlay_report(CapabilityState::Ok), 4);
    assert_eq!(calls(&mock).last(), Some(&Call::Track(true)));
    runtime.handle_menu(MenuCommand::ToggleFollow, 5);
    runtime.handle_platform(overlay_report(CapabilityState::Degraded), 6);
    runtime.fire_hide_timer(1505);
    mock.0.lock().unwrap().calls.clear();
    runtime.handle_platform(overlay_report(CapabilityState::Ok), 1506);
    assert!(
        !calls(&mock)
            .iter()
            .any(|c| matches!(c, Call::Show(_, _) | Call::Track(true)))
    );
}

#[test]
fn overlay_recovery_rechecks_dpi_even_when_the_hint_preceded_ok() {
    let mut config = Config::default();
    config.badge.mode = BadgeMode::Follow;
    let (mut runtime, mock, _rx, _dir) = fixture(config);
    runtime.initialize(0);
    runtime.handle_platform(overlay_report(CapabilityState::Degraded), 1);
    set_layout(&mock, 2, "en");
    runtime.handle_platform(notice(), 2);
    mock.0.lock().unwrap().dpi = 192;
    runtime.handle_platform(PlatformEvent::OverlayScaleChanged { dpi: 192 }, 3);
    runtime.handle_platform(overlay_report(CapabilityState::Ok), 4);
    assert!(calls(&mock).contains(&Call::Show(
        192,
        ResolvedAnchor::Cursor(Point { x: 10, y: 20 })
    )));
    assert_eq!(runtime.last.as_ref().unwrap().dpi, 192);
}

#[test]
fn queued_pointer_from_previous_show_cannot_move_a_new_badge_to_an_old_position() {
    let (mut runtime, mock, _rx, _dir) = fixture(Config::default());
    runtime.initialize(0);
    set_layout(&mock, 2, "en");
    runtime.handle_platform(notice(), 10);
    runtime.fire_hide_timer(1510);
    mock.0.lock().unwrap().cursor = Some(Point { x: 100, y: 100 });
    set_layout(&mock, 1, "ru");
    runtime.handle_platform(notice(), 1600);
    let before = calls(&mock).len();
    runtime.handle_platform(
        PlatformEvent::PointerMoved {
            pos: Point { x: 15, y: 15 },
        },
        1601,
    );
    assert_eq!(calls(&mock).len(), before);
    assert_eq!(
        runtime.last.as_ref().unwrap().anchor,
        ResolvedAnchor::Cursor(Point { x: 100, y: 100 })
    );
}
