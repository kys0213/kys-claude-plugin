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
    let mut comment = format!(
        "<!-- autodev:analysis -->\n\
         ## Autodev Analysis Report\n\n\
         **Verdict**: {} (confidence: {:.0}%)\n\n\
         {}",
        a.verdict,
        a.confidence * 100.0,
        a.report
    );

    if !a.related_issues.is_empty() {
        comment.push_str("\n\n### Related Issues\n\n| # | Relation | Confidence | Summary |\n|---|----------|------------|---------|");
        for ri in &a.related_issues {
            comment.push_str(&format!(
                "\n| #{} | {} | {:.0}% | {} |",
                ri.number,
                ri.relation,
                ri.confidence * 100.0,
                ri.summary
            ));
        }
    }

    comment.push_str(
        "\n\n---\n\
         > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
         > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요.",
    );

    comment
}

/// 파싱 실패 시 raw report를 분석 코멘트로 포맷
///
/// `format_analysis_comment`와 동일한 마커/헤더/푸터를 사용하여 일관성을 유지한다.
pub fn format_raw_analysis_comment(report: &str) -> String {
    format!(
        "<!-- autodev:analysis -->\n\
         ## Autodev Analysis Report\n\n\
         {report}\n\n\
         ---\n\
         > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
         > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude::output::{AnalysisResult, RelatedIssue, Relation, Verdict};

    #[test]
    fn format_analysis_comment_contains_marker_and_fields() {
        let a = AnalysisResult {
            verdict: Verdict::Implement,
            confidence: 0.85,
            summary: "Clear issue".to_string(),
            questions: vec![],
            reason: None,
            report: "Affected files: src/main.rs\n\nDirection: refactor".to_string(),
            related_issues: vec![],
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
            related_issues: vec![],
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
            related_issues: vec![],
        };
        let comment = format_clarification_comment(&a);
        assert!(comment.contains("autodev:waiting"));
        assert!(comment.contains("1. Which API?"));
        assert!(comment.contains("2. Target version?"));
    }

    #[test]
    fn format_analysis_comment_includes_related_issues() {
        let a = AnalysisResult {
            verdict: Verdict::Implement,
            confidence: 0.85,
            summary: "Clear issue".to_string(),
            questions: vec![],
            reason: None,
            report: "Fix the handler".to_string(),
            related_issues: vec![
                RelatedIssue {
                    number: 10,
                    relation: Relation::Related,
                    confidence: 0.7,
                    summary: "Similar auth issue".to_string(),
                },
                RelatedIssue {
                    number: 15,
                    relation: Relation::Duplicate,
                    confidence: 0.9,
                    summary: "Same bug reported".to_string(),
                },
            ],
        };
        let comment = format_analysis_comment(&a);
        assert!(comment.contains("### Related Issues"));
        assert!(comment.contains("#10"));
        assert!(comment.contains("related"));
        assert!(comment.contains("70%"));
        assert!(comment.contains("#15"));
        assert!(comment.contains("duplicate"));
        assert!(comment.contains("90%"));
    }

    #[test]
    fn format_analysis_comment_omits_related_when_empty() {
        let a = AnalysisResult {
            verdict: Verdict::Implement,
            confidence: 0.85,
            summary: "Clear issue".to_string(),
            questions: vec![],
            reason: None,
            report: "Fix it".to_string(),
            related_issues: vec![],
        };
        let comment = format_analysis_comment(&a);
        assert!(!comment.contains("Related Issues"));
    }
}
