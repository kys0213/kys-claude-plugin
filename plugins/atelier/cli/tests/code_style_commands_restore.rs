mod code_style_mocks;

use atelier::code_style::commands::{check, restore};
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, rules: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    restore::run(&deps(fs, &clock), &paths(), &langs(rules))
}

fn check_exit(fs: &MemFs) -> i32 {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &[])
        .unwrap()
        .exit_code()
}

#[test]
fn excluded_common_rule_is_restored_rendered_and_check_is_ok() {
    let fs = MemFs::installed(&["go", "rust"]);
    fs.delete(&copy("common", "comments.md"));

    let report = run(&fs, &["common/comments.md"]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "installed: {}\n→ 1 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded\n",
            copy("common", "comments.md")
        )
    );
    assert_eq!(
        fs.content(&copy("common", "comments.md")),
        Some(rendered("comments.md", &["go", "rust"]))
    );
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn excluded_language_rule_is_restored_verbatim() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&copy("go", "go-naming.md"));

    run(&fs, &["go/go-naming.md"]).unwrap();

    assert_eq!(
        fs.content(&copy("go", "go-naming.md")),
        Some(body("go", "go-naming.md"))
    );
}

#[test]
fn rule_already_present_is_left_as_is() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");

    let report = run(&fs, &["go/go-naming.md"]).unwrap();

    assert_eq!(
        report.render(),
        "→ 0 installed, 0 synced, 0 removed, 1 unchanged, 0 excluded\n"
    );
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn rule_of_a_language_not_installed_is_an_error() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &["rust/rust-naming.md"]).unwrap_err();

    assert_eq!(
        err,
        "rust/rust-naming.md: language rust is not installed (installed: go)"
    );
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn rule_without_source_is_an_error() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &["common/nope.md"]).unwrap_err();

    assert_eq!(err, "unknown rule: common/nope.md");
}

#[test]
fn rule_not_shaped_group_slash_file_is_an_error() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &["comments.md"]).unwrap_err();

    assert_eq!(
        err,
        "invalid rule: comments.md — expected <group>/<file>.md, e.g. common/comments.md"
    );
}

#[test]
fn restore_requires_a_rule() {
    let fs = MemFs::installed(&["go"]);
    let err = run(&fs, &[]).unwrap_err();
    assert_eq!(err, "at least one --rule is required");
}

#[test]
fn nothing_installed_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["common/comments.md"]).unwrap_err();
    assert!(err.starts_with("no code-style language installed"), "{err}");
}
