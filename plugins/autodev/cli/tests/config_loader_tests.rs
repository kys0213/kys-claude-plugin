use autodev::core::config::loader;
use autodev::core::config::models::WorkflowConfig;
use autodev::core::config::Env;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

/// 테스트용 환경 변수 모킹 — #[serial] 없이 병렬 실행 가능
struct TestEnv {
    vars: HashMap<String, String>,
}

impl TestEnv {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    fn with_home(mut self, home: &str) -> Self {
        self.vars.insert("HOME".to_string(), home.to_string());
        self
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

// ═══════════════════════════════════════════════
// 1. 기본값 검증
// ═══════════════════════════════════════════════

#[test]
fn default_config_has_expected_values() {
    let config = WorkflowConfig::default();
    assert_eq!(config.sources.github.scan_interval_secs, 300);
    assert_eq!(config.sources.github.scan_targets, vec!["issues", "pulls"]);
    assert_eq!(config.sources.github.issue_concurrency, 1);
    assert_eq!(config.sources.github.model, "sonnet");
    // DaemonConfig defaults
    assert_eq!(config.daemon.tick_interval_secs, 10);
    assert_eq!(config.daemon.daily_report_hour, 6);
    assert_eq!(config.daemon.log_dir, "logs");
    assert_eq!(config.daemon.log_retention_days, 30);
    // Workflows defaults (v2)
    assert!(config.workflows.analyze.command.is_none());
    assert!(config.workflows.implement.command.is_none());
    assert!(config.workflows.review.command.is_none());
    assert_eq!(config.workflows.review.max_iterations, 2);
}

#[test]
fn load_merged_no_files_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());
    // 존재하지 않는 경로 → 양쪽 모두 None → default
    let config = loader::load_merged(&env, Some(tmp.path()));
    assert_eq!(config.sources.github.scan_interval_secs, 300);
    assert!(config.workflows.analyze.command.is_none());
}

#[test]
fn load_merged_none_path_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());
    let config = loader::load_merged(&env, None);
    assert_eq!(config.sources.github.scan_interval_secs, 300);
}

// ═══════════════════════════════════════════════
// 2. load_merged — 글로벌만 존재
// ═══════════════════════════════════════════════

#[test]
fn load_merged_global_only() {
    let tmp = TempDir::new().unwrap();

    let yaml = r#"
sources:
  github:
    scan_interval_secs: 60
    model: opus
workflows:
  review:
    max_iterations: 5
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    assert_eq!(config.sources.github.scan_interval_secs, 60);
    assert_eq!(config.sources.github.model, "opus");
    assert_eq!(config.workflows.review.max_iterations, 5);
    // 미지정 필드는 default 유지
    assert_eq!(config.sources.github.issue_concurrency, 1);
    assert!(config.workflows.analyze.command.is_none());
}

// ═══════════════════════════════════════════════
// 3. load_merged — 레포만 존재
// ═══════════════════════════════════════════════

#[test]
fn load_merged_repo_only() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    // HOME에는 글로벌 YAML 없음
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let yaml = r#"
sources:
  github:
    scan_interval_secs: 120
    ignore_authors:
      - bot1
      - bot2
"#;
    fs::write(repo_dir.join(".autodev.yaml"), yaml).unwrap();

    let config = loader::load_merged(&env, Some(&repo_dir));
    assert_eq!(config.sources.github.scan_interval_secs, 120);
    assert_eq!(config.sources.github.ignore_authors, vec!["bot1", "bot2"]);
    // 나머지는 default
    assert_eq!(config.sources.github.model, "sonnet");
}

// ═══════════════════════════════════════════════
// 4. load_merged — 글로벌 + 레포 오버라이드 (딥머지)
// ═══════════════════════════════════════════════

#[test]
fn load_merged_repo_overrides_global() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    // 글로벌 설정
    let global_yaml = r#"
sources:
  github:
    scan_interval_secs: 60
    model: opus
    issue_concurrency: 2
workflows:
  review:
    max_iterations: 5
"#;
    fs::write(tmp.path().join(".autodev.yaml"), global_yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    // 레포 오버라이드 — model과 max_iterations만 덮어씀
    let repo_yaml = r#"
sources:
  github:
    model: haiku
workflows:
  review:
    max_iterations: 3
"#;
    fs::write(repo_dir.join(".autodev.yaml"), repo_yaml).unwrap();

    let config = loader::load_merged(&env, Some(&repo_dir));

    // 오버라이드된 값
    assert_eq!(config.sources.github.model, "haiku");
    assert_eq!(config.workflows.review.max_iterations, 3);

    // 글로벌에서 유지되는 값
    assert_eq!(config.sources.github.scan_interval_secs, 60);
    assert_eq!(config.sources.github.issue_concurrency, 2);

    // 양쪽 모두 미지정 → default
    assert_eq!(config.daemon.tick_interval_secs, 10);
}

// ═══════════════════════════════════════════════
// 5. YAML 파싱 엣지 케이스
// ═══════════════════════════════════════════════

#[test]
fn load_merged_ignores_malformed_yaml() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    // 잘못된 YAML
    fs::write(repo_dir.join(".autodev.yaml"), "{{invalid yaml!!!").unwrap();

    // 파싱 실패 시 기본값 반환 (패닉하지 않음)
    let config = loader::load_merged(&env, Some(&repo_dir));
    assert_eq!(config.sources.github.scan_interval_secs, 300);
}

#[test]
fn load_merged_empty_yaml_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    // 빈 파일
    fs::write(repo_dir.join(".autodev.yaml"), "").unwrap();

    let config = loader::load_merged(&env, Some(&repo_dir));
    assert_eq!(config.sources.github.scan_interval_secs, 300);
}

#[test]
fn load_merged_partial_yaml_fills_defaults() {
    let tmp = TempDir::new().unwrap();

    let yaml = "sources:\n  github:\n    model: gpt-4\n";
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    assert_eq!(config.sources.github.model, "gpt-4");
    // 나머지 전부 default
    assert_eq!(config.sources.github.scan_interval_secs, 300);
    assert!(config.workflows.analyze.command.is_none());
    assert_eq!(config.workflows.review.max_iterations, 2);
}

// ═══════════════════════════════════════════════
// 6. DaemonConfig — YAML 파싱 + backward compat
// ═══════════════════════════════════════════════

#[test]
fn daemon_config_parsed_from_yaml() {
    let tmp = TempDir::new().unwrap();

    let yaml = r#"
daemon:
  tick_interval_secs: 30
  daily_report_hour: 9
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    assert_eq!(config.daemon.tick_interval_secs, 30);
    assert_eq!(config.daemon.daily_report_hour, 9);
}

#[test]
fn daemon_config_backward_compat_without_section() {
    let tmp = TempDir::new().unwrap();

    // daemon 섹션 없는 기존 YAML — backward compat 보장
    let yaml = r#"
sources:
  github:
    scan_interval_secs: 60
    model: opus
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    // sources.github 값은 오버라이드
    assert_eq!(config.sources.github.scan_interval_secs, 60);
    // daemon은 전부 default
    assert_eq!(config.daemon.tick_interval_secs, 10);
    assert_eq!(config.daemon.daily_report_hour, 6);
}

#[test]
fn daemon_config_partial_override() {
    let tmp = TempDir::new().unwrap();

    // daemon 섹션에 일부만 지정 — 나머지는 default
    let yaml = r#"
daemon:
  daily_report_hour: 12
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    assert_eq!(config.daemon.tick_interval_secs, 10); // default
    assert_eq!(config.daemon.daily_report_hour, 12); // overridden
}

// ═══════════════════════════════════════════════
// 7. 타입 오류 시 default fallback (#131)
// ═══════════════════════════════════════════════

#[test]
fn load_merged_type_error_falls_back_to_defaults() {
    let tmp = TempDir::new().unwrap();

    // scan_interval_secs는 u64인데 문자열을 넣으면 역직렬화 실패
    let yaml = r#"
sources:
  github:
    scan_interval_secs: "oops"
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    // 타입 오류 시에도 패닉 없이 default 반환
    let config = loader::load_merged(&env, None);
    assert_eq!(config.sources.github.scan_interval_secs, 300);
    assert_eq!(config.sources.github.model, "sonnet");
}

#[test]
fn load_merged_wrong_type_in_nested_field_falls_back() {
    let tmp = TempDir::new().unwrap();

    // max_iterations는 u32인데 문자열을 넣음
    let yaml = r#"
workflows:
  review:
    max_iterations: "not_a_number"
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    // default fallback 확인
    assert_eq!(config.workflows.review.max_iterations, 2);
    assert_eq!(config.sources.github.scan_interval_secs, 300);
}

// ═══════════════════════════════════════════════
// 8. v1 deprecated 키 호환성 (deny_unknown_fields 제거)
// ═══════════════════════════════════════════════

#[test]
fn load_merged_ignores_deprecated_v1_keys() {
    let tmp = TempDir::new().unwrap();

    // v1 YAML with commands, develop, workflow keys
    // deny_unknown_fields 제거로 이제 무시됨 (fallback 아님)
    let yaml = r#"
sources:
  github:
    scan_interval_secs: 60
commands:
  design: /old-design
develop:
  review:
    multi_llm: true
workflow:
  issue: builtin
  pr: builtin
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    // 유효한 키는 정상 파싱됨 (v1 키가 있어도 fallback 아님)
    assert_eq!(config.sources.github.scan_interval_secs, 60);
    // workflows는 default
    assert!(config.workflows.analyze.command.is_none());
}

#[test]
fn load_merged_unknown_top_level_field_no_longer_fails() {
    let tmp = TempDir::new().unwrap();

    // deny_unknown_fields 제거로 알 수 없는 필드도 무시됨
    let yaml = r#"
sources:
  github:
    scan_interval_secs: 60
totally_unknown_field: 42
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    // 유효한 키는 정상 파싱됨 (unknown field 무시)
    assert_eq!(config.sources.github.scan_interval_secs, 60);
}

// ═══════════════════════════════════════════════
// 9. workflows 섹션 파싱
// ═══════════════════════════════════════════════

#[test]
fn workflows_custom_command_parsed() {
    let tmp = TempDir::new().unwrap();

    let yaml = r#"
workflows:
  analyze:
    command: /review:multi-analyze
    agent: null
  review:
    command: /review:multi-review
    agent: null
    max_iterations: 3
"#;
    fs::write(tmp.path().join(".autodev.yaml"), yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    let config = loader::load_merged(&env, None);
    assert_eq!(
        config.workflows.analyze.command.as_deref(),
        Some("/review:multi-analyze")
    );
    assert_eq!(
        config.workflows.review.command.as_deref(),
        Some("/review:multi-review")
    );
    assert_eq!(config.workflows.review.max_iterations, 3);
    // implement은 default 유지
    assert!(config.workflows.implement.command.is_none());
}

#[test]
fn workflows_deep_merge_preserves_global_stage() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    // 글로벌: analyze에 커스텀 커맨드
    let global_yaml = r#"
workflows:
  analyze:
    command: /custom-analyze
  review:
    max_iterations: 5
"#;
    fs::write(tmp.path().join(".autodev.yaml"), global_yaml).unwrap();
    let env = TestEnv::new().with_home(tmp.path().to_str().unwrap());

    // 레포: review만 오버라이드
    let repo_yaml = r#"
workflows:
  review:
    max_iterations: 3
"#;
    fs::write(repo_dir.join(".autodev.yaml"), repo_yaml).unwrap();

    let config = loader::load_merged(&env, Some(&repo_dir));

    // 글로벌에서 유지
    assert_eq!(
        config.workflows.analyze.command.as_deref(),
        Some("/custom-analyze")
    );
    // 레포에서 오버라이드
    assert_eq!(config.workflows.review.max_iterations, 3);
}
