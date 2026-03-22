use super::models::QueueType;
use super::phase::TaskKind;

// ─── Work ID / Source ID 생성 헬퍼 ───

/// v5 source_id 형식: "github:{repo_name}#{number}"
///
/// 같은 외부 엔티티에서 파생된 모든 아이템을 연결하는 식별자.
pub fn make_source_id(repo_name: &str, number: i64) -> String {
    format!("github:{repo_name}#{number}")
}

/// v5 work_id 형식: "github:{repo_name}#{number}:{state}"
///
/// source_id에 state(task_kind)를 결합하여 개별 작업 단계를 식별한다.
pub fn make_work_id(
    queue_type: QueueType,
    repo_name: &str,
    number: i64,
    task_kind: TaskKind,
) -> String {
    let _ = queue_type; // v5에서는 source_id 기반이므로 queue_type 미사용, 호환성 유지
    format!(
        "{}:{}",
        make_source_id(repo_name, number),
        task_kind.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::labels;

    #[test]
    fn make_work_id_format() {
        assert_eq!(
            make_work_id(QueueType::Issue, "org/repo", 42, TaskKind::Analyze),
            "github:org/repo#42:analyze"
        );
        assert_eq!(
            make_work_id(QueueType::Pr, "org/repo", 15, TaskKind::Review),
            "github:org/repo#15:review"
        );
    }

    #[test]
    fn make_source_id_format() {
        assert_eq!(make_source_id("org/repo", 42), "github:org/repo#42");
    }

    #[test]
    fn label_constants() {
        assert_eq!(labels::WIP, "autodev:wip");
        assert_eq!(labels::DONE, "autodev:done");
        assert_eq!(labels::SKIP, "autodev:skip");

        // v2 라벨
        assert_eq!(labels::ANALYZED, "autodev:analyzed");
        assert_eq!(labels::APPROVED_ANALYSIS, "autodev:approved-analysis");
        assert_eq!(labels::IMPLEMENTING, "autodev:implementing");
    }

    #[test]
    fn iteration_label_format() {
        assert_eq!(labels::iteration_label(1), "autodev:iteration/1");
        assert_eq!(labels::iteration_label(2), "autodev:iteration/2");
        assert_eq!(labels::iteration_label(0), "autodev:iteration/0");
    }

    #[test]
    fn parse_iteration_from_labels() {
        assert_eq!(
            labels::parse_iteration(&["autodev:wip", "autodev:iteration/2"]),
            2
        );
        assert_eq!(labels::parse_iteration(&["autodev:wip"]), 0);
        assert_eq!(labels::parse_iteration(&[]), 0);
        assert_eq!(
            labels::parse_iteration(&["autodev:iteration/3", "autodev:iteration/1"]),
            3, // 첫 번째 매칭 반환
        );
    }
}
