use crate::session::output::AnalysisResult;

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
