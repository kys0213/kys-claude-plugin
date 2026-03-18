use serde::Serialize;

/// Kanban board rendering interface (OCP: new renderers can be added without modifying existing code).
pub trait BoardRenderer: Send + Sync {
    fn render(&self, state: &BoardState) -> String;
}

/// Aggregate state of all kanban boards across repos.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BoardState {
    pub repos: Vec<RepoBoardState>,
    /// Cross-repo HITL pending summary.
    pub hitl_summary: HitlSummary,
    /// Claw daemon status.
    pub claw_status: ClawStatus,
}

/// Cross-repo HITL summary for board header.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HitlSummary {
    pub pending: i64,
    pub total: i64,
}

/// Claw daemon status indicator.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ClawStatus {
    pub running: bool,
    /// Last claw decision timestamp (ISO 8601), if available.
    pub last_decision_at: Option<String>,
    /// Daemon tick interval in seconds (for "next tick" estimation).
    pub tick_interval_secs: Option<u64>,
}

/// Per-repo board state with specs and queue columns.
#[derive(Debug, Clone, Serialize)]
pub struct RepoBoardState {
    pub repo_name: String,
    pub specs: Vec<SpecBoardEntry>,
    pub columns: Vec<BoardColumn>,
    /// Issues not linked to any spec.
    pub orphan_issues: Vec<BoardItem>,
}

/// Summary of a spec for board display.
#[derive(Debug, Clone, Serialize)]
pub struct SpecBoardEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub progress: String, // e.g. "3/5"
    pub hitl_count: u32,
    /// Acceptance criteria (from spec, if defined).
    pub acceptance_criteria: Option<String>,
}

/// A named column in the kanban board.
#[derive(Debug, Clone, Serialize)]
pub struct BoardColumn {
    pub name: String, // "Pending", "Ready", "Running", "Done", "Skipped"
    pub items: Vec<BoardItem>,
}

/// A single item within a board column.
#[derive(Debug, Clone, Serialize)]
pub struct BoardItem {
    pub work_id: String,
    pub title: String,
    pub queue_type: String, // "issue" or "pr"
}
