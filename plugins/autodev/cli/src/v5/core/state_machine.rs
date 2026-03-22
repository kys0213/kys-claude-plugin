use super::phase::V5QueuePhase;

/// v5 상태 전이 규칙.
///
/// Conveyor belt 원칙: 아이템은 한 방향으로만 흐르고, 되돌아가지 않는다.
/// 전이 규칙:
///   Pending → Ready (자동)
///   Ready → Running (concurrency 제한)
///   Running → Completed (handler 성공)
///   Running → Skipped (preflight 실패)
///   Completed → Done (evaluate + on_done 성공)
///   Completed → Hitl (evaluate 판정)
///   Completed → Failed (on_done 실패)
///   Hitl → Done (사람 승인 + on_done 성공)
///   Hitl → Skipped (사람 스킵)
///   Hitl → Failed (on_done 실패)
///   Failed → Done (retry-script 성공)
///   Failed → Skipped (사람 스킵)
///
/// escalation에 의한 retry는 새 아이템 생성이므로 전이가 아님.
pub fn is_valid_transition(from: V5QueuePhase, to: V5QueuePhase) -> bool {
    use V5QueuePhase::*;
    matches!(
        (from, to),
        (Pending, Ready)
            | (Ready, Running)
            | (Running, Completed)
            | (Running, Skipped)
            | (Completed, Done)
            | (Completed, Hitl)
            | (Completed, Failed)
            | (Hitl, Done)
            | (Hitl, Skipped)
            | (Hitl, Failed)
            | (Failed, Done)
            | (Failed, Skipped)
    )
}

/// 상태 전이를 시도하고 유효하지 않으면 에러를 반환한다.
pub fn transit(from: V5QueuePhase, to: V5QueuePhase) -> Result<(), TransitionError> {
    if from == to {
        return Err(TransitionError::SamePhase(from));
    }
    if is_valid_transition(from, to) {
        Ok(())
    } else {
        Err(TransitionError::Invalid { from, to })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    SamePhase(V5QueuePhase),
    Invalid {
        from: V5QueuePhase,
        to: V5QueuePhase,
    },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::SamePhase(p) => write!(f, "cannot transit to same phase: {p}"),
            TransitionError::Invalid { from, to } => {
                write!(f, "invalid transition: {from} → {to}")
            }
        }
    }
}

impl std::error::Error for TransitionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use V5QueuePhase::*;

    #[test]
    fn happy_path_pending_to_done() {
        assert!(transit(Pending, Ready).is_ok());
        assert!(transit(Ready, Running).is_ok());
        assert!(transit(Running, Completed).is_ok());
        assert!(transit(Completed, Done).is_ok());
    }

    #[test]
    fn completed_branches() {
        assert!(transit(Completed, Done).is_ok());
        assert!(transit(Completed, Hitl).is_ok());
        assert!(transit(Completed, Failed).is_ok());
    }

    #[test]
    fn hitl_exits() {
        assert!(transit(Hitl, Done).is_ok());
        assert!(transit(Hitl, Skipped).is_ok());
        assert!(transit(Hitl, Failed).is_ok());
    }

    #[test]
    fn failed_exits() {
        assert!(transit(Failed, Done).is_ok());
        assert!(transit(Failed, Skipped).is_ok());
    }

    #[test]
    fn running_skip() {
        assert!(transit(Running, Skipped).is_ok());
    }

    // --- 불허 케이스 ---

    #[test]
    fn backward_transitions_rejected() {
        assert!(transit(Ready, Pending).is_err());
        assert!(transit(Running, Ready).is_err());
        assert!(transit(Completed, Running).is_err());
        assert!(transit(Done, Pending).is_err());
        assert!(transit(Done, Running).is_err());
    }

    #[test]
    fn terminal_cannot_transition() {
        // Done은 터미널 상태
        assert!(transit(Done, Pending).is_err());
        assert!(transit(Done, Ready).is_err());
        assert!(transit(Done, Running).is_err());
        assert!(transit(Done, Completed).is_err());
        assert!(transit(Done, Hitl).is_err());
        assert!(transit(Done, Failed).is_err());
        assert!(transit(Done, Skipped).is_err());

        // Skipped도 터미널 상태
        assert!(transit(Skipped, Pending).is_err());
        assert!(transit(Skipped, Ready).is_err());
        assert!(transit(Skipped, Running).is_err());
    }

    #[test]
    fn same_phase_rejected() {
        let phases = [
            Pending, Ready, Running, Completed, Done, Hitl, Failed, Skipped,
        ];
        for phase in phases {
            let err = transit(phase, phase).unwrap_err();
            assert_eq!(err, TransitionError::SamePhase(phase));
        }
    }

    #[test]
    fn skip_transitions() {
        // Pending → Skipped: 불허 (Running 전에는 skip 불가)
        assert!(transit(Pending, Skipped).is_err());
        // Ready → Skipped: 불허
        assert!(transit(Ready, Skipped).is_err());
        // Running → Skipped: 허용 (preflight 실패)
        assert!(transit(Running, Skipped).is_ok());
    }

    #[test]
    fn pending_cannot_jump() {
        assert!(transit(Pending, Running).is_err());
        assert!(transit(Pending, Completed).is_err());
        assert!(transit(Pending, Done).is_err());
    }

    #[test]
    fn running_cannot_go_done_directly() {
        assert!(transit(Running, Done).is_err());
        assert!(transit(Running, Hitl).is_err());
        assert!(transit(Running, Failed).is_err());
    }

    #[test]
    fn exhaustive_transition_count() {
        // 전수 조사: 8x8 = 64 조합 중 유효 전이 수 확인
        let phases = [
            Pending, Ready, Running, Completed, Done, Hitl, Failed, Skipped,
        ];
        let valid_count = phases
            .iter()
            .flat_map(|&from| phases.iter().map(move |&to| (from, to)))
            .filter(|&(from, to)| is_valid_transition(from, to))
            .count();
        assert_eq!(valid_count, 12, "expected 12 valid transitions");
    }

    #[test]
    fn transition_error_display() {
        let err = TransitionError::Invalid {
            from: Done,
            to: Pending,
        };
        assert_eq!(err.to_string(), "invalid transition: done → pending");

        let err = TransitionError::SamePhase(Running);
        assert_eq!(err.to_string(), "cannot transit to same phase: running");
    }
}
