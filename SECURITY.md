# Security

## Supported versions

lang-switcher is at the alpha stage. Only the most recent release receives fixes.

| Version | Supported |
| --- | --- |
| 0.1.0-alpha.1 | yes |
| anything older | no |

## Reporting a vulnerability

Use GitHub's private reporting: **Security → Report a vulnerability** on
<https://github.com/evk-soft/lang-switcher>. Please do not open a public issue for a
vulnerability.

Include the version from the tray tooltip or the binary's Details tab, the Windows build,
and what an attacker would gain. If the report concerns a crash, the day's log file from
`%LOCALAPPDATA%\evk-soft\lang-switcher\data\logs` helps.

Expect a first response within a week. This is a one-person project, so please do not
assume silence means the report was dismissed — ping the thread.

## What this program does and does not do

Useful when judging whether something is a security problem:

- It **reads** the active keyboard layout and the mouse cursor position. It does not read
  keystrokes, does not hook the keyboard, and does not change the layout.
- It makes **no network connections at all**. No telemetry, no update check. Translations
  are compiled into the binary and are never fetched or read from disk.
- It runs with the privileges of the user who started it and never asks for elevation.
- It writes to exactly three places: `%APPDATA%\evk-soft\lang-switcher`,
  `%LOCALAPPDATA%\evk-soft\lang-switcher`, and, if you enable autostart from the tray, one
  value under `HKCU\...\CurrentVersion\Run`.
- Logs contain layout identifiers, language tags and window-independent diagnostics. They
  contain no typed text.

## Alpha releases are not code-signed

There is no code-signing certificate for this project yet, so Windows SmartScreen warns
about the downloaded files. `SHA256SUMS.txt` is published with every release and is the
only integrity check available; verify it before running an installer. See the
[installation guide](docs/guide/installation.en.md).

## Known dependency advisories

Checked against OSV before each release.

### RUSTSEC-2026-0192 — `ttf-parser` 0.25.1

An **unmaintained** notice, not a vulnerability. `ttf-parser` is reached through
`ab_glyph`, which rasterizes the badge text.

Accepted for the alpha. The only font this program ever parses is the Inter SemiBold subset
compiled into the binary; it never loads a font from disk, from a document or from the
network, so the parser is never given untrusted input. This is re-evaluated if an actual
CVE is filed against it.

### RUSTSEC-2026-0009 — `time` before 0.3.47

Fixed. The dependency was updated to 0.3.55, which required raising the minimum supported
Rust version to 1.88
([ADR-0023](docs/architecture/adr/0023-msrv-1-88-for-time-advisory.md), Russian).
