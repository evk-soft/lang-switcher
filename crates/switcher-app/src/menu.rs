//! Stable command IDs, independent of the native menu objects.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    ToggleFollow,
    ToggleSound,
    ToggleAutostart,
    Quit,
}

pub mod ids {
    pub const FOLLOW: &str = "follow";
    pub const SOUND: &str = "sound";
    pub const AUTOSTART: &str = "autostart";
    pub const QUIT: &str = "quit";
    pub const STATUS: &str = "status";
}

pub fn command_for(id: &str) -> Option<MenuCommand> {
    match id {
        ids::FOLLOW => Some(MenuCommand::ToggleFollow),
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
        assert_eq!(command_for(ids::SOUND), Some(MenuCommand::ToggleSound));
        assert_eq!(
            command_for(ids::AUTOSTART),
            Some(MenuCommand::ToggleAutostart)
        );
        assert_eq!(command_for(ids::QUIT), Some(MenuCommand::Quit));
        assert_eq!(command_for(ids::STATUS), None);
        assert_eq!(command_for("other"), None);
    }
}
