//! v5 DataSource + AgentRuntime trait 통합 테스트.
//!
//! Mock 구현체를 사용하여 trait 계약을 검증한다.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use autodev::core::datasource::{
    DataSource, ItemContext, QueueContext, SourceConfig, SourceContext, WorkspaceConfig,
};
use autodev::core::phase::TaskKind;
use autodev::core::queue_item::QueueItem;
use autodev::core::runtime::{AgentRuntime, RuntimeRegistry, RuntimeRequest};
use autodev::infra::datasource::mock::MockDataSource;
use autodev::infra::runtime::mock::MockAgentRuntime;

fn test_workspace_config() -> WorkspaceConfig {
    let mut sources = HashMap::new();
    sources.insert(
        "github".to_string(),
        SourceConfig {
            url: "https://github.com/org/repo".to_string(),
            scan_interval_secs: 300,
            concurrency: 1,
        },
    );
    WorkspaceConfig {
        name: "test-workspace".to_string(),
        sources,
        concurrency: 2,
    }
}

fn test_item_context(work_id: &str) -> ItemContext {
    ItemContext {
        work_id: work_id.to_string(),
        workspace: "test-workspace".to_string(),
        queue: QueueContext {
            phase: "Running".to_string(),
            state: "implement".to_string(),
            source_id: "github:org/repo#42".to_string(),
        },
        source: SourceContext {
            source_type: "github".to_string(),
            url: "https://github.com/org/repo".to_string(),
            default_branch: Some("main".to_string()),
            extra: HashMap::new(),
        },
        history: vec![],
        worktree: Some("/tmp/autodev/test-42".to_string()),
    }
}

fn test_repo_ref() -> autodev::core::queue_item::RepoRef {
    autodev::core::queue_item::RepoRef {
        id: "r1".into(),
        name: "org/repo".into(),
        url: "https://github.com/org/repo".into(),
        gh_host: None,
    }
}

fn test_runtime_request(prompt: &str) -> RuntimeRequest {
    RuntimeRequest {
        working_dir: PathBuf::from("/tmp/test-worktree"),
        prompt: prompt.to_string(),
        model: None,
        system_prompt: None,
        structured_output: None,
        session_id: None,
    }
}

// ─── DataSource trait tests ───

#[tokio::test]
async fn mock_datasource_returns_enqueued_items() {
    let ds = MockDataSource::new("test");

    let item = QueueItem::new_issue(
        &test_repo_ref(),
        42,
        TaskKind::Analyze,
        "Test issue".into(),
        Some("body".into()),
        vec!["autodev:analyze".into()],
        "user".into(),
    );

    ds.enqueue_items(vec![item]);

    let config = test_workspace_config();
    let items = ds.collect(&config).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].github_number, 42);
    assert_eq!(items[0].task_kind, TaskKind::Analyze);
}

#[tokio::test]
async fn mock_datasource_returns_empty_when_no_items() {
    let ds = MockDataSource::new("test");
    let config = test_workspace_config();
    let items = ds.collect(&config).await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn mock_datasource_tracks_collect_count() {
    let ds = MockDataSource::new("test");
    let config = test_workspace_config();

    ds.collect(&config).await.unwrap();
    ds.collect(&config).await.unwrap();

    assert_eq!(*ds.collect_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn mock_datasource_get_context_returns_enqueued_context() {
    let ds = MockDataSource::new("test");
    let ctx = test_item_context("issue:org/repo:42");
    ds.enqueue_context(ctx);

    let item = QueueItem::new_issue(
        &test_repo_ref(),
        42,
        TaskKind::Analyze,
        "Test".into(),
        None,
        vec![],
        "user".into(),
    );

    let result = ds.get_context(&item).await.unwrap();
    assert_eq!(result.work_id, "issue:org/repo:42");
    assert_eq!(result.queue.source_id, "github:org/repo#42");
}

#[tokio::test]
async fn mock_datasource_get_context_fails_when_empty() {
    let ds = MockDataSource::new("test");
    let item = QueueItem::new_issue(
        &test_repo_ref(),
        42,
        TaskKind::Analyze,
        "Test".into(),
        None,
        vec![],
        "user".into(),
    );

    let result = ds.get_context(&item).await;
    assert!(result.is_err());
}

#[test]
fn datasource_name_returns_configured_name() {
    let ds = MockDataSource::new("jira");
    assert_eq!(ds.name(), "jira");
}

// ─── AgentRuntime trait tests ───

#[tokio::test]
async fn mock_runtime_returns_enqueued_response() {
    let rt = MockAgentRuntime::new("claude");
    rt.enqueue_response("analysis complete", 0);

    let request = test_runtime_request("analyze this issue");
    let response = rt.invoke(request).await;

    assert_eq!(response.exit_code, 0);
    assert_eq!(response.stdout, "analysis complete");
    assert!(response.is_success());
}

#[tokio::test]
async fn mock_runtime_tracks_calls() {
    let rt = MockAgentRuntime::new("claude");
    rt.enqueue_response("ok", 0);

    let request = test_runtime_request("test prompt");
    rt.invoke(request).await;

    assert_eq!(rt.call_count(), 1);
    let calls = rt.calls.lock().unwrap();
    assert_eq!(calls[0].prompt, "test prompt");
}

#[tokio::test]
async fn mock_runtime_returns_error_when_no_response() {
    let rt = MockAgentRuntime::new("claude");
    let request = test_runtime_request("test");
    let response = rt.invoke(request).await;

    assert_eq!(response.exit_code, 1);
    assert!(!response.is_success());
}

#[test]
fn runtime_capabilities_are_correct() {
    let rt = MockAgentRuntime::new("claude");
    let caps = rt.capabilities();
    assert!(caps.structured_output);
    assert!(!caps.session_resume);
}

// ─── RuntimeRegistry tests ───

#[test]
fn registry_resolves_registered_runtime() {
    let mut registry = RuntimeRegistry::new("claude".to_string());
    let rt = Arc::new(MockAgentRuntime::new("claude"));
    registry.register(rt);

    let resolved = registry.resolve("claude");
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().name(), "claude");
}

#[test]
fn registry_falls_back_to_default_runtime() {
    let mut registry = RuntimeRegistry::new("claude".to_string());
    let rt = Arc::new(MockAgentRuntime::new("claude"));
    registry.register(rt);

    let resolved = registry.resolve("nonexistent");
    assert!(resolved.is_some());
    assert_eq!(resolved.unwrap().name(), "claude");
}

#[test]
fn registry_returns_none_when_empty() {
    let registry = RuntimeRegistry::new("claude".to_string());
    let resolved = registry.resolve("claude");
    assert!(resolved.is_none());
}

#[test]
fn registry_lists_registered_names() {
    let mut registry = RuntimeRegistry::new("claude".to_string());
    registry.register(Arc::new(MockAgentRuntime::new("claude")));
    registry.register(Arc::new(MockAgentRuntime::new("gemini")));

    let mut names = registry.names();
    names.sort();
    assert_eq!(names, vec!["claude", "gemini"]);
}

// ─── RuntimeResponse tests ───

#[test]
fn runtime_response_error_has_negative_exit_code() {
    let resp = autodev::core::runtime::RuntimeResponse::error("something broke");
    assert_eq!(resp.exit_code, -1);
    assert!(!resp.is_success());
    assert_eq!(resp.stderr, "something broke");
    assert!(resp.stdout.is_empty());
}

// ─── End-to-end: DataSource collect → AgentRuntime invoke ───

#[tokio::test]
async fn datasource_collect_then_runtime_invoke_integration() {
    // 1. DataSource가 아이템을 수집
    let ds = MockDataSource::new("github");
    let item = QueueItem::new_issue(
        &test_repo_ref(),
        99,
        TaskKind::Implement,
        "Implement feature".into(),
        Some("implement this".into()),
        vec!["autodev:implement".into()],
        "dev".into(),
    );
    ds.enqueue_items(vec![item]);

    let config = test_workspace_config();
    let items = ds.collect(&config).await.unwrap();
    assert_eq!(items.len(), 1);

    // 2. 수집된 아이템으로 AgentRuntime 호출
    let rt = MockAgentRuntime::new("claude");
    rt.enqueue_response("implementation complete", 0);

    let request = RuntimeRequest {
        working_dir: PathBuf::from("/tmp/worktree-99"),
        prompt: format!("Implement: {}", items[0].title),
        model: None,
        system_prompt: None,
        structured_output: None,
        session_id: None,
    };

    let response = rt.invoke(request).await;
    assert!(response.is_success());
    assert_eq!(response.stdout, "implementation complete");

    // 3. 호출 기록 검증
    let calls = rt.calls.lock().unwrap();
    assert_eq!(calls[0].prompt, "Implement: Implement feature");
    assert_eq!(calls[0].working_dir, "/tmp/worktree-99");
}
