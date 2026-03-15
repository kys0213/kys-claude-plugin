use std::fmt;

use serde::{Deserialize, Serialize};

/// 작업 종류를 나타내는 enum.
///
/// QueueItem이 어떤 Task로 처리되어야 하는지를 결정한다.
/// QueuePhase(Pending/Running)와 조합하여 기존 10개 string phase를 대체한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Analyze,
    Implement,
    Review,
    Improve,
    Extract,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Analyze => "analyze",
            TaskKind::Implement => "implement",
            TaskKind::Review => "review",
            TaskKind::Improve => "improve",
            TaskKind::Extract => "extract",
        }
    }
}

impl std::str::FromStr for TaskKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "analyze" => Ok(TaskKind::Analyze),
            "implement" => Ok(TaskKind::Implement),
            "review" => Ok(TaskKind::Review),
            "improve" => Ok(TaskKind::Improve),
            "extract" => Ok(TaskKind::Extract),
            _ => Err(format!("invalid task kind: {s}")),
        }
    }
}

impl fmt::Display for TaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_kind_display_roundtrip() {
        for kind in [
            TaskKind::Analyze,
            TaskKind::Implement,
            TaskKind::Review,
            TaskKind::Improve,
            TaskKind::Extract,
        ] {
            let s = kind.to_string();
            let parsed: TaskKind = s.parse().unwrap();
            assert_eq!(kind, parsed);
        }
    }

    #[test]
    fn task_kind_invalid_parse() {
        assert!("unknown".parse::<TaskKind>().is_err());
    }

    #[test]
    fn task_kind_as_str() {
        assert_eq!(TaskKind::Analyze.as_str(), "analyze");
        assert_eq!(TaskKind::Implement.as_str(), "implement");
        assert_eq!(TaskKind::Review.as_str(), "review");
        assert_eq!(TaskKind::Improve.as_str(), "improve");
        assert_eq!(TaskKind::Extract.as_str(), "extract");
    }
}
