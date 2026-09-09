//! UI localization: embedded Fluent catalogs, locale negotiation, English fallback
//! (ADR-0021, ADR-0022).
//!
//! Adding a language is one `.ftl` file plus one row in [`SUPPORTED`]. Nothing else in
//! the application knows how many languages exist.

use std::borrow::Cow;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// Message identifiers used by the shell. Grouped in one place so a test can assert that
/// every one of them resolves in every catalog: a typo here would otherwise surface as a
/// raw identifier in the tray menu of whichever language nobody checked.
pub mod ids {
    pub const MENU_FOLLOW: &str = "menu-follow";
    pub const MENU_LAYOUT_FALLBACK: &str = "menu-layout-fallback";
    pub const MENU_SOUND: &str = "menu-sound";
    pub const MENU_AUTOSTART: &str = "menu-autostart";
    pub const MENU_STATUS: &str = "menu-status";
    pub const MENU_LANGUAGE: &str = "menu-language";
    pub const MENU_LANGUAGE_AUTO: &str = "menu-language-auto";
    pub const MENU_QUIT: &str = "menu-quit";

    pub const TOOLTIP_STARTING: &str = "tooltip-starting";
    pub const TOOLTIP_BADGE: &str = "tooltip-badge";
    pub const TOOLTIP_WARNING: &str = "tooltip-warning";
    pub const TOOLTIP_DETAILS: &str = "tooltip-details";

    pub const STATUS_ALL_OK: &str = "status-all-ok";
    pub const STATUS_DIAGNOSTICS: &str = "status-diagnostics";

    pub const CAPABILITY_LAYOUT: &str = "capability-layout";
    pub const CAPABILITY_POINTER: &str = "capability-pointer";
    pub const CAPABILITY_CARET: &str = "capability-caret";
    pub const CAPABILITY_OVERLAY: &str = "capability-overlay";
    pub const CAPABILITY_SOUND: &str = "capability-sound";
    pub const CAPABILITY_AUTOSTART: &str = "capability-autostart";

    /// Every identifier the shell asks for, in catalog order.
    pub const ALL: &[&str] = &[
        MENU_FOLLOW,
        MENU_LAYOUT_FALLBACK,
        MENU_SOUND,
        MENU_AUTOSTART,
        MENU_STATUS,
        MENU_LANGUAGE,
        MENU_LANGUAGE_AUTO,
        MENU_QUIT,
        TOOLTIP_STARTING,
        TOOLTIP_BADGE,
        TOOLTIP_WARNING,
        TOOLTIP_DETAILS,
        STATUS_ALL_OK,
        STATUS_DIAGNOSTICS,
        CAPABILITY_LAYOUT,
        CAPABILITY_POINTER,
        CAPABILITY_CARET,
        CAPABILITY_OVERLAY,
        CAPABILITY_SOUND,
        CAPABILITY_AUTOSTART,
    ];
}

/// One shipped translation.
#[derive(Debug, Clone, Copy)]
pub struct Locale {
    /// Canonical Unicode language identifier, exactly as it may appear in `ui_language`.
    pub tag: &'static str,
    /// The language's name in that language. Never translated: a language picker is read
    /// by someone who does not yet understand the current interface language.
    pub native_name: &'static str,
    ftl: &'static str,
}

/// The English catalog is the fallback and the reference set of identifiers (ADR-0021).
pub const FALLBACK_TAG: &str = "en";

/// The shipped translations. Order is the order of the tray language submenu.
pub const SUPPORTED: &[Locale] = &[
    Locale {
        tag: "en",
        native_name: "English",
        ftl: include_str!("../assets/i18n/en.ftl"),
    },
    Locale {
        tag: "ru",
        native_name: "Русский",
        ftl: include_str!("../assets/i18n/ru.ftl"),
    },
    Locale {
        tag: "de",
        native_name: "Deutsch",
        ftl: include_str!("../assets/i18n/de.ftl"),
    },
    Locale {
        tag: "es",
        native_name: "Español",
        ftl: include_str!("../assets/i18n/es.ftl"),
    },
    Locale {
        tag: "fr",
        native_name: "Français",
        ftl: include_str!("../assets/i18n/fr.ftl"),
    },
    Locale {
        tag: "zh-Hans",
        native_name: "简体中文",
        ftl: include_str!("../assets/i18n/zh-Hans.ftl"),
    },
];

fn fallback_locale() -> &'static Locale {
    SUPPORTED
        .iter()
        .find(|locale| locale.tag == FALLBACK_TAG)
        .expect("the fallback catalog is part of SUPPORTED")
}

/// Best shipped catalog for one requested tag, or `None` if nothing matches.
///
/// Three passes, most specific first. The last one matches on the language subtag alone,
/// so `zh-TW` resolves to the only Chinese catalog we ship (`zh-Hans`) rather than to
/// English. That is a deliberate simplification for the alpha: distinguishing Traditional
/// from Simplified for a region-only tag needs CLDR likely-subtags data, which is worth
/// adding on the day a `zh-Hant` catalog exists.
fn match_tag(requested: &LanguageIdentifier) -> Option<&'static Locale> {
    let parsed: Vec<(LanguageIdentifier, &'static Locale)> = SUPPORTED
        .iter()
        .filter_map(|locale| Some((locale.tag.parse().ok()?, locale)))
        .collect();
    let exact = parsed.iter().find(|(tag, _)| tag == requested);
    let by_script = || {
        parsed
            .iter()
            .find(|(tag, _)| tag.language == requested.language && tag.script == requested.script)
    };
    let by_language = || {
        parsed
            .iter()
            .find(|(tag, _)| tag.language == requested.language)
    };
    exact
        .or_else(by_script)
        .or_else(by_language)
        .map(|(_, locale)| *locale)
}

/// Picks the catalog for a `ui_language` config value.
///
/// `configured` is either [`switcher_core::config::UI_LANGUAGE_AUTO`] or a canonical tag
/// (the core guarantees that, ADR-0022). `system` is the OS display-language preference
/// list, most preferred first; it is consulted only in `auto` mode.
pub fn resolve(configured: &str, system: &[String]) -> &'static Locale {
    if configured != switcher_core::config::UI_LANGUAGE_AUTO {
        return configured
            .parse()
            .ok()
            .as_ref()
            .and_then(match_tag)
            .unwrap_or_else(|| {
                // A valid tag we ship no catalog for: keep the config value, show English.
                tracing::info!(
                    tag = configured,
                    "no catalog for the configured interface language; using English"
                );
                fallback_locale()
            });
    }
    system
        .iter()
        .filter_map(|tag| tag.parse().ok())
        .find_map(|tag: LanguageIdentifier| match_tag(&tag))
        .unwrap_or_else(fallback_locale)
}

fn bundle_for(locale: &'static Locale) -> FluentBundle<FluentResource> {
    let langid: LanguageIdentifier = locale.tag.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    // Fluent wraps interpolated arguments in FSI/PDI marks by default. Win32 menu labels
    // and the 126-UTF-16-unit tray tooltip would carry them as invisible characters that
    // still cost budget, and no shipped catalog is RTL (ADR-0021).
    bundle.set_use_isolating(false);
    match FluentResource::try_new(locale.ftl.to_owned()) {
        Ok(resource) => {
            if let Err(errors) = bundle.add_resource(resource) {
                tracing::error!(tag = locale.tag, ?errors, "translation catalog is unusable");
            }
        }
        Err((_, errors)) => {
            tracing::error!(
                tag = locale.tag,
                ?errors,
                "translation catalog does not parse"
            );
        }
    }
    bundle
}

/// Turns message identifiers into text for one locale, falling back to English.
pub struct Translator {
    locale: &'static Locale,
    bundle: FluentBundle<FluentResource>,
    /// Absent when `locale` already is the English catalog.
    fallback: Option<FluentBundle<FluentResource>>,
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translator")
            .field("locale", &self.locale.tag)
            .finish_non_exhaustive()
    }
}

impl Translator {
    pub fn new(locale: &'static Locale) -> Self {
        Self {
            locale,
            bundle: bundle_for(locale),
            fallback: (locale.tag != FALLBACK_TAG).then(|| bundle_for(fallback_locale())),
        }
    }

    /// Convenience for the startup and menu paths: negotiate, then build.
    pub fn for_config(configured: &str, system: &[String]) -> Self {
        Self::new(resolve(configured, system))
    }

    pub fn locale(&self) -> &'static Locale {
        self.locale
    }

    pub fn text(&self, id: &str) -> String {
        self.format(id, None)
    }

    pub fn text_with(&self, id: &str, args: &FluentArgs) -> String {
        self.format(id, Some(args))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        if let Some(text) = lookup(&self.bundle, id, args) {
            return text;
        }
        if let Some(fallback) = &self.fallback
            && let Some(text) = lookup(fallback, id, args)
        {
            tracing::debug!(tag = self.locale.tag, id, "message missing, used English");
            return text;
        }
        // Not even English has it: show the identifier rather than an empty menu row, so
        // the breakage is visible instead of silent.
        tracing::error!(id, "unknown message identifier");
        id.to_owned()
    }
}

fn lookup(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs>,
) -> Option<String> {
    let pattern = bundle.get_message(id)?.value()?;
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    if !errors.is_empty() {
        tracing::warn!(id, ?errors, "message formatting reported errors");
    }
    Some(match text {
        Cow::Borrowed(text) => text.to_owned(),
        Cow::Owned(text) => text,
    })
}

#[cfg(test)]
mod tests;
