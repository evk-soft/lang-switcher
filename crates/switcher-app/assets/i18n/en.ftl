### lang-switcher user interface, English.
###
### This catalog is the base one (ADR-0021): it defines the complete set of message
### identifiers, and a test compares every other catalog against it. Adding a message
### here without adding it everywhere else makes those catalogs fall back to English
### for that one line; the test reports it as missing.
###
### Machine-readable diagnostics — capability keys, adapter error codes, log fields and
### config values — are deliberately absent: they must stay identical in every language.

## Tray menu

menu-follow = Follow the cursor
menu-layout-fallback = Layout fallback check
menu-sound = Sound
menu-autostart = Start with Windows
menu-status = Status
menu-language = Language
menu-language-auto = Same as Windows
menu-quit = Quit

## Tray tooltip. $label is the badge text of the current input language (RU, EN, ...),
## $names is an already joined list of limited features.

tooltip-starting = lang-switcher · starting
tooltip-badge = lang-switcher · { $label }
tooltip-warning = ⚠ { $names }
tooltip-details = ⚠ See "Status" for details

## Status submenu

status-all-ok = Everything works
status-diagnostics = Limited features. The codes below are diagnostics and stay in English:

## Feature names used in the tooltip warning. Lower case: they appear inside a list.

capability-layout = layout
capability-pointer = cursor
capability-caret = caret
capability-overlay = badge
capability-sound = sound
capability-autostart = autostart
