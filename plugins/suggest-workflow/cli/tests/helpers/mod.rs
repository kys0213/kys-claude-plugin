#![allow(dead_code)]

use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create an isolated test environment with fixture JSONL files.
///
/// Sets up a temp directory structure that mimics the real `~/.claude/projects/` layout.
/// The returned `TempDir` must be passed to `cli_with_home()` as HOME override.
///
/// Returns (TempDir, project_path) where project_path is the "project" directory.
pub fn setup_project(fixtures: &[&str]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();

    // Create a "project" directory inside the temp dir
    let project_dir = tmp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Encode the project path the same way the CLI does
    let canonical = project_dir.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));

    // Create sessions dir at $tmp/.claude/projects/{encoded}/
    let sessions_dir = tmp.path().join(".claude").join("projects").join(&encoded);
    std::fs::create_dir_all(&sessions_dir).unwrap();

    // Copy fixtures
    for fixture in fixtures {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sessions")
            .join(fixture);
        let dest = sessions_dir.join(fixture);
        std::fs::copy(&src, &dest).unwrap();
    }

    (tmp, project_dir)
}

/// Build a CLI command with HOME overridden to the temp directory.
#[allow(deprecated)]
pub fn cli_with_home(tmp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("suggest-workflow").unwrap();
    cmd.env("HOME", tmp.path());
    cmd
}

/// Get the fixture path for custom query SQL files.
pub fn fixture_sql(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/custom_queries")
        .join(name)
}
