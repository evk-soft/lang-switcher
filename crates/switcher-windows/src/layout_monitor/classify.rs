//! User-controlled fallback policy; window classes do not prove delivery (ADR-0019).

#[derive(Debug, Clone, Copy)]
pub struct ForegroundFacts {
    pub tid: u32,
    pub is_own_process: bool,
}

impl ForegroundFacts {
    pub fn is_foreign(self) -> bool {
        self.tid != 0 && !self.is_own_process
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollDecision {
    Arm(&'static str),
    Disarm,
}

pub fn decide(facts: &ForegroundFacts, enabled: bool, foreground_hook: bool) -> PollDecision {
    if !enabled {
        PollDecision::Disarm
    } else if !foreground_hook {
        // A heartbeat discovers the next foreign window when notifications are absent.
        // It still must not read an own/unknown window's HKL.
        PollDecision::Arm("foreground_hook_unavailable")
    } else if facts.is_foreign() {
        PollDecision::Arm("foreign_foreground")
    } else {
        PollDecision::Disarm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FOREIGN: ForegroundFacts = ForegroundFacts {
        tid: 42,
        is_own_process: false,
    };
    const OWN: ForegroundFacts = ForegroundFacts {
        tid: 42,
        is_own_process: true,
    };
    const UNKNOWN: ForegroundFacts = ForegroundFacts {
        tid: 0,
        is_own_process: false,
    };

    #[test]
    fn ordinary_foreground_keeps_a_fallback_when_language_events_are_missing() {
        assert!(matches!(decide(&FOREIGN, true, true), PollDecision::Arm(_)));
    }

    #[test]
    fn own_foreground_stops_an_existing_fallback() {
        assert_eq!(decide(&OWN, true, true), PollDecision::Disarm);
        assert_eq!(decide(&UNKNOWN, true, true), PollDecision::Disarm);
    }

    #[test]
    fn disabling_fallback_stops_it_even_when_all_notifications_are_unavailable() {
        for facts in [FOREIGN, OWN, UNKNOWN] {
            for hook in [true, false] {
                assert_eq!(decide(&facts, false, hook), PollDecision::Disarm);
            }
        }
    }

    #[test]
    fn missing_foreground_hook_keeps_discovery_alive_without_reading_own_layout() {
        for facts in [FOREIGN, OWN, UNKNOWN] {
            assert!(matches!(decide(&facts, true, false), PollDecision::Arm(_)));
        }
        assert!(FOREIGN.is_foreign());
        assert!(!OWN.is_foreign());
        assert!(!UNKNOWN.is_foreign());
    }
}
