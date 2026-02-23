use autodev::infrastructure::claude::mock::MockClaude;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::suggest_workflow::mock::MockSuggestWorkflow;
use autodev::knowledge::models::*;

// ═══════════════════════════════════════════════════
// 1. MockSuggestWorkflow 기본 동작 테스트
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn mock_suggest_workflow_returns_empty_when_no_responses() {
    let sw = MockSuggestWorkflow::new();

    let result = sw_trait(&sw).query_tool_frequency(None).await.unwrap();
    assert!(result.is_empty());

    let result = sw_trait(&sw)
        .query_filtered_sessions("[autodev]", None, None)
        .await
        .unwrap();
    assert!(result.is_empty());

    let result = sw_trait(&sw).query_repetition(None).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn mock_suggest_workflow_returns_enqueued_responses() {
    let sw = MockSuggestWorkflow::new();

    sw.enqueue_tool_frequency(vec![ToolFrequencyEntry {
        tool: "Bash:test".into(),
        frequency: 12,
        sessions: 3,
    }]);

    let result = sw_trait(&sw).query_tool_frequency(None).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tool, "Bash:test");
    assert_eq!(result[0].frequency, 12);
}

#[tokio::test]
async fn mock_suggest_workflow_records_calls() {
    let sw = MockSuggestWorkflow::new();
    sw.enqueue_tool_frequency(vec![]);

    let _ = sw_trait(&sw)
        .query_tool_frequency(Some("first_prompt_snippet LIKE '[autodev]%'"))
        .await;

    assert_eq!(sw.call_count(), 1);
    let calls = sw.calls.lock().unwrap();
    assert_eq!(calls[0].0, "query_tool_frequency");
    assert!(calls[0].1[0].contains("[autodev]"));
}

// ═══════════════════════════════════════════════════
// 2. Per-task knowledge extraction + suggest-workflow 연동
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn extract_task_knowledge_includes_sw_data_in_prompt() {
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let gh = MockGh::new();
    let sw = MockSuggestWorkflow::new();

    // suggest-workflow가 tool-frequency 데이터 반환
    sw.enqueue_tool_frequency(vec![
        ToolFrequencyEntry {
            tool: "Bash:test".into(),
            frequency: 15,
            sessions: 1,
        },
        ToolFrequencyEntry {
            tool: "Edit".into(),
            frequency: 8,
            sessions: 1,
        },
    ]);

    let tmp = tempfile::TempDir::new().unwrap();

    let result = autodev::knowledge::extractor::extract_task_knowledge(
        &claude,
        &gh,
        &sw,
        "org/repo",
        42,
        "issue",
        tmp.path(),
        None,
    )
    .await
    .unwrap();

    assert!(result.is_some());

    // Claude에 전달된 프롬프트에 suggest-workflow 데이터가 포함되어야 함
    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].1;
    assert!(
        prompt.contains("suggest-workflow session data"),
        "prompt should contain suggest-workflow section"
    );
    assert!(
        prompt.contains("Bash:test"),
        "prompt should contain tool name"
    );

    // suggest-workflow가 호출되었는지 확인
    assert!(sw.call_count() > 0);
}

#[tokio::test]
async fn extract_task_knowledge_works_without_sw_data() {
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let gh = MockGh::new();
    let sw = MockSuggestWorkflow::new();
    // suggest-workflow 데이터 없음 (빈 응답)

    let tmp = tempfile::TempDir::new().unwrap();

    let result = autodev::knowledge::extractor::extract_task_knowledge(
        &claude,
        &gh,
        &sw,
        "org/repo",
        42,
        "issue",
        tmp.path(),
        None,
    )
    .await
    .unwrap();

    assert!(result.is_some());

    // suggest-workflow 데이터가 없으면 프롬프트에 포함되지 않아야 함
    let calls = claude.calls.lock().unwrap();
    let prompt = &calls[0].1;
    assert!(
        !prompt.contains("suggest-workflow session data"),
        "prompt should NOT contain suggest-workflow section when no data"
    );
}

// ═══════════════════════════════════════════════════
// 3. Daily report + cross analysis 연동
// ═══════════════════════════════════════════════════

#[tokio::test]
async fn enrich_with_cross_analysis_populates_report() {
    let sw = MockSuggestWorkflow::new();

    sw.enqueue_filtered_sessions(vec![SessionEntry {
        id: "session-1".into(),
        prompt_count: 10,
        tool_use_count: 25,
        first_prompt: "[autodev] implement: issue #42".into(),
        started_at: Some("2026-02-22T10:00:00Z".into()),
        duration_minutes: Some(30.5),
    }]);

    sw.enqueue_tool_frequency(vec![
        ToolFrequencyEntry {
            tool: "Bash:test".into(),
            frequency: 45,
            sessions: 5,
        },
        ToolFrequencyEntry {
            tool: "Edit".into(),
            frequency: 38,
            sessions: 5,
        },
    ]);

    sw.enqueue_repetition(vec![RepetitionEntry {
        session_id: "session-1".into(),
        tool: "Bash:test".into(),
        cnt: 45,
        deviation_score: 3.2,
    }]);

    let mut report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 3,
            prs_done: 1,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 5000,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: None,
    };

    autodev::knowledge::daily::enrich_with_cross_analysis(&mut report, &sw).await;

    let ca = report
        .cross_analysis
        .as_ref()
        .expect("cross_analysis should be set");
    assert_eq!(ca.sessions.len(), 1);
    assert_eq!(ca.sessions[0].id, "session-1");
    assert_eq!(ca.tool_frequencies.len(), 2);
    assert_eq!(ca.tool_frequencies[0].tool, "Bash:test");
    assert_eq!(ca.anomalies.len(), 1);
    assert_eq!(ca.anomalies[0].deviation_score, 3.2);
}

#[tokio::test]
async fn enrich_with_cross_analysis_empty_when_no_data() {
    let sw = MockSuggestWorkflow::new();
    // All queries return empty (default)

    let mut report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 0,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 0,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: None,
    };

    autodev::knowledge::daily::enrich_with_cross_analysis(&mut report, &sw).await;

    assert!(
        report.cross_analysis.is_none(),
        "cross_analysis should remain None when no data"
    );
}

#[tokio::test]
async fn daily_suggestions_prompt_includes_cross_analysis_hint() {
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 3,
            prs_done: 1,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 5000,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: Some(CrossAnalysis {
            tool_frequencies: vec![ToolFrequencyEntry {
                tool: "Bash:test".into(),
                frequency: 45,
                sessions: 5,
            }],
            anomalies: vec![],
            sessions: vec![],
        }),
    };

    let tmp = tempfile::TempDir::new().unwrap();

    let _ =
        autodev::knowledge::daily::generate_daily_suggestions(&claude, &report, tmp.path()).await;

    let calls = claude.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let prompt = &calls[0].1;
    assert!(
        prompt.contains("cross_analysis"),
        "prompt should mention cross_analysis"
    );
    assert!(
        prompt.contains("tool usage patterns"),
        "prompt should include cross-analysis hint"
    );
}

#[tokio::test]
async fn daily_suggestions_prompt_no_cross_hint_without_analysis() {
    let claude = MockClaude::new();
    claude.enqueue_response(r#"{"suggestions":[]}"#, 0);

    let report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 1,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 1000,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: None,
    };

    let tmp = tempfile::TempDir::new().unwrap();

    let _ =
        autodev::knowledge::daily::generate_daily_suggestions(&claude, &report, tmp.path()).await;

    let calls = claude.calls.lock().unwrap();
    let prompt = &calls[0].1;
    assert!(
        !prompt.contains("cross_analysis"),
        "prompt should NOT mention cross_analysis when absent"
    );
}

// ═══════════════════════════════════════════════════
// 4. cross_analysis 모델 직렬화/역직렬화
// ═══════════════════════════════════════════════════

#[test]
fn cross_analysis_serializes_in_daily_report() {
    let report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 1,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 1000,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: Some(CrossAnalysis {
            tool_frequencies: vec![ToolFrequencyEntry {
                tool: "Bash:test".into(),
                frequency: 15,
                sessions: 3,
            }],
            anomalies: vec![RepetitionEntry {
                session_id: "s1".into(),
                tool: "Bash:test".into(),
                cnt: 15,
                deviation_score: 2.5,
            }],
            sessions: vec![SessionEntry {
                id: "s1".into(),
                prompt_count: 10,
                tool_use_count: 20,
                first_prompt: "[autodev] implement: issue #1".into(),
                started_at: None,
                duration_minutes: Some(15.0),
            }],
        }),
    };

    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("cross_analysis"));
    assert!(json.contains("Bash:test"));
    assert!(json.contains("deviation_score"));

    // Roundtrip
    let parsed: DailyReport = serde_json::from_str(&json).unwrap();
    let ca = parsed.cross_analysis.unwrap();
    assert_eq!(ca.tool_frequencies.len(), 1);
    assert_eq!(ca.anomalies[0].deviation_score, 2.5);
}

#[test]
fn cross_analysis_omitted_when_none() {
    let report = DailyReport {
        date: "2026-02-22".into(),
        summary: DailySummary {
            issues_done: 0,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 0,
        },
        patterns: vec![],
        suggestions: vec![],
        cross_analysis: None,
    };

    let json = serde_json::to_string(&report).unwrap();
    assert!(
        !json.contains("cross_analysis"),
        "cross_analysis should be omitted from JSON when None"
    );
}

// ═══════════════════════════════════════════════════
// Helper
// ═══════════════════════════════════════════════════

fn sw_trait(
    sw: &MockSuggestWorkflow,
) -> &dyn autodev::infrastructure::suggest_workflow::SuggestWorkflow {
    sw as &dyn autodev::infrastructure::suggest_workflow::SuggestWorkflow
}
