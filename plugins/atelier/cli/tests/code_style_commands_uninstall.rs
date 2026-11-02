mod code_style_mocks;

use atelier::code_style::commands::{check, uninstall};
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, names: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    uninstall::run(&deps(fs, &clock), &paths(), &langs(names))
}

fn check_exit(fs: &MemFs) -> i32 {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &[])
        .unwrap()
        .exit_code()
}

#[test]
fn language_is_removed_with_backup_and_common_rerendered_for_the_rest() {
    let fs = MemFs::installed(&["go", "rust"]);

    let report = run(&fs, &["rust"]).unwrap();

    let rust_bak = format!("{}.bak-{TS}", copy("rust", "rust-naming.md"));
    assert!(report.render().contains(&format!(
        "removed: {} (backup: {rust_bak})\n",
        copy("rust", "rust-naming.md")
    )));
    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
    assert_eq!(fs.content(&rust_bak), Some(body("rust", "rust-naming.md")));
    assert_eq!(
        fs.content(&copy("common", "cohesion.md")),
        Some(rendered("cohesion.md", &["go"]))
    );
    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"])));
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn excluded_common_rule_stays_out_when_common_is_rerendered() {
    let fs = MemFs::installed(&["go", "rust"]);
    fs.delete(&copy("common", "comments.md"));

    run(&fs, &["rust"]).unwrap();

    assert_eq!(fs.content(&copy("common", "comments.md")), None);
    assert!(fs
        .content(&record_path())
        .unwrap()
        .contains("\"common/comments.md\""));
}

#[test]
fn last_language_removes_common_and_the_record() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["go"]).unwrap();

    assert!(report
        .render()
        .ends_with("→ 0 installed, 0 synced, 4 removed, 0 unchanged, 0 excluded\n"));
    for group in ["go", "common"] {
        for file in rules_of(group) {
            assert_eq!(fs.content(&copy(group, file)), None);
        }
    }
    assert_eq!(fs.content(&record_path()), None);
}

#[test]
fn language_not_installed_is_an_error_and_changes_nothing() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &["rust"]).unwrap_err();

    assert_eq!(err, "language not installed: rust (installed: go)");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn uninstall_requires_a_lang() {
    let fs = MemFs::installed(&["go"]);
    let err = run(&fs, &[]).unwrap_err();
    assert_eq!(err, "at least one --lang is required (installed: go)");
}
