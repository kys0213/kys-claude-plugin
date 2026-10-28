use assert_cmd::Command;
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
    fn new() -> Self {
        let fx = Fixture {
            tmp: tempfile::TempDir::new().unwrap(),
        };
        for (group, file) in [
            ("go", "go-errors.md"),
            ("go", "go-naming.md"),
            ("rust", "rust-naming.md"),
            ("common", "comments.md"),
        ] {
            write(&fx.source(group, file), &format!("# {group}/{file}\n"));
        }
        std::fs::create_dir_all(fx.project_dir()).unwrap();
        fx
    }

    fn at(&self, rel: &str) -> String {
        self.tmp.path().join(rel).to_str().unwrap().to_string()
    }

    fn plugin_root(&self) -> String {
        self.at("plugin")
    }

    fn project_dir(&self) -> String {
        self.at("project")
    }

    fn source(&self, group: &str, file: &str) -> String {
        self.at(&format!("plugin/rules/code-style/{group}/{file}"))
    }

    fn copy(&self, group: &str, file: &str) -> String {
        self.at(&format!(
            "project/.claude/rules/atelier/code-style/{group}/{file}"
        ))
    }

    fn cmd(&self, sub: &str, langs: &[&str]) -> Command {
        let mut cmd = atelier();
        cmd.args(["code-style", sub, "--plugin-root", &self.plugin_root()])
            .args(["--project-dir", &self.project_dir()]);
        for lang in langs {
            cmd.args(["--lang", lang]);
        }
        cmd
    }
}

#[test]
fn code_style_bare_prints_help_and_exits_zero() {
    atelier()
        .arg("code-style")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn install_go_installs_common_and_check_is_ok() {
    let fx = Fixture::new();

    fx.cmd("install", &["go"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "installed: {}",
            fx.copy("common", "comments.md")
        )))
        .stdout(predicate::str::contains(
            "→ 3 installed, 0 synced, 0 removed, 0 unchanged",
        ));
    assert!(Path::new(&fx.copy("go", "go-naming.md")).is_file());
    assert!(!Path::new(&fx.copy("rust", "rust-naming.md")).exists());

    fx.cmd("check", &[]).assert().success().stdout(
        "code-style/common/comments.md=OK\n\
         code-style/go/go-errors.md=OK\n\
         code-style/go/go-naming.md=OK\n\
         → 3 checked, 0 drifted, 0 new, 0 orphaned\n",
    );
}

#[test]
fn project_dir_defaults_to_cwd() {
    let fx = Fixture::new();
    atelier()
        .current_dir(fx.project_dir())
        .args(["code-style", "install", "--plugin-root", &fx.plugin_root()])
        .args(["--lang", "rust"])
        .assert()
        .success();
    assert!(Path::new(&fx.copy("rust", "rust-naming.md")).is_file());
}

#[test]
fn modified_source_is_drifted_then_update_restores_ok_with_backup() {
    let fx = Fixture::new();
    fx.cmd("install", &["go"]).assert().success();
    write(&fx.source("go", "go-naming.md"), "# go naming v2\n");

    fx.cmd("check", &["go"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(format!(
            "code-style/go/go-naming.md=DRIFTED ({})",
            fx.copy("go", "go-naming.md")
        )));
    fx.cmd("update", &[])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "synced: {} (backup: ",
            fx.copy("go", "go-naming.md")
        )));
    fx.cmd("check", &[]).assert().success();

    assert_eq!(
        std::fs::read_to_string(fx.copy("go", "go-naming.md")).unwrap(),
        "# go naming v2\n"
    );
    let backups: Vec<_> = std::fs::read_dir(fx.at("project/.claude/rules/atelier/code-style/go"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("go-naming.md.bak-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(backups[0].path()).unwrap(),
        "# go/go-naming.md\n"
    );
}

#[test]
fn new_source_file_is_new_then_update_installs_it() {
    let fx = Fixture::new();
    fx.cmd("install", &["go"]).assert().success();
    write(&fx.source("common", "testing.md"), "# testing\n");

    fx.cmd("check", &[])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("code-style/common/testing.md=NEW"));
    fx.cmd("update", &[])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "installed: {}",
            fx.copy("common", "testing.md")
        )));
    fx.cmd("check", &[]).assert().success();
}

#[test]
fn removed_source_is_orphaned_then_update_removes_copy() {
    let fx = Fixture::new();
    fx.cmd("install", &["go"]).assert().success();
    std::fs::remove_file(fx.source("go", "go-errors.md")).unwrap();

    fx.cmd("check", &[])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "code-style/go/go-errors.md=ORPHANED",
        ))
        .stdout(predicate::str::contains("1 orphaned"));
    fx.cmd("update", &[])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "removed: {} (backup: ",
            fx.copy("go", "go-errors.md")
        )));
    assert!(!Path::new(&fx.copy("go", "go-errors.md")).exists());
    fx.cmd("check", &[]).assert().success();
}

#[test]
fn unknown_lang_exits_two() {
    let fx = Fixture::new();
    for sub in ["install", "update", "check"] {
        fx.cmd(sub, &["cobol"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains(
                "Error: unknown language: cobol — expected one of: go, rust",
            ));
    }
    assert!(!Path::new(&fx.at("project/.claude")).exists());
}

#[test]
fn install_without_lang_exits_two() {
    let fx = Fixture::new();
    fx.cmd("install", &[])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("at least one --lang is required"));
}

#[test]
fn check_with_nothing_installed_exits_two() {
    let fx = Fixture::new();
    fx.cmd("check", &[])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no code-style language installed"));
}
