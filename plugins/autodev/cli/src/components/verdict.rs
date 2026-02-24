use crate::infrastructure::claude::output::AnalysisResult;

/// wontfix verdict 댓글 포맷
pub fn format_wontfix_comment(a: &AnalysisResult) -> String {
    let reason = a
        .reason
        .as_deref()
        .unwrap_or("No additional details provided.");
    format!(
        "<!-- autodev:wontfix -->\n\
         ## Autodev Analysis\n\n\
         **Verdict**: Won't fix\n\n\
         **Summary**: {}\n\n\
         **Reason**: {reason}",
        a.summary
    )
}

/// needs_clarification verdict 댓글 포맷
pub fn format_clarification_comment(a: &AnalysisResult) -> String {
    let mut comment = format!(
        "<!-- autodev:waiting -->\n\
         ## Autodev Analysis\n\n\
         **Summary**: {}\n\n\
         This issue needs clarification before implementation can proceed.\n\n",
        a.summary
    );

    if !a.questions.is_empty() {
        comment.push_str("**Questions**:\n");
        for (i, q) in a.questions.iter().enumerate() {
            comment.push_str(&format!("{}. {q}\n", i + 1));
        }
    }

    comment
}

/// v2: 분석 리포트를 이슈 코멘트로 포맷 (HITL 게이트용)
///
/// `<!-- autodev:analysis -->` 마커를 포함하여 `scan_approved()`에서 추출 가능하게 한다.
pub fn format_analysis_comment(a: &AnalysisResult) -> String {
    format!(
        "<!-- autodev:analysis -->\n\
         ## Autodev Analysis Report\n\n\
         **Verdict**: {} (confidence: {:.0}%)\n\n\
         {}\n\n\
         ---\n\
         > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
         > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요.",
        a.verdict,
        a.confidence * 100.0,
        a.report
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude::output::{AnalysisResult, Verdict};

    #[test]
    fn format_analysis_comment_contains_marker_and_fields() {
        let a = AnalysisResult {
            verdict: Verdict::Implement,
            confidence: 0.85,
            summary: "Clear issue".to_string(),
            questions: vec![],
            reason: None,
            report: "Affected files: src/main.rs\n\nDirection: refactor".to_string(),
        };
        let comment = format_analysis_comment(&a);
        assert!(comment.contains("<!-- autodev:analysis -->"));
        assert!(comment.contains("implement"));
        assert!(comment.contains("85%"));
        assert!(comment.contains("src/main.rs"));
        assert!(comment.contains("autodev:approved-analysis"));
    }

    #[test]
    fn format_wontfix_comment_contains_reason() {
        let a = AnalysisResult {
            verdict: Verdict::Wontfix,
            confidence: 0.9,
            summary: "Duplicate".to_string(),
            questions: vec![],
            reason: Some("Already fixed in #10".to_string()),
            report: "".to_string(),
        };
        let comment = format_wontfix_comment(&a);
        assert!(comment.contains("autodev:wontfix"));
        assert!(comment.contains("Already fixed in #10"));
    }

    #[test]
    fn format_clarification_comment_lists_questions() {
        let a = AnalysisResult {
            verdict: Verdict::NeedsClarification,
            confidence: 0.4,
            summary: "Unclear scope".to_string(),
            questions: vec!["Which API?".to_string(), "Target version?".to_string()],
            reason: None,
            report: "".to_string(),
        };
        let comment = format_clarification_comment(&a);
        assert!(comment.contains("autodev:waiting"));
        assert!(comment.contains("1. Which API?"));
        assert!(comment.contains("2. Target version?"));
    }
}
