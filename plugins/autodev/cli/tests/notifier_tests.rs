use autodev::components::notifier::Notifier;
use autodev::infrastructure::gh::MockGh;

// ═══════════════════════════════════════════════
// is_issue_open
// ═══════════════════════════════════════════════

#[tokio::test]
async fn is_issue_open_returns_true_when_open() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "issues/42", ".state", "open");

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_issue_open("org/repo", 42, None).await);
}

#[tokio::test]
async fn is_issue_open_returns_false_when_closed() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "issues/42", ".state", "closed");

    let notifier = Notifier::new(&gh);
    assert!(!notifier.is_issue_open("org/repo", 42, None).await);
}

#[tokio::test]
async fn is_issue_open_returns_true_on_api_failure() {
    let gh = MockGh::new();
    // No mock set → api_get_field returns None

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_issue_open("org/repo", 42, None).await);
}

// ═══════════════════════════════════════════════
// is_pr_reviewable
// ═══════════════════════════════════════════════

#[tokio::test]
async fn is_pr_reviewable_open_no_approvals() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/10", ".state", "open");
    gh.set_field(
        "org/repo",
        "pulls/10/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "0",
    );

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_pr_reviewable("org/repo", 10, None).await);
}

#[tokio::test]
async fn is_pr_reviewable_returns_false_when_approved() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/10", ".state", "open");
    gh.set_field(
        "org/repo",
        "pulls/10/reviews",
        r#"[.[] | select(.state == "APPROVED")] | length"#,
        "1",
    );

    let notifier = Notifier::new(&gh);
    assert!(!notifier.is_pr_reviewable("org/repo", 10, None).await);
}

#[tokio::test]
async fn is_pr_reviewable_returns_false_when_closed() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/10", ".state", "closed");

    let notifier = Notifier::new(&gh);
    assert!(!notifier.is_pr_reviewable("org/repo", 10, None).await);
}

#[tokio::test]
async fn is_pr_reviewable_returns_true_on_state_api_failure() {
    let gh = MockGh::new();
    // No mock → api_get_field returns None → best effort true

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_pr_reviewable("org/repo", 10, None).await);
}

#[tokio::test]
async fn is_pr_reviewable_returns_true_on_review_api_failure() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/10", ".state", "open");
    // reviews API not set → None → best effort true

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_pr_reviewable("org/repo", 10, None).await);
}

// ═══════════════════════════════════════════════
// is_pr_mergeable
// ═══════════════════════════════════════════════

#[tokio::test]
async fn is_pr_mergeable_returns_true_when_open() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/5", ".state", "open");

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_pr_mergeable("org/repo", 5, None).await);
}

#[tokio::test]
async fn is_pr_mergeable_returns_false_when_merged() {
    let gh = MockGh::new();
    gh.set_field("org/repo", "pulls/5", ".state", "closed");

    let notifier = Notifier::new(&gh);
    assert!(!notifier.is_pr_mergeable("org/repo", 5, None).await);
}

#[tokio::test]
async fn is_pr_mergeable_returns_true_on_api_failure() {
    let gh = MockGh::new();

    let notifier = Notifier::new(&gh);
    assert!(notifier.is_pr_mergeable("org/repo", 5, None).await);
}

// ═══════════════════════════════════════════════
// post_issue_comment
// ═══════════════════════════════════════════════

#[tokio::test]
async fn post_issue_comment_records_call() {
    let gh = MockGh::new();
    let notifier = Notifier::new(&gh);

    let result = notifier
        .post_issue_comment("org/repo", 42, "Test comment", None)
        .await;

    assert!(result);
    let comments = gh.posted_comments.lock().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].0, "org/repo");
    assert_eq!(comments[0].1, 42);
    assert_eq!(comments[0].2, "Test comment");
}
