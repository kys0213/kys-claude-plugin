use super::models::QueueType;

// ─── Work ID 생성 헬퍼 ───

/// work_id 형식: "{type}:{repo_name}:{number}"
pub fn make_work_id(queue_type: QueueType, repo_name: &str, number: i64) -> String {
    format!("{}:{repo_name}:{number}", queue_type.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::labels;

    #[test]
    fn make_work_id_format() {
        assert_eq!(
            make_work_id(QueueType::Issue, "org/repo", 42),
            "issue:org/repo:42"
        );
        assert_eq!(
            make_work_id(QueueType::Pr, "org/repo", 15),
            "pr:org/repo:15"
        );
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
