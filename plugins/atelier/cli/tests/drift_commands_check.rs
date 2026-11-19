mod drift_mocks;

use atelier::drift::commands::check;
use atelier::drift::core::types::{
    CheckReport, BEGIN_MARKER, END_MARKER, HERDR_MOD_FILES, POLICY_RULES,
};
use drift_mocks::*;

fn run(fs: &MemFs) -> Result<CheckReport, String> {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths())
}

fn policy_lines(status: &str, detail: bool) -> String {
    POLICY_RULES
        .iter()
        .map(|name| {
            if detail {
                format!("user-rules/{name}={status} ({})\n", user_rule_copy(name))
            } else {
                format!("user-rules/{name}={status}\n")
            }
        })
        .collect()
}

fn herdr_lines(status: &str, detail: bool) -> String {
    HERDR_MOD_FILES
        .iter()
        .map(|name| {
            if detail {
                format!("herdr-mod/{name}={status} ({})\n", herdr_mod_copy(name))
            } else {
                format!("herdr-mod/{name}={status}\n")
            }
        })
        .collect()
}

/// Every finding: the CLAUDE.md block, the policy and herdr scopes.
fn checked() -> usize {
    1 + POLICY_RULES.len() + HERDR_MOD_FILES.len()
}

#[test]
fn all_in_sync_reports_ok_and_exit_zero() {
    let fs = MemFs::with_sources("shared body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("# mine\n{}tail\n", block("shared body")),
    );
    fs.install_user_rule_copies();
    fs.install_herdr_mod_copies();

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=OK\n\
             {}{}→ {} checked, 0 drifted, 0 new, 0 missing\n",
            policy_lines("OK", false),
            herdr_lines("OK", false),
            checked()
        )
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn all_drifted_reports_drifted_and_exit_one() {
    let fs = MemFs::with_sources("new body");
    fs.insert(USER_CLAUDE_MD, &block("old body"));
    for name in POLICY_RULES {
        fs.insert(&user_rule_copy(name), "locally edited\n");
    }
    for name in HERDR_MOD_FILES {
        fs.insert(&herdr_mod_copy(name), "locally edited\n");
    }

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=DRIFTED ({USER_CLAUDE_MD})\n\
             {}{}→ {} checked, {} drifted, 0 new, 0 missing\n",
            policy_lines("DRIFTED", true),
            herdr_lines("DRIFTED", true),
            checked(),
            checked()
        )
    );
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn all_absent_reports_not_installed_and_exit_zero() {
    let fs = MemFs::with_sources("body");

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=NOT_INSTALLED ({USER_CLAUDE_MD})\n\
             {}{}→ {} checked, 0 drifted, 0 new, {} missing\n",
            policy_lines("NOT_INSTALLED", true),
            herdr_lines("NOT_INSTALLED", true),
            checked(),
            checked()
        )
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_manifest_file_in_installed_scope_is_new_and_exit_one() {
    let fs = MemFs::with_sources("shared body");
    fs.insert(USER_CLAUDE_MD, &block("shared body"));
    fs.install_user_rule_copies();
    let added = POLICY_RULES[POLICY_RULES.len() - 1];
    fs.files.borrow_mut().remove(&user_rule_copy(added));

    let report = run(&fs).unwrap();
    let rendered = report.render();
    assert!(rendered.contains(&format!(
        "user-rules/{added}=NEW ({})\n",
        user_rule_copy(added)
    )));
    assert!(rendered.ends_with(&format!(
        "→ {} checked, 0 drifted, 1 new, {} missing\n",
        checked(),
        HERDR_MOD_FILES.len()
    )));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn missing_panel_file_in_installed_herdr_mod_is_new() {
    let fs = MemFs::with_sources("body");
    fs.install_herdr_mod_copies();
    let added = HERDR_MOD_FILES[HERDR_MOD_FILES.len() - 1];
    fs.files.borrow_mut().remove(&herdr_mod_copy(added));

    let report = run(&fs).unwrap();
    let rendered = report.render();
    assert!(rendered.contains(&format!(
        "herdr-mod/{added}=NEW ({})\n",
        herdr_mod_copy(added)
    )));
    assert!(rendered.contains(&format!("herdr-mod/{}=OK\n", HERDR_MOD_FILES[0])));
    assert!(rendered.contains(&policy_lines("NOT_INSTALLED", true)));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn missing_herdr_mod_source_is_an_error() {
    let fs = MemFs::with_sources("body");
    fs.files
        .borrow_mut()
        .remove(&template_herdr_mod_file(HERDR_MOD_FILES[0]));

    let err = run(&fs).unwrap_err();
    assert!(err.contains("plugin source file not found"), "{err}");
}

#[test]
fn claude_md_without_any_marker_is_not_installed() {
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
    let fs = MemFs::with_sources("shared body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!(
            "note: the [coding-style:begin] marker guards the block below\n{}",
            block("shared body")
        ),
    );

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=OK\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_claude_md_source_is_an_error() {
    let fs = MemFs::with_sources("body");
    fs.files.borrow_mut().remove(TEMPLATE_CLAUDE_MD);

    let err = run(&fs).unwrap_err();
    assert_eq!(
        err,
        format!("plugin source file not found: {TEMPLATE_CLAUDE_MD}")
    );
}

#[test]
fn missing_user_rule_source_is_an_error() {
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
    let fs = MemFs::with_sources("shared body");
    fs.insert(USER_CLAUDE_MD, &block("shared body").replace('\n', "\r\n"));

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=OK\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn non_utf8_user_claude_md_is_drifted_not_an_error() {
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_CLAUDE_MD, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=DRIFTED (not valid UTF-8)"));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn non_utf8_user_rule_copy_is_drifted_not_an_error() {
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_RULE_COPY, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report.render().contains(&format!(
        "user-rules/{}=DRIFTED (not valid UTF-8)",
        POLICY_RULES[0]
    )));
    assert_eq!(report.exit_code(), 1);
}
