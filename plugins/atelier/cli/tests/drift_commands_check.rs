mod drift_mocks;

use atelier::drift::commands::check;
use atelier::drift::core::types::{CheckReport, BEGIN_MARKER, END_MARKER, POLICY_RULES};
use drift_mocks::*;

fn run(fs: &MemFs) -> Result<CheckReport, String> {
    let clock = FixedClock;
    check::run(&deps(fs, &clock), &paths())
}

fn scope_lines(prefix: &str, status: &str, detail: Option<fn(&str) -> String>) -> String {
    POLICY_RULES
        .iter()
        .map(|name| match detail {
            Some(detail) => format!("{prefix}{name}={status} ({})\n", detail(name)),
            None => format!("{prefix}{name}={status}\n"),
        })
        .collect()
}

fn policy_lines(status: &str, detail: bool) -> String {
    let user: Option<fn(&str) -> String> = detail.then_some(user_rule_copy as fn(&str) -> String);
    let project: Option<fn(&str) -> String> =
        detail.then_some(project_rule_copy as fn(&str) -> String);
    format!(
        "{}{}",
        scope_lines("user-rules/", status, user),
        scope_lines("project-rules/", status, project)
    )
}

#[test]
fn all_in_sync_reports_ok_and_exit_zero() {
    let fs = MemFs::with_sources("shared body");
    fs.insert(
        USER_CLAUDE_MD,
        &format!("# mine\n{}tail\n", block("shared body")),
    );
    fs.insert(RULES_COPY, RULES_BODY);
    fs.install_user_rule_copies();
    fs.install_project_rule_copies();

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=OK\n\
             rules/agent-design-principles.md=OK\n\
             {}→ {} checked, 0 drifted, 0 new, 0 missing\n",
            policy_lines("OK", false),
            2 + 2 * POLICY_RULES.len()
        )
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn all_drifted_reports_drifted_and_exit_one() {
    let fs = MemFs::with_sources("new body");
    fs.insert(USER_CLAUDE_MD, &block("old body"));
    fs.insert(RULES_COPY, "locally edited\n");
    for name in POLICY_RULES {
        fs.insert(&user_rule_copy(name), "locally edited\n");
        fs.insert(&project_rule_copy(name), "locally edited\n");
    }

    let report = run(&fs).unwrap();
    assert_eq!(
        report.render(),
        format!(
            "claude-md-coding-style-block=DRIFTED ({USER_CLAUDE_MD})\n\
             rules/agent-design-principles.md=DRIFTED ({RULES_COPY})\n\
             {}→ {} checked, {} drifted, 0 new, 0 missing\n",
            policy_lines("DRIFTED", true),
            2 + 2 * POLICY_RULES.len(),
            2 + 2 * POLICY_RULES.len()
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
             rules/agent-design-principles.md=NOT_INSTALLED ({RULES_COPY})\n\
             {}→ {} checked, 0 drifted, 0 new, {} missing\n",
            policy_lines("NOT_INSTALLED", true),
            2 + 2 * POLICY_RULES.len(),
            2 + 2 * POLICY_RULES.len()
        )
    );
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_manifest_file_in_installed_scope_is_new_and_exit_one() {
    let fs = MemFs::with_sources("shared body");
    fs.insert(USER_CLAUDE_MD, &block("shared body"));
    fs.insert(RULES_COPY, RULES_BODY);
    fs.install_user_rule_copies();
    let added = POLICY_RULES[POLICY_RULES.len() - 1];
    fs.files.borrow_mut().remove(&user_rule_copy(added));

    let report = run(&fs).unwrap();
    let rendered = report.render();
    assert!(rendered.contains(&format!(
        "user-rules/{added}=NEW ({})\n",
        user_rule_copy(added)
    )));
    assert!(rendered.contains(&format!(
        "project-rules/{added}=NOT_INSTALLED ({})\n",
        project_rule_copy(added)
    )));
    assert!(rendered.ends_with(&format!(
        "→ {} checked, 0 drifted, 1 new, {} missing\n",
        2 + 2 * POLICY_RULES.len(),
        POLICY_RULES.len()
    )));
    assert_eq!(report.exit_code(), 1);
}

#[test]
fn new_is_judged_per_scope() {
    let fs = MemFs::with_sources("body");
    let added = POLICY_RULES[POLICY_RULES.len() - 1];
    fs.insert(&project_rule_copy(POLICY_RULES[0]), USER_RULE_BODY);

    let rendered = run(&fs).unwrap().render();
    assert!(rendered.contains(&format!("project-rules/{added}=NEW")));
    assert!(rendered.contains(&format!("user-rules/{added}=NOT_INSTALLED")));
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
    fs.insert(RULES_COPY, RULES_BODY);

    let report = run(&fs).unwrap();
    assert!(report
        .render()
        .contains("claude-md-coding-style-block=OK\n"));
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn missing_plugin_source_is_an_error() {
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
    fs.insert(RULES_COPY, RULES_BODY);

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
fn non_utf8_rules_copy_is_drifted_not_an_error() {
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
    let fs = MemFs::with_sources("body");
    fs.insert_bytes(USER_RULE_COPY, b"\xff\xfe not utf-8");

    let report = run(&fs).unwrap();
    assert!(report.render().contains(&format!(
        "user-rules/{}=DRIFTED (not valid UTF-8)",
        POLICY_RULES[0]
    )));
    assert_eq!(report.exit_code(), 1);
}
