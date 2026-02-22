use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use super::Gh;

/// 테스트용 Gh 구현체 — 미리 설정된 응답을 반환
pub struct MockGh {
    /// key: "{repo_name}/{path}" → value: field value
    fields: Mutex<HashMap<String, String>>,
    /// key: "{repo_name}/{endpoint}" → value: raw JSON bytes
    paginate_responses: Mutex<HashMap<String, Vec<u8>>>,
    /// 게시된 댓글 기록: (repo_name, number, body)
    pub posted_comments: Mutex<Vec<(String, i64, String)>>,
    /// 제거된 라벨 기록: (repo_name, number, label)
    pub removed_labels: Mutex<Vec<(String, i64, String)>>,
    /// 추가된 라벨 기록: (repo_name, number, label)
    pub added_labels: Mutex<Vec<(String, i64, String)>>,
}

impl Default for MockGh {
    fn default() -> Self {
        Self {
            fields: Mutex::new(HashMap::new()),
            paginate_responses: Mutex::new(HashMap::new()),
            posted_comments: Mutex::new(Vec::new()),
            removed_labels: Mutex::new(Vec::new()),
            added_labels: Mutex::new(Vec::new()),
        }
    }
}

impl MockGh {
    pub fn new() -> Self {
        Self::default()
    }

    /// api_get_field 응답 설정
    pub fn set_field(&self, repo_name: &str, path: &str, jq: &str, value: &str) {
        let key = format!("{repo_name}/{path}:{jq}");
        self.fields.lock().unwrap().insert(key, value.to_string());
    }

    /// api_paginate 응답 설정
    pub fn set_paginate(&self, repo_name: &str, endpoint: &str, json_bytes: Vec<u8>) {
        let key = format!("{repo_name}/{endpoint}");
        self.paginate_responses
            .lock()
            .unwrap()
            .insert(key, json_bytes);
    }
}

#[async_trait]
impl Gh for MockGh {
    async fn api_get_field(
        &self,
        repo_name: &str,
        path: &str,
        jq: &str,
        _host: Option<&str>,
    ) -> Option<String> {
        let key = format!("{repo_name}/{path}:{jq}");
        self.fields.lock().unwrap().get(&key).cloned()
    }

    async fn api_paginate(
        &self,
        repo_name: &str,
        endpoint: &str,
        _params: &[(&str, &str)],
        _host: Option<&str>,
    ) -> Result<Vec<u8>> {
        let key = format!("{repo_name}/{endpoint}");
        self.paginate_responses
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no mock response for {key}"))
    }

    async fn issue_comment(
        &self,
        repo_name: &str,
        number: i64,
        body: &str,
        _host: Option<&str>,
    ) -> bool {
        self.posted_comments.lock().unwrap().push((
            repo_name.to_string(),
            number,
            body.to_string(),
        ));
        true
    }

    async fn label_remove(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        _host: Option<&str>,
    ) -> bool {
        self.removed_labels.lock().unwrap().push((
            repo_name.to_string(),
            number,
            label.to_string(),
        ));
        true
    }

    async fn label_add(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        _host: Option<&str>,
    ) -> bool {
        self.added_labels.lock().unwrap().push((
            repo_name.to_string(),
            number,
            label.to_string(),
        ));
        true
    }
}
