# Translations

[Русский](translations.ru.md) · [Installation](installation.en.md) · [Settings](settings.en.md)

Two independent sets of translations: the application's interface, and the documentation.

## Interface language

The format is [Fluent](https://projectfluent.org/), one `.ftl` file per language in
`crates/switcher-app/assets/i18n/`. Catalogs are embedded into the binary at build time, so
a new language needs a rebuild
([ADR-0021](../architecture/adr/0021-fluent-embedded-ui-catalogs.md), Russian).

Currently shipped: `en`, `ru`, `de`, `es`, `fr`, `zh-Hans`.

### Adding a language

1. Copy `assets/i18n/en.ftl` to a file named after the canonical language tag, for example
   `pt-BR.ftl` or `ja.ftl`. Case matters: language lowercase, script title case, region
   uppercase — `zh-Hans`, `pt-BR`.
2. Translate the values. **Leave the identifiers left of `=` alone** and delete none of
   them: the set of identifiers must match `en.ftl` exactly.
3. Save as UTF-8 without a BOM.
4. Add one row to the `SUPPORTED` array in
   [`crates/switcher-app/src/i18n.rs`](../../crates/switcher-app/src/i18n.rs):

   ```rust
   Locale {
       tag: "pt-BR",
       native_name: "Português (Brasil)",
       ftl: include_str!("../assets/i18n/pt-BR.ftl"),
   },
   ```

   `native_name` is the language's name **in that language**: a language picker is read by
   someone who does not yet understand the current interface language.
5. Run `cargo test -p switcher-app i18n`.

There is nothing else to edit. The tray submenu, locale negotiation and the completeness
check all read `SUPPORTED`.

### What the tests check

- every catalog parses and loads;
- the identifier set matches `en.ftl` — nothing missing, nothing extra;
- no catalog declares an identifier twice;
- every message the code asks for exists in every catalog;
- arguments (`{ $label }`, `{ $names }`) are substituted;
- no message is empty or resolves to its own identifier;
- the tag is canonical and survives the config sanitizer;
- native names are unique.

### Fluent syntax, briefly

```ftl
# A comment.
menu-quit = Quit
tooltip-badge = lang-switcher · { $label }
```

The braced argument is filled in by the application. Its name must not change; its position
in the sentence can and should, if the language needs it.

### What the catalogs do not contain

Capability keys (`sound`, `layout.tsf`), adapter error codes, log fields and config values
are not translated. They are what people grep logs for and quote in bug reports, so they are
identical in every language.

### Notes on language matching

- A requested tag is matched exactly, then by language and script, then by language alone.
  That is why `fr-CA` finds `fr`.
- While only `zh-Hans` ships, every Chinese preference resolves to it, `zh-TW` included.
  Once a `zh-Hant` catalog exists, resolving region-only tags correctly will need CLDR
  likely-subtags data.
- Right-to-left languages are neither shipped nor tested. Fluent alone is not enough for
  them: the Win32 native menu layout has to be checked as well.

## Documentation

Files under `docs/guide/` come in pairs: `<name>.ru.md` and `<name>.en.md`. The top line of
each file links to its counterpart.

To add a language, put `<name>.<tag>.md` next to them and add a link to the headers of the
existing pair. `README.md` is Russian, `README.en.md` is English.

ADRs and research notes under `docs/architecture/` and `docs/research/` are kept in Russian
only: they are an internal decision log, not user documentation.
