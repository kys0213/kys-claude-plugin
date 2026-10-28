mod code_style_mocks;

use atelier::code_style::commands::check;
use atelier::code_style::core::types::CheckReport;
use code_style_mocks::*;

fn run(fs: &MemFs, names: &[&str]) -> Result<CheckReport, String> {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &langs(names))
}

fn ok_lines(group: &str) -> String {
    rules_of(group)
        .iter()
        .map(|file| format!("code-style/{group}/{file}=OK\n"))
        .collect()
}

#[test]
fn installed_in_sync_reports_ok_with_common_and_exit_zero() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}→ 4 checked, 0 drifted, 0 new, 0 orphaned\n",
            ok_lines("common"),
            ok_lines("go")
        )
    );
    assert_eq!(report.exit_code(), 0);
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn omitted_lang_checks_every_installed_language() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("rust");
    fs.install_group("common");

    let report = run(&fs, &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}{}→ 5 checked, 0 drifted, 0 new, 0 orphaned\n",
            ok_lines("common"),
            ok_lines("go"),
            ok_lines("rust")
        )
    );
}

#[test]
fn omitted_lang_skips_languages_not_installed() {
    let fs = MemFs::with_sources();
    fs.install_group("rust");
    fs.install_group("common");

    let report = run(&fs, &[]).unwrap();

    assert!(!report.render().contains("code-style/go/"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn edited_copy_is_drifted_and_exit_one() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");

    let report = run(&fs, &["go"]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-naming.md=DRIFTED ({})\n",
        copy("go", "go-naming.md")
    )));
    assert!(report
        .render()
        .ends_with("→ 4 checked, 1 drifted, 0 new, 0 orphaned\n"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn non_utf8_copy_is_drifted() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs.insert_bytes(&copy("common", "comments.md"), &[0xff, 0xfe]);

    let report = run(&fs, &["go"]).unwrap();

    assert!(report
        .render()
        .contains("code-style/common/comments.md=DRIFTED (not valid UTF-8)\n"));
}

#[test]
fn source_without_copy_is_new() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs.insert(&source("go", "go-testing.md"), "# testing\n");

    let report = run(&fs, &["go"]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-testing.md=NEW ({})\n",
        copy("go", "go-testing.md")
    )));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn copy_without_source_is_orphaned() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs.delete(&source("go", "go-errors.md"));

    let report = run(&fs, &["go"]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-errors.md=ORPHANED ({})\n",
        copy("go", "go-errors.md")
    )));
    assert!(report
        .render()
        .ends_with("→ 4 checked, 0 drifted, 0 new, 1 orphaned\n"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn backups_and_non_markdown_files_are_not_rules() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs.insert(&format!("{}.bak-{TS}", copy("go", "go-naming.md")), "old\n");
    fs.insert(&source("go", "README.txt"), "notes\n");

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(report.exit_code(), 0);
    assert!(!report.render().contains(".bak-"));
    assert!(!report.render().contains("README.txt"));
}

#[test]
fn explicit_lang_not_installed_reports_every_file_new() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");

    let report = run(&fs, &["rust"]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/rust/rust-naming.md=NEW ({})\n",
        copy("rust", "rust-naming.md")
    )));
    assert!(!report.render().contains("code-style/go/"));
}

#[test]
fn installed_language_removed_from_plugin_is_all_orphaned() {
    let fs = MemFs::with_sources();
    fs.install_group("common");
    fs.insert(&copy("python", "py-naming.md"), "# py\n");

    let report = run(&fs, &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/python/py-naming.md=ORPHANED ({})\n",
        copy("python", "py-naming.md")
    )));
}

#[test]
fn unknown_lang_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["cobol"]).unwrap_err();
    assert_eq!(err, "unknown language: cobol — expected one of: go, rust");
}

#[test]
fn common_is_not_a_lang_value() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["common"]).unwrap_err();
    assert_eq!(err, "unknown language: common — expected one of: go, rust");
}

#[test]
fn nothing_installed_without_lang_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &[]).unwrap_err();
    assert_eq!(
        err,
        format!(
            "no code-style language installed in {PROJECT_DIR}/.claude/rules/atelier/code-style — pass --lang or run install"
        )
    );
}

#[test]
fn missing_plugin_source_root_is_an_error() {
    let fs = MemFs::default();
    let err = run(&fs, &["go"]).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source directory not found: {PLUGIN_ROOT}/rules/code-style")
    );
}

#[test]
fn missing_common_source_is_an_error() {
    let fs = MemFs::with_sources();
    for file in COMMON_RULES {
        fs.delete(&source("common", file));
    }
    let err = run(&fs, &["go"]).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source directory not found: {PLUGIN_ROOT}/rules/code-style/common")
    );
}
