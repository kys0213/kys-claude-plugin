use serde::Serialize;

/// Kanban board rendering interface (OCP: new renderers can be added without modifying existing code).
pub trait BoardRenderer: Send + Sync {
    fn render(&self, state: &BoardState) -> String;
}

/// Aggregate state of all kanban boards across repos.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BoardState {
    pub repos: Vec<RepoBoardState>,
}

/// Per-repo board state with specs and queue columns.
#[derive(Debug, Clone, Serialize)]
pub struct RepoBoardState {
    pub repo_name: String,
    pub specs: Vec<SpecBoardEntry>,
    pub columns: Vec<BoardColumn>,
}

/// Summary of a spec for board display.
#[derive(Debug, Clone, Serialize)]
pub struct SpecBoardEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub progress: String, // e.g. "3/5"
    pub hitl_count: u32,
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
