//! The core state machine: pure `(state, event, now) -> effects`. No OS calls, no
//! clocks, no channels — the runtime supplies `now_ms` and executes the effects.

use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
use switcher_platform::ports::SoundCue;

use crate::config::{BadgeMode, Config};
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
    // Constructed once anchor resolution lands in the next step; drop this allow then.
    #[allow(dead_code)]
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
            Event::AnchorResolved { .. } => todo!("task 6"),
            Event::Pointer { .. }
            | Event::HideTimerFired
            | Event::SetMode(_)
            | Event::SetSoundEnabled(_)
            | Event::SetAutostart(_) => todo!("task 7"),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use switcher_platform::events::{LangTag, LayoutId, LayoutSource};
    use switcher_platform::ports::SoundCue;

    use crate::config::Config;

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
}
