mod code_style_mocks;

use atelier::code_style::commands::install;
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, names: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    install::run(&deps(fs, &clock), &paths(), &langs(names))
}

#[test]
fn fresh_install_copies_language_and_common_creating_directories() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(
        report.render(),
        format!(
            "installed: {}\ninstalled: {}\ninstalled: {}\ninstalled: {}\n\
             → 4 installed, 0 synced, 0 removed, 0 unchanged\n",
            copy("common", "cohesion.md"),
            copy("common", "comments.md"),
            copy("go", "go-errors.md"),
            copy("go", "go-naming.md"),
        )
    );
    for group in ["go", "common"] {
        for file in rules_of(group) {
            assert_eq!(fs.content(&copy(group, file)), Some(body(group, file)));
        }
    }
    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
}

#[test]
fn reinstall_in_sync_changes_nothing() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");

    let report = run(&fs, &["go"]).unwrap();

    assert_eq!(
        report.render(),
        "→ 0 installed, 0 synced, 0 removed, 4 unchanged\n"
    );
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn install_over_edited_copy_backs_it_up_first() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
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
fn several_langs_install_common_once() {
    let fs = MemFs::with_sources();

    let report = run(&fs, &["rust", "go", "rust"]).unwrap();

    assert!(report
        .render()
        .ends_with("→ 5 installed, 0 synced, 0 removed, 0 unchanged\n"));
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
