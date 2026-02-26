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

/// EnabledRepo + per-repo config가 해석된 value object.
///
/// daemon tick마다 한번 생성하여 recovery/reconcile/knowledge에 전달한다.
/// gh_host 등 per-repo 설정을 내부에 보유하므로 호출측이 config를 반복 로드할 필요가 없다.
#[allow(dead_code)]
pub struct ResolvedRepo {
    pub id: String,
    pub url: String,
    pub name: String,
    pub gh_host: Option<String>,
}

impl ResolvedRepo {
    pub fn gh_host(&self) -> Option<&str> {
        self.gh_host.as_deref()
    }
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
