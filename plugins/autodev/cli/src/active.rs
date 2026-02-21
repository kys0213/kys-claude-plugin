use std::collections::HashSet;

/// 인메모리 중복 방지 — 큐에 존재하는 항목을 추적
/// key: "{queue_type}:{repo_id}:{number}"
pub struct ActiveItems(HashSet<String>);

impl ActiveItems {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn contains(&self, queue_type: &str, repo_id: &str, number: i64) -> bool {
        self.0.contains(&Self::key(queue_type, repo_id, number))
    }

    pub fn insert(&mut self, queue_type: &str, repo_id: &str, number: i64) {
        self.0.insert(Self::key(queue_type, repo_id, number));
    }

    pub fn remove(&mut self, queue_type: &str, repo_id: &str, number: i64) {
        self.0.remove(&Self::key(queue_type, repo_id, number));
    }

    fn key(queue_type: &str, repo_id: &str, number: i64) -> String {
        format!("{queue_type}:{repo_id}:{number}")
    }
}
