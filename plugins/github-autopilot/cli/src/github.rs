use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;
use std::sync::Arc;

// ── Domain Types ──

/// Response from the GitHub Events API.
pub struct EventsResponse {
    /// Parsed repository events.
    pub events: Vec<RepoEvent>,
    /// ETag for conditional requests (pass to next fetch_events call).
    pub etag: String,
    /// Server-recommended poll interval in seconds (X-Poll-Interval header).
    pub poll_interval: u64,
}

/// A single repository event from the Events API.
pub struct RepoEvent {
    /// Unique event ID (monotonically increasing string).
    pub id: String,
    /// Event type classification.
    pub event_type: EventType,
    /// Parsed event payload.
    pub payload: EventPayload,
}

/// Known event types from the Events API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    Push,
    WorkflowRun,
    Issues,
    Other(String),
}

/// Parsed payload for each event type.
#[derive(Debug, Clone)]
pub enum EventPayload {
    Push {
        branch: String,
        before: String,
        after: String,
        size: u64,
    },
    WorkflowRun {
        run_id: u64,
        name: String,
        branch: String,
        conclusion: String,
    },
    Issues {
        action: String,
        number: u64,
        title: String,
    },
    Unknown,
}

// ── Trait ──

/// Domain-level GitHub operations.
///
/// Encapsulates GitHub API interactions behind a testable interface.
/// The real implementation uses `gh` CLI; tests use MockGitHub.
pub trait GitHub: Send + Sync {
    /// Fetch repository events with optional ETag for conditional requests.
    ///
    /// Returns `Ok(None)` on 304 Not Modified (no new events).
    fn fetch_events(&self, etag: Option<&str>) -> Result<Option<EventsResponse>>;

    /// Get the default branch name for the repository.
    fn default_branch(&self) -> Result<String>;
}

// ── Real Implementation ──

/// GitHub trait implementation using the `gh` CLI.
pub struct GhCliGitHub {
    /// Cached "owner/repo" identifier.
    repo: String,
}

impl GhCliGitHub {
    /// Create a new client, resolving owner/repo from the current directory.
    pub fn new() -> Result<Self> {
        let output = Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "--jq",
                ".nameWithOwner",
            ])
            .output()
            .context("gh CLI not found")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("failed to resolve repo: {}", stderr.trim());
        }

        let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Self { repo })
    }
}

/// Raw event JSON from the Events API (for deserialization).
#[derive(Deserialize)]
struct RawEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    payload: serde_json::Value,
}

// ── JSON extraction helpers ──

fn json_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn parse_event(raw: RawEvent) -> RepoEvent {
    let event_type = match raw.event_type.as_str() {
        "PushEvent" => EventType::Push,
        "WorkflowRunEvent" => EventType::WorkflowRun,
        "IssuesEvent" => EventType::Issues,
        other => EventType::Other(other.to_string()),
    };

    let payload = match &event_type {
        EventType::Push => {
            let branch = raw
                .payload
                .get("ref")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .strip_prefix("refs/heads/")
                .unwrap_or("")
                .to_string();
            EventPayload::Push {
                branch,
                before: json_str(&raw.payload, "before"),
                after: json_str(&raw.payload, "head"),
                size: json_u64(&raw.payload, "size"),
            }
        }
        EventType::WorkflowRun => {
            let wr = raw.payload.get("workflow_run");
            let empty = serde_json::Value::Null;
            let wr = wr.unwrap_or(&empty);
            EventPayload::WorkflowRun {
                run_id: json_u64(wr, "id"),
                name: json_str(wr, "name"),
                branch: json_str(wr, "head_branch"),
                conclusion: json_str(wr, "conclusion"),
            }
        }
        EventType::Issues => {
            let issue = raw.payload.get("issue");
            let empty = serde_json::Value::Null;
            let issue = issue.unwrap_or(&empty);
            EventPayload::Issues {
                action: json_str(&raw.payload, "action"),
                number: json_u64(issue, "number"),
                title: json_str(issue, "title"),
            }
        }
        _ => EventPayload::Unknown,
    };

    RepoEvent {
        id: raw.id,
        event_type,
        payload,
    }
}

impl GitHub for GhCliGitHub {
    fn fetch_events(&self, etag: Option<&str>) -> Result<Option<EventsResponse>> {
        let url = format!("/repos/{}/events", self.repo);
        let mut args = vec!["api", &url, "--include"];

        let etag_header;
        if let Some(tag) = etag {
            etag_header = format!("If-None-Match: {tag}");
            args.push("-H");
            args.push(&etag_header);
        }

        let output = Command::new("gh")
            .args(&args)
            .output()
            .context("gh CLI not found")?;

        let raw = String::from_utf8_lossy(&output.stdout);

        // Split headers from body — try \r\n\r\n first, fall back to \n\n
        let separator = if raw.contains("\r\n\r\n") {
            "\r\n\r\n"
        } else {
            "\n\n"
        };
        let (headers, body) = raw
            .split_once(separator)
            .ok_or_else(|| anyhow::anyhow!("unexpected gh api response format"))?;

        Self::parse_response(headers, body)
    }

    fn default_branch(&self) -> Result<String> {
        let output = Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "defaultBranchRef",
                "--jq",
                ".defaultBranchRef.name",
            ])
            .output()
            .context("gh CLI not found")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("failed to get default branch: {}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl GhCliGitHub {
    fn parse_response(headers: &str, body: &str) -> Result<Option<EventsResponse>> {
        let mut etag = String::new();
        let mut poll_interval = 60u64;

        for line in headers.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("http/") && lower.contains("304") {
                return Ok(None);
            }
            if lower.starts_with("etag:") {
                etag = line
                    .split_once(':')
                    .map(|x| x.1)
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
            if lower.starts_with("x-poll-interval:") {
                poll_interval = line
                    .split_once(':')
                    .and_then(|x| x.1.trim().parse().ok())
                    .unwrap_or(60);
            }
        }

        let raw_events: Vec<RawEvent> =
            serde_json::from_str(body.trim()).context("failed to parse events JSON")?;

        let events = raw_events.into_iter().map(parse_event).collect();

        Ok(Some(EventsResponse {
            events,
            etag,
            poll_interval,
        }))
    }
}

/// Convenience: create a shared real client.
pub fn real() -> Result<Arc<dyn GitHub>> {
    Ok(Arc::new(GhCliGitHub::new()?))
}
