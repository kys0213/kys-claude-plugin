mod mock_gh;

use mock_gh::MockGh;
use serde_json::json;

fn default_create_args() -> autopilot::cmd::issue::CreateArgs {
    autopilot::cmd::issue::CreateArgs {
        title: "feat(auth): implement token refresh".to_string(),
        label: vec!["autopilot:ready".to_string()],
        fingerprint: "gap:spec/auth.md:token-refresh".to_string(),
        body: "## Requirement".to_string(),
        simhash: None,
    }
}

fn extract_body_from_calls(calls: &[Vec<String>]) -> String {
    let create_call = calls
        .iter()
        .find(|c| c.contains(&"create".to_string()))
        .expect("should call gh issue create");
    let body_idx = create_call
        .iter()
        .position(|a| a == "--body")
        .expect("should have --body");
    create_call[body_idx + 1].clone()
}

// ============================================================
// Edge case tests for fingerprint dedup (#580, #581)
// ============================================================

#[test]
fn check_dup_returns_1_when_duplicate_exists() {
    let gh = MockGh::new().on_list_containing(
        "in:body",
        vec![json!({"number": 42, "title": "existing issue"})],
    );

    let code = autopilot::cmd::issue::check_dup(&gh, "gap:spec/auth.md:token-refresh").unwrap();
    assert_eq!(code, 1);
}

#[test]
fn check_dup_returns_0_when_no_duplicate() {
    let gh = MockGh::new().on_list_containing("in:body", vec![]);

    let code = autopilot::cmd::issue::check_dup(&gh, "gap:spec/auth.md:token-refresh").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn create_skips_when_duplicate_exists() {
    let gh = MockGh::new().on_list_containing(
        "in:body",
        vec![json!({"number": 42, "title": "existing issue"})],
    );

    let args = default_create_args();

    let code = autopilot::cmd::issue::create(&gh, &args).unwrap();
    assert_eq!(code, 1);

    // Should NOT have called `issue create`
    let calls = gh.calls.lock().unwrap();
    assert!(
        !calls.iter().any(|c| c.contains(&"create".to_string())),
        "should not call gh issue create when duplicate exists"
    );
}

#[test]
fn create_creates_issue_when_no_duplicate() {
    let gh = MockGh::new()
        .on_list_containing("in:body", vec![])
        .on_run_containing("create", "https://github.com/owner/repo/issues/99");

    let args = default_create_args();

    let code = autopilot::cmd::issue::create(&gh, &args).unwrap();
    assert_eq!(code, 0);

    let calls = gh.calls.lock().unwrap();
    let body = extract_body_from_calls(&calls);
    assert!(
        body.contains("`fingerprint: gap:spec/auth.md:token-refresh`"),
        "body should contain searchable plain text fingerprint"
    );
    assert!(
        body.contains("<!-- fingerprint: gap:spec/auth.md:token-refresh -->"),
        "body should contain fingerprint HTML comment"
    );
}

#[test]
fn close_resolved_closes_merged_branch_issues() {
    let gh = MockGh::new()
        // List CI failure issues
        .on_list_containing(
            "ci-failure",
            vec![
                json!({"number": 10, "title": "fix: CI failure in validate.yml on feat/add-auth"}),
                json!({"number": 11, "title": "fix: CI failure in build.yml on feat/other"}),
            ],
        )
        // feat/add-auth has a merged PR
        .on_list_containing("feat/add-auth", vec![json!({"number": 50})])
        // feat/other has NO merged PR
        .on_list_containing("feat/other", vec![])
        // close succeeds
        .on_run_containing("close", "");

    let code = autopilot::cmd::issue::close_resolved(&gh, "autopilot:").unwrap();
    assert_eq!(code, 0);

    // Verify only issue #10 was closed (feat/add-auth had merged PR)
    let calls = gh.calls.lock().unwrap();
    let close_calls: Vec<_> = calls
        .iter()
        .filter(|c| c.contains(&"close".to_string()))
        .collect();
    assert_eq!(close_calls.len(), 1);
    assert!(close_calls[0].contains(&"10".to_string()));
}

#[test]
fn close_resolved_returns_empty_when_no_ci_issues() {
    let gh = MockGh::new().on_list_containing("ci-failure", vec![]);

    let code = autopilot::cmd::issue::close_resolved(&gh, "autopilot:").unwrap();
    assert_eq!(code, 0);
}

// ============================================================
// Edge case: create with simhash embeds both plain text + HTML comment (#580)
// ============================================================

#[test]
fn create_with_simhash_embeds_searchable_fingerprint_and_metadata() {
    let gh = MockGh::new()
        .on_list_containing("in:body", vec![])
        .on_run_containing("create", "https://github.com/owner/repo/issues/55");

    let mut args = default_create_args();
    args.simhash = Some("0xA3F2B81C4D5E6F1B".to_string());

    let code = autopilot::cmd::issue::create(&gh, &args).unwrap();
    assert_eq!(code, 0);

    let calls = gh.calls.lock().unwrap();
    let body = extract_body_from_calls(&calls);

    // Both searchable plain text and HTML comment must exist
    assert!(
        body.contains("`fingerprint: gap:spec/auth.md:token-refresh`"),
        "searchable fingerprint missing"
    );
    assert!(
        body.contains("<!-- fingerprint: gap:spec/auth.md:token-refresh -->"),
        "HTML comment fingerprint missing"
    );
    assert!(
        body.contains("<!-- simhash: 0xA3F2B81C4D5E6F1B -->"),
        "simhash comment missing"
    );
}

// ============================================================
// Edge case: duplicate detected on second create attempt (#580, #581)
// Simulates tick 1 creating an issue, tick 2 finding it as duplicate
// ============================================================

#[test]
fn create_detects_duplicate_from_previous_tick() {
    // Tick 1: no duplicate → creates issue
    let gh_tick1 = MockGh::new()
        .on_list_containing("in:body", vec![])
        .on_run_containing("create", "https://github.com/owner/repo/issues/100");

    let args = autopilot::cmd::issue::CreateArgs {
        title: "fix: CI failure in build.yml on feat/auth".to_string(),
        label: vec![
            "autopilot:ci-failure".to_string(),
            "autopilot:ready".to_string(),
        ],
        fingerprint: "ci:build.yml:feat/auth:test-failure".to_string(),
        body: "## CI failure".to_string(),
        simhash: None,
    };

    let code = autopilot::cmd::issue::create(&gh_tick1, &args).unwrap();
    assert_eq!(code, 0, "tick 1 should create issue");

    // Tick 2: duplicate exists → skip
    let gh_tick2 = MockGh::new().on_list_containing(
        "in:body",
        vec![json!({"number": 100, "title": "fix: CI failure in build.yml on feat/auth"})],
    );

    let code = autopilot::cmd::issue::create(&gh_tick2, &args).unwrap();
    assert_eq!(code, 1, "tick 2 should detect duplicate");

    // No create call on tick 2
    let calls = gh_tick2.calls.lock().unwrap();
    assert!(
        !calls.iter().any(|c| c.contains(&"create".to_string())),
        "tick 2 should not call gh issue create"
    );
}

// ============================================================
// Edge case: fingerprint with special characters in search query
// ============================================================

#[test]
fn check_dup_handles_fingerprint_with_colons_and_slashes() {
    let gh = MockGh::new()
        .on_list_containing("in:body", vec![json!({"number": 77, "title": "existing"})]);

    let code =
        autopilot::cmd::issue::check_dup(&gh, "ci:validate.yml:fix/CP-920:build-error").unwrap();
    assert_eq!(code, 1, "should find duplicate with complex fingerprint");

    // Verify the search query was passed correctly
    let calls = gh.calls.lock().unwrap();
    let search_call = calls.first().expect("should have a call");
    let search_arg = search_call
        .iter()
        .find(|a| a.contains("in:body"))
        .expect("should have search arg");
    assert!(
        search_arg.contains("ci:validate.yml:fix/CP-920:build-error"),
        "search query should contain full fingerprint"
    );
}

// ============================================================
// Edge case: multiple labels on create
// ============================================================

#[test]
fn create_passes_multiple_labels_correctly() {
    let gh = MockGh::new()
        .on_list_containing("in:body", vec![])
        .on_run_containing("create", "https://github.com/owner/repo/issues/88");

    let args = autopilot::cmd::issue::CreateArgs {
        title: "fix: CI failure".to_string(),
        label: vec![
            "autopilot:ci-failure".to_string(),
            "autopilot:ready".to_string(),
        ],
        fingerprint: "ci:build.yml:main:test-failure".to_string(),
        body: "content".to_string(),
        simhash: None,
    };

    autopilot::cmd::issue::create(&gh, &args).unwrap();

    let calls = gh.calls.lock().unwrap();
    let create_call = calls
        .iter()
        .find(|c| c.contains(&"create".to_string()))
        .unwrap();

    // Both labels should appear
    let label_positions: Vec<_> = create_call
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--label")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(label_positions.len(), 2, "should have 2 --label flags");
    assert_eq!(create_call[label_positions[0] + 1], "autopilot:ci-failure");
    assert_eq!(create_call[label_positions[1] + 1], "autopilot:ready");
}

// ============================================================
// Edge case: close_resolved with mixed branch states (#579)
// ============================================================

#[test]
fn close_resolved_skips_issues_without_branch_in_title() {
    let gh = MockGh::new().on_list_containing(
        "ci-failure",
        vec![json!({"number": 10, "title": "some unrelated title"})],
    );

    let code = autopilot::cmd::issue::close_resolved(&gh, "autopilot:").unwrap();
    assert_eq!(code, 0);

    let calls = gh.calls.lock().unwrap();
    assert!(
        !calls.iter().any(|c| c.contains(&"close".to_string())),
        "should not close issues without branch in title"
    );
}

// ============================================================
// Edge case: close_resolved with multiple issues, some merged some not (#579)
// Verifies only the correct subset is closed
// ============================================================

#[test]
fn close_resolved_handles_mixed_merged_and_open_branches() {
    let gh = MockGh::new()
        .on_list_containing(
            "ci-failure",
            vec![
                json!({"number": 20, "title": "fix: CI failure in test.yml on feat/merged-branch"}),
                json!({"number": 21, "title": "fix: CI failure in lint.yml on feat/open-branch"}),
                json!({"number": 22, "title": "fix: CI failure in build.yml on feat/also-merged"}),
            ],
        )
        // feat/merged-branch has merged PR
        .on_list_containing("feat/merged-branch", vec![json!({"number": 60})])
        // feat/open-branch has NO merged PR
        .on_list_containing("feat/open-branch", vec![])
        // feat/also-merged has merged PR
        .on_list_containing("feat/also-merged", vec![json!({"number": 61})])
        .on_run_containing("close", "");

    let code = autopilot::cmd::issue::close_resolved(&gh, "autopilot:").unwrap();
    assert_eq!(code, 0);

    let calls = gh.calls.lock().unwrap();
    let close_calls: Vec<_> = calls
        .iter()
        .filter(|c| c.contains(&"close".to_string()))
        .collect();

    // Only #20 and #22 should be closed (merged branches)
    assert_eq!(close_calls.len(), 2, "should close exactly 2 issues");
    let closed_numbers: Vec<bool> = vec![
        close_calls.iter().any(|c| c.contains(&"20".to_string())),
        close_calls.iter().any(|c| c.contains(&"22".to_string())),
    ];
    assert!(
        closed_numbers.iter().all(|&b| b),
        "should close #20 and #22"
    );
    // #21 should NOT be closed
    assert!(
        !close_calls.iter().any(|c| c.contains(&"21".to_string())),
        "should not close #21 (open branch)"
    );
}

// ============================================================
// Edge case: append_fingerprint preserves original body content
// ============================================================

#[test]
fn append_fingerprint_preserves_body_with_special_markdown() {
    let body = "## Summary\n\n- item 1\n- item 2\n\n```rust\nfn main() {}\n```";
    let fp = "gap:spec/complex-path.md:multi-word-keyword";
    let result = autopilot::cmd::issue::append_fingerprint(body, fp);

    // Original body preserved
    assert!(result.starts_with(body));
    // Separator present
    assert!(result.contains("\n\n---\n"));
    // Both formats present
    assert!(result.contains(&format!("`fingerprint: {fp}`")));
    assert!(result.contains(&format!("<!-- fingerprint: {fp} -->")));
    // Plain text comes before HTML comment
    let plain_pos = result.find(&format!("`fingerprint: {fp}`")).unwrap();
    let html_pos = result.find(&format!("<!-- fingerprint: {fp} -->")).unwrap();
    assert!(
        plain_pos < html_pos,
        "searchable text should come before HTML comment"
    );
}

// ============================================================
// Edge case: extract_branch_from_ci_title with various formats
// ============================================================

#[test]
fn extract_branch_from_ci_title_various_formats() {
    // Standard format
    assert_eq!(
        autopilot::cmd::issue::extract_branch_from_ci_title(
            "fix: CI failure in validate.yml on feat/add-auth"
        ),
        "feat/add-auth"
    );

    // Branch with nested slashes
    assert_eq!(
        autopilot::cmd::issue::extract_branch_from_ci_title(
            "fix: CI failure in build.yml on fix/CP-920/gofmt"
        ),
        "fix/CP-920/gofmt"
    );

    // Trailing whitespace
    assert_eq!(
        autopilot::cmd::issue::extract_branch_from_ci_title(
            "fix: CI failure in test.yml on main  "
        ),
        "main"
    );

    // No " on " → empty
    assert_eq!(
        autopilot::cmd::issue::extract_branch_from_ci_title("some random title"),
        ""
    );

    // " on " appears multiple times → last one wins
    assert_eq!(
        autopilot::cmd::issue::extract_branch_from_ci_title(
            "fix: CI failure on build on feat/branch"
        ),
        "feat/branch"
    );
}
