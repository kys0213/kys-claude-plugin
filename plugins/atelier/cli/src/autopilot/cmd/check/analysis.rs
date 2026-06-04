use anyhow::Result;

use super::state::LoopState;

/// Context passed to each analysis strategy.
pub struct AnalysisContext<'a> {
    pub loop_name: &'a str,
    pub changed_files: &'a [String],
    pub spec_files: &'a [String],
    pub code_files: &'a [String],
    pub state: &'a LoopState,
}

/// Result of a single analysis strategy.
pub struct AnalysisOutcome {
    /// If set, overrides the default exit code.
    pub exit_override: Option<i32>,
    /// Additional fields to merge into the output JSON.
    pub extra_fields: serde_json::Value,
}

/// A strategy that analyzes diff results and produces an outcome.
///
/// New analysis concerns (e.g. stagnation detection) implement this trait
/// and are injected via CheckService constructor — no existing code modified.
pub trait DiffAnalysis: Send + Sync {
    fn analyze(&self, ctx: &AnalysisContext) -> Result<AnalysisOutcome>;
}
