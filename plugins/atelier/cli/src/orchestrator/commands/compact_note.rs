//! `orchestrator compact-note` — the fixed compaction-preservation notice.
//! No stdin: this command's output never varies with input. The hooks
//! module calls it unconditionally from `session.compact` when
//! the compacting context is the main loop (`agentId` absent), and the
//! instruction text itself is what tells the summarizer whether an
//! orchestrator run was actually in progress.

/// The conditional instruction appended to compaction: preserve orchestrator
/// run state if one was in progress, otherwise ignore this notice.
pub fn instructions() -> String {
    "orchestrator 런이 진행 중이었다면, 이번 요약에 epic 브랜치 전체 이름, log_dir 경로, \
     등록된 task 들의 id 와 상태, 담당 agent 이름, 진행 중인 worktree 경로, 남은 작업, \
     정지 조건을 그대로 보존하세요. orchestrator 런이 진행 중이 아니었다면 이 지시는 \
     무시하세요."
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_are_nonempty_and_conditional() {
        let text = instructions();
        assert!(!text.trim().is_empty());
        assert!(text.contains("orchestrator 런이 진행 중이었다면"));
        assert!(text.contains("진행 중이 아니었다면"));
    }
}
