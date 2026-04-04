mod mock_gh;

use mock_gh::MockGh;
use serde_json::json;

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

    let args = autopilot::cmd::issue::CreateArgs {
        title: "feat(auth): implement token refresh".to_string(),
        label: vec!["autopilot:ready".to_string()],
        fingerprint: "gap:spec/auth.md:token-refresh".to_string(),
        body: "## Requirement".to_string(),
    };

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

    let args = autopilot::cmd::issue::CreateArgs {
        title: "feat(auth): implement token refresh".to_string(),
        label: vec!["autopilot:ready".to_string()],
        fingerprint: "gap:spec/auth.md:token-refresh".to_string(),
        body: "## Requirement".to_string(),
    };

    let code = autopilot::cmd::issue::create(&gh, &args).unwrap();
    assert_eq!(code, 0);

    // Verify the create call was made with fingerprint in body
    let calls = gh.calls.lock().unwrap();
    let create_call = calls
        .iter()
        .find(|c| c.contains(&"create".to_string()))
        .expect("should call gh issue create");
    let body_idx = create_call
        .iter()
        .position(|a| a == "--body")
        .expect("should have --body");
    let body = &create_call[body_idx + 1];
    assert!(
        body.contains("<!-- fingerprint: gap:spec/auth.md:token-refresh -->"),
        "body should contain fingerprint comment"
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
