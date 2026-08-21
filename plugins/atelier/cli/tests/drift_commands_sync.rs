//! Black-box tests for the `drift sync` write path. The in-memory filesystem's
//! write log is the point: success paths must capture a backup before the
//! overwrite, and every refusal must leave the target completely untouched.

mod drift_mocks;

use atelier::drift::commands::sync;
use atelier::drift::core::types::{SyncTarget, BEGIN_MARKER, END_MARKER};
use drift_mocks::*;

fn run(fs: &MemFs, target: SyncTarget) -> Result<String, String> {
    let clock = FixedClock;
    sync::run(&deps(fs, &clock), &paths(), target).map(|report| report.render())
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
