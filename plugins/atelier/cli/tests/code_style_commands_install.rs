mod code_style_mocks;

use atelier::code_style::commands::install;
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, langs: &[&str], apps: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    install::run(&deps(fs, &clock), &paths(), &select(langs, apps))
}

#[test]
fn fresh_install_copies_base_and_language_creating_directories() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["go"], &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "installed: {}\ninstalled: {}\ninstalled: {}\ninstalled: {}\n\
             → 4 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded\n",
            copy("base", "cohesion.md"),
            copy("base", "comments.md"),
            copy("go", "go-errors.md"),
            copy("go", "go-naming.md"),
        )
    );
    for group in ["go", "base"] {
        for file in rules_of(group) {
            assert_eq!(fs.content(&copy(group, file)), Some(body(group, file)));
        }
    }
    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
    assert_eq!(fs.content(&copy("cli", "cli-output.md")), None);
}

#[test]
fn app_is_installed_beside_the_language() {
    let fs = MemFs::with_sources();

    run(&fs, &["go"], &["cli"]).unwrap();

    assert_eq!(
        fs.content(&copy("cli", "cli-output.md")),
        Some(body("cli", "cli-output.md"))
    );
    assert_eq!(
        fs.content(&record_path()),
        Some(full_record(&["go"], &["cli"]))
    );
}

#[test]
fn fresh_install_writes_the_install_record() {
    let fs = MemFs::with_sources();

    run(&fs, &["go"], &[]).unwrap();

    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"], &[])));
}

#[test]
fn reinstall_in_sync_changes_nothing() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["go"], &[]).unwrap();

    assert_eq!(
        report.render(),
        "→ 0 installed, 0 synced, 0 removed, 4 unchanged, 0 excluded\n"
    );
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn install_over_edited_copy_backs_it_up_first() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");

    let report = run(&fs, &["go"], &[]).unwrap();

    let backup = format!("{}.bak-{TS}", copy("go", "go-naming.md"));
    assert!(report.render().contains(&format!(
        "synced: {} (backup: {backup})\n",
        copy("go", "go-naming.md")
    )));
    assert_eq!(fs.content(&backup).as_deref(), Some("locally edited\n"));
    assert_eq!(
        fs.content(&copy("go", "go-naming.md")),
        Some(body("go", "go-naming.md"))
    );
}

#[test]
fn several_langs_install_base_once() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["rust", "go", "rust"], &[]).unwrap();

    assert!(report
        .render()
        .ends_with("→ 5 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded\n"));
}

#[test]
fn adding_a_language_keeps_what_is_installed_and_records_both() {
    let fs = MemFs::installed_with(&["go"], &["cli"]);

    let report = run(&fs, &["rust"], &[]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "installed: {}\n→ 1 installed, 0 synced, 0 removed, 2 unchanged, 0 excluded\n",
            copy("rust", "rust-naming.md")
        )
    );
    assert_eq!(
        fs.content(&record_path()),
        Some(full_record(&["go", "rust"], &["cli"]))
    );
}

#[test]
fn excluded_rule_stays_out_on_install() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&copy("base", "comments.md"));

    let report = run(&fs, &["go"], &[]).unwrap();

    assert_eq!(
        report.render(),
        "→ 0 installed, 0 synced, 0 removed, 3 unchanged, 1 excluded\n"
    );
    assert_eq!(fs.content(&copy("base", "comments.md")), None);
}

#[test]
fn install_requires_a_lang() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &[], &["cli"]).unwrap_err();
    assert_eq!(err, "at least one --lang is required (one of: go, rust)");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn unknown_lang_installs_nothing() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["go", "cobol"], &[]).unwrap_err();
    assert_eq!(err, "unknown language: cobol — expected one of: go, rust");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn unknown_app_installs_nothing() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["go"], &["server"]).unwrap_err();
    assert_eq!(err, "unknown app: server — expected one of: cli, library");
    assert_eq!(fs.mutation_count(), 0);
}
