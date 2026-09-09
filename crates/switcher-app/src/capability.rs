//! Capability presentation belongs to the shell, not the core.

use std::collections::BTreeMap;

use fluent_bundle::FluentArgs;
use switcher_platform::events::{Capability, CapabilityReport, CapabilityState};

use crate::i18n::{Translator, ids};

#[derive(Debug, Default)]
pub struct CapabilityMap {
    reports: BTreeMap<Capability, CapabilityReport>,
}

impl CapabilityMap {
    pub fn apply(&mut self, report: CapabilityReport) -> bool {
        if self.reports.get(&report.capability) == Some(&report) {
            return false;
        }
        self.reports.insert(report.capability, report);
        true
    }

    pub fn state(&self, capability: Capability) -> CapabilityState {
        self.reports
            .get(&capability)
            .map_or(CapabilityState::Ok, |r| r.state)
    }

    pub fn degraded(&self) -> impl Iterator<Item = &CapabilityReport> {
        self.reports
            .values()
            .filter(|r| r.state != CapabilityState::Ok)
    }
}

/// User-facing name of each capability, for the tooltip warning line. The three redundant
/// layout sources share one name because the user has one layout indicator, not three.
fn capability_message(capability: Capability) -> &'static str {
    match capability {
        Capability::LayoutShellHook | Capability::LayoutForegroundHook | Capability::LayoutTsf => {
            ids::CAPABILITY_LAYOUT
        }
        Capability::Pointer => ids::CAPABILITY_POINTER,
        Capability::Caret => ids::CAPABILITY_CARET,
        Capability::Overlay => ids::CAPABILITY_OVERLAY,
        Capability::Sound => ids::CAPABILITY_SOUND,
        Capability::Autostart => ids::CAPABILITY_AUTOSTART,
    }
}

pub fn compose_tooltip(label: &str, map: &CapabilityMap, tr: &Translator) -> String {
    let mut names = Vec::new();
    if map.degraded().any(|r| r.capability.is_layout_source()) {
        names.push(tr.text(ids::CAPABILITY_LAYOUT));
    }
    for cap in [
        Capability::Pointer,
        Capability::Caret,
        Capability::Overlay,
        Capability::Sound,
        Capability::Autostart,
    ] {
        if map.state(cap) != CapabilityState::Ok {
            names.push(tr.text(capability_message(cap)));
        }
    }
    let mut args = FluentArgs::new();
    args.set("label", label.to_owned());
    let text = tr.text_with(ids::TOOLTIP_BADGE, &args);
    let text = if names.is_empty() {
        text
    } else {
        let mut args = FluentArgs::new();
        args.set("names", names.join(", "));
        format!("{text}\n{}", tr.text_with(ids::TOOLTIP_WARNING, &args))
    };
    truncate_utf16(&text, 126)
}

/// The status submenu is a diagnostic view. Capability keys, adapter codes and details are
/// deliberately not translated (ADR-0021): they are what a bug report quotes and what the
/// log contains. Only the two rows that frame them are.
pub fn compose_status(map: &CapabilityMap, tr: &Translator) -> Vec<String> {
    let rows: Vec<_> = map
        .degraded()
        .map(|r| {
            format!(
                "{}: {} — {}",
                r.capability.key(),
                r.code,
                r.detail.replace(['\r', '\n', '\t'], " ")
            )
        })
        .collect();
    if rows.is_empty() {
        return vec![tr.text(ids::STATUS_ALL_OK)];
    }
    std::iter::once(tr.text(ids::STATUS_DIAGNOSTICS))
        .chain(rows)
        .collect()
}

pub(crate) fn truncate_utf16(text: &str, limit: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= limit
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(capability: Capability) -> CapabilityReport {
        CapabilityReport {
            capability,
            state: CapabilityState::Off,
            code: "test_failure",
            detail: "Unavailable".into(),
        }
    }

    #[test]
    fn default_map_has_no_failures_and_later_report_wins() {
        let mut map = CapabilityMap::default();
        assert_eq!(map.degraded().count(), 0);
        map.apply(report(Capability::Sound));
        assert_eq!(map.state(Capability::Sound), CapabilityState::Off);
        let mut recovered = report(Capability::Sound);
        recovered.state = CapabilityState::Ok;
        map.apply(recovered);
        assert_eq!(map.degraded().count(), 0);
    }

    fn russian() -> Translator {
        Translator::for_config("ru", &[])
    }

    #[test]
    fn tooltip_aggregates_layout_and_limits_utf16_without_splitting_characters() {
        let tr = russian();
        let mut map = CapabilityMap::default();
        for cap in [
            Capability::LayoutShellHook,
            Capability::LayoutForegroundHook,
            Capability::LayoutTsf,
        ] {
            map.apply(report(cap));
        }
        let tip = compose_tooltip("RU", &map, &tr);
        assert_eq!(tip.matches(&tr.text(ids::CAPABILITY_LAYOUT)).count(), 1);
        assert!(
            compose_tooltip(&"🦀".repeat(100), &map, &tr)
                .encode_utf16()
                .count()
                <= 126
        );
        let status = compose_status(&map, &tr);
        for cap in [
            Capability::LayoutShellHook,
            Capability::LayoutForegroundHook,
            Capability::LayoutTsf,
        ] {
            assert!(
                status
                    .iter()
                    .any(|s| s.contains(cap.key()) && s.contains("test_failure"))
            );
        }
    }

    /// The tooltip is the only place where the user is told, in their own language, that
    /// something is limited. The status rows below it stay machine-readable.
    #[test]
    fn tooltip_is_localized_while_diagnostic_rows_are_not() {
        let mut map = CapabilityMap::default();
        map.apply(report(Capability::Sound));

        let ru = compose_tooltip("EN", &map, &russian());
        let de = compose_tooltip("EN", &map, &Translator::for_config("de", &[]));
        assert!(ru.contains("звук"), "{ru:?}");
        assert!(de.contains("Ton"), "{de:?}");
        assert_ne!(ru, de);

        for tr in [russian(), Translator::for_config("de", &[])] {
            let rows = compose_status(&map, &tr);
            assert!(
                rows.iter().any(|row| row.contains("sound: test_failure")),
                "the diagnostic row must not be translated: {rows:?}"
            );
        }
    }

    #[test]
    fn a_healthy_map_reports_one_localized_row() {
        let map = CapabilityMap::default();
        assert_eq!(
            compose_status(&map, &russian()),
            vec!["Всё работает".to_owned()]
        );
        assert_eq!(
            compose_status(&map, &Translator::for_config("en", &[])),
            vec!["Everything works".to_owned()]
        );
    }

    /// Every capability has to name itself; a `_ =>` arm would have quietly labelled a new
    /// one with someone else's name.
    #[test]
    fn every_capability_has_a_distinct_user_facing_name() {
        let tr = Translator::for_config("en", &[]);
        for cap in Capability::ALL {
            let id = capability_message(cap);
            assert!(ids::ALL.contains(&id), "{} has no message", cap.key());
            assert_ne!(tr.text(id), id, "{} resolves to nothing", cap.key());
        }
        for cap in Capability::ALL.iter().filter(|c| c.is_layout_source()) {
            assert_eq!(capability_message(*cap), ids::CAPABILITY_LAYOUT);
        }
    }
}
