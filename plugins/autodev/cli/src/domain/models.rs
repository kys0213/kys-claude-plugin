use super::labels;

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

#[derive(Clone)]
pub struct EnabledRepo {
    pub id: String,
    pub url: String,
    pub name: String,
}

// ─── Pre-fetched Value Objects ───

/// GitHub Issue value object (pre-fetched, state-aware).
///
/// GitHub issues API 응답을 파싱하여 typed 필드로 보유한다.
/// 라벨 기반 상태 판별 메서드를 제공하여 소비자가 문자열 비교를 직접 할 필요가 없다.
pub struct RepoIssue {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    pub author: String,
    pub labels: Vec<String>,
}

impl RepoIssue {
    /// GitHub issues API JSON으로부터 RepoIssue를 생성한다.
    /// `pull_request` 필드가 있으면 PR이므로 None을 반환한다.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        if v.get("pull_request").is_some() {
            return None;
        }
        Some(Self {
            number: v["number"].as_i64().filter(|n| *n > 0)?,
            title: v["title"].as_str().unwrap_or("").to_string(),
            body: v["body"].as_str().map(|s| s.to_string()),
            author: v["user"]["login"].as_str().unwrap_or("").to_string(),
            labels: v["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l == label)
    }

    pub fn is_wip(&self) -> bool {
        self.has_label(labels::WIP)
    }
    pub fn is_done(&self) -> bool {
        self.has_label(labels::DONE)
    }
    pub fn is_terminal(&self) -> bool {
        self.is_done() || self.has_label(labels::SKIP)
    }
    pub fn is_analyze(&self) -> bool {
        self.has_label(labels::ANALYZE)
    }
    pub fn is_analyzed(&self) -> bool {
        self.has_label(labels::ANALYZED)
    }
    pub fn is_approved(&self) -> bool {
        self.has_label(labels::APPROVED_ANALYSIS)
    }
    pub fn is_implementing(&self) -> bool {
        self.has_label(labels::IMPLEMENTING)
    }
}

/// GitHub PR value object (pre-fetched, state-aware).
///
/// GitHub pulls API 응답을 파싱하여 typed 필드로 보유한다.
/// head/base branch, source issue 번호 등 PR 고유 정보를 포함한다.
pub struct RepoPull {
    pub number: i64,
    pub title: String,
    pub body: Option<String>,
    #[allow(dead_code)]
    pub author: String,
    pub labels: Vec<String>,
    pub head_branch: String,
    pub base_branch: String,
}

impl RepoPull {
    /// GitHub pulls API JSON으로부터 RepoPull을 생성한다.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        Some(Self {
            number: v["number"].as_i64().filter(|n| *n > 0)?,
            title: v["title"].as_str().unwrap_or("").to_string(),
            body: v["body"].as_str().map(|s| s.to_string()),
            author: v["user"]["login"].as_str().unwrap_or("").to_string(),
            labels: v["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            head_branch: v["head"]["ref"].as_str().unwrap_or("").to_string(),
            base_branch: v["base"]["ref"].as_str().unwrap_or("main").to_string(),
        })
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l == label)
    }

    pub fn is_wip(&self) -> bool {
        self.has_label(labels::WIP)
    }
    pub fn is_done(&self) -> bool {
        self.has_label(labels::DONE)
    }
    pub fn is_terminal(&self) -> bool {
        self.is_done() || self.has_label(labels::SKIP)
    }

    /// PR body에서 `Closes #N`, `Fixes #N`, `Resolves #N` 패턴을 파싱하여
    /// source issue number를 추출한다.
    pub fn source_issue_number(&self) -> Option<i64> {
        let body = self.body.as_deref()?;
        let lower = body.to_lowercase();
        for prefix in &["closes #", "fixes #", "resolves #"] {
            if let Some(pos) = lower.find(prefix) {
                let start = pos + prefix.len();
                let num_str: String = lower[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(n) = num_str.parse::<i64>() {
                    if n > 0 {
                        return Some(n);
                    }
                }
            }
        }
        None
    }

    /// 라벨에서 리뷰 반복 횟수를 파싱한다.
    pub fn review_iteration(&self) -> u32 {
        let label_refs: Vec<&str> = self.labels.iter().map(|s| s.as_str()).collect();
        labels::parse_iteration(&label_refs)
    }
}

/// EnabledRepo + per-repo config + pre-fetched GitHub state.
///
/// daemon tick마다 한번 생성하여 recovery/reconcile/knowledge에 전달한다.
/// gh_host 등 per-repo 설정과 open issues/pulls를 내부에 보유하므로
/// 소비자가 config 로드나 API 호출을 반복할 필요가 없다.
pub struct ResolvedRepo {
    pub id: String,
    pub url: String,
    pub name: String,
    pub gh_host: Option<String>,
    pub issues: Vec<RepoIssue>,
    pub pulls: Vec<RepoPull>,
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
