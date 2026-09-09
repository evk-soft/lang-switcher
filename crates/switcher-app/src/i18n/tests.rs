use std::collections::BTreeSet;

use super::*;

/// Message identifiers declared by a catalog, in file order.
///
/// Deliberately textual instead of going through `fluent-syntax`: the parser is a private
/// dependency of `fluent-bundle`, and an independent reading is what makes the
/// completeness check meaningful — a bug in how we load a catalog cannot hide a missing
/// message from a test that reads the file itself.
fn identifiers(ftl: &str) -> Vec<String> {
    ftl.lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with(char::is_whitespace))
        .filter_map(|line| line.split_once('='))
        .map(|(id, _)| id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .collect()
}

fn english() -> &'static Locale {
    fallback_locale()
}

#[test]
fn every_catalog_parses_and_loads() {
    for locale in SUPPORTED {
        let resource = FluentResource::try_new(locale.ftl.to_owned())
            .unwrap_or_else(|(_, errors)| panic!("{} does not parse: {errors:?}", locale.tag));
        let langid: LanguageIdentifier = locale
            .tag
            .parse()
            .unwrap_or_else(|error| panic!("{} is not a language tag: {error}", locale.tag));
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle
            .add_resource(resource)
            .unwrap_or_else(|errors| panic!("{} is unusable: {errors:?}", locale.tag));
    }
}

#[test]
fn catalogs_declare_exactly_the_english_identifiers() {
    let reference: BTreeSet<String> = identifiers(english().ftl).into_iter().collect();
    assert!(
        !reference.is_empty(),
        "the base catalog must declare messages"
    );
    for locale in SUPPORTED {
        let declared: BTreeSet<String> = identifiers(locale.ftl).into_iter().collect();
        let missing: Vec<_> = reference.difference(&declared).collect();
        let extra: Vec<_> = declared.difference(&reference).collect();
        assert!(missing.is_empty(), "{} is missing {missing:?}", locale.tag);
        assert!(
            extra.is_empty(),
            "{} declares {extra:?}, which no other catalog has",
            locale.tag
        );
    }
}

#[test]
fn no_catalog_declares_an_identifier_twice() {
    for locale in SUPPORTED {
        let declared = identifiers(locale.ftl);
        let unique: BTreeSet<&String> = declared.iter().collect();
        assert_eq!(
            declared.len(),
            unique.len(),
            "{} declares a duplicate message; Fluent would silently keep only one",
            locale.tag
        );
    }
}

/// The catalogs and `ids::ALL` are two independent lists; this is what keeps them equal.
#[test]
fn the_catalogs_and_the_identifier_list_agree() {
    let declared: BTreeSet<String> = identifiers(english().ftl).into_iter().collect();
    let used: BTreeSet<String> = ids::ALL.iter().map(|id| (*id).to_owned()).collect();
    assert_eq!(declared, used);
    assert_eq!(ids::ALL.len(), used.len(), "ids::ALL repeats an identifier");
}

#[test]
fn every_used_message_resolves_in_every_catalog_without_falling_back() {
    for locale in SUPPORTED {
        let bundle = bundle_for(locale);
        for id in ids::ALL {
            assert!(bundle.has_message(id), "{} has no {id}", locale.tag);
        }
    }
}

#[test]
fn arguments_are_substituted_and_no_isolation_marks_leak() {
    for locale in SUPPORTED {
        let translator = Translator::new(locale);
        let mut args = FluentArgs::new();
        args.set("label", "RU");
        let badge = translator.text_with(ids::TOOLTIP_BADGE, &args);
        assert!(
            badge.contains("RU"),
            "{} dropped the badge argument: {badge:?}",
            locale.tag
        );

        let mut args = FluentArgs::new();
        args.set("names", "sound, badge");
        let warning = translator.text_with(ids::TOOLTIP_WARNING, &args);
        assert!(
            warning.contains("sound, badge"),
            "{} dropped the warning argument: {warning:?}",
            locale.tag
        );

        // U+2068 FIRST STRONG ISOLATE / U+2069 POP DIRECTIONAL ISOLATE: invisible, but
        // they count against the 126-UTF-16-unit tray tooltip limit (ADR-0021).
        for text in [&badge, &warning] {
            assert!(
                !text.contains('\u{2068}') && !text.contains('\u{2069}'),
                "{} emitted directional isolation marks: {text:?}",
                locale.tag
            );
        }
    }
}

#[test]
fn no_message_is_left_untranslated_or_empty() {
    for locale in SUPPORTED {
        let translator = Translator::new(locale);
        for id in ids::ALL {
            let text = translator.text(id);
            assert!(!text.trim().is_empty(), "{} has an empty {id}", locale.tag);
            assert_ne!(&text, id, "{} did not resolve {id}", locale.tag);
        }
    }
}

#[test]
fn locale_registry_is_well_formed() {
    let mut tags = BTreeSet::new();
    let mut names = BTreeSet::new();
    for locale in SUPPORTED {
        let langid: LanguageIdentifier = locale.tag.parse().expect("tag parses");
        assert_eq!(
            langid.to_string(),
            locale.tag,
            "{} is not in canonical form, so it can never equal a sanitized config value",
            locale.tag
        );
        assert!(tags.insert(locale.tag), "duplicate tag {}", locale.tag);
        assert!(!locale.native_name.trim().is_empty());
        assert!(
            names.insert(locale.native_name),
            "duplicate native name {}",
            locale.native_name
        );
    }
    assert!(tags.contains(FALLBACK_TAG), "the fallback must be shipped");
}

#[test]
fn an_explicit_language_wins_and_an_unshipped_one_falls_back_to_english() {
    for (configured, expected) in [
        ("ru", "ru"),
        ("en", "en"),
        ("zh-Hans", "zh-Hans"),
        // Region and script variants of a shipped language resolve to it.
        ("ru-RU", "ru"),
        ("fr-CA", "fr"),
        ("zh-Hans-CN", "zh-Hans"),
        // Valid tags we ship no catalog for.
        ("ja", "en"),
        ("ar-EG", "en"),
    ] {
        assert_eq!(
            resolve(configured, &["de-DE".to_owned()]).tag,
            expected,
            "configured {configured:?} must ignore the system list"
        );
    }
}

#[test]
fn auto_follows_the_system_preference_order() {
    let auto = switcher_core::config::UI_LANGUAGE_AUTO;
    let list = |tags: &[&str]| -> Vec<String> { tags.iter().map(|t| (*t).to_owned()).collect() };

    assert_eq!(resolve(auto, &list(&["de-DE"])).tag, "de");
    assert_eq!(resolve(auto, &list(&["fr-CA", "de-DE"])).tag, "fr");
    // The first entry we ship wins, not the first entry overall.
    assert_eq!(resolve(auto, &list(&["ja-JP", "es-MX"])).tag, "es");
    assert_eq!(resolve(auto, &list(&["ja-JP"])).tag, "en");
    assert_eq!(resolve(auto, &[]).tag, "en");
    // A malformed entry from the OS must be skipped, not abort the search.
    assert_eq!(resolve(auto, &list(&["", "ru-RU"])).tag, "ru");
    // Only Simplified Chinese ships, so any Chinese preference lands on it (see match_tag).
    for tag in ["zh-Hans-CN", "zh-CN", "zh", "zh-TW"] {
        assert_eq!(resolve(auto, &list(&[tag])).tag, "zh-Hans", "system {tag}");
    }
}

/// A translator built for a catalog that is missing a message must borrow it from English
/// rather than show a blank menu row.
#[test]
fn a_missing_message_comes_from_english() {
    static PARTIAL: Locale = Locale {
        tag: "de",
        native_name: "Deutsch (Test)",
        ftl: "menu-quit = Beenden\n",
    };
    let translator = Translator::new(&PARTIAL);
    assert_eq!(translator.text(ids::MENU_QUIT), "Beenden");
    assert_eq!(
        translator.text(ids::MENU_SOUND),
        Translator::new(english()).text(ids::MENU_SOUND)
    );
}

#[test]
fn an_unknown_identifier_shows_itself_instead_of_nothing() {
    let translator = Translator::new(english());
    assert_eq!(translator.text("no-such-message"), "no-such-message");
}

/// Every tag in `SUPPORTED` must survive the core's sanitizer unchanged, or the tray
/// would write a language the config then rewrites into something else.
#[test]
fn shipped_tags_round_trip_through_the_config() {
    for locale in SUPPORTED {
        let toml = format!("ui_language = {:?}\n", locale.tag);
        let (config, warnings) = switcher_core::config::Config::from_toml_str(&toml).unwrap();
        assert_eq!(config.ui_language, locale.tag);
        assert!(warnings.is_empty(), "{} warned: {warnings:?}", locale.tag);
    }
}
