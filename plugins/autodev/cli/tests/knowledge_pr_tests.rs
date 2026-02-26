use std::collections::HashMap;

use autodev::components::workspace::Workspace;
use autodev::config::Env;
use autodev::infrastructure::gh::mock::MockGh;
use autodev::infrastructure::git::mock::MockGit;
use autodev::knowledge::daily::create_knowledge_prs;
use autodev::knowledge::models::*;

// ─── TestEnv ───

struct TestEnv {
    vars: HashMap<String, String>,
}

impl TestEnv {
    fn new(tmpdir: &tempfile::TempDir) -> Self {
        let mut vars = HashMap::new();
        vars.insert(
            "AUTODEV_HOME".to_string(),
            tmpdir.path().to_str().unwrap().to_string(),
        );
        Self { vars }
    }
}

impl Env for TestEnv {
    fn var(&self, key: &str) -> Result<String, std::env::VarError> {
        self.vars
            .get(key)
            .cloned()
            .ok_or(std::env::VarError::NotPresent)
    }
}

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

/// worktree path 계산: $AUTODEV_HOME/workspaces/{sanitized_repo}/{task_id}
fn worktree_path(tmpdir: &tempfile::TempDir, task_id: &str) -> std::path::PathBuf {
    tmpdir
        .path()
        .join("workspaces")
        .join("org-repo")
        .join(task_id)
}

// ═══════════════════════════════════════════════
// create_knowledge_prs 기본 동작
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_creates_branch_commit_pr_and_skip_label() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    // ensure_cloned 시뮬레이션: base(main) 디렉토리 생성
    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "Always run tests",
            "Caught 3 bugs",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    // git: worktree 생성 확인
    let git_calls = git.calls.lock().unwrap();
    assert!(
        git_calls.iter().any(|(m, _)| m == "worktree_add"),
        "should create worktree"
    );

    // git: branch 생성 확인
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

    // git: worktree 정리 확인
    assert!(
        git_calls.iter().any(|(m, _)| m == "worktree_remove"),
        "should remove worktree after PR"
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
// 파일 쓰기 검증 (worktree 격리 경로)
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_writes_file_content_to_worktree_path() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "Always run tests before committing",
            "Tests caught bugs",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    // git 호출에서 worktree 경로 사용을 확인 (worktree는 PR 후 정리됨)
    let git_calls = git.calls.lock().unwrap();
    let wt_path = worktree_path(&tmpdir, "knowledge-daily-2026-02-22-0");
    let wt_str = wt_path.to_str().unwrap();

    // checkout_new_branch가 worktree 경로에서 실행됨
    let branch_call = git_calls
        .iter()
        .find(|(m, _)| m == "checkout_new_branch")
        .unwrap();
    assert!(
        branch_call.1.contains(wt_str),
        "branch should be created in worktree, not base_path"
    );

    // add_commit_push가 worktree 경로에서 실행됨
    let commit_call = git_calls
        .iter()
        .find(|(m, _)| m == "add_commit_push")
        .unwrap();
    assert!(
        commit_call.1.contains(wt_str),
        "commit should happen in worktree, not base_path"
    );
    assert!(
        commit_call.1.contains(".claude/rules/test.md"),
        "committed file should be the target"
    );

    // base_path에는 파일이 없어야 함 (격리 확인)
    assert!(!base.join(".claude/rules/test.md").exists());
}

// ═══════════════════════════════════════════════
// worktree 격리: base_path 오염 없음 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_does_not_pollute_base_path() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    // base_path에 기존 파일 생성
    let file_dir = base.join(".claude/rules");
    std::fs::create_dir_all(&file_dir).unwrap();
    std::fs::write(file_dir.join("existing.md"), "Original content").unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/existing.md",
            "Replacement content",
            "overwrite test",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    // base_path의 기존 파일은 변경되지 않아야 함 (격리 확인)
    let content = std::fs::read_to_string(base.join(".claude/rules/existing.md")).unwrap();
    assert_eq!(
        content, "Original content",
        "base_path file should be untouched (worktree isolation)"
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
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let report = make_report("2026-02-22", vec![]);
    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

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
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![
            make_suggestion(".claude/rules/a.md", "Rule A", "Reason A"),
            make_suggestion(".claude/rules/b.md", "Rule B", "Reason B"),
            make_suggestion("CLAUDE.md", "Main config", "Reason C"),
        ],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

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

    // 각 suggestion에 대해 worktree 생성+제거 확인
    let wt_add_count = git_calls
        .iter()
        .filter(|(m, _)| m == "worktree_add")
        .count();
    let wt_remove_count = git_calls
        .iter()
        .filter(|(m, _)| m == "worktree_remove")
        .count();
    assert_eq!(wt_add_count, 3, "should create 3 worktrees");
    assert_eq!(wt_remove_count, 3, "should remove 3 worktrees");
}

// ═══════════════════════════════════════════════
// PR 본문 포맷 검증
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_body_contains_suggestion_details() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![Suggestion {
            suggestion_type: SuggestionType::Hook,
            target_file: ".claude/hooks.json".to_string(),
            content: "Add linter hook".to_string(),
            reason: "Consistent formatting".to_string(),
        }],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

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
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            ".claude/rules/test.md",
            "content",
            "improve CI",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

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

// ═══════════════════════════════════════════════
// path traversal 방어: 위험 경로 → PR 미생성 + worktree 정리
// ═══════════════════════════════════════════════

#[tokio::test]
async fn create_prs_rejects_parent_traversal() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            "../../../etc/passwd",
            "malicious content",
            "traversal attack",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    // PR이 생성되지 않아야 함
    let prs = gh.created_prs.lock().unwrap();
    assert!(prs.is_empty(), "traversal path should not create PR");

    // worktree는 정리되어야 함
    let git_calls = git.calls.lock().unwrap();
    assert!(
        git_calls.iter().any(|(m, _)| m == "worktree_remove"),
        "worktree should be cleaned up after path rejection"
    );
}

#[tokio::test]
async fn create_prs_rejects_absolute_path() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![make_suggestion(
            "/tmp/pwned.txt",
            "malicious content",
            "absolute path attack",
        )],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    let prs = gh.created_prs.lock().unwrap();
    assert!(prs.is_empty(), "absolute path should not create PR");
}

#[tokio::test]
async fn create_prs_mixed_safe_and_unsafe_skips_only_unsafe() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let gh = MockGh::new();
    let git = MockGit::new();
    let env = TestEnv::new(&tmpdir);
    let workspace = Workspace::new(&git, &env);

    let base = tmpdir.path().join("workspaces/org-repo/main");
    std::fs::create_dir_all(&base).unwrap();

    let report = make_report(
        "2026-02-22",
        vec![
            make_suggestion(".claude/rules/safe.md", "safe content", "valid"),
            make_suggestion("../../escape.txt", "bad", "traversal"),
            make_suggestion("CLAUDE.md", "also safe", "valid too"),
        ],
    );

    create_knowledge_prs(&gh, &workspace, "org/repo", &report, None).await;

    // 안전한 2개만 PR 생성
    let prs = gh.created_prs.lock().unwrap();
    assert_eq!(prs.len(), 2, "only safe suggestions should create PRs");
}
