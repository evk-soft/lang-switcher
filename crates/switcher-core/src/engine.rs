//! The core state machine: pure `(state, event, now) -> effects`. No OS calls, no
//! clocks, no channels — the runtime supplies `now_ms` and executes the effects.

use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
use switcher_platform::ports::SoundCue;

use crate::config::{AnchorPref, BadgeMode, Config};
use crate::content::{BadgeContent, cue_for};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A current snapshot confirmed by the runtime when processing a platform
    /// notification, not the notification's potentially delayed payload (ADR-0011).
    Layout {
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
    },
    /// Runtime's synchronous answer to `Effect::QueryAnchor`.
    AnchorResolved {
        caret: Option<Point>,
        cursor: Option<Point>,
    },
    Pointer {
        pos: Point,
    },
    HideTimerFired,
    SetMode(BadgeMode),
    SetSoundEnabled(bool),
    SetLayoutFallbackEnabled(bool),
    SetAutostart(bool),
    /// The runtime's answer to `Effect::ApplyAutostart`, and the same path startup
    /// reconciliation uses: `Autostart::is_enabled()` is the OS truth, the config only
    /// mirrors it (ADR-0007).
    AutostartApplied {
        requested: bool,
        ok: bool,
    },
}

// `ResolvedAnchor` lives in `switcher-platform` because the overlay adapter must see it
// (ADR-0005). Re-exported here so `switcher_core::engine::ResolvedAnchor` keeps resolving
// for the app shell — the core's own tests would compile off a plain `use` either way.
pub use switcher_platform::events::ResolvedAnchor;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Ask the runtime to query caret/cursor availability and feed back AnchorResolved.
    QueryAnchor,
    ShowBadge {
        content: BadgeContent,
        anchor: ResolvedAnchor,
    },
    /// New anchor for the visible badge; the adapter re-derives offset, DPI scaling and
    /// clamping from it. Carrying the anchor kind rather than a bare point keeps
    /// `OverlayWindow::move_to` stateless and is what M2 caret tracking will need.
    MoveBadge {
        anchor: ResolvedAnchor,
    },
    HideBadge,
    ArmHideTimer {
        after_ms: u64,
    },
    CancelHideTimer,
    SetPointerTracking(bool),
    PlaySound {
        cue: SoundCue,
        volume: f32,
    },
    UpdateTray {
        label: String,
        lang: LangTag,
    },
    SetLayoutFallbackEnabled(bool),
    ApplyAutostart(bool),
    /// Config changed: runtime saves it and re-syncs tray checkmarks.
    PersistConfig,
    /// Re-sync tray checkmarks from `Engine::config()` **without** writing the file: the
    /// user toggled a checkbox that the OS then refused, so the menu must snap back
    /// while the config stays as it was (ADR-0007).
    SyncTrayMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadgeState {
    Hidden,
    /// QueryAnchor issued, waiting for AnchorResolved.
    AwaitingAnchor,
    Visible {
        tracking: bool,
    },
}

#[derive(Debug)]
pub struct Engine {
    cfg: Config,
    layout: Option<(LayoutId, LangTag)>,
    badge: BadgeState,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            layout: None,
            badge: BadgeState::Hidden,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn handle(&mut self, event: Event, _now_ms: u64) -> Vec<Effect> {
        match event {
            Event::Layout {
                layout,
                lang,
                source,
            } => self.on_layout(layout, lang, source),
            Event::AnchorResolved { caret, cursor } => self.on_anchor(caret, cursor),
            // `tracking` is only ever true for a cursor-anchored badge (see `on_anchor`),
            // so the anchor kind here is always `Cursor`.
            Event::Pointer { pos } => match self.badge {
                BadgeState::Visible { tracking: true } => vec![Effect::MoveBadge {
                    anchor: ResolvedAnchor::Cursor(pos),
                }],
                _ => vec![],
            },
            Event::HideTimerFired => match self.badge {
                BadgeState::Visible { .. } if self.cfg.badge.mode == BadgeMode::Transient => {
                    self.badge = BadgeState::Hidden;
                    vec![Effect::HideBadge, Effect::SetPointerTracking(false)]
                }
                _ => vec![],
            },
            Event::SetMode(mode) => self.on_set_mode(mode),
            Event::SetSoundEnabled(enabled) => {
                // Dedup like `on_set_mode`: the config is the only source of truth for
                // this checkbox, so a no-op click must not rewrite the file on disk.
                if self.cfg.sound.enabled == enabled {
                    return vec![];
                }
                self.cfg.sound.enabled = enabled;
                vec![Effect::PersistConfig]
            }
            Event::SetLayoutFallbackEnabled(enabled) => {
                if self.cfg.layout.fallback_enabled == enabled {
                    return vec![];
                }
                self.cfg.layout.fallback_enabled = enabled;
                vec![
                    Effect::SetLayoutFallbackEnabled(enabled),
                    Effect::PersistConfig,
                ]
            }
            // `cfg.autostart` mirrors a registry value, so the OS — not the click —
            // decides. No dedup on the request: the Run key may have drifted (a cleaner
            // tool, a manual edit, another copy of the app), and a repeated toggle must
            // be able to re-assert it.
            Event::SetAutostart(enabled) => vec![Effect::ApplyAutostart(enabled)],
            Event::AutostartApplied { requested, ok } => {
                if !ok {
                    // Refused: the config keeps its old value and the menu snaps back.
                    // The reason travels separately as CapabilityChanged(Autostart, ..).
                    return vec![Effect::SyncTrayMenu];
                }
                if self.cfg.autostart == requested {
                    return vec![];
                }
                self.cfg.autostart = requested;
                vec![Effect::PersistConfig]
            }
        }
    }

    fn on_layout(&mut self, layout: LayoutId, lang: LangTag, source: LayoutSource) -> Vec<Effect> {
        if self.layout.as_ref().map(|(id, _)| *id) == Some(layout) {
            return vec![];
        }
        self.layout = Some((layout, lang.clone()));

        let content = BadgeContent::for_lang(&lang, self.cfg.badge.style, &self.cfg.badge.colors);
        let mut fx = vec![Effect::UpdateTray {
            label: content.label,
            lang: lang.clone(),
        }];
        if source == LayoutSource::Initial {
            if self.cfg.badge.mode == BadgeMode::Follow {
                self.badge = BadgeState::AwaitingAnchor;
                fx.push(Effect::QueryAnchor);
            }
            return fx;
        }
        // Superseding a still-visible transient badge: cancel its pending hide timer so
        // the single-timer invariant holds across the AwaitingAnchor gap (the new badge
        // re-arms in on_anchor). Every other path out of Visible already clears the timer.
        if matches!(self.badge, BadgeState::Visible { .. })
            && self.cfg.badge.mode == BadgeMode::Transient
        {
            fx.push(Effect::CancelHideTimer);
        }
        if self.cfg.sound.enabled {
            fx.push(Effect::PlaySound {
                cue: cue_for(&lang),
                volume: self.cfg.sound.volume,
            });
        }
        self.badge = BadgeState::AwaitingAnchor;
        fx.push(Effect::QueryAnchor);
        fx
    }

    fn on_anchor(&mut self, caret: Option<Point>, cursor: Option<Point>) -> Vec<Effect> {
        if self.badge != BadgeState::AwaitingAnchor {
            return vec![];
        }
        let anchor = match self.cfg.badge.anchor {
            AnchorPref::Auto => caret
                .map(ResolvedAnchor::Caret)
                .or(cursor.map(ResolvedAnchor::Cursor))
                .unwrap_or(ResolvedAnchor::Fixed),
            AnchorPref::Cursor => cursor
                .map(ResolvedAnchor::Cursor)
                .unwrap_or(ResolvedAnchor::Fixed),
            AnchorPref::Fixed => ResolvedAnchor::Fixed,
        };
        let tracking = matches!(anchor, ResolvedAnchor::Cursor(_));
        let (_, lang) = self
            .layout
            .as_ref()
            .expect("badge is pending only after an accepted layout event");
        let content = BadgeContent::for_lang(lang, self.cfg.badge.style, &self.cfg.badge.colors);
        self.badge = BadgeState::Visible { tracking };
        let mut fx = vec![
            Effect::ShowBadge { content, anchor },
            Effect::SetPointerTracking(tracking),
        ];
        match self.cfg.badge.mode {
            BadgeMode::Transient => fx.push(Effect::ArmHideTimer {
                after_ms: self.cfg.badge.show_ms,
            }),
            BadgeMode::Follow => fx.push(Effect::CancelHideTimer),
        }
        fx
    }

    fn on_set_mode(&mut self, mode: BadgeMode) -> Vec<Effect> {
        if self.cfg.badge.mode == mode {
            return vec![];
        }
        self.cfg.badge.mode = mode;
        let mut fx = vec![Effect::PersistConfig];
        match (mode, self.badge) {
            (BadgeMode::Follow, BadgeState::Hidden) if self.layout.is_some() => {
                self.badge = BadgeState::AwaitingAnchor;
                fx.push(Effect::QueryAnchor);
            }
            (BadgeMode::Follow, BadgeState::Visible { .. }) => fx.push(Effect::CancelHideTimer),
            (BadgeMode::Transient, BadgeState::Visible { .. }) => fx.push(Effect::ArmHideTimer {
                after_ms: self.cfg.badge.show_ms,
            }),
            _ => {}
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
    use switcher_platform::ports::SoundCue;

    use crate::config::Config;
    use crate::content::{BadgeContent, BadgeStyle};

    pub(super) const RU_ID: LayoutId = LayoutId(0x0419_0419);
    pub(super) const EN_ID: LayoutId = LayoutId(0x0409_0409);

    pub(super) fn ru() -> LangTag {
        LangTag::new("ru-RU")
    }

    pub(super) fn en() -> LangTag {
        LangTag::new("en-US")
    }

    pub(super) fn layout(id: LayoutId, lang: LangTag, source: LayoutSource) -> Event {
        Event::Layout {
            layout: id,
            lang,
            source,
        }
    }

    /// Engine already past startup: Initial EN at t=0.
    pub(super) fn engine_after_initial() -> Engine {
        let mut e = Engine::new(Config::default());
        e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        e
    }

    #[test]
    fn initial_layout_updates_tray_only() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        assert_eq!(
            fx,
            vec![Effect::UpdateTray {
                label: "EN".to_owned(),
                lang: en(),
            }]
        );
    }

    #[test]
    fn initial_layout_shows_persisted_follow_badge_without_sound() {
        let mut cfg = Config::default();
        cfg.badge.mode = BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        assert_eq!(
            e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0),
            vec![
                Effect::UpdateTray {
                    label: "EN".to_owned(),
                    lang: en()
                },
                Effect::QueryAnchor,
            ]
        );
        assert_eq!(
            e.handle(resolved(None, Some(p(10, 20))), 0),
            vec![
                Effect::ShowBadge {
                    content: default_content(&en()),
                    anchor: ResolvedAnchor::Cursor(p(10, 20)),
                },
                Effect::SetPointerTracking(true),
                Effect::CancelHideTimer,
            ]
        );
    }

    #[test]
    fn follow_selected_before_initial_layout_shows_on_initial() {
        let mut e = Engine::new(Config::default());
        e.handle(Event::SetMode(BadgeMode::Follow), 0);
        assert!(
            e.handle(layout(EN_ID, en(), LayoutSource::Initial), 1)
                .contains(&Effect::QueryAnchor)
        );
    }

    #[test]
    fn layout_change_updates_tray_plays_sound_and_queries_anchor() {
        let mut e = engine_after_initial();
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        assert_eq!(
            fx,
            vec![
                Effect::UpdateTray {
                    label: "RU".to_owned(),
                    lang: ru(),
                },
                Effect::PlaySound {
                    cue: SoundCue::Ru,
                    volume: 0.4,
                },
                Effect::QueryAnchor,
            ]
        );
    }

    #[test]
    fn first_event_from_any_source_is_accepted() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::Tsf), 5);
        assert!(fx.contains(&Effect::QueryAnchor));
    }

    #[test]
    fn same_layout_from_another_source_is_deduplicated() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::Tsf), 1020);
        assert_eq!(fx, vec![]);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ForegroundChange), 9000);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn rapid_return_to_previous_layout_is_accepted_from_every_source() {
        for source in [
            LayoutSource::ShellHook,
            LayoutSource::ForegroundChange,
            LayoutSource::ForegroundPoll,
            LayoutSource::Tsf,
        ] {
            let mut e = engine_after_initial();
            e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
            let fx = e.handle(layout(EN_ID, en(), source), 1100);
            assert!(
                fx.contains(&Effect::UpdateTray {
                    label: "EN".to_owned(),
                    lang: en()
                }),
                "a confirmed return from {source:?} must update the indicator"
            );
            let shown = e.handle(resolved(None, Some(p(10, 20))), 1100);
            assert!(shown.contains(&Effect::ShowBadge {
                content: default_content(&en()),
                anchor: ResolvedAnchor::Cursor(p(10, 20)),
            }));
        }
    }

    #[test]
    fn later_return_to_previous_layout_is_accepted() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ShellHook), 1150);
        assert!(fx.contains(&Effect::QueryAnchor));
    }

    #[test]
    fn sound_disabled_suppresses_play_sound() {
        let mut cfg = Config::default();
        cfg.sound.enabled = false;
        let mut e = Engine::new(cfg);
        e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        assert!(!fx.iter().any(|f| matches!(f, Effect::PlaySound { .. })));
        assert!(fx.contains(&Effect::QueryAnchor));
    }

    pub(super) fn resolved(caret: Option<Point>, cursor: Option<Point>) -> Event {
        Event::AnchorResolved { caret, cursor }
    }

    pub(super) fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    pub(super) fn default_content(lang: &LangTag) -> BadgeContent {
        BadgeContent::for_lang(lang, BadgeStyle::Text, &BTreeMap::new())
    }

    #[test]
    fn auto_anchor_prefers_caret_and_does_not_track_pointer() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Caret(p(5, 6)),
                },
                Effect::SetPointerTracking(false),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn auto_anchor_falls_back_to_cursor_and_tracks() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(9, 9))), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Cursor(p(9, 9)),
                },
                Effect::SetPointerTracking(true),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn auto_anchor_falls_back_to_fixed_when_nothing_available() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, None), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Fixed,
                },
                Effect::SetPointerTracking(false),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn cursor_pref_ignores_caret() {
        let mut cfg = Config::default();
        cfg.badge.anchor = crate::config::AnchorPref::Cursor;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&ru()),
            anchor: ResolvedAnchor::Cursor(p(9, 9)),
        }));
    }

    #[test]
    fn fixed_pref_ignores_caret_and_cursor() {
        let mut cfg = Config::default();
        cfg.badge.anchor = crate::config::AnchorPref::Fixed;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&ru()),
            anchor: ResolvedAnchor::Fixed,
        }));
        assert!(fx.contains(&Effect::SetPointerTracking(false)));
    }

    #[test]
    fn follow_mode_cancels_timer_instead_of_arming() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::CancelHideTimer));
        assert!(!fx.iter().any(|f| matches!(f, Effect::ArmHideTimer { .. })));
    }

    #[test]
    fn anchor_resolved_while_not_awaiting_is_ignored() {
        let mut e = engine_after_initial();
        let fx = e.handle(resolved(None, Some(p(9, 9))), 500);
        assert_eq!(fx, vec![]);
    }

    /// Engine with a visible cursor-anchored RU badge (transient mode), t=1000.
    pub(super) fn engine_with_visible_badge() -> Engine {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(10, 10))), 1000);
        assert!(fx.iter().any(|f| matches!(f, Effect::ShowBadge { .. })));
        e
    }

    #[test]
    fn pointer_moves_visible_tracking_badge() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(
            fx,
            vec![Effect::MoveBadge {
                anchor: ResolvedAnchor::Cursor(p(50, 60)),
            }]
        );
    }

    #[test]
    fn pointer_is_ignored_when_badge_hidden() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn pointer_is_ignored_when_caret_anchored() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn hide_timer_hides_badge_and_stops_tracking() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::HideTimerFired, 2500);
        assert_eq!(
            fx,
            vec![Effect::HideBadge, Effect::SetPointerTracking(false)]
        );
        // Once hidden, pointer noise is ignored.
        let fx = e.handle(Event::Pointer { pos: p(1, 1) }, 2600);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn stale_hide_timer_in_follow_mode_is_ignored() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(None, Some(p(9, 9))), 1001);
        let fx = e.handle(Event::HideTimerFired, 2500);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn second_change_while_visible_requeries_anchor_and_rearms() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ShellHook), 2000);
        assert!(fx.contains(&Effect::QueryAnchor));
        // The superseded transient badge's hide timer must be cancelled here, otherwise it
        // stays armed across the AwaitingAnchor gap (regression: on_layout used to leak it).
        assert!(fx.contains(&Effect::CancelHideTimer));
        let fx = e.handle(resolved(None, Some(p(20, 20))), 2001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&en()),
            anchor: ResolvedAnchor::Cursor(p(20, 20)),
        }));
        assert!(fx.contains(&Effect::ArmHideTimer { after_ms: 1500 }));
    }

    #[test]
    fn set_mode_follow_while_hidden_shows_badge_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 3000);
        assert_eq!(fx, vec![Effect::PersistConfig, Effect::QueryAnchor]);
        assert_eq!(e.config().badge.mode, crate::config::BadgeMode::Follow);
    }

    #[test]
    fn set_mode_follow_before_any_layout_only_persists() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 10);
        assert_eq!(fx, vec![Effect::PersistConfig]);
    }

    #[test]
    fn set_mode_follow_while_visible_cancels_timer() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 1200);
        assert_eq!(fx, vec![Effect::PersistConfig, Effect::CancelHideTimer]);
    }

    #[test]
    fn set_mode_transient_while_visible_arms_timer() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(None, Some(p(9, 9))), 1001);
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Transient), 1200);
        assert_eq!(
            fx,
            vec![
                Effect::PersistConfig,
                Effect::ArmHideTimer { after_ms: 1500 }
            ]
        );
    }

    #[test]
    fn set_mode_same_is_noop() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Transient), 1200);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn set_sound_enabled_updates_config_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetSoundEnabled(false), 1200);
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(!e.config().sound.enabled);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 2000);
        assert!(!fx.iter().any(|f| matches!(f, Effect::PlaySound { .. })));
    }

    #[test]
    fn set_sound_enabled_same_value_is_a_noop() {
        let mut e = engine_after_initial(); // sound.enabled == true by default
        assert_eq!(e.handle(Event::SetSoundEnabled(true), 1200), vec![]);
        assert!(e.config().sound.enabled);
    }

    #[test]
    fn set_layout_fallback_updates_preference_requests_port_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetLayoutFallbackEnabled(false), 1200);
        assert_eq!(
            fx,
            vec![
                Effect::SetLayoutFallbackEnabled(false),
                Effect::PersistConfig
            ]
        );
        assert!(!e.config().layout.fallback_enabled);
    }

    #[test]
    fn set_layout_fallback_same_value_is_a_noop() {
        let mut e = engine_after_initial();
        assert_eq!(
            e.handle(Event::SetLayoutFallbackEnabled(true), 1200),
            vec![]
        );
        assert!(e.config().layout.fallback_enabled);
    }

    // `cfg.autostart` mirrors a registry value, so every test below asserts the same
    // invariant from a different angle: the config never claims what the OS has not
    // confirmed. See ADR-0007.

    #[test]
    fn set_autostart_requests_the_os_before_touching_the_config() {
        let mut e = engine_after_initial();
        assert_eq!(
            e.handle(Event::SetAutostart(true), 1200),
            vec![Effect::ApplyAutostart(true)]
        );
        assert!(
            !e.config().autostart,
            "config must not claim what the OS has not confirmed"
        );
        let fx = e.handle(
            Event::AutostartApplied {
                requested: true,
                ok: true,
            },
            1210,
        );
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(e.config().autostart);
    }

    #[test]
    fn refused_autostart_never_reaches_the_config() {
        let mut e = engine_after_initial();
        e.handle(Event::SetAutostart(true), 1200);
        let fx = e.handle(
            Event::AutostartApplied {
                requested: true,
                ok: false,
            },
            1210,
        );
        assert_eq!(fx, vec![Effect::SyncTrayMenu]);
        assert!(!e.config().autostart);
    }

    #[test]
    fn autostart_confirmation_matching_config_is_a_noop() {
        let mut e = engine_after_initial(); // default autostart == false
        let fx = e.handle(
            Event::AutostartApplied {
                requested: false,
                ok: true,
            },
            1210,
        );
        assert_eq!(fx, vec![]);
    }

    /// Startup reconciliation: the registry wins over the config file.
    #[test]
    fn startup_reconciliation_adopts_the_os_value() {
        let mut e = engine_after_initial();
        let fx = e.handle(
            Event::AutostartApplied {
                requested: true,
                ok: true,
            },
            5,
        );
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(e.config().autostart);
    }

    #[test]
    fn repeated_toggle_re_asserts_the_registry() {
        let mut e = engine_after_initial();
        e.handle(Event::SetAutostart(true), 1200);
        e.handle(
            Event::AutostartApplied {
                requested: true,
                ok: true,
            },
            1210,
        );
        let fx = e.handle(Event::SetAutostart(true), 1300);
        assert_eq!(
            fx,
            vec![Effect::ApplyAutostart(true)],
            "the Run key may have drifted, so a repeated toggle must re-assert it"
        );
    }
}
