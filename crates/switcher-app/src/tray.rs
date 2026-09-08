//! Plain commands cross threads; all native tray objects stay on the main thread.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Checks {
    pub follow: bool,
    pub layout_fallback: bool,
    pub sound: bool,
    pub autostart: bool,
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
    Shutdown,
}

#[derive(Debug)]
pub struct TrayInit {
    pub rgba: Vec<u8>,
    pub size: u32,
    pub tooltip: String,
    pub status: Vec<String>,
    pub checks: Checks,
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
    use crate::menu::ids;
    use crossbeam_channel::TryRecvError;
    use switcher_windows::win_util::{PumpVerdict, pump_messages};
    use tray_icon::{
        Icon, TrayIconBuilder,
        menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    };

    let follow = CheckMenuItem::with_id(
        ids::FOLLOW,
        "Следовать за курсором",
        true,
        init.checks.follow,
        None,
    );
    let layout_fallback = CheckMenuItem::with_id(
        ids::LAYOUT_FALLBACK,
        "Резервная проверка раскладки",
        true,
        init.checks.layout_fallback,
        None,
    );
    let sound = CheckMenuItem::with_id(ids::SOUND, "Звук", true, init.checks.sound, None);
    let autostart = CheckMenuItem::with_id(
        ids::AUTOSTART,
        "Автозапуск",
        init.autostart_available,
        init.checks.autostart,
        None,
    );
    // Separate inert rows are readable; newline characters in one Win32 menu label
    // do not form a reliable multiline status panel.
    let status = Submenu::with_id(ids::STATUS, "Состояние", true);
    let set_status = |rows: Vec<String>| -> anyhow::Result<()> {
        while status.remove_at(0).is_some() {}
        for row in rows {
            status.append(&MenuItem::new(row.replace('&', "&&"), false, None))?;
        }
        Ok(())
    };
    set_status(init.status)?;
    let quit = MenuItem::with_id(ids::QUIT, "Выход", true, None);
    let menu = Menu::with_items(&[
        &follow,
        &layout_fallback,
        &sound,
        &autostart,
        &PredefinedMenuItem::separator(),
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
