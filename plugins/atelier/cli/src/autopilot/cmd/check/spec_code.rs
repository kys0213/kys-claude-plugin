use anyhow::Result;

use super::analysis::{AnalysisContext, AnalysisOutcome, DiffAnalysis};

/// Classifies changed files into spec vs code categories.
///
/// This is the original check diff logic extracted as a strategy.
pub struct SpecCodeAnalysis;

impl DiffAnalysis for SpecCodeAnalysis {
    fn analyze(&self, ctx: &AnalysisContext) -> Result<AnalysisOutcome> {
        Ok(AnalysisOutcome {
            exit_override: None,
            extra_fields: serde_json::json!({
                "spec_files": ctx.spec_files,
                "code_files": ctx.code_files,
            }),
        })
    }
}
