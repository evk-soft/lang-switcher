//! Plain commands cross threads; all native tray objects stay on the main thread.

use crate::i18n::{self, Translator, ids};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checks {
    pub follow: bool,
    pub layout_fallback: bool,
    pub sound: bool,
    pub autostart: bool,
    /// `UI_LANGUAGE_AUTO` or a language tag: which row of the language submenu is ticked.
    pub ui_language: String,
}

impl Default for Checks {
    fn default() -> Self {
        Self {
            follow: false,
            layout_fallback: false,
            sound: false,
            autostart: false,
            ui_language: switcher_core::config::UI_LANGUAGE_AUTO.to_owned(),
        }
    }
}

/// Every menu label that depends on the interface language. The names of the languages
/// themselves are not here: they are always written in their own language (ADR-0021), so
/// they never change when the interface language does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLabels {
    pub follow: String,
    pub layout_fallback: String,
    pub sound: String,
    pub autostart: String,
    pub status: String,
    pub language: String,
    pub language_auto: String,
    pub quit: String,
}

impl TrayLabels {
    pub fn new(tr: &Translator) -> Self {
        Self {
            follow: tr.text(ids::MENU_FOLLOW),
            layout_fallback: tr.text(ids::MENU_LAYOUT_FALLBACK),
            sound: tr.text(ids::MENU_SOUND),
            autostart: tr.text(ids::MENU_AUTOSTART),
            status: tr.text(ids::MENU_STATUS),
            language: tr.text(ids::MENU_LANGUAGE),
            language_auto: tr.text(ids::MENU_LANGUAGE_AUTO),
            quit: tr.text(ids::MENU_QUIT),
        }
    }
}

/// Win32 reads `&` in a menu label as the mnemonic marker and swallows it. Translations
/// are text, not markup, so every one of them is escaped on the way to a native label.
#[cfg(windows)]
fn escaped(text: &str) -> String {
    text.replace('&', "&&")
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrayCommand {
    SetIcon {
        rgba_straight: Vec<u8>,
        size: u32,
    },
    SetTooltip(String),
    SetStatus {
        rows: Vec<String>,
        autostart_available: bool,
    },
    SyncChecks(Checks),
    /// The interface language changed: rewrite every label in place. Rebuilding the menu
    /// would drop the tray icon's menu handle while the user may have it open.
    Localize(Box<TrayLabels>),
    Shutdown,
}

#[derive(Debug)]
pub struct TrayInit {
    pub rgba: Vec<u8>,
    pub size: u32,
    pub tooltip: String,
    pub status: Vec<String>,
    pub checks: Checks,
    pub labels: TrayLabels,
    pub autostart_available: bool,
}

#[cfg(windows)]
pub fn install_event_handlers(tx: crossbeam_channel::Sender<crate::menu::MenuCommand>) {
    use tray_icon::{TrayIconEvent, menu::MenuEvent};
    // Both libraries store handlers in OnceCell. Install before any HWND or event.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(command) = crate::menu::command_for(event.id.as_ref()) {
            tracing::debug!(?command, "tray menu command");
            if tx.send(command).is_err() {
                switcher_windows::win_util::post_quit();
            }
        }
    }));
    TrayIconEvent::set_event_handler(Some(|event| tracing::trace!(?event, "tray event")));
}

#[cfg(windows)]
pub fn run_tray(
    init: TrayInit,
    rx: crossbeam_channel::Receiver<TrayCommand>,
) -> anyhow::Result<()> {
    use crate::menu::ids as menu_ids;
    use crossbeam_channel::TryRecvError;
    use switcher_core::config::UI_LANGUAGE_AUTO;
    use switcher_windows::win_util::{PumpVerdict, pump_messages};
    use tray_icon::{
        Icon, TrayIconBuilder,
        menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    };

    let follow = CheckMenuItem::with_id(
        menu_ids::FOLLOW,
        escaped(&init.labels.follow),
        true,
        init.checks.follow,
        None,
    );
    let layout_fallback = CheckMenuItem::with_id(
        menu_ids::LAYOUT_FALLBACK,
        escaped(&init.labels.layout_fallback),
        true,
        init.checks.layout_fallback,
        None,
    );
    let sound = CheckMenuItem::with_id(
        menu_ids::SOUND,
        escaped(&init.labels.sound),
        true,
        init.checks.sound,
        None,
    );
    let autostart = CheckMenuItem::with_id(
        menu_ids::AUTOSTART,
        escaped(&init.labels.autostart),
        init.autostart_available,
        init.checks.autostart,
        None,
    );
    // One row per shipped catalog, built from the registry rather than from a hand-written
    // list: adding a translation is a new `.ftl` and a new row in `i18n::SUPPORTED`.
    // Windows has no radio-group menu item here, so these are check items kept mutually
    // exclusive by `sync_language`.
    let language_choices: Vec<(String, String)> = std::iter::once((
        UI_LANGUAGE_AUTO.to_owned(),
        init.labels.language_auto.clone(),
    ))
    .chain(
        i18n::SUPPORTED
            .iter()
            .map(|locale| (locale.tag.to_owned(), locale.native_name.to_owned())),
    )
    .collect();
    let language_items: Vec<(String, CheckMenuItem)> = language_choices
        .iter()
        .map(|(tag, text)| {
            let item = CheckMenuItem::with_id(
                menu_ids::language(tag),
                escaped(text),
                true,
                *tag == init.checks.ui_language,
                None,
            );
            (tag.clone(), item)
        })
        .collect();
    let language = Submenu::with_id(menu_ids::LANGUAGE, escaped(&init.labels.language), true);
    for (_, item) in &language_items {
        language.append(item)?;
    }
    let sync_language = |selected: &str| {
        for (tag, item) in &language_items {
            item.set_checked(tag == selected);
        }
    };
    // Separate inert rows are readable; newline characters in one Win32 menu label
    // do not form a reliable multiline status panel.
    let status = Submenu::with_id(menu_ids::STATUS, escaped(&init.labels.status), true);
    let set_status = |rows: Vec<String>| -> anyhow::Result<()> {
        while status.remove_at(0).is_some() {}
        for row in rows {
            status.append(&MenuItem::new(escaped(&row), false, None))?;
        }
        Ok(())
    };
    set_status(init.status)?;
    let quit = MenuItem::with_id(menu_ids::QUIT, escaped(&init.labels.quit), true, None);
    let menu = Menu::with_items(&[
        &follow,
        &layout_fallback,
        &sound,
        &autostart,
        &PredefinedMenuItem::separator(),
        &language,
        &status,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(init.tooltip)
        .with_icon(Icon::from_rgba(init.rgba, init.size, init.size)?)
        .with_menu_on_left_click(true)
        .with_hover_tracking(false) // No unused Enter/Move/Leave timer; see ADR-0017.
        .build()?;
    tracing::info!("tray ready");
    pump_messages(|| {
        // Limit one batch so an accidental producer flood cannot starve native input.
        for _ in 0..256 {
            let cmd = match rx.try_recv() {
                Ok(cmd) => cmd,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return PumpVerdict::Quit,
            };
            let applied: anyhow::Result<()> = match cmd {
                TrayCommand::SetIcon {
                    rgba_straight,
                    size,
                } => Icon::from_rgba(rgba_straight, size, size)
                    .map_err(anyhow::Error::from)
                    .and_then(|icon| tray.set_icon(Some(icon)).map_err(Into::into)),
                TrayCommand::SetTooltip(text) => tray.set_tooltip(Some(text)).map_err(Into::into),
                TrayCommand::SetStatus {
                    rows,
                    autostart_available,
                } => {
                    autostart.set_enabled(autostart_available);
                    set_status(rows)
                }
                TrayCommand::SyncChecks(checks) => {
                    follow.set_checked(checks.follow);
                    layout_fallback.set_checked(checks.layout_fallback);
                    sound.set_checked(checks.sound);
                    autostart.set_checked(checks.autostart);
                    sync_language(&checks.ui_language);
                    Ok(())
                }
                TrayCommand::Localize(labels) => {
                    follow.set_text(escaped(&labels.follow));
                    layout_fallback.set_text(escaped(&labels.layout_fallback));
                    sound.set_text(escaped(&labels.sound));
                    autostart.set_text(escaped(&labels.autostart));
                    language.set_text(escaped(&labels.language));
                    status.set_text(escaped(&labels.status));
                    quit.set_text(escaped(&labels.quit));
                    // Only the "same as the system" row is translated; the rest name
                    // themselves and stay as they are.
                    if let Some((_, item)) = language_items
                        .iter()
                        .find(|(tag, _)| tag == UI_LANGUAGE_AUTO)
                    {
                        item.set_text(escaped(&labels.language_auto));
                    }
                    Ok(())
                }
                TrayCommand::Shutdown => return PumpVerdict::Quit,
            };
            if let Err(error) = applied {
                tracing::warn!(%error, "could not update tray");
            }
        }
        if !rx.is_empty() {
            let waker = switcher_windows::win_util::PumpWaker::new(
                switcher_windows::win_util::current_thread_id(),
            );
            if let Err(error) = waker.wake() {
                tracing::warn!(%error, "tray continuation wake failed");
            }
        }
        PumpVerdict::Continue
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_come_from_the_active_catalog() {
        let ru = TrayLabels::new(&Translator::for_config("ru", &[]));
        let en = TrayLabels::new(&Translator::for_config("en", &[]));
        assert_eq!(ru.quit, "Выход");
        assert_eq!(en.quit, "Quit");
        assert_ne!(ru, en);
    }

    /// A label that still reads as an identifier means the catalog and `ids` drifted apart.
    #[test]
    fn every_label_resolves_in_every_shipped_language() {
        for locale in i18n::SUPPORTED {
            let labels = TrayLabels::new(&Translator::new(locale));
            for (field, text) in [
                ("follow", &labels.follow),
                ("layout_fallback", &labels.layout_fallback),
                ("sound", &labels.sound),
                ("autostart", &labels.autostart),
                ("status", &labels.status),
                ("language", &labels.language),
                ("language_auto", &labels.language_auto),
                ("quit", &labels.quit),
            ] {
                assert!(!text.trim().is_empty(), "{} {field} is empty", locale.tag);
                assert!(
                    !text.starts_with("menu-"),
                    "{} {field} fell through to its identifier: {text:?}",
                    locale.tag
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn ampersands_are_escaped_so_win32_does_not_eat_them() {
        assert_eq!(escaped("Sound && badge"), "Sound &&&& badge");
        assert_eq!(escaped("Ton"), "Ton");
    }

    /// The tray ticks the row whose tag equals the config value, so the default has to be
    /// a tag the submenu actually offers.
    #[test]
    fn the_default_check_state_names_an_offered_language() {
        let checks = Checks::default();
        let offered: Vec<&str> = std::iter::once(switcher_core::config::UI_LANGUAGE_AUTO)
            .chain(i18n::SUPPORTED.iter().map(|locale| locale.tag))
            .collect();
        assert!(offered.contains(&checks.ui_language.as_str()));
    }
}
