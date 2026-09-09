# lang-switcher

[Русский](README.md)

A lightweight keyboard-layout indicator: a small badge near the mouse cursor and an
optional sound cue when the layout changes, so you always know which language you are
typing in.

**Status: alpha.** Windows 11 x64 only. The application indicates the input language; it
does not switch layouts — Windows still does that.

## Download

[Latest release](https://github.com/evk-soft/lang-switcher/releases/latest) — installer,
portable archive and checksums.

Installing needs no administrator rights. The files are **not code-signed**, so Windows
will show a SmartScreen warning; verify the SHA-256 sum before running anything. Details
are in the [installation guide](docs/guide/installation.en.md).

## Features

- A badge for the current input language near the cursor: for ~1.5 seconds on a layout
  change, or permanently. Always on top, click-through, never steals focus, correct across
  monitors with different DPI.
- Distinct cues for Russian and English, a neutral one for other languages.
- Tray menu: follow mode, layout fallback check, sound, autostart, interface language,
  status, quit.
- Interface in six languages: English, Русский, Deutsch, Español, Français, 简体中文.
  "Same as Windows" follows the Windows display language.
- A failure in any part of the OS limits one capability instead of crashing the
  application; the reason is visible under "Status".
- No network connections at all: no telemetry, no update check.

## Alpha limitations

- Windows 11 x64 only; macOS and Linux are not shipped.
- The files are unsigned.
- IME modes are untested; regional variants of one language share a label.
- Caret anchoring is not implemented (M2).
- The 5–15 MB memory target is not met: measured RSS is 17–21 MiB.

The full list is in the [CHANGELOG](CHANGELOG.md).

## Documentation

| | |
| --- | --- |
| [Installation](docs/guide/installation.en.md) | downloading, SmartScreen, upgrading, uninstalling |
| [Settings](docs/guide/settings.en.md) | tray menu, every `config.toml` field, logs |
| [Translations](docs/guide/translations.en.md) | adding an interface or documentation language |
| [Development](docs/guide/development.en.md) | building, gates, releasing |

Internal documents are kept in Russian only:
[architecture overview](docs/architecture/overview.md),
[ADRs](docs/architecture/adr/),
[design specification](docs/superpowers/specs/2026-07-03-lang-switcher-design.md),
[research notes](docs/research/).

## Technology

Pure native **Rust**, no webview: a Cargo workspace with a UI-independent core and
per-OS adapters. The reasoning is in the
[technology audit](docs/research/2026-07-03-tech-audit.md) and
[ADR-0001](docs/architecture/adr/0001-pure-native-rust.md), both in Russian.

| Crate | Role |
| --- | --- |
| `switcher-core` | domain: layout state machine, badge anchor decision, config model. No OS dependencies, `#![forbid(unsafe_code)]` |
| `switcher-platform` | ports (traits) and flat event types |
| `switcher-windows` | Win32/COM adapters — the only place `unsafe` is allowed |
| `switcher-app` | shell: tray, main loop, badge rasterization, localization, sound, wiring |

## Building

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Rust stable with the MSVC toolchain; MSRV 1.88. Details and release packaging are in the
[development guide](docs/guide/development.en.md).

## Security

How to report a vulnerability, and exactly what this program does with your data:
[SECURITY.md](SECURITY.md).

## Licence

Dual-licensed at your option: [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
The embedded Inter SemiBold subset is covered by
[SIL OFL 1.1](crates/switcher-app/assets/fonts/LICENSE.txt);
[provenance and reproduction](crates/switcher-app/assets/fonts/README.md).
Dependency licences ship with each release in `THIRD-PARTY-LICENSES.md`.
