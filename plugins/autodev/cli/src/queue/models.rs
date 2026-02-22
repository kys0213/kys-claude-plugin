// ─── Input models (INSERT) ───

pub struct NewConsumerLog {
    pub repo_id: String,
    pub queue_type: String,
    pub queue_item_id: String,
    pub worker_id: String,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
}

// ─── Query result models (projections) ───

pub struct EnabledRepo {
    pub id: String,
    pub url: String,
    pub name: String,
}

pub struct RepoInfo {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

pub struct RepoStatusRow {
    pub name: String,
    pub enabled: bool,
}

pub struct LogEntry {
    pub started_at: String,
    pub queue_type: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}
