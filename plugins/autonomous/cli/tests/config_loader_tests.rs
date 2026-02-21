use autodev::config::loader;
use autodev::config::models::WorkflowConfig;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

// ═══════════════════════════════════════════════
// 1. 기본값 검증
// ═══════════════════════════════════════════════

#[test]
fn default_config_has_expected_values() {
    let config = WorkflowConfig::default();
    assert_eq!(config.consumer.scan_interval_secs, 300);
    assert_eq!(config.consumer.scan_targets, vec!["issues", "pulls"]);
    assert_eq!(config.consumer.issue_concurrency, 1);
    assert_eq!(config.consumer.model, "sonnet");
    assert_eq!(config.workflow.issue, "/develop-workflow:develop-auto");
    assert_eq!(config.workflow.pr, "/develop-workflow:multi-review");
    assert_eq!(config.commands.design, "/multi-llm-design");
    assert_eq!(config.commands.commit_and_pr, "/commit-and-pr");
}

#[test]
#[serial]
fn load_merged_no_files_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("HOME", tmp.path());
    // 존재하지 않는 경로 → 양쪽 모두 None → default
    let config = loader::load_merged(Some(tmp.path()));
    assert_eq!(config.consumer.scan_interval_secs, 300);
    assert_eq!(config.commands.design, "/multi-llm-design");
}

#[test]
#[serial]
fn load_merged_none_path_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("HOME", tmp.path());
    let config = loader::load_merged(None);
    assert_eq!(config.consumer.scan_interval_secs, 300);
}

// ═══════════════════════════════════════════════
// 2. load_merged — 글로벌만 존재
// ═══════════════════════════════════════════════

#[test]
#[serial]
fn load_merged_global_only() {
    let tmp = TempDir::new().unwrap();

    let yaml = r#"
consumer:
  scan_interval_secs: 60
  model: opus
commands:
  design: /custom-design
"#;
    fs::write(tmp.path().join(".develop-workflow.yaml"), yaml).unwrap();
    std::env::set_var("HOME", tmp.path());

    let config = loader::load_merged(None);
    assert_eq!(config.consumer.scan_interval_secs, 60);
    assert_eq!(config.consumer.model, "opus");
    assert_eq!(config.commands.design, "/custom-design");
    // 미지정 필드는 default 유지
    assert_eq!(config.commands.branch, "/git-branch");
    assert_eq!(config.consumer.issue_concurrency, 1);
}

// ═══════════════════════════════════════════════
// 3. load_merged — 레포만 존재
// ═══════════════════════════════════════════════

#[test]
#[serial]
fn load_merged_repo_only() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    // HOME에는 글로벌 YAML 없음
    std::env::set_var("HOME", tmp.path());

    let yaml = r#"
consumer:
  scan_interval_secs: 120
  ignore_authors:
    - bot1
    - bot2
"#;
    fs::write(repo_dir.join(".develop-workflow.yaml"), yaml).unwrap();

    let config = loader::load_merged(Some(&repo_dir));
    assert_eq!(config.consumer.scan_interval_secs, 120);
    assert_eq!(config.consumer.ignore_authors, vec!["bot1", "bot2"]);
    // 나머지는 default
    assert_eq!(config.consumer.model, "sonnet");
}

// ═══════════════════════════════════════════════
// 4. load_merged — 글로벌 + 레포 오버라이드 (딥머지)
// ═══════════════════════════════════════════════

#[test]
#[serial]
fn load_merged_repo_overrides_global() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    // 글로벌 설정
    let global_yaml = r#"
consumer:
  scan_interval_secs: 60
  model: opus
  issue_concurrency: 2
commands:
  design: /global-design
  review: /global-review
"#;
    fs::write(tmp.path().join(".develop-workflow.yaml"), global_yaml).unwrap();
    std::env::set_var("HOME", tmp.path());

    // 레포 오버라이드 — model과 design만 덮어씀
    let repo_yaml = r#"
consumer:
  model: haiku
commands:
  design: /repo-design
"#;
    fs::write(repo_dir.join(".develop-workflow.yaml"), repo_yaml).unwrap();

    let config = loader::load_merged(Some(&repo_dir));

    // 오버라이드된 값
    assert_eq!(config.consumer.model, "haiku");
    assert_eq!(config.commands.design, "/repo-design");

    // 글로벌에서 유지되는 값
    assert_eq!(config.consumer.scan_interval_secs, 60);
    assert_eq!(config.consumer.issue_concurrency, 2);
    assert_eq!(config.commands.review, "/global-review");

    // 양쪽 모두 미지정 → default
    assert_eq!(config.commands.branch, "/git-branch");
}

// ═══════════════════════════════════════════════
// 5. init_global — YAML 직렬화 + 파일 쓰기
// ═══════════════════════════════════════════════

#[test]
#[serial]
fn init_global_writes_yaml_file() {
    let tmp = TempDir::new().unwrap();
    std::env::set_var("HOME", tmp.path());

    let config = WorkflowConfig::default();
    loader::init_global(&config).unwrap();

    let path = tmp.path().join(".develop-workflow.yaml");
    assert!(path.exists());

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("scan_interval_secs"));
    assert!(content.contains("sonnet"));
}

// ═══════════════════════════════════════════════
// 6. YAML 파싱 엣지 케이스
// ═══════════════════════════════════════════════

#[test]
#[serial]
fn load_merged_ignores_malformed_yaml() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    std::env::set_var("HOME", tmp.path());

    // 잘못된 YAML
    fs::write(repo_dir.join(".develop-workflow.yaml"), "{{invalid yaml!!!").unwrap();

    // 파싱 실패 시 기본값 반환 (패닉하지 않음)
    let config = loader::load_merged(Some(&repo_dir));
    assert_eq!(config.consumer.scan_interval_secs, 300);
}

#[test]
#[serial]
fn load_merged_empty_yaml_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    std::env::set_var("HOME", tmp.path());

    // 빈 파일
    fs::write(repo_dir.join(".develop-workflow.yaml"), "").unwrap();

    let config = loader::load_merged(Some(&repo_dir));
    assert_eq!(config.consumer.scan_interval_secs, 300);
}

#[test]
#[serial]
fn load_merged_partial_yaml_fills_defaults() {
    let tmp = TempDir::new().unwrap();

    let yaml = "consumer:\n  model: gpt-4\n";
    fs::write(tmp.path().join(".develop-workflow.yaml"), yaml).unwrap();
    std::env::set_var("HOME", tmp.path());

    let config = loader::load_merged(None);
    assert_eq!(config.consumer.model, "gpt-4");
    // 나머지 전부 default
    assert_eq!(config.consumer.scan_interval_secs, 300);
    assert_eq!(config.workflow.issue, "/develop-workflow:develop-auto");
    assert_eq!(config.commands.design, "/multi-llm-design");
    assert!(config.develop.review.multi_llm);
}
