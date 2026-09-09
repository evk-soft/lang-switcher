# Installation

[Русский](installation.ru.md) · [Settings](settings.en.md) · [Translations](translations.en.md)

## Requirements

- Windows 11 x64 (build 22000 or newer).
- Administrator rights are **not** needed: the application installs into the current
  user's profile.

Windows 10 has not been tested. The installer refuses to run on it: it declares a minimum
of 10.0.22000.

## Downloads

Release files live on the
[Releases](https://github.com/evk-soft/lang-switcher/releases) page.

| File | What it is |
| --- | --- |
| `lang-switcher-<version>-windows-x64-setup.exe` | installer |
| `lang-switcher-<version>-windows-x64-portable.zip` | no-install build |
| `SHA256SUMS.txt` | checksums for both |

## The SmartScreen warning

Alpha files are **not code-signed** — the project has no certificate yet. Windows will
show a blue "Windows protected your PC" dialog. To continue: "More info" → "Run anyway".

Verify the checksum before you do. For an unsigned file it is the only integrity check
you have:

```powershell
Get-FileHash .\lang-switcher-0.1.0-alpha.1-windows-x64-setup.exe -Algorithm SHA256
```

The result must match the line for that file in `SHA256SUMS.txt` (letter case does not
matter). If it does not match, do not run the file.

## Installing

Run `...-setup.exe` and follow the wizard. It:

- installs into `%LOCALAPPDATA%\Programs\lang-switcher`;
- creates a Start menu shortcut;
- registers an entry in Apps & Features;
- offers to launch the application when it finishes.

The wizard is available in English, Russian, German, Spanish and French.

The installer does **not** enable start-at-logon. That is a checkbox in the tray menu —
see [settings](settings.en.md).

### Silent install

```powershell
.\lang-switcher-0.1.0-alpha.1-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

## Portable build

Unpack the archive anywhere and run `lang-switcher.exe`. Settings and logs still live in
`%APPDATA%` and `%LOCALAPPDATA%`, exactly as for an installed copy.

One difference matters: autostart records the **absolute path** of the executable you
enabled it from. Move the directory afterwards and the autostart entry points at the old
location. Turn the checkbox off and on again after moving.

## Upgrading

Run the new installer over the old installation. The `AppId` is unchanged, so the new
version replaces the previous one instead of appearing beside it. Settings and logs are
kept.

If the application is running, the installer asks you to close it — it detects a running
copy through a named mutex.

## Uninstalling

Settings → Apps → Installed apps → lang-switcher → Uninstall.

Uninstalling removes the program files, the shortcut, and the autostart registry value if
one was created.

**Settings and logs are kept on purpose** — they are your data. To remove them too:

```powershell
Remove-Item "$env:APPDATA\evk-soft\lang-switcher" -Recurse
Remove-Item "$env:LOCALAPPDATA\evk-soft\lang-switcher" -Recurse
```

## Where things live

| What | Path |
| --- | --- |
| Program | `%LOCALAPPDATA%\Programs\lang-switcher` |
| Settings | `%APPDATA%\evk-soft\lang-switcher\config\config.toml` |
| Logs | `%LOCALAPPDATA%\evk-soft\lang-switcher\data\logs` |
| Autostart | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `lang-switcher` |

## If something does not work

- **It will not start a second time.** That is deliberate: one copy per session. The icon
  is already in the tray, possibly hidden behind the "Show hidden icons" arrow.
- **No badge appears when the language changes.** Check that "Layout fallback check" is
  ticked in the tray menu. Some windows never send Windows a language-change
  notification, and without the fallback the change goes unnoticed.
- **Something works only partly.** Tray menu → "Status". Those rows are diagnostics: they
  are not translated and exist to be pasted into a bug report, together with that day's
  log file.
