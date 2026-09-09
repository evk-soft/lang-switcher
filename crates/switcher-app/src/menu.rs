//! Stable command IDs, independent of the native menu objects.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    ToggleFollow,
    ToggleLayoutFallback,
    ToggleSound,
    ToggleAutostart,
    /// `switcher_core::config::UI_LANGUAGE_AUTO` or a shipped catalog tag (ADR-0022).
    SetUiLanguage(String),
    Quit,
}

pub mod ids {
    pub const FOLLOW: &str = "follow";
    pub const LAYOUT_FALLBACK: &str = "layout_fallback";
    pub const SOUND: &str = "sound";
    pub const AUTOSTART: &str = "autostart";
    pub const QUIT: &str = "quit";
    pub const STATUS: &str = "status";
    pub const LANGUAGE: &str = "language";
    /// One language item per shipped catalog, plus one for "same as the system". The tag
    /// travels inside the id so that adding a catalog needs no new constant here.
    pub const LANGUAGE_PREFIX: &str = "language:";

    pub fn language(tag: &str) -> String {
        format!("{LANGUAGE_PREFIX}{tag}")
    }
}

pub fn command_for(id: &str) -> Option<MenuCommand> {
    if let Some(tag) = id.strip_prefix(ids::LANGUAGE_PREFIX) {
        // An empty tag would ask the config for a language named "", which sanitizes back
        // to "auto" — a silent reset instead of an ignored click.
        return (!tag.is_empty()).then(|| MenuCommand::SetUiLanguage(tag.to_owned()));
    }
    match id {
        ids::FOLLOW => Some(MenuCommand::ToggleFollow),
        ids::LAYOUT_FALLBACK => Some(MenuCommand::ToggleLayoutFallback),
        ids::SOUND => Some(MenuCommand::ToggleSound),
        ids::AUTOSTART => Some(MenuCommand::ToggleAutostart),
        ids::QUIT => Some(MenuCommand::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_actionable_ids_produce_commands() {
        assert_eq!(command_for(ids::FOLLOW), Some(MenuCommand::ToggleFollow));
        assert_eq!(
            command_for(ids::LAYOUT_FALLBACK),
            Some(MenuCommand::ToggleLayoutFallback)
        );
        assert_eq!(command_for(ids::SOUND), Some(MenuCommand::ToggleSound));
        assert_eq!(
            command_for(ids::AUTOSTART),
            Some(MenuCommand::ToggleAutostart)
        );
        assert_eq!(command_for(ids::QUIT), Some(MenuCommand::Quit));
        assert_eq!(command_for(ids::STATUS), None);
        // The submenu row itself opens the submenu; it is not a language choice.
        assert_eq!(command_for(ids::LANGUAGE), None);
        assert_eq!(command_for("other"), None);
    }

    #[test]
    fn language_ids_round_trip_through_the_command() {
        for tag in ["auto", "en", "ru", "zh-Hans"] {
            let id = ids::language(tag);
            assert_eq!(
                command_for(&id),
                Some(MenuCommand::SetUiLanguage(tag.to_owned())),
                "id {id}"
            );
        }
        assert_eq!(command_for(ids::LANGUAGE_PREFIX), None);
    }

    /// Every shipped catalog, plus "same as the system", must have a distinct id that no
    /// other menu entry can collide with.
    #[test]
    fn language_ids_are_unique_and_never_shadow_a_toggle() {
        use std::collections::BTreeSet;

        let fixed = [
            ids::FOLLOW,
            ids::LAYOUT_FALLBACK,
            ids::SOUND,
            ids::AUTOSTART,
            ids::QUIT,
            ids::STATUS,
            ids::LANGUAGE,
        ];
        let mut seen: BTreeSet<String> = fixed.iter().map(|id| (*id).to_owned()).collect();
        assert_eq!(seen.len(), fixed.len(), "duplicate fixed menu id");
        for tag in std::iter::once(switcher_core::config::UI_LANGUAGE_AUTO)
            .chain(crate::i18n::SUPPORTED.iter().map(|locale| locale.tag))
        {
            assert!(
                seen.insert(ids::language(tag)),
                "duplicate language id {tag}"
            );
        }
    }
}
