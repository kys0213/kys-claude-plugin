#![allow(dead_code)]

use anyhow::Result;
use autopilot::github::{EventPayload, EventType, EventsResponse, GitHub, RepoEvent};

/// A mock GitHub client for testing watch functionality.
pub struct MockGitHub {
    events_response: Option<EventsResponse>,
    default_branch: String,
}

impl MockGitHub {
    pub fn new() -> Self {
        Self {
            events_response: None,
            default_branch: "main".to_string(),
        }
    }

    /// Set the events response. None simulates 304 Not Modified.
    pub fn with_events(mut self, events: Vec<RepoEvent>, etag: &str) -> Self {
        self.events_response = Some(EventsResponse {
            events,
            etag: etag.to_string(),
            poll_interval: 60,
        });
        self
    }

    /// Simulate 304 Not Modified (no new events).
    pub fn with_no_changes(self) -> Self {
        // events_response is already None
        self
    }

    pub fn with_default_branch(mut self, branch: &str) -> Self {
        self.default_branch = branch.to_string();
        self
    }
}

impl GitHub for MockGitHub {
    fn fetch_events(&self, _etag: Option<&str>) -> Result<Option<EventsResponse>> {
        match &self.events_response {
            None => Ok(None),
            Some(resp) => Ok(Some(EventsResponse {
                events: resp
                    .events
                    .iter()
                    .map(|e| RepoEvent {
                        id: e.id.clone(),
                        event_type: e.event_type.clone(),
                        payload: e.payload.clone(),
                    })
                    .collect(),
                etag: resp.etag.clone(),
                poll_interval: resp.poll_interval,
            })),
        }
    }

    fn default_branch(&self) -> Result<String> {
        Ok(self.default_branch.clone())
    }
}

// ── Test data builders ──

pub fn push_event(id: &str, branch: &str, before: &str, after: &str, size: u64) -> RepoEvent {
    RepoEvent {
        id: id.to_string(),
        event_type: EventType::Push,
        payload: EventPayload::Push {
            branch: branch.to_string(),
            before: before.to_string(),
            after: after.to_string(),
            size,
        },
    }
}

pub fn workflow_run_event(
    id: &str,
    run_id: u64,
    name: &str,
    branch: &str,
    conclusion: &str,
) -> RepoEvent {
    RepoEvent {
        id: id.to_string(),
        event_type: EventType::WorkflowRun,
        payload: EventPayload::WorkflowRun {
            run_id,
            name: name.to_string(),
            branch: branch.to_string(),
            conclusion: conclusion.to_string(),
        },
    }
}

pub fn issues_event(id: &str, action: &str, number: u64, title: &str) -> RepoEvent {
    RepoEvent {
        id: id.to_string(),
        event_type: EventType::Issues,
        payload: EventPayload::Issues {
            action: action.to_string(),
            number,
            title: title.to_string(),
        },
    }
}
