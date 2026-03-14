use autodev::cli::convention::{
    bootstrap, detect_tech_stack, format_bootstrap_result, format_tech_stack, generate_conventions,
};
use tempfile::TempDir;

fn setup_temp_repo() -> TempDir {
    tempfile::tempdir().unwrap()
}

// ─── detect_tech_stack tests ───

#[test]
fn detect_finds_rust_from_cargo_toml() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "my-app"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"Rust".to_string()));
    assert!(stack.frameworks.contains(&"Tokio".to_string()));
    assert!(stack.test_tools.contains(&"cargo test".to_string()));
}

#[test]
fn detect_finds_typescript_from_package_json() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{
  "dependencies": {
    "typescript": "^5.0.0",
    "react": "^18.0.0",
    "vitest": "^1.0.0"
  }
}"#,
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"TypeScript".to_string()));
    assert!(stack.frameworks.contains(&"React".to_string()));
    assert!(stack.test_tools.contains(&"Vitest".to_string()));
}

#[test]
fn detect_finds_javascript_without_typescript_dep() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{
  "dependencies": {
    "express": "^4.0.0"
  }
}"#,
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"JavaScript".to_string()));
    assert!(!stack.languages.contains(&"TypeScript".to_string()));
    assert!(stack.frameworks.contains(&"Express".to_string()));
}

#[test]
fn detect_finds_go_from_go_mod() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("go.mod"),
        "module example.com/myapp\n\ngo 1.21\n",
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"Go".to_string()));
}

#[test]
fn detect_finds_python_from_pyproject_toml() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"myapp\"\n",
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"Python".to_string()));
}

#[test]
fn detect_finds_python_from_requirements_txt() {
    let tmp = setup_temp_repo();
    std::fs::write(tmp.path().join("requirements.txt"), "flask==3.0\n").unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"Python".to_string()));
}

#[test]
fn detect_finds_postgres_from_docker_compose() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("docker-compose.yml"),
        r#"
services:
  db:
    image: postgres:16
  cache:
    image: redis:7
"#,
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.databases.contains(&"PostgreSQL".to_string()));
    assert!(stack.databases.contains(&"Redis".to_string()));
}

#[test]
fn detect_empty_dir_returns_empty_stack() {
    let tmp = setup_temp_repo();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.is_empty());
    assert!(stack.frameworks.is_empty());
    assert!(stack.databases.is_empty());
    assert!(stack.test_tools.is_empty());
    assert!(stack.build_tools.is_empty());
}

#[test]
fn detect_multiple_languages() {
    let tmp = setup_temp_repo();
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();
    std::fs::write(tmp.path().join("go.mod"), "module a\n").unwrap();
    std::fs::write(tmp.path().join("pyproject.toml"), "[project]\n").unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.languages.contains(&"Rust".to_string()));
    assert!(stack.languages.contains(&"Go".to_string()));
    assert!(stack.languages.contains(&"Python".to_string()));
    assert_eq!(stack.languages.len(), 3);
}

#[test]
fn detect_github_actions_and_makefile() {
    let tmp = setup_temp_repo();
    std::fs::create_dir_all(tmp.path().join(".github/workflows")).unwrap();
    std::fs::write(tmp.path().join("Makefile"), "all:\n\techo hello\n").unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.build_tools.contains(&"GitHub Actions".to_string()));
    assert!(stack.build_tools.contains(&"Make".to_string()));
}

#[test]
fn detect_rust_frameworks_axum_actix() {
    let tmp = setup_temp_repo();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        r#"
[package]
name = "web"

[dependencies]
axum = "0.7"
"#,
    )
    .unwrap();

    let stack = detect_tech_stack(tmp.path());
    assert!(stack.frameworks.contains(&"Axum".to_string()));
}

// ─── generate_conventions tests ───

#[test]
fn generate_for_rust_includes_error_handling() {
    let stack = autodev::cli::convention::TechStack {
        languages: vec!["Rust".to_string()],
        ..Default::default()
    };

    let files = generate_conventions(&stack);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    assert!(paths.contains(&".claude/rules/rust-error-handling.md"));
    assert!(paths.contains(&".claude/rules/rust-project-structure.md"));
    assert!(paths.contains(&".claude/rules/rust-testing.md"));
    assert!(paths.contains(&".claude/rules/rust-clippy.md"));
}

#[test]
fn generate_for_typescript_includes_type_strategy() {
    let stack = autodev::cli::convention::TechStack {
        languages: vec!["TypeScript".to_string()],
        ..Default::default()
    };

    let files = generate_conventions(&stack);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    assert!(paths.contains(&".claude/rules/typescript-type-strategy.md"));
    assert!(paths.contains(&".claude/rules/typescript-project-structure.md"));
    assert!(paths.contains(&".claude/rules/typescript-testing.md"));
    assert!(paths.contains(&".claude/rules/typescript-linting.md"));
}

#[test]
fn generate_always_includes_common_git_workflow() {
    // Even with empty stack, common conventions should be included
    let stack = autodev::cli::convention::TechStack::default();

    let files = generate_conventions(&stack);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    assert!(paths.contains(&".claude/rules/git-workflow.md"));
    assert!(paths.contains(&".claude/rules/code-review.md"));
}

// ─── bootstrap tests ───

#[test]
fn bootstrap_apply_writes_files_to_disk() {
    let tmp = setup_temp_repo();
    let stack = autodev::cli::convention::TechStack {
        languages: vec!["Rust".to_string()],
        ..Default::default()
    };

    let result = bootstrap(tmp.path(), &stack, true).unwrap();

    assert!(!result.files_written.is_empty());
    // Verify files actually exist
    for f in &result.files_written {
        assert!(tmp.path().join(f).exists(), "expected file to exist: {f}");
    }
}

#[test]
fn bootstrap_dry_run_does_not_write_files() {
    let tmp = setup_temp_repo();
    let stack = autodev::cli::convention::TechStack {
        languages: vec!["Rust".to_string()],
        ..Default::default()
    };

    let result = bootstrap(tmp.path(), &stack, false).unwrap();

    assert!(!result.files_written.is_empty());
    // Verify files do NOT exist (dry-run)
    for f in &result.files_written {
        assert!(
            !tmp.path().join(f).exists(),
            "expected file to NOT exist in dry-run: {f}"
        );
    }
}

#[test]
fn bootstrap_skips_existing_files() {
    let tmp = setup_temp_repo();
    let rules_dir = tmp.path().join(".claude/rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("git-workflow.md"), "custom content").unwrap();

    let stack = autodev::cli::convention::TechStack {
        languages: vec!["Rust".to_string()],
        ..Default::default()
    };

    let result = bootstrap(tmp.path(), &stack, true).unwrap();

    assert!(result
        .files_skipped
        .contains(&".claude/rules/git-workflow.md".to_string()));
    // Verify existing file was NOT overwritten
    let content = std::fs::read_to_string(rules_dir.join("git-workflow.md")).unwrap();
    assert_eq!(content, "custom content");
}

// ─── format tests ───

#[test]
fn format_tech_stack_empty_shows_no_detection() {
    let stack = autodev::cli::convention::TechStack::default();
    let output = format_tech_stack(&stack);
    assert!(output.contains("no technology stack detected"));
}

#[test]
fn format_tech_stack_shows_languages() {
    let stack = autodev::cli::convention::TechStack {
        languages: vec!["Rust".to_string(), "Go".to_string()],
        ..Default::default()
    };
    let output = format_tech_stack(&stack);
    assert!(output.contains("Rust, Go"));
}

#[test]
fn format_bootstrap_dry_run_shows_would_create() {
    let result = autodev::cli::convention::BootstrapResult {
        files_written: vec![".claude/rules/test.md".to_string()],
        files_skipped: vec![],
    };
    let output = format_bootstrap_result(&result, true);
    assert!(output.contains("[would create]"));
    assert!(output.contains("Dry-run"));
}

#[test]
fn format_bootstrap_apply_shows_created() {
    let result = autodev::cli::convention::BootstrapResult {
        files_written: vec![".claude/rules/test.md".to_string()],
        files_skipped: vec![".claude/rules/existing.md".to_string()],
    };
    let output = format_bootstrap_result(&result, false);
    assert!(output.contains("[created]"));
    assert!(output.contains("[skipped"));
}
