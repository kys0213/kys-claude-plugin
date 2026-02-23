use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use crate::knowledge::models::{RepetitionEntry, SessionEntry, ToolFrequencyEntry};

use super::SuggestWorkflow;

/// 테스트용 SuggestWorkflow 구현체 — 미리 설정된 응답을 반환
#[allow(dead_code)]
pub struct MockSuggestWorkflow {
    pub tool_frequency_responses: Mutex<Vec<Vec<ToolFrequencyEntry>>>,
    pub filtered_sessions_responses: Mutex<Vec<Vec<SessionEntry>>>,
    pub repetition_responses: Mutex<Vec<Vec<RepetitionEntry>>>,
    /// 호출 기록: (method, args)
    pub calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl Default for MockSuggestWorkflow {
    fn default() -> Self {
        Self {
            tool_frequency_responses: Mutex::new(Vec::new()),
            filtered_sessions_responses: Mutex::new(Vec::new()),
            repetition_responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[allow(dead_code)]
impl MockSuggestWorkflow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue_tool_frequency(&self, entries: Vec<ToolFrequencyEntry>) {
        self.tool_frequency_responses.lock().unwrap().push(entries);
    }

    pub fn enqueue_filtered_sessions(&self, entries: Vec<SessionEntry>) {
        self.filtered_sessions_responses
            .lock()
            .unwrap()
            .push(entries);
    }

    pub fn enqueue_repetition(&self, entries: Vec<RepetitionEntry>) {
        self.repetition_responses.lock().unwrap().push(entries);
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl SuggestWorkflow for MockSuggestWorkflow {
    async fn query_tool_frequency(
        &self,
        session_filter: Option<&str>,
    ) -> Result<Vec<ToolFrequencyEntry>> {
        self.calls.lock().unwrap().push((
            "query_tool_frequency".into(),
            vec![session_filter.unwrap_or("").to_string()],
        ));

        let mut responses = self.tool_frequency_responses.lock().unwrap();
        if responses.is_empty() {
            Ok(vec![])
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn query_filtered_sessions(
        &self,
        prompt_pattern: &str,
        since: Option<&str>,
        top: Option<u32>,
    ) -> Result<Vec<SessionEntry>> {
        self.calls.lock().unwrap().push((
            "query_filtered_sessions".into(),
            vec![
                prompt_pattern.to_string(),
                since.unwrap_or("").to_string(),
                top.map(|t| t.to_string()).unwrap_or_default(),
            ],
        ));

        let mut responses = self.filtered_sessions_responses.lock().unwrap();
        if responses.is_empty() {
            Ok(vec![])
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn query_repetition(&self, session_filter: Option<&str>) -> Result<Vec<RepetitionEntry>> {
        self.calls.lock().unwrap().push((
            "query_repetition".into(),
            vec![session_filter.unwrap_or("").to_string()],
        ));

        let mut responses = self.repetition_responses.lock().unwrap();
        if responses.is_empty() {
            Ok(vec![])
        } else {
            Ok(responses.remove(0))
        }
    }
}
