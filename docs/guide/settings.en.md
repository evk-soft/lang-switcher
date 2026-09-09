# Settings

[Русский](settings.ru.md) · [Installation](installation.en.md) · [Translations](translations.en.md)

Everything is controlled from the tray icon's menu (left or right click). This version has
no separate settings window; that belongs to M2.

## Tray menu

| Item | What it does |
| --- | --- |
| **Follow the cursor** | the badge stays visible and follows the mouse pointer instead of appearing for ~1.5 s on a layout change |
| **Layout fallback check** | polls the active window every 200 ms for windows that never notify Windows of a language change |
| **Sound** | short cue on a layout change |
| **Start with Windows** | launch at logon |
| **Language** | interface language of the application itself |
| **Status** | diagnostics: what is working only partly |
| **Quit** | shut down |

Every change applies immediately and is written to the settings file.

### Layout fallback check

On by default. Some windows never send Windows a notification when the input language
changes — without the fallback the badge in them will not update until the next event.

Unticking it removes the timer entirely: the application stays fully event-driven, but a
language change in such a window can go unnoticed. Reasoning and measurements are in
[ADR-0019](../architecture/adr/0019-configurable-layout-fallback.md) (Russian).

### Language

The "Language" submenu lists "Same as Windows" plus each language written in itself:
English, Русский, Deutsch, Español, Français, 简体中文.

This is the **application's interface** language. It has nothing to do with what you type:
Windows owns the keyboard layouts, this application only shows them.

"Same as Windows" reads the Windows display language
(`GetUserPreferredUILanguages`) — not the keyboard layout and not the regional date
formats. If no translation ships for that language, the interface is English.

Changing the language redraws the menu, the tooltip and the status immediately. It plays
no sound and shows no badge: picking a menu language is not an input event.

### Status

Shows "Everything works", or a list of limitations. A limitation row looks like this:

```
sound: device_open_failed — No audio endpoint available
```

Those rows are **deliberately not translated**: they are a capability key, a stable error
code and the adapter's technical detail. They are what belongs in a bug report — identical
in every interface language, and matching what the log contains.

## Settings file

`%APPDATA%\evk-soft\lang-switcher\config\config.toml`

The file is created when a setting is first saved, not at first launch. You can edit it by
hand while the application is closed.

```toml
version = 1
autostart = false
log_level = "info"
ui_language = "auto"

[badge]
mode = "transient"
anchor = "auto"
style = "text"
show_ms = 1500

[badge.colors]

[sound]
enabled = true
volume = 0.4

[layout]
fallback_enabled = true
```

| Field | Values | Default | Meaning |
| --- | --- | --- | --- |
| `version` | `1` | `1` | schema version; a newer one is neither opened nor overwritten |
| `autostart` | `true` / `false` | `false` | mirrors the registry value; the registry is the truth, not this file |
| `log_level` | `error` `warn` `info` `debug` `trace` `off` | `info` | log verbosity |
| `ui_language` | `auto` or a language tag | `auto` | interface language |
| `badge.mode` | `transient` / `follow` | `transient` | shown briefly, or permanently |
| `badge.anchor` | `auto` / `cursor` / `fixed` | `auto` | what the badge is anchored to |
| `badge.style` | `text` / `color` | `text` | language letters, or a coloured square |
| `badge.show_ms` | 200…10000 | `1500` | how long the badge stays in `transient` mode |
| `badge.colors` | tag → `"#RRGGBB"` | empty | custom background colour per language |
| `sound.enabled` | `true` / `false` | `true` | cue on a layout change |
| `sound.volume` | 0.0…1.0 | `0.4` | cue volume |
| `layout.fallback_enabled` | `true` / `false` | `true` | the fallback check above |

### How bad values are handled

A broken file never stops the application from starting:

- an out-of-range value is clamped (`show_ms = 50` becomes `200`);
- an unknown `log_level` becomes `info`;
- a `ui_language` that is not a language tag becomes `auto`;
- a colour that is not `#RRGGBB` is dropped;
- unknown fields are ignored, so the file survives a downgrade to an earlier version.

Every correction is logged and listed under "Status".

**A corrupt or newer file is never overwritten.** The application starts with defaults and
disables saving for that session, so it cannot destroy a file you may have been editing.

### Custom badge colours

The key is the primary language subtag. Region and letter case are normalized (`ru-RU` and
`RU` both become `ru`):

```toml
[badge.colors]
ru = "#D64545"
en = "#3D6FD9"
de = "#2E8B57"
```

Russian and English have their own colours by default; every other language gets a neutral
grey.

## Logs

`%LOCALAPPDATA%\evk-soft\lang-switcher\data\logs`, one file per UTC day, last seven kept.

An environment variable overrides the level from the settings file, which is convenient for
a one-off diagnosis without editing the config:

```powershell
$env:LANG_SWITCHER_LOG = "debug"
& "$env:LOCALAPPDATA\Programs\lang-switcher\lang-switcher.exe"
```

## Diagnostic run

A separate instance with its own data directory. It does not disturb a normal copy and is
exempt from the one-copy-per-session rule:

```powershell
& "$env:LOCALAPPDATA\Programs\lang-switcher\lang-switcher.exe" --data-dir C:\temp\ls-diag --run-for 30
```

`--run-for` accepts 1…3600 seconds. Without it the application runs until you quit from the
tray.
