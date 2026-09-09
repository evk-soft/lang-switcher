# First public alpha: localization and distribution research

Date: 2026-09-09. [Русский](2026-09-09-alpha-localization.ru.md).

This is a researched proposal, not a release announcement or a claim that the proposed features are implemented. Baseline: `8adbe8e`.

## Current capabilities

- The executable currently implements Windows adapters only. The platform-independent core also has Linux CI coverage; that does not constitute a working Linux application.
- The application observes the active input language. Windows or the foreground application performs the actual keyboard-layout switching.
- `switcher-windows/src/layout_monitor/snapshot.rs::language_for` resolves the language from an HKL via `LCIDToLocaleName`. This is not hard-coded to Russian and English.
- `switcher-core/src/content.rs` assigns special colors and sounds to RU/EN. Other recognized languages receive a neutral color and sound. The current label takes the first two characters of the primary language subtag. Before advertising broader support, retain complete two- or three-letter language codes: truncating `fil` to `FI`, for example, produces a misleading Finnish-looking label.
- Regional variants currently share a language label. Input-method composition modes, such as Japanese direct input versus hiragana, are not a verified capability.
- The configuration accepts `ui_language = "ru"` or `"en"`, but native menu strings and status text remain hard-coded in Russian. English configuration acceptance is not evidence of an English UI.

[Microsoft documents HKL](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeyboardlayout) as an input locale identifier that may also represent an IME. This warrants explicit testing and bounded claims rather than advertising universal IME support.

## Rust localization approaches

| Approach | Benefits | Costs and fit |
| --- | --- | --- |
| Fluent with `fluent-bundle` | Translation-owned grammar, plural/select expressions, named arguments, Unicode isolation; direct control over loaded catalogs | Requires a small application wrapper for resource lookup and fallback. Recommended for an extensible native UI. |
| `rust-i18n` | Embedded YAML/JSON/TOML catalogs, convenient translation macro, locale fallback | A viable simpler alternative; its API and global-locale conventions differ from the existing channel-driven application. |
| Handwritten Rust string tables | Minimal initial dependency overhead | Each language grows application code; grammar, interpolation and translator tooling would need additional design. Poor fit for the requested future extension. |

Primary references: [Project Fluent](https://projectfluent.org/), [fluent-bundle](https://docs.rs/fluent-bundle/latest/fluent_bundle/), [rust-i18n](https://docs.rs/rust-i18n/latest/rust_i18n/). `fluent-templates` also provides an [embedded static loader](https://docs.rs/fluent-templates/latest/fluent_templates/); it is an alternative to writing the small resource-selection wrapper.

## Proposed application contract

1. Put language identifiers, native language names and embedded `.ftl` catalogs in one application localization module. Keep catalogs separate from Rust source. Load only the selected language and the English fallback when using the direct bundle approach.
2. Use Unicode language identifiers, including region/script subtags where needed. `unic-langid` provides [parsing and canonicalization](https://docs.rs/unic-langid/latest/unic_langid/); do not substitute arbitrary string truncation for locale negotiation.
3. Offer `Language -> System / English / Русский / ...` in the tray, display native language names, apply changes to menu labels, tooltip and status immediately, and persist the preference. UI language remains independent of keyboard input language.
4. Resolve `System` from Windows display-language preferences, not keyboard layout or date/number formatting settings. The Windows adapter calls [GetUserPreferredUILanguages](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-getuserpreferreduilanguages); the shell consumes plain strings through a platform boundary.
5. Negotiate supported language variants and use English for missing messages or unsupported languages. Keep supported-language registration centralized so adding a catalog does not require edits throughout the tray implementation.
6. Preserve existing explicit preferences and unrelated config values. New installations default to `System`. Machine-readable diagnostic codes and developer logs stay stable; user-facing descriptions are localized.
7. Validate every shipped catalog in CI: syntax, duplicate and missing message IDs, argument compatibility, formatting errors, fallback behavior and supported locale selection. Test runtime language switching and persistence separately from keyboard-layout changes.
8. Keep text Unicode-safe and preserve Fluent's directional isolation. RTL language support needs UI validation when an RTL translation is added; using Fluent alone does not prove the native menu layout is correct in RTL.

The proposed initial interface languages are Russian, English, German, Spanish, French and Brazilian Portuguese; the user is choosing the final set. The user has already requested Russian and English documentation and room for additional languages.

## Installer and release proposal

Publish `v0.1.0-alpha.1` as a GitHub prerelease with a downloadable Windows installer, a portable ZIP, SHA-256 checksums and bilingual release notes. The proposed initial supported target is Windows 11 x64; broader OS/CPU support requires an explicit scope and validation.

Use [Inno Setup](https://jrsoftware.org/isinfo.php) to build the installer. It provides multilingual Unicode installers, uninstall support and per-user installation. Its [license](https://jrsoftware.org/files/is/license.txt) permits redistribution subject to its stated conditions; retain the notices included in the unmodified installer runtime. WiX is another option for MSI-based deployment, but its current [maintenance-fee policy](https://docs.firegiant.com/wix/osmf/) and additional packaging machinery should be evaluated before choosing it. MSI is not required for a first per-user desktop alpha.

Proposed installation behavior:

- Install into a stable per-user application directory using `PrivilegesRequired=lowest`; do not request administrative rights.
- Add a Start menu shortcut and an uninstall entry. Keep autostart optional, controlled by the application tray.
- Preserve existing user configuration across upgrades. Uninstall application files and the application's relevant autostart entry without deleting unrelated files or registry values. Preserve user data by default.
- Prevent duplicate production instances. Use a named application mutex that the installer also checks before updating or uninstalling a running application. Keep explicitly isolated diagnostic runs separate. [Inno Setup AppMutex](https://jrsoftware.org/ishelp/topic_setup_appmutex.htm) describes this application/installer contract.
- Build the Windows MSVC target explicitly, embed version/manifest resources, bundle third-party license notices and test installation, launching, upgrading and uninstalling in CI. A clean Windows user trial remains useful for checks hosted runners cannot establish.
- Generate release assets from a fixed source commit with pinned build tools. Run formatting, linting, tests, dependency/security checks and installer smoke checks before attaching assets to a draft release. Publish the complete draft as a prerelease. [GitHub release workflow](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository).

The repository is still private. There are four older stacked PRs and no releases. Publication should present the consolidated alpha on `main` after its checks pass, and reconcile superseded PRs. Existing personal email in commit metadata requires the user's visibility preference before opening the repository; no history rewrite has been performed.

## Decisions requested from the user

- Initial interface language set.
- Supported Windows versions and CPU architectures.
- Whether to retain existing commit metadata when making the repository public.
- Whether bilingual documentation includes all historical research/ADRs or the public installation, configuration, development, contribution and release guides first.
- Availability of Windows code signing, or acceptance of an unsigned first alpha.

These choices affect the release scope. No installer, localization implementation, visibility change, push, merge or release publication has been performed as part of this research.
