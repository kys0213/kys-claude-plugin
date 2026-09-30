//! Execution-tier ladder for model generation names. This ladder is mirrored
//! elsewhere in the repo, so a new model generation needs updating in more
//! than one place.

/// Declaration order doubles as the ladder order — `derive(Ord)` gives
/// `Haiku < Sonnet < Opus < Fable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Haiku,
    Sonnet,
    Opus,
    Fable,
}

impl Tier {
    /// Returns `None` when no known tier name appears — callers must treat
    /// that as "cannot judge", never as the lowest tier.
    pub fn detect(raw: &str) -> Option<Tier> {
        let lower = raw.to_lowercase();
        if lower.contains("fable") || lower.contains("mythos") {
            Some(Tier::Fable)
        } else if lower.contains("opus") {
            Some(Tier::Opus)
        } else if lower.contains("sonnet") {
            Some(Tier::Sonnet)
        } else if lower.contains("haiku") {
            Some(Tier::Haiku)
        } else {
            None
        }
    }

    pub fn alias(self) -> &'static str {
        match self {
            Tier::Haiku => "haiku",
            Tier::Sonnet => "sonnet",
            Tier::Opus => "opus",
            Tier::Fable => "fable",
        }
    }

    pub fn execution_cap(self) -> Tier {
        match self {
            Tier::Fable => Tier::Opus,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_order_matches_sheet() {
        assert!(Tier::Haiku < Tier::Sonnet);
        assert!(Tier::Sonnet < Tier::Opus);
        assert!(Tier::Opus < Tier::Fable);
    }

    #[test]
    fn detect_is_case_insensitive_substring() {
        assert_eq!(Tier::detect("claude-opus-5-5"), Some(Tier::Opus));
        assert_eq!(Tier::detect("claude-sonnet-5"), Some(Tier::Sonnet));
        assert_eq!(Tier::detect("claude-haiku-4-5-20251001"), Some(Tier::Haiku));
        assert_eq!(Tier::detect("claude-fable-5-1"), Some(Tier::Fable));
        assert_eq!(Tier::detect("MYTHOS"), Some(Tier::Fable));
        assert_eq!(Tier::detect("gpt-5"), None);
    }

    #[test]
    fn execution_cap_matches_sheet() {
        assert_eq!(Tier::Fable.execution_cap(), Tier::Opus);
        assert_eq!(Tier::Opus.execution_cap(), Tier::Opus);
        assert_eq!(Tier::Sonnet.execution_cap(), Tier::Sonnet);
        assert_eq!(Tier::Haiku.execution_cap(), Tier::Haiku);
    }
}
