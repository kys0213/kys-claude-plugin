//! Black-box tests for the `drift sync` write path. The in-memory filesystem's
//! write log is the point: success paths must capture a backup before the
//! overwrite, and every refusal must leave the target completely untouched.

mod drift_mocks;

use atelier::drift::commands::sync;
use atelier::drift::core::types::{SyncTarget, BEGIN_MARKER, END_MARKER, USER_RULES};
use drift_mocks::*;

fn run_named(fs: &MemFs, target: SyncTarget, name: Option<&str>) -> Result<String, String> {
    let clock = FixedClock;
    sync::run(&deps(fs, &clock), &paths(), target, name)
        .map(|reports| reports.iter().map(|r| r.render()).collect())
}

fn run(fs: &MemFs, target: SyncTarget) -> Result<String, String> {
    run_named(fs, target, None)
}

#[test]
fn sync_claude_md_replaces_block_and_preserves_user_lines() {
    // Only the marker range is replaced; user lines above and below survive.
    let fs = MemFs::with_sources("new body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("# mine\n{}tail\n", block("old body")),
    );

    let line = run(&fs, SyncTarget::ClaudeMd).unwrap();
    let backup = format!("{USER_CLAUDE_MD}.bak-{TS}");
    assert_eq!(
        line,
        format!("synced: coding-style block in {USER_CLAUDE_MD} (backup: {backup})\n")
    );
    assert_eq!(
        fs.content(USER_CLAUDE_MD).unwrap(),
        format!("# mine\n{}tail\n", block("new body"))
    );
    // The backup write happened first and holds the pre-sync content.
    assert_eq!(
        fs.writes.borrow()[0],
        (backup, format!("# mine\n{}tail\n", block("old body")))
    );
}

#[test]
fn resync_is_idempotent() {
    // Syncing an already-synced block writes the same content again.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, &block("old"));

    run(&fs, SyncTarget::ClaudeMd).unwrap();
    let first = fs.content(USER_CLAUDE_MD).unwrap();
    run(&fs, SyncTarget::ClaudeMd).unwrap();
    assert_eq!(fs.content(USER_CLAUDE_MD).unwrap(), first);
}

#[test]
fn sync_rules_overwrites_copy_and_backs_up() {
    // The rules copy is replaced wholesale with the plugin source.
    let fs = MemFs::with_sources("body");
    fs.insert(RULES_COPY, "locally edited\n");

    let line = run(&fs, SyncTarget::Rules).unwrap();
    let backup = format!("{RULES_COPY}.bak-{TS}");
    assert_eq!(line, format!("synced: {RULES_COPY} (backup: {backup})\n"));
    assert_eq!(fs.content(RULES_COPY).unwrap(), RULES_BODY);
    assert_eq!(
        fs.writes.borrow()[0],
        (backup, "locally edited\n".to_string())
    );
}

#[test]
fn sync_user_rules_overwrites_copy_and_backs_up() {
    // Every manifest copy is replaced wholesale with its plugin source, one
    // synced line per file, all backups sharing the run's single timestamp.
    let fs = MemFs::with_sources("body");
    fs.install_user_rule_copies();
    fs.insert(USER_RULE_COPY, "locally edited\n");

    let lines = run(&fs, SyncTarget::UserRules).unwrap();
    let expected: String = USER_RULES
        .iter()
        .map(|name| {
            let copy = user_rule_copy(name);
            format!("synced: {copy} (backup: {copy}.bak-{TS})\n")
        })
        .collect();
    assert_eq!(lines, expected);
    assert_eq!(fs.content(USER_RULE_COPY).unwrap(), USER_RULE_BODY);
    assert_eq!(
        fs.writes.borrow()[0],
        (
            format!("{USER_RULE_COPY}.bak-{TS}"),
            "locally edited\n".to_string()
        )
    );
}

#[test]
fn sync_user_rules_with_name_syncs_only_that_file() {
    // --name narrows the unit to one manifest file: other copies are neither
    // validated nor touched, so a per-file overwrite consent stays per-file.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_RULE_COPY, "locally edited\n");

    let line = run_named(&fs, SyncTarget::UserRules, Some(USER_RULES[0])).unwrap();
    assert_eq!(
        line,
        format!("synced: {USER_RULE_COPY} (backup: {USER_RULE_COPY}.bak-{TS})\n")
    );
    assert_eq!(fs.content(USER_RULE_COPY).unwrap(), USER_RULE_BODY);
    // backup + overwrite of the named file only — absent siblings stay absent.
    assert_eq!(fs.write_count(), 2);
}

#[test]
fn refuses_unknown_user_rule_name_with_zero_writes() {
    // --name is validated against the manifest before anything is touched.
    let fs = MemFs::with_sources("body");
    fs.install_user_rule_copies();

    let err = run_named(&fs, SyncTarget::UserRules, Some("bogus.md")).unwrap_err();
    assert!(err.starts_with("unknown user rule: bogus.md"));
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_name_on_non_user_rules_target_with_zero_writes() {
    // --name has no meaning for single-artifact targets.
    let fs = MemFs::with_sources("body");
    fs.insert(RULES_COPY, RULES_BODY);

    let err = run_named(&fs, SyncTarget::Rules, Some(USER_RULES[0])).unwrap_err();
    assert_eq!(err, "--name is only supported with --target user-rules");
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_user_rule_copy_absent_with_zero_writes() {
    // Installing the user-rules copies is setup's job, not sync's.
    let fs = MemFs::with_sources("body");

    let err = run(&fs, SyncTarget::UserRules).unwrap_err();
    assert_eq!(
        err,
        format!("not installed: {USER_RULE_COPY} — run /atelier:setup")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_non_utf8_user_rule_copy_with_zero_writes() {
    // Same refusal as the project rules copy: no faithful backup, no write —
    // and one bad file blocks the whole unit before any sibling is written.
    let fs = MemFs::with_sources("body");
    fs.install_user_rule_copies();
    fs.insert_bytes(USER_RULE_COPY, b"\xff\xfe not utf-8");

    let err = run(&fs, SyncTarget::UserRules).unwrap_err();
    assert_eq!(
        err,
        format!(
            "{USER_RULE_COPY} is not valid UTF-8 — fix encoding or run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_user_rule_source_missing_with_zero_writes() {
    // A manifest entry without its plugin source is an environment error.
    let fs = MemFs::with_sources("body");
    fs.install_user_rule_copies();
    fs.files.borrow_mut().remove(TEMPLATE_USER_RULE);

    let err = run(&fs, SyncTarget::UserRules).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source file not found: {TEMPLATE_USER_RULE}")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_claude_md_absent_with_zero_writes() {
    // sync never installs: an absent CLAUDE.md is refused, nothing written.
    let fs = MemFs::with_sources("body");

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!("coding-style block not installed in {USER_CLAUDE_MD} — run /atelier:setup")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_no_marker_with_zero_writes() {
    // A CLAUDE.md without the block is "not installed", never a write target.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, "# my own file\n");

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!("coding-style block not installed in {USER_CLAUDE_MD} — run /atelier:setup")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_one_marker_missing_with_zero_writes() {
    // A half-present block is corruption — replacing blindly could eat user text.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, &format!("{BEGIN_MARKER}\nbody\n"));

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!(
            "broken coding-style block in {USER_CLAUDE_MD} (one marker missing) — run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_duplicated_markers_with_zero_writes() {
    // More than one marker pair makes the replacement range ambiguous.
    let fs = MemFs::with_sources("body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("{BEGIN_MARKER}\n{BEGIN_MARKER}\nbody\n{END_MARKER}\n"),
    );

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!(
            "broken coding-style block in {USER_CLAUDE_MD} (markers duplicated: begin=2, end=1) — run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_reversed_markers_with_zero_writes() {
    // end before begin is not a range — writing would destroy user content.
    let fs = MemFs::with_sources("body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("{END_MARKER}\nbody\n{BEGIN_MARKER}\n"),
    );

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!(
            "broken coding-style block in {USER_CLAUDE_MD} (markers out of order) — run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_rules_copy_absent_with_zero_writes() {
    // Installing the rules copy is setup's job, not sync's.
    let fs = MemFs::with_sources("body");

    let err = run(&fs, SyncTarget::Rules).unwrap_err();
    assert_eq!(
        err,
        format!("not installed: {RULES_COPY} — run /atelier:setup")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_when_plugin_source_missing_with_zero_writes() {
    // Without a source there is nothing safe to write from.
    let fs = MemFs::default();
    fs.insert(USER_CLAUDE_MD, &block("body"));

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source file not found: {TEMPLATE_CLAUDE_MD}")
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_crlf_claude_md_with_zero_writes() {
    // The line-based rebuild would rewrite every CRLF ending in the file —
    // user content outside the markers included — so sync refuses.
    let fs = MemFs::with_sources("new body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("# mine\n{}tail\n", block("old body")).replace('\n', "\r\n"),
    );

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!(
            "{USER_CLAUDE_MD} uses CRLF line endings — convert to LF or run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_non_utf8_claude_md_with_zero_writes() {
    // An undecodable target cannot be backed up or rebuilt faithfully.
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_CLAUDE_MD, b"\xff\xfe not utf-8");

    let err = run(&fs, SyncTarget::ClaudeMd).unwrap_err();
    assert_eq!(
        err,
        format!(
            "{USER_CLAUDE_MD} is not valid UTF-8 — fix encoding or run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}

#[test]
fn refuses_non_utf8_rules_copy_with_zero_writes() {
    // Same refusal for the rules target: no faithful backup, no write.
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(RULES_COPY, b"\xff\xfe not utf-8");

    let err = run(&fs, SyncTarget::Rules).unwrap_err();
    assert_eq!(
        err,
        format!(
            "{RULES_COPY} is not valid UTF-8 — fix encoding or run /atelier:setup to reinstall"
        )
    );
    assert_eq!(fs.write_count(), 0);
}
