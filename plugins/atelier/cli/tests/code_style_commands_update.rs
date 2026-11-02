mod code_style_mocks;

use atelier::code_style::commands::{check, update};
use atelier::code_style::core::types::ApplyReport;
use code_style_mocks::*;

fn run(fs: &MemFs, names: &[&str]) -> Result<ApplyReport, String> {
    let clock = FixedClock;
    update::run(&deps(fs, &clock), &paths(), &langs(names))
}

fn check_exit(fs: &MemFs) -> i32 {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths(), &[])
        .unwrap()
        .exit_code()
}

#[test]
fn drifted_new_and_orphaned_are_all_resolved_then_check_is_clean() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");
    fs.insert(
        &source("common", "testing.md"),
        &body("common", "testing.md"),
    );
    fs.delete(&source("go", "go-errors.md"));

    let report = run(&fs, &[]).unwrap();

    let naming_bak = format!("{}.bak-{TS}", copy("go", "go-naming.md"));
    let errors_bak = format!("{}.bak-{TS}", copy("go", "go-errors.md"));
    assert_eq!(
        report.render(),
        format!(
            "installed: {}\nremoved: {} (backup: {errors_bak})\nsynced: {} (backup: {naming_bak})\n\
             → 1 installed, 1 synced, 1 removed, 2 unchanged, 0 excluded\n",
            copy("common", "testing.md"),
            copy("go", "go-errors.md"),
            copy("go", "go-naming.md"),
        )
    );
    assert_eq!(fs.content(&naming_bak).as_deref(), Some("locally edited\n"));
    assert_eq!(fs.content(&errors_bak), Some(body("go", "go-errors.md")));
    assert_eq!(fs.content(&copy("go", "go-errors.md")), None);
    assert_eq!(
        fs.content(&copy("common", "testing.md")),
        Some(rendered("testing.md", &["go"]))
    );
    assert_eq!(
        fs.content(&record_path()),
        Some(record_json(
            &["go"],
            &[
                "common/cohesion.md",
                "common/comments.md",
                "common/testing.md",
                "go/go-naming.md"
            ]
        ))
    );
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn omitted_lang_leaves_languages_not_installed_alone() {
    let fs = MemFs::installed(&["go"]);

    run(&fs, &[]).unwrap();

    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn explicit_lang_limits_update_to_that_language_and_common() {
    let fs = MemFs::installed(&["go", "rust"]);
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");
    fs.insert(&copy("rust", "rust-naming.md"), "locally edited\n");

    run(&fs, &["rust"]).unwrap();

    assert_eq!(
        fs.content(&copy("rust", "rust-naming.md")),
        Some(body("rust", "rust-naming.md"))
    );
    assert_eq!(
        fs.content(&copy("go", "go-naming.md")).as_deref(),
        Some("locally edited\n")
    );
}

#[test]
fn explicit_lang_still_renders_common_for_every_installed_language() {
    let fs = MemFs::installed(&["go", "rust"]);
    fs.insert(
        &copy("common", "cohesion.md"),
        &rendered("cohesion.md", &["rust"]),
    );

    run(&fs, &["rust"]).unwrap();

    assert_eq!(
        fs.content(&copy("common", "cohesion.md")),
        Some(rendered("cohesion.md", &["go", "rust"]))
    );
}

#[test]
fn excluded_rule_is_not_reinstalled() {
    let fs = MemFs::installed(&["go"]);
    fs.delete(&copy("go", "go-naming.md"));

    let report = run(&fs, &[]).unwrap();

    assert!(report.render().ends_with("1 excluded\n"));
    assert_eq!(fs.content(&copy("go", "go-naming.md")), None);
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn install_without_record_gets_a_record_on_first_update() {
    let fs = MemFs::installed_without_record(&["go"]);
    fs.insert(&copy("common", "cohesion.md"), "---\nold paths\n---\n");

    let report = run(&fs, &[]).unwrap();

    assert!(report.render().contains(&format!(
        "synced: {} (backup: ",
        copy("common", "cohesion.md")
    )));
    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"])));
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn language_removed_from_plugin_is_cleaned_and_dropped_from_record() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&copy("python", "py-naming.md"), "# py\n");
    let mut rules = all_rules(&["go"]);
    rules.push("python/py-naming.md".to_string());
    let refs: Vec<&str> = rules.iter().map(String::as_str).collect();
    fs.insert(&record_path(), &record_json(&["go", "python"], &refs));

    run(&fs, &[]).unwrap();

    assert_eq!(fs.content(&copy("python", "py-naming.md")), None);
    assert_eq!(fs.content(&record_path()), Some(full_record(&["go"])));
}

#[test]
fn non_utf8_copy_is_refused_before_any_write() {
    let fs = MemFs::installed(&["go"]);
    fs.insert(&source("go", "go-testing.md"), "# testing\n");
    fs.insert_bytes(&copy("go", "go-naming.md"), &[0xff, 0xfe]);

    let err = run(&fs, &[]).unwrap_err();

    assert_eq!(
        err,
        format!(
            "{} is not valid UTF-8 — fix its encoding or remove it, then re-run",
            copy("go", "go-naming.md")
        )
    );
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn nothing_installed_is_an_error_and_writes_nothing() {
    let fs = MemFs::with_sources();
    assert!(run(&fs, &[]).is_err());
    assert_eq!(fs.mutation_count(), 0);
}
