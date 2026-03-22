use std::fmt;

use serde::{Deserialize, Serialize};

/// v5 Queue item phase lifecycle (8 phases).
///
/// v4의 5개(Pending/Ready/Running/Done/Skipped)에서
/// Completed, HITL, Failed 3개가 추가되었다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum V5QueuePhase {
    /// DataSource.collect()로 감지되어 큐에 등록됨
    Pending,
    /// 실행 준비 완료 (자동 전이)
    Ready,
    /// Worktree 생성 + handler 실행 중
    Running,
    /// 모든 handler 성공, evaluate 대기
    Completed,
    /// evaluate 판정 완료 + on_done script 성공
    Done,
    /// evaluate가 사람 판단 필요로 분류
    Hitl,
    /// on_done script 실패, 인프라 에러 등
    Failed,
    /// escalation skip 또는 preflight 실패
    Skipped,
}

impl V5QueuePhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            V5QueuePhase::Pending => "pending",
            V5QueuePhase::Ready => "ready",
            V5QueuePhase::Running => "running",
            V5QueuePhase::Completed => "completed",
            V5QueuePhase::Done => "done",
            V5QueuePhase::Hitl => "hitl",
            V5QueuePhase::Failed => "failed",
            V5QueuePhase::Skipped => "skipped",
        }
    }

    /// 터미널 상태인지 (Done, Skipped)
    pub fn is_terminal(&self) -> bool {
        matches!(self, V5QueuePhase::Done | V5QueuePhase::Skipped)
    }

    /// 사람 개입이 필요한 상태인지
    pub fn needs_human(&self) -> bool {
        matches!(self, V5QueuePhase::Hitl | V5QueuePhase::Failed)
    }
}

impl std::str::FromStr for V5QueuePhase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(V5QueuePhase::Pending),
            "ready" => Ok(V5QueuePhase::Ready),
            "running" => Ok(V5QueuePhase::Running),
            "completed" => Ok(V5QueuePhase::Completed),
            "done" => Ok(V5QueuePhase::Done),
            "hitl" => Ok(V5QueuePhase::Hitl),
            "failed" => Ok(V5QueuePhase::Failed),
            "skipped" => Ok(V5QueuePhase::Skipped),
            _ => Err(format!("invalid v5 queue phase: {s}")),
        }
    }
}

impl fmt::Display for V5QueuePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_phases_roundtrip() {
        let phases = [
            V5QueuePhase::Pending,
            V5QueuePhase::Ready,
            V5QueuePhase::Running,
            V5QueuePhase::Completed,
            V5QueuePhase::Done,
            V5QueuePhase::Hitl,
            V5QueuePhase::Failed,
            V5QueuePhase::Skipped,
        ];
        for phase in phases {
            let s = phase.to_string();
            let parsed: V5QueuePhase = s.parse().unwrap();
            assert_eq!(phase, parsed, "roundtrip failed for {phase}");
        }
    }

    #[test]
    fn as_str_matches_display() {
        let phases = [
            (V5QueuePhase::Pending, "pending"),
            (V5QueuePhase::Ready, "ready"),
            (V5QueuePhase::Running, "running"),
            (V5QueuePhase::Completed, "completed"),
            (V5QueuePhase::Done, "done"),
            (V5QueuePhase::Hitl, "hitl"),
            (V5QueuePhase::Failed, "failed"),
            (V5QueuePhase::Skipped, "skipped"),
        ];
        for (phase, expected) in phases {
            assert_eq!(phase.as_str(), expected);
            assert_eq!(phase.to_string(), expected);
        }
    }

    #[test]
    fn invalid_parse() {
        assert!("unknown".parse::<V5QueuePhase>().is_err());
        assert!("PENDING".parse::<V5QueuePhase>().is_err());
        assert!("".parse::<V5QueuePhase>().is_err());
    }

    #[test]
    fn terminal_phases() {
        assert!(V5QueuePhase::Done.is_terminal());
        assert!(V5QueuePhase::Skipped.is_terminal());
        assert!(!V5QueuePhase::Pending.is_terminal());
        assert!(!V5QueuePhase::Running.is_terminal());
        assert!(!V5QueuePhase::Completed.is_terminal());
        assert!(!V5QueuePhase::Hitl.is_terminal());
        assert!(!V5QueuePhase::Failed.is_terminal());
    }

    #[test]
    fn needs_human_phases() {
        assert!(V5QueuePhase::Hitl.needs_human());
        assert!(V5QueuePhase::Failed.needs_human());
        assert!(!V5QueuePhase::Done.needs_human());
        assert!(!V5QueuePhase::Running.needs_human());
    }

    #[test]
    fn serde_json_roundtrip() {
        let phase = V5QueuePhase::Completed;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, "\"completed\"");
        let parsed: V5QueuePhase = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, phase);
    }

    #[test]
    fn phase_count_is_eight() {
        let all = [
            V5QueuePhase::Pending,
            V5QueuePhase::Ready,
            V5QueuePhase::Running,
            V5QueuePhase::Completed,
            V5QueuePhase::Done,
            V5QueuePhase::Hitl,
            V5QueuePhase::Failed,
            V5QueuePhase::Skipped,
        ];
        assert_eq!(all.len(), 8);
    }
}
