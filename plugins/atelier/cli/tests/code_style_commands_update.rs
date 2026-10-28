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

fn installed_go_and_common() -> MemFs {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("common");
    fs
}

#[test]
fn drifted_new_and_orphaned_are_all_resolved_then_check_is_clean() {
    let fs = installed_go_and_common();
    fs.insert(&copy("go", "go-naming.md"), "locally edited\n");
    fs.insert(&source("common", "testing.md"), "# testing\n");
    fs.delete(&source("go", "go-errors.md"));

    let report = run(&fs, &[]).unwrap();

    let naming_bak = format!("{}.bak-{TS}", copy("go", "go-naming.md"));
    let errors_bak = format!("{}.bak-{TS}", copy("go", "go-errors.md"));
    assert_eq!(
        report.render(),
        format!(
            "installed: {}\nremoved: {} (backup: {errors_bak})\nsynced: {} (backup: {naming_bak})\n\
             → 1 installed, 1 synced, 1 removed, 2 unchanged\n",
            copy("common", "testing.md"),
            copy("go", "go-errors.md"),
            copy("go", "go-naming.md"),
        )
    );
    assert_eq!(fs.content(&naming_bak).as_deref(), Some("locally edited\n"));
    assert_eq!(fs.content(&errors_bak), Some(body("go", "go-errors.md")));
    assert_eq!(fs.content(&copy("go", "go-errors.md")), None);
    assert_eq!(
        fs.content(&copy("common", "testing.md")).as_deref(),
        Some("# testing\n")
    );
    assert_eq!(check_exit(&fs), 0);
}

#[test]
fn omitted_lang_leaves_languages_not_installed_alone() {
    let fs = installed_go_and_common();

    run(&fs, &[]).unwrap();

    assert_eq!(fs.content(&copy("rust", "rust-naming.md")), None);
    assert_eq!(fs.mutation_count(), 0);
}

#[test]
fn explicit_lang_limits_update_to_that_language_and_common() {
    let fs = MemFs::with_sources();
    fs.install_group("go");
    fs.install_group("rust");
    fs.install_group("common");
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
fn non_utf8_copy_is_refused_before_any_write() {
    let fs = installed_go_and_common();
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
