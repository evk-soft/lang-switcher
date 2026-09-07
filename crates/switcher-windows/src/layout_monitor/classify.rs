//! Conditional fallback policy (ADR-0007). Native failures are explicit facts.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    Ok,
    AccessDenied,
    OtherError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageOutcome {
    NoPackage,
    Packaged,
    ProbeFailed,
}

#[derive(Debug)]
pub struct ForegroundFacts {
    pub tid: u32,
    pub is_own_process: bool,
    pub class_name: String,
    pub open_process: OpenOutcome,
    pub package: PackageOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollDecision {
    Keep,
    Arm(&'static str),
    Disarm,
}

pub fn decide(facts: &ForegroundFacts) -> PollDecision {
    if facts.tid == 0 || facts.is_own_process {
        return PollDecision::Keep;
    }
    // Empirical class names remain a conservative supplement to the process probes.
    if matches!(
        facts.class_name.as_str(),
        "ConsoleWindowClass"
            | "PseudoConsoleWindow"
            | "CASCADIA_HOSTING_WINDOW_CLASS"
            | "ApplicationFrameWindow"
            | "Windows.UI.Core.CoreWindow"
    ) {
        return PollDecision::Arm("blind_window_class");
    }
    match (facts.open_process, facts.package) {
        (OpenOutcome::AccessDenied, _) => PollDecision::Arm("opaque_process"),
        (OpenOutcome::OtherError, _) | (_, PackageOutcome::ProbeFailed) => {
            PollDecision::Arm("probe_failed")
        }
        (OpenOutcome::Ok, PackageOutcome::Packaged) => PollDecision::Arm("packaged_app"),
        (OpenOutcome::Ok, PackageOutcome::NoPackage) => PollDecision::Disarm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_is_armed_only_for_blind_or_unreadable_foregrounds() {
        let cases = [
            (
                0,
                false,
                "",
                OpenOutcome::OtherError,
                PackageOutcome::ProbeFailed,
                PollDecision::Keep,
            ),
            (
                1,
                true,
                "ConsoleWindowClass",
                OpenOutcome::AccessDenied,
                PackageOutcome::ProbeFailed,
                PollDecision::Keep,
            ),
            (
                1,
                false,
                "ConsoleWindowClass",
                OpenOutcome::Ok,
                PackageOutcome::NoPackage,
                PollDecision::Arm("blind_window_class"),
            ),
            (
                1,
                false,
                "OrdinaryWindow",
                OpenOutcome::AccessDenied,
                PackageOutcome::ProbeFailed,
                PollDecision::Arm("opaque_process"),
            ),
            (
                1,
                false,
                "OrdinaryWindow",
                OpenOutcome::Ok,
                PackageOutcome::NoPackage,
                PollDecision::Disarm,
            ),
            (
                1,
                false,
                "OrdinaryWindow",
                OpenOutcome::Ok,
                PackageOutcome::Packaged,
                PollDecision::Arm("packaged_app"),
            ),
            (
                1,
                false,
                "OrdinaryWindow",
                OpenOutcome::Ok,
                PackageOutcome::ProbeFailed,
                PollDecision::Arm("probe_failed"),
            ),
            (
                1,
                false,
                "OrdinaryWindow",
                OpenOutcome::OtherError,
                PackageOutcome::NoPackage,
                PollDecision::Arm("probe_failed"),
            ),
        ];
        for (tid, own, class, open_process, package, expected) in cases {
            let facts = ForegroundFacts {
                tid,
                is_own_process: own,
                class_name: class.into(),
                open_process,
                package,
            };
            assert_eq!(decide(&facts), expected, "{facts:?}");
        }
    }
}
