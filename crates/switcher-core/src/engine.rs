//! The core state machine: pure `(state, event, now) -> effects`. No OS calls, no
//! clocks, no channels — the runtime supplies `now_ms` and executes the effects.

use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
use switcher_platform::ports::SoundCue;

use crate::config::{AnchorPref, BadgeMode, Config};
use crate::content::{BadgeContent, cue_for};

/// A source reporting the layout we just switched AWAY from within this window
/// is treated as a stale echo of the same physical switch, not a new switch.
pub const STALE_ECHO_WINDOW_MS: u64 = 150;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
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
    SetAutostart(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Caret(Point),
    Cursor(Point),
    Fixed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Ask the runtime to query caret/cursor availability and feed back AnchorResolved.
    QueryAnchor,
    ShowBadge {
        content: BadgeContent,
        anchor: ResolvedAnchor,
    },
    /// New anchor position for the visible badge (runtime applies the offset).
    MoveBadge {
        pos: Point,
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
    ApplyAutostart(bool),
    /// Config changed: runtime saves it and re-syncs tray checkmarks.
    PersistConfig,
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
    prev_layout: Option<LayoutId>,
    last_change_ms: u64,
    badge: BadgeState,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            layout: None,
            prev_layout: None,
            last_change_ms: 0,
            badge: BadgeState::Hidden,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn handle(&mut self, event: Event, now_ms: u64) -> Vec<Effect> {
        match event {
            Event::Layout {
                layout,
                lang,
                source,
            } => self.on_layout(layout, lang, source, now_ms),
            Event::AnchorResolved { caret, cursor } => self.on_anchor(caret, cursor),
            Event::Pointer { pos } => match self.badge {
                BadgeState::Visible { tracking: true } => vec![Effect::MoveBadge { pos }],
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
                self.cfg.sound.enabled = enabled;
                vec![Effect::PersistConfig]
            }
            Event::SetAutostart(enabled) => {
                self.cfg.autostart = enabled;
                vec![Effect::ApplyAutostart(enabled), Effect::PersistConfig]
            }
        }
    }

    fn on_layout(
        &mut self,
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
        now_ms: u64,
    ) -> Vec<Effect> {
        if self.layout.as_ref().map(|(id, _)| *id) == Some(layout) {
            return vec![];
        }
        let stale_echo = self.prev_layout == Some(layout)
            && now_ms.saturating_sub(self.last_change_ms) < STALE_ECHO_WINDOW_MS;
        if stale_echo {
            return vec![];
        }
        self.prev_layout = self.layout.take().map(|(id, _)| id);
        self.layout = Some((layout, lang.clone()));
        self.last_change_ms = now_ms;

        let content = BadgeContent::for_lang(&lang, self.cfg.badge.style, &self.cfg.badge.colors);
        let mut fx = vec![Effect::UpdateTray {
            label: content.label,
            lang: lang.clone(),
        }];
        if source == LayoutSource::Initial {
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
    fn stale_echo_of_previous_layout_within_window_is_ignored() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ForegroundChange), 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn toggle_back_after_stale_window_is_accepted() {
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
        assert_eq!(fx, vec![Effect::MoveBadge { pos: p(50, 60) }]);
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
    fn set_autostart_applies_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetAutostart(true), 1200);
        assert_eq!(
            fx,
            vec![Effect::ApplyAutostart(true), Effect::PersistConfig]
        );
        assert!(e.config().autostart);
    }
}
