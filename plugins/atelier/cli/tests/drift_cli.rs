//! End-to-end black-box tests for `atelier drift ...` — the routing layer and
//! the shell-compatible exit-code contract (0 no drift / 1 drift / 2 error),
//! exercised against the real binary with TempDir fixtures.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

const BEGIN: &str = "<!-- [coding-style:begin] DO NOT REMOVE THIS LINE -->";
const END: &str = "<!-- [coding-style:end] DO NOT REMOVE THIS LINE -->";
const RULES_BODY: &str = "# Agent design principles\n";

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn block(body: &str) -> String {
    format!("{BEGIN}\n{body}\n{END}\n")
}

/// Builds a plugin root with both source files; the template block body is
/// `tpl_body`. Returns (plugin_root, claude_md, project_dir) paths as strings.
struct Fixture {
    tmp: tempfile::TempDir,
}

impl Fixture {
    fn new(tpl_body: &str) -> Self {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("plugin");
        write(
            &root.join("templates/claude-md/CLAUDE.md"),
            &block(tpl_body),
        );
        write(&root.join("rules/agent-design-principles.md"), RULES_BODY);
        Fixture { tmp }
    }

    fn plugin_root(&self) -> String {
        self.tmp.path().join("plugin").to_str().unwrap().to_string()
    }

    fn claude_md(&self) -> String {
        self.tmp
            .path()
            .join("home/.claude/CLAUDE.md")
            .to_str()
            .unwrap()
            .to_string()
    }

    fn project_dir(&self) -> String {
        self.tmp.path().join("proj").to_str().unwrap().to_string()
    }

    fn install(&self, claude_md_body: &str) {
        write(
            &self.tmp.path().join("home/.claude/CLAUDE.md"),
            &format!("# mine\n{}tail\n", block(claude_md_body)),
        );
        write(
            &self
                .tmp
                .path()
                .join("proj/.claude/rules/agent-design-principles.md"),
            RULES_BODY,
        );
    }

    fn check(&self) -> Command {
        let mut cmd = atelier();
        cmd.args([
            "drift",
            "check",
            "--plugin-root",
            &self.plugin_root(),
            "--claude-md",
            &self.claude_md(),
            "--project-dir",
            &self.project_dir(),
        ]);
        cmd
    }
}

#[test]
fn drift_bare_prints_help_and_exits_zero() {
    // Bare `atelier drift` prints help + exit 0, like the other subsystems.
    atelier()
        .arg("drift")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn drift_check_in_sync_exits_zero() {
    // Both artifacts matching their sources → OK lines, summary, exit 0.
    let fx = Fixture::new("shared body");
    fx.install("shared body");
    fx.check()
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-md-coding-style-block=OK"))
        .stdout(predicate::str::contains(
            "→ 2 checked, 0 drifted, 0 missing",
        ));
}

#[test]
fn drift_check_drifted_exits_one() {
    // A stale block is a finding (exit 1), not an error.
    let fx = Fixture::new("new body");
    fx.install("old body");
    fx.check().assert().code(1).stdout(predicate::str::contains(
        "claude-md-coding-style-block=DRIFTED",
    ));
}

#[test]
fn drift_sync_then_check_roundtrip() {
    // sync brings a drifted block back in line: the follow-up check exits 0
    // and a timestamped backup of the pre-sync file exists.
    let fx = Fixture::new("new body");
    fx.install("old body");
    atelier()
        .args([
            "drift",
            "sync",
            "--target",
            "claude-md",
            "--plugin-root",
            &fx.plugin_root(),
            "--claude-md",
            &fx.claude_md(),
            "--project-dir",
            &fx.project_dir(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("synced: coding-style block in"));
    fx.check().assert().success();

    let backups: Vec<_> = std::fs::read_dir(fx.tmp.path().join("home/.claude"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("CLAUDE.md.bak-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(backups[0].path()).unwrap(),
        format!("# mine\n{}tail\n", block("old body"))
    );
}

#[test]
fn drift_sync_bad_target_is_clap_usage_error() {
    // --target is a closed enum; an unknown value fails at the clap boundary.
    let fx = Fixture::new("body");
    atelier()
        .args([
            "drift",
            "sync",
            "--target",
            "bogus",
            "--plugin-root",
            &fx.plugin_root(),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn drift_check_missing_plugin_source_exits_two() {
    // An empty plugin root cannot be judged against — error contract, exit 2.
    let tmp = tempfile::TempDir::new().unwrap();
    atelier()
        .args([
            "drift",
            "check",
            "--plugin-root",
            tmp.path().to_str().unwrap(),
            "--claude-md",
            tmp.path().join("CLAUDE.md").to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Error: plugin source file not found:",
        ));
}
