mod drift_mocks;

use assert_cmd::Command;
use atelier::drift::core::types::{
    POLICY_RULES, RULES_COPY_NAME, TEMPLATE_CLAUDE_MD_REL, TEMPLATE_POLICY_RULES_DIR_REL,
    TEMPLATE_RULES_REL,
};
use drift_mocks::{block, RULES_BODY, USER_RULE_BODY};
use predicates::prelude::*;
use std::path::Path;

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn write(path: &str, content: &str) {
    let path = Path::new(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

struct Fixture {
    tmp: tempfile::TempDir,
}

impl Fixture {
    fn new(tpl_body: &str) -> Self {
        let fx = Fixture {
            tmp: tempfile::TempDir::new().unwrap(),
        };
        write(
            &fx.at(&format!("plugin/{TEMPLATE_CLAUDE_MD_REL}")),
            &block(tpl_body),
        );
        write(&fx.at(&format!("plugin/{TEMPLATE_RULES_REL}")), RULES_BODY);
        for name in POLICY_RULES {
            write(
                &fx.at(&format!("plugin/{TEMPLATE_POLICY_RULES_DIR_REL}/{name}")),
                USER_RULE_BODY,
            );
        }
        fx
    }

    fn at(&self, rel: &str) -> String {
        self.tmp.path().join(rel).to_str().unwrap().to_string()
    }

    fn plugin_root(&self) -> String {
        self.at("plugin")
    }

    fn claude_md(&self) -> String {
        self.at("home/.claude/CLAUDE.md")
    }

    fn user_rules_dir(&self) -> String {
        self.at("home/.claude/rules/atelier")
    }

    fn install(&self, claude_md_body: &str) {
        write(
            &self.claude_md(),
            &format!("# mine\n{}tail\n", block(claude_md_body)),
        );
        write(
            &format!("{}/{RULES_COPY_NAME}", self.user_rules_dir()),
            RULES_BODY,
        );
        for name in POLICY_RULES {
            write(&format!("{}/{name}", self.user_rules_dir()), USER_RULE_BODY);
        }
    }

    fn path_flags(&self) -> [String; 6] {
        [
            "--plugin-root".into(),
            self.plugin_root(),
            "--claude-md".into(),
            self.claude_md(),
            "--user-rules-dir".into(),
            self.user_rules_dir(),
        ]
    }

    fn check(&self) -> Command {
        let mut cmd = atelier();
        cmd.args(["drift", "check"]).args(self.path_flags());
        cmd
    }

    fn sync(&self, target: &str) -> Command {
        let mut cmd = atelier();
        cmd.args(["drift", "sync", "--target", target])
            .args(self.path_flags());
        cmd
    }
}

#[test]
fn drift_bare_prints_help_and_exits_zero() {
    atelier()
        .arg("drift")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn drift_check_in_sync_exits_zero() {
    let fx = Fixture::new("shared body");
    fx.install("shared body");
    fx.check()
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-md-coding-style-block=OK"))
        .stdout(predicate::str::contains(format!(
            "user-rules/{}=OK",
            POLICY_RULES[0]
        )))
        .stdout(predicate::str::contains(format!(
            "→ {} checked, 0 drifted, 0 new, 0 missing",
            2 + POLICY_RULES.len()
        )));
}

#[test]
fn drift_sync_user_rules_then_check_roundtrip() {
    let fx = Fixture::new("body");
    fx.install("body");
    write(
        &format!("{}/{}", fx.user_rules_dir(), POLICY_RULES[0]),
        "locally edited\n",
    );
    fx.sync("user-rules")
        .assert()
        .success()
        .stdout(predicate::str::contains("synced: "));
    fx.check().assert().success();
}

#[test]
fn drift_new_manifest_file_check_sync_roundtrip() {
    let fx = Fixture::new("body");
    fx.install("body");
    let added = POLICY_RULES[POLICY_RULES.len() - 1];
    let copy = format!("{}/{added}", fx.user_rules_dir());
    std::fs::remove_file(&copy).unwrap();

    fx.check()
        .assert()
        .code(1)
        .stdout(predicate::str::contains(format!("user-rules/{added}=NEW")));
    fx.sync("user-rules")
        .args(["--name", added])
        .assert()
        .success()
        .stdout(format!("installed: {copy}\n"));
    fx.check().assert().success();
}

#[test]
fn drift_check_drifted_is_a_finding_not_an_error() {
    let fx = Fixture::new("new body");
    fx.install("old body");
    fx.check().assert().code(1).stdout(predicate::str::contains(
        "claude-md-coding-style-block=DRIFTED",
    ));
}

#[test]
fn drift_sync_then_check_roundtrip_leaves_backup() {
    let fx = Fixture::new("new body");
    fx.install("old body");
    fx.sync("claude-md")
        .assert()
        .success()
        .stdout(predicate::str::contains("synced: coding-style block in"));
    fx.check().assert().success();

    let backups: Vec<_> = std::fs::read_dir(fx.at("home/.claude"))
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
