use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::knowledge::daily::create_knowledge_prs;
use autodev::knowledge::models::*;

// ─── Helpers ───

fn make_report(date: &str, suggestions: Vec<Suggestion>) -> DailyReport {
    DailyReport {
        date: date.to_string(),
        summary: DailySummary {
            issues_done: 1,
            prs_done: 0,
            failed: 0,
            skipped: 0,
            avg_duration_ms: 1000,
        },
        patterns: vec![],
        suggestions,
        cross_analysis: None,
    }
}

fn make_suggestion(target: &str, content: &str, reason: &str) -> Suggestion {
    Suggestion {
        suggestion_type: SuggestionType::Rule,
        target_file: target.to_string(),
        content: content.to_string(),
        reason: reason.to_string(),
    }
}

// ═══════════════════════════════════════════════
// create_knowledge_prs 기본 동작
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_creates_branch_commit_pr_and_skip_label() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "Always run tests",
            "Caught 3 bugs",
        )],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    // git: branch 생성 확인
    let git_calls = git.calls.lock().unwrap();
    let branch_call = git_calls
        .iter()
        .find(|(m, _)| m == "checkout_new_branch")
        .expect("should create branch");
    assert!(
        branch_call.1.contains("autodev/knowledge/2026-02-22-0"),
        "branch name should include date and index"
    );

    // git: commit + push 확인
    assert!(
        git_calls.iter().any(|(m, _)| m == "add_commit_push"),
        "should commit and push"
    );

    // gh: PR 생성 확인
    let prs = gh.created_prs.lock().unwrap();
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].0, "org/repo"); // repo_name
    assert!(prs[0].3.contains("[autodev] rule:")); // title
    assert!(prs[0].4.contains("autodev:knowledge-pr")); // body marker

    // gh: autodev:skip 라벨 부착 확인
    let labels = gh.added_labels.lock().unwrap();
    assert!(
        labels
            .iter()
            .any(|(r, _, l)| r == "org/repo" && l == "autodev:skip"),
        "should add skip label to knowledge PR"
    );
}

// ═══════════════════════════════════════════════
// 파일 쓰기 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_writes_file_content_to_target_path() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "Always run tests before committing",
            "Tests caught bugs",
        )],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    let file_path = tmpdir.path().join(".claude/rules/test.md");
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Always run tests before committing");
}

// ═══════════════════════════════════════════════
// C-1: fs::write 덮어쓰기 동작 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_overwrites_existing_file_c1() {
    let tmpdir = tempfile::TempDir::new().unwrap();

    // 기존 파일 생성
    let file_dir = tmpdir.path().join(".claude/rules");
    std::fs::create_dir_all(&file_dir).unwrap();
    std::fs::write(
        file_dir.join("existing.md"),
        "Original content that should be preserved",
    )
    .unwrap();

    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/existing.md",
            "Replacement content",
            "overwrite test",
        )],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    // C-1 BUG: fs::write는 append가 아니라 overwrite
    let content = std::fs::read_to_string(tmpdir.path().join(".claude/rules/existing.md")).unwrap();
    assert_eq!(
        content, "Replacement content",
        "C-1: fs::write overwrites existing file instead of appending"
    );
}

// ═══════════════════════════════════════════════
// 빈 suggestions → no-op
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_empty_suggestions_does_nothing() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report("2026-02-22", vec![]);
    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    let git_calls = git.calls.lock().unwrap();
    assert!(git_calls.is_empty(), "no git calls for empty suggestions");

    let prs = gh.created_prs.lock().unwrap();
    assert!(prs.is_empty(), "no PRs created for empty suggestions");
}

// ═══════════════════════════════════════════════
// 복수 suggestions → 복수 PR 생성
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_multiple_suggestions_creates_multiple_prs() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![
            make_suggestion(".claude/rules/a.md", "Rule A", "Reason A"),
            make_suggestion(".claude/rules/b.md", "Rule B", "Reason B"),
            make_suggestion("CLAUDE.md", "Main config", "Reason C"),
        ],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    // 3개 PR 생성 확인
    let prs = gh.created_prs.lock().unwrap();
    assert_eq!(prs.len(), 3);

    // 브랜치 이름 고유성 확인
    let git_calls = git.calls.lock().unwrap();
    let branches: Vec<_> = git_calls
        .iter()
        .filter(|(m, _)| m == "checkout_new_branch")
        .map(|(_, args)| args.clone())
        .collect();
    assert_eq!(branches.len(), 3);
    assert!(branches[0].contains("2026-02-22-0"));
    assert!(branches[1].contains("2026-02-22-1"));
    assert!(branches[2].contains("2026-02-22-2"));

    // 모든 PR에 skip 라벨 부착 확인
    let labels = gh.added_labels.lock().unwrap();
    let skip_count = labels
        .iter()
        .filter(|(_, _, l)| l == "autodev:skip")
        .count();
    assert_eq!(skip_count, 3);
}

// ═══════════════════════════════════════════════
// PR 본문 포맷 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_body_contains_suggestion_details() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![Suggestion {
            suggestion_type: SuggestionType::Hook,
            target_file: ".claude/hooks.json".to_string(),
            content: "Add linter hook".to_string(),
            reason: "Consistent formatting".to_string(),
        }],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    let prs = gh.created_prs.lock().unwrap();
    let body = &prs[0].4;
    assert!(body.contains("Knowledge Suggestion"));
    assert!(body.contains(".claude/hooks.json"));
    assert!(body.contains("Add linter hook"));
    assert!(body.contains("Consistent formatting"));
}

// ═══════════════════════════════════════════════
// 커밋 메시지 포맷 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_commit_message_includes_autodev_prefix() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "content",
            "improve CI",
        )],
    );

    create_knowledge_prs(&gh, &git, "org/repo", &report, tmpdir.path(), None).await;

    let git_calls = git.calls.lock().unwrap();
    let commit_call = git_calls
        .iter()
        .find(|(m, _)| m == "add_commit_push")
        .expect("should have commit call");
    assert!(
        commit_call.1.contains("[autodev] knowledge:"),
        "commit message should include [autodev] knowledge: prefix"
    );
}
