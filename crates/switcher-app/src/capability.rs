//! Capability presentation belongs to the shell, not the core.

use std::collections::BTreeMap;
use switcher_platform::events::{Capability, CapabilityReport, CapabilityState};

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

pub fn compose_tooltip(label: &str, map: &CapabilityMap) -> String {
    let mut names = Vec::new();
    if map.degraded().any(|r| r.capability.is_layout_source()) {
        names.push("раскладка");
    }
    for (cap, name) in [
        (Capability::Pointer, "курсор"),
        (Capability::Caret, "каретка"),
        (Capability::Overlay, "бейдж"),
        (Capability::Sound, "звук"),
        (Capability::Autostart, "автозапуск"),
    ] {
        if map.state(cap) != CapabilityState::Ok {
            names.push(name);
        }
    }
    let text = if names.is_empty() {
        format!("lang-switcher · {label}")
    } else {
        format!("lang-switcher · {label}\n⚠ {}", names.join(", "))
    };
    truncate_utf16(&text, 126)
}

pub fn compose_status(map: &CapabilityMap) -> Vec<String> {
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
        vec!["Всё работает".into()]
    } else {
        rows
    }
}

fn truncate_utf16(text: &str, limit: usize) -> String {
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

    #[test]
    fn tooltip_aggregates_layout_and_limits_utf16_without_splitting_characters() {
        let mut map = CapabilityMap::default();
        for cap in [
            Capability::LayoutShellHook,
            Capability::LayoutForegroundHook,
            Capability::LayoutTsf,
        ] {
            map.apply(report(cap));
        }
        let tip = compose_tooltip("RU", &map);
        assert_eq!(tip.matches("раскладка").count(), 1);
        assert!(
            compose_tooltip(&"🦀".repeat(100), &map)
                .encode_utf16()
                .count()
                <= 126
        );
        let status = compose_status(&map);
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
}
