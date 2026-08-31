//! Black-box tests for the `drift check` judgement. The filesystem is an
//! in-memory double, so every status rule (OK / DRIFTED / NOT_INSTALLED) and
//! the report's exit-code contract are pinned without real files.

mod drift_mocks;

use atelier::drift::commands::check;
use atelier::drift::core::types::{CheckReport, BEGIN_MARKER, END_MARKER};
use drift_mocks::*;

fn run(fs: &MemFs) -> Result<CheckReport, String> {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths())
}

#[test]
fn all_in_sync_reports_ok_and_exit_zero() {
    // In-sync copies of every artifact report OK with no detail, exit 0.
    let fs = MemFs::with_sources("shared body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("# mine\n{}tail\n", block("shared body")),
    );
    fs.insert(RULES_COPY, RULES_BODY);
    fs.insert(USER_RULE_COPY, USER_RULE_BODY);

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        "claude-md-coding-style-block=OK\n\
         rules/agent-design-principles.md=OK\n\
         user-rules/plan-vs-spec.md=OK\n\
         → 3 checked, 0 drifted, 0 missing\n"
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn all_drifted_reports_drifted_and_exit_one() {
    // Any content difference inside the block / copies is DRIFTED, exit 1.
    let fs = MemFs::with_sources("new body");
    fs.insert(USER_CLAUDE_MD, &block("old body"));
    fs.insert(RULES_COPY, "locally edited\n");
    fs.insert(USER_RULE_COPY, "locally edited\n");

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=DRIFTED ({USER_CLAUDE_MD})\n\
             rules/agent-design-principles.md=DRIFTED ({RULES_COPY})\n\
             user-rules/plan-vs-spec.md=DRIFTED ({USER_RULE_COPY})\n\
             → 3 checked, 3 drifted, 0 missing\n"
        )
    );
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn all_absent_reports_not_installed_and_exit_zero() {
    // NOT_INSTALLED is a report, not drift — missing artifacts still exit 0.
    let fs = MemFs::with_sources("body");

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=NOT_INSTALLED ({USER_CLAUDE_MD})\n\
             rules/agent-design-principles.md=NOT_INSTALLED ({RULES_COPY})\n\
             user-rules/plan-vs-spec.md=NOT_INSTALLED ({USER_RULE_COPY})\n\
             → 3 checked, 0 drifted, 3 missing\n"
        )
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn claude_md_without_any_marker_is_not_installed() {
    // A CLAUDE.md that exists but carries neither marker is NOT_INSTALLED.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, "# just my own notes\n");

    let report = run(&fs).unwrap();
    assert!(report.render().contains(&format!(
        "claude-md-coding-style-block=NOT_INSTALLED ({USER_CLAUDE_MD})"
    )));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn begin_marker_missing_is_drifted() {
    // End marker without begin is a broken block — DRIFTED, not NOT_INSTALLED.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, &format!("body\n{END_MARKER}\n"));

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=DRIFTED (begin marker missing)"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn end_marker_missing_is_drifted() {
    // Begin marker without end is the mirror-image broken block.
    let fs = MemFs::with_sources("body");
    fs.insert(USER_CLAUDE_MD, &format!("{BEGIN_MARKER}\nbody\n"));

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=DRIFTED (end marker missing)"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn marker_substring_mention_does_not_match() {
    // Markers match on whole-line equality only — a prose mention of the
    // marker text inside a longer line must not start the block.
    let fs = MemFs::with_sources("shared body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!(
            "note: the [coding-style:begin] marker guards the block below\n{}",
            block("shared body")
        ),
    );
    fs.insert(RULES_COPY, RULES_BODY);

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=OK\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_plugin_source_is_an_error() {
    // A missing plugin source makes judgement impossible — error, not finding.
    let fs = MemFs::default();
    fs.insert(TEMPLATE_CLAUDE_MD, &block("body"));

    let err = run(&fs).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source file not found: {TEMPLATE_RULES}")
    );
}

#[test]
fn missing_user_rule_source_is_an_error() {
    // The manifest's user-rule sources are judged the same way: absence is an
    // environment error, not a finding.
    let fs = MemFs::with_sources("body");
    fs.files.borrow_mut().remove(TEMPLATE_USER_RULE);

    let err = run(&fs).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source file not found: {TEMPLATE_USER_RULE}")
    );
}

#[test]
fn crlf_identical_block_reports_ok() {
    // check judges content, not bytes: lines() strips \r, so a CRLF-encoded
    // but semantically identical block is OK (sync is the strict side).
    let fs = MemFs::with_sources("shared body");
    fs.insert(USER_CLAUDE_MD, &block("shared body").replace('\n', "\r\n"));
    fs.insert(RULES_COPY, RULES_BODY);

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=OK\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn non_utf8_user_claude_md_is_drifted_not_an_error() {
    // A user file the UTF-8 source can never equal is drift, not an exit-2
    // error (exit 2 is documented as plugin-source/usage failure).
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_CLAUDE_MD, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=DRIFTED (not valid UTF-8)"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn non_utf8_rules_copy_is_drifted_not_an_error() {
    // The rules copy gets the same treatment: undecodable → DRIFTED finding.
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(RULES_COPY, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("rules/agent-design-principles.md=DRIFTED (not valid UTF-8)"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn non_utf8_user_rule_copy_is_drifted_not_an_error() {
    // User-rules copies share the verbatim-copy judgement.
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_RULE_COPY, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("user-rules/plan-vs-spec.md=DRIFTED (not valid UTF-8)"));
    assert_eq!(report.exit_code(), 1);
}
