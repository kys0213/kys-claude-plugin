mod code_style_mocks;

use atelier::code_style::commands::{check, uninstall};
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, langs: &[&str], apps: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    uninstall::run(&deps(fs, &clock), &paths(), &select(langs, apps))
}

fn check_exit(fs: &MemFs) -> i32 {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &select(&[], &[]))
        .unwrap()
        .exit_code()
}

#[test]
fn language_is_removed_with_backup_and_the_rest_stays() {
    let fs = MemFs::installed_with(&["go", "rust"], &["cli"]);

    let report = run(&fs, &["rust"], &[]).unwrap();

    let rust_bak = format!("{}.bak-{TS}", copy("rust", "rust-naming.md"));
    assert_eq!(
        report.render(),
        format!(
            "removed: {} (backup: {rust_bak})\n→ 0 installed, 0 synced, 1 removed, 0 unchanged, 0 excluded\n",
            copy("rust", "rust-naming.md")
        )
    );
    assert_eq!(fs.content(&rust_bak), Some(body("rust", "rust-naming.md")));
    assert_eq!(
        fs.content(&record_path()),
        Some(full_record(&["go"], &["cli"]))
    );
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn app_is_removed_and_dropped_from_record() {
    let fs = MemFs::installed_with(&["go"], &["cli"]);

    run(&fs, &[], &["cli"]).unwrap();

    assert_eq!(fs.content(&copy("cli", "cli-output.md")), None);
    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"], &[])));
}

#[test]
fn last_language_removes_base_apps_and_the_record() {
    let fs = MemFs::installed_with(&["go"], &["cli"]);

    let report = run(&fs, &["go"], &[]).unwrap();

    assert!(report
        .render()
        .ends_with("→ 0 installed, 0 synced, 5 removed, 0 unchanged, 0 excluded\n"));
    for group in ["go", "base", "cli"] {
        for file in rules_of(group) {
            assert_eq!(fs.content(&copy(group, file)), None);
        }
    }
    assert_eq!(fs.content(&record_path()), None);
}

#[test]
fn language_not_installed_is_an_error_and_changes_nothing() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &["rust"], &[]).unwrap_err();

    assert_eq!(err, "language not installed: rust (installed: go)");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn app_not_installed_is_an_error() {
    let fs = MemFs::installed(&["go"]);

    let err = run(&fs, &[], &["cli"]).unwrap_err();

    assert_eq!(err, "app not installed: cli (installed: none)");
}

#[test]
fn uninstall_requires_a_lang_or_app() {
    let fs = MemFs::installed(&["go"]);
    let err = run(&fs, &[], &[]).unwrap_err();
    assert_eq!(err, "at least one --lang or --app is required");
}
