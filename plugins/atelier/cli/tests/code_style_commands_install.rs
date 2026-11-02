mod code_style_mocks;

use atelier::code_style::commands::install;
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, names: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    install::run(&deps(fs, &clock), &paths(), &langs(names))
}

#[test]
fn fresh_install_copies_language_and_rendered_common_creating_directories() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "installed: {}\ninstalled: {}\ninstalled: {}\ninstalled: {}\n\
             → 4 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded\n",
            copy("common", "cohesion.md"),
            copy("common", "comments.md"),
            copy("go", "go-errors.md"),
            copy("go", "go-naming.md"),
        )
    );
    for group in ["go", "common"] {
        for file in rules_of(group) {
            assert_eq!(
                fs.content(&copy(group, file)),
                Some(expected(group, file, &["go"]))
            );
        }
    }
    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
    assert_eq!(fs.content(&copy("go", "profile.json")), None);
}

#[test]
fn fresh_install_writes_the_install_record() {
    let fs = MemFs::with_sources();

    run(&fs, &["go"]).unwrap();

    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"])));
}

#[test]
fn reinstall_in_sync_changes_nothing() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["go"]).unwrap();

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

    let report = run(&fs, &["go"]).unwrap();

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
fn several_langs_install_common_once_rendered_for_all() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["rust", "go", "rust"]).unwrap();

    assert!(report
        .render()
        .ends_with("→ 5 installed, 0 synced, 0 removed, 0 unchanged, 0 excluded\n"));
    assert_eq!(
        fs.content(&copy("common", "cohesion.md")),
        Some(rendered("cohesion.md", &["go", "rust"]))
    );
}

#[test]
fn adding_a_language_rerenders_common_and_records_both() {
    let fs = MemFs::installed(&["go"]);

    let report = run(&fs, &["rust"]).unwrap();

    let backup = format!("{}.bak-{TS}", copy("common", "cohesion.md"));
    assert!(report.render().contains(&format!(
        "synced: {} (backup: {backup})\n",
        copy("common", "cohesion.md")
    )));
    assert_eq!(
        fs.content(&copy("common", "cohesion.md")),
        Some(rendered("cohesion.md", &["go", "rust"]))
    );
    assert_eq!(
        fs.content(&record_path()),
        Some(full_record(&["go", "rust"]))
    );
}

#[test]
fn excluded_rule_stays_out_on_install() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&copy("common", "comments.md"));

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(
        report.render(),
        "→ 0 installed, 0 synced, 0 removed, 3 unchanged, 1 excluded\n"
    );
    assert_eq!(fs.content(&copy("common", "comments.md")), None);
}

#[test]
fn install_requires_a_lang() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &[]).unwrap_err();
    assert_eq!(err, "at least one --lang is required (one of: go, rust)");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn unknown_lang_installs_nothing() {
    let fs = MemFs::with_sources();
    let err = run(&fs, &["go", "cobol"]).unwrap_err();
    assert_eq!(err, "unknown language: cobol — expected one of: go, rust");
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn render_failure_installs_nothing() {
    let fs = MemFs::with_sources();
    fs.insert(
        &source("common", "comments.md"),
        &common_source("comments.md", "roles: [docs]"),
    );

    let err = run(&fs, &["go"]).unwrap_err();

    assert_eq!(
        err,
        format!(
            "{}: unknown role: docs — expected one of: source, config, boundary, test",
            source("common", "comments.md")
        )
    );
    assert_eq!(fs.mutation_count(), 0);
}
