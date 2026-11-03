mod code_style_mocks;

use atelier::code_style::commands::check;
use atelier::code_style::core::types::CheckReport;
use code_style_mocks::*;

fn run(fs: &MemFs, langs: &[&str], apps: &[&str]) -> Result<CheckReport, String> {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &select(langs, apps))
}

fn ok_lines(group: &str) -> String {
    rules_of(group)
        .iter()
        .map(|file| format!("code-style/{group}/{file}=OK\n"))
        .collect()
}

#[test]
fn installed_in_sync_reports_ok_with_base_and_exit_zero() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["go"], &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}→ 4 checked, 0 drifted, 0 new, 0 orphaned, 0 excluded\n",
            ok_lines("base"),
            ok_lines("go")
        )
    );
    assert_eq!(report.exit_code(), 0);
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn omitted_selection_checks_every_installed_language_and_app() {
    let fs = MemFs::installed_with(&["go", "rust"], &["cli"]);

    let report = run(&fs, &[], &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}{}{}→ 6 checked, 0 drifted, 0 new, 0 orphaned, 0 excluded\n",
            ok_lines("base"),
            ok_lines("go"),
            ok_lines("rust"),
            ok_lines("cli")
        )
    );
}

#[test]
fn omitted_selection_skips_groups_not_installed() {
    let fs = MemFs::installed(&["rust"]);

    let report = run(&fs, &[], &[]).unwrap();

    assert!(!report.render().contains("code-style/go/"));
    assert!(!report.render().contains("code-style/cli/"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn explicit_app_limits_check_to_that_app_and_base() {
    let fs = MemFs::installed_with(&["go"], &["cli"]);

    let report = run(&fs, &[], &["cli"]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}→ 3 checked, 0 drifted, 0 new, 0 orphaned, 0 excluded\n",
            ok_lines("base"),
            ok_lines("cli")
        )
    );
}

#[test]
fn edited_copy_is_drifted_and_exit_one() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");

    let report = run(&fs, &["go"], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-naming.md=DRIFTED ({})\n",
        copy("go", "go-naming.md")
    )));
    assert!(report
        .render()
        .ends_with("→ 4 checked, 1 drifted, 0 new, 0 orphaned, 0 excluded\n"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn non_utf8_copy_is_drifted() {
    let fs = MemFs::installed(&["go"]);
    fs.insert_bytes(&copy("base", "comments.md"), &[0xff, 0xfe]);

    let report = run(&fs, &["go"], &[]).unwrap();

    assert!(report
        .render()
        .contains("code-style/base/comments.md=DRIFTED (not valid UTF-8)\n"));
}

#[test]
fn source_without_copy_or_record_entry_is_new() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&source("go", "go-testing.md"), "# testing\n");

    let report = run(&fs, &["go"], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-testing.md=NEW ({})\n",
        copy("go", "go-testing.md")
    )));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn recorded_rule_whose_copy_was_deleted_is_excluded_and_exit_zero() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&copy("base", "comments.md"));

    let report = run(&fs, &[], &[]).unwrap();

    assert!(report
        .render()
        .contains("code-style/base/comments.md=EXCLUDED\n"));
    assert!(report
        .render()
        .ends_with("→ 4 checked, 0 drifted, 0 new, 0 orphaned, 1 excluded\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn deleted_copy_without_record_is_new() {
    let fs = MemFs::installed_without_record(&["go"]);
    fs.delete(&copy("base", "comments.md"));

    let report = run(&fs, &[], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/base/comments.md=NEW ({})\n",
        copy("base", "comments.md")
    )));
}

#[test]
fn install_without_record_counts_directories_as_languages() {
    let fs = MemFs::installed_without_record(&["go"]);

    let report = run(&fs, &[], &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "{}{}→ 4 checked, 0 drifted, 0 new, 0 orphaned, 0 excluded\n",
            ok_lines("base"),
            ok_lines("go")
        )
    );
}

#[test]
fn old_common_directory_without_record_is_orphaned() {
    let fs = MemFs::installed_without_record(&["go"]);
    fs.insert(&copy("common", "cohesion.md"), "# old\n");

    let report = run(&fs, &[], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/common/cohesion.md=ORPHANED ({})\n",
        copy("common", "cohesion.md")
    )));
}

#[test]
fn copy_without_source_is_orphaned() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&source("go", "go-errors.md"));

    let report = run(&fs, &["go"], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/go/go-errors.md=ORPHANED ({})\n",
        copy("go", "go-errors.md")
    )));
    assert!(report
        .render()
        .ends_with("→ 4 checked, 0 drifted, 0 new, 1 orphaned, 0 excluded\n"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn backups_and_non_markdown_files_are_not_rules() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&format!("{}.bak-{TS}", copy("go", "go-naming.md")), "old\n");
    fs.insert(&source("go", "README.txt"), "notes\n");

    let report = run(&fs, &["go"], &[]).unwrap();

    assert_eq!(report.exit_code(), 0);
    assert!(!report.render().contains(".bak-"));
    assert!(!report.render().contains("README.txt"));
}

#[test]
fn directory_not_in_the_record_is_left_alone() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("team", "our-rule.md"), "# ours\n");

    let report = run(&fs, &[], &[]).unwrap();

    assert!(!report.render().contains("code-style/team/"));
}

#[test]
fn explicit_lang_not_installed_reports_its_files_new() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["rust"], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/rust/rust-naming.md=NEW ({})\n",
        copy("rust", "rust-naming.md")
    )));
    assert!(!report.render().contains("code-style/go/"));
}

#[test]
fn installed_language_removed_from_plugin_is_all_orphaned() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("python", "py-naming.md"), "# py\n");
    fs.insert(&record_path(), &record_json(&["go", "python"], &[], &[]));

    let report = run(&fs, &[], &[]).unwrap();

    assert!(report.render().contains(&format!(
        "code-style/python/py-naming.md=ORPHANED ({})\n",
        copy("python", "py-naming.md")
    )));
}

#[test]
fn malformed_record_is_an_error() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&record_path(), "{\"langs\": \"go\"}\n");

    let err = run(&fs, &[], &[]).unwrap_err();

    assert!(err.starts_with(&format!("{}: ", record_path())), "{err}");
}

#[test]
fn unknown_lang_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["cobol"], &[]).unwrap_err();
    assert_eq!(err, "unknown language: cobol — expected one of: go, rust");
}

#[test]
fn unknown_app_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["go"], &["server"]).unwrap_err();
    assert_eq!(err, "unknown app: server — expected one of: cli, library");
}

#[test]
fn an_app_is_not_a_lang_value() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["cli"], &[]).unwrap_err();
    assert_eq!(err, "unknown language: cli — expected one of: go, rust");
}

#[test]
fn base_is_not_a_lang_value() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["base"], &[]).unwrap_err();
    assert_eq!(err, "unknown language: base — expected one of: go, rust");
}

#[test]
fn name_shared_by_a_language_and_an_app_is_an_error() {
    let fs = MemFs::with_sources();
    fs.insert(
        &format!("{PLUGIN_ROOT}/rules/code-style/app/go/x.md"),
        "# x\n",
    );

    let err = run(&fs, &["go"], &[]).unwrap_err();

    assert_eq!(
        err,
        "go is both a language and an app in the plugin sources"
    );
}

#[test]
fn nothing_installed_without_selection_is_an_error() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &[], &[]).unwrap_err();
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
    let err = run(&fs, &["go"], &[]).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source directory not found: {PLUGIN_ROOT}/rules/code-style")
    );
}

#[test]
fn missing_base_source_is_an_error() {
    let fs = MemFs::with_sources();
    for file in BASE_RULES {
        fs.delete(&source("base", file));
    }
    let err = run(&fs, &["go"], &[]).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source directory not found: {PLUGIN_ROOT}/rules/code-style/base")
    );
}
