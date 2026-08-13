//! Black-box tests for `setup guard` — the command that replaced the markdown
//! bash an LLM used to re-execute on every setup. What is pinned here is the
//! part that was fragile by hand: a detection failure must *omit* the flag
//! rather than emit a bare one, the project-dir placeholder must survive
//! verbatim, and a re-run must not leave a second guard behind.

mod git_mocks;

use atelier::git::commands::guard_setup::{run, GuardSetupDeps, GuardSetupInput};
use atelier::git::commands::hook::create_hook_command;
use atelier::git::types::{CmdResult, GuardSetupOutput, HookScope};
use git_mocks::{MockFs, MockGit, MockGitHub, MockWarmer, Recorder};
use serde_json::Value;
use std::rc::Rc;

const PROJECT_DIR: &str = "/tmp/guard-setup-project";

/// One setup run's world. Defaults describe the ordinary case — a successful
/// warm-up, a repository that detects `main`, no `gh`, project scope, a real
/// write — so each test names only what it varies with `..Default::default()`.
struct Scenario {
    warmer: MockWarmer,
    git: MockGit,
    gh: MockGitHub,
    scope: HookScope,
    dry_run: bool,
}

impl Default for Scenario {
    fn default() -> Self {
        Scenario {
            warmer: MockWarmer::default(),
            git: MockGit::default(),
            gh: MockGitHub::default(),
            scope: HookScope::Project,
            dry_run: false,
        }
    }
}

/// Assembles the deps and runs, so each test states only what it varies.
fn setup(fs: &MockFs, s: Scenario) -> CmdResult<GuardSetupOutput> {
    let hook = create_hook_command(fs);
    let deps = GuardSetupDeps {
        warmer: &s.warmer,
        git: &s.git,
        gh: &s.gh,
        hook: &hook,
    };
    let input = GuardSetupInput {
        project_dir: PROJECT_DIR.to_string(),
        scope: s.scope,
        dry_run: s.dry_run,
    };
    run(&deps, &input)
}

fn ok(result: CmdResult<GuardSetupOutput>) -> GuardSetupOutput {
    match result {
        CmdResult::Ok(out) => out,
        CmdResult::Err(e) => panic!("expected success, got error: {e}"),
    }
}

/// A `GitService` whose default-branch detection fails — the "no remote" case.
fn git_without_detection() -> MockGit {
    MockGit {
        detect_default_branch: Box::new(|| Err("no remote".to_string())),
        ..Default::default()
    }
}

fn gh_returning(raw: &'static str) -> MockGitHub {
    MockGitHub {
        default_branch: Box::new(move || Some(raw.to_string())),
        ..Default::default()
    }
}

/// Settings JSON as written, read back through the path the command reports.
fn written(fs: &MockFs, out: &GuardSetupOutput) -> Value {
    serde_json::from_str(
        &fs.get(&out.settings_path)
            .expect("settings.json should have been written"),
    )
    .unwrap()
}

/// Every registered command string, flattened across matcher groups.
fn registered_commands(settings: &Value) -> Vec<String> {
    settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse array")
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap().iter())
        .map(|hook| hook["command"].as_str().unwrap().to_string())
        .collect()
}

// ---- default-branch pin ----

#[test]
fn omits_default_branch_flag_when_detection_fails() {
    // The high-risk failure: a bare `--default-branch` makes the hook exit 2 on
    // clap's usage error, which Claude Code reads as "block every edit".
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            git: git_without_detection(),
            ..Default::default()
        },
    ));
    assert_eq!(out.default_branch, None);
    for command in &out.commands {
        assert!(
            !command.contains("--default-branch"),
            "flag must be omitted entirely, got: {command}"
        );
    }
}

#[test]
fn omits_flag_when_gh_returns_blank_or_whitespace() {
    // `gh` can exit 0 with an empty/whitespace body; that is absence, not a
    // branch named "".
    for blank in ["", "   ", "\n"] {
        let fs = MockFs::new();
        let gh = MockGitHub {
            default_branch: Box::new(move || Some(blank.to_string())),
            ..Default::default()
        };
        let out = ok(setup(
            &fs,
            Scenario {
                git: git_without_detection(),
                gh,
                ..Default::default()
            },
        ));
        assert_eq!(out.default_branch, None, "blank input {blank:?}");
        assert!(out.commands.iter().all(|c| !c.contains("--default-branch")));
    }
}

#[test]
fn emits_default_branch_flag_when_detected() {
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            git: git_without_detection(),
            gh: gh_returning("trunk"),
            ..Default::default()
        },
    ));
    assert_eq!(out.default_branch, Some("trunk".to_string()));
    assert!(out
        .commands
        .iter()
        .all(|c| c.ends_with("--default-branch trunk")));
}

#[test]
fn falls_back_to_git_detection_when_gh_is_unavailable() {
    let fs = MockFs::new();
    let git = MockGit {
        detect_default_branch: Box::new(|| Ok("develop".to_string())),
        ..Default::default()
    };
    let out = ok(setup(
        &fs,
        Scenario {
            git,
            ..Default::default()
        },
    ));
    assert_eq!(out.default_branch, Some("develop".to_string()));
}

// ---- scope ----

#[test]
fn user_scope_never_pins_default_branch() {
    // A single global pin would force one repository's default branch onto
    // every project (#810); the origin/HEAD warm-up covers it at runtime.
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            gh: gh_returning("trunk"),
            scope: HookScope::User,
            ..Default::default()
        },
    ));
    assert_eq!(out.default_branch, None);
    assert!(
        out.commands.iter().all(|c| !c.contains("--default-branch")),
        "user scope must not pin: {:?}",
        out.commands
    );
}

#[test]
fn project_scope_pins_detected_default_branch() {
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            gh: gh_returning("trunk"),
            ..Default::default()
        },
    ));
    assert_eq!(out.default_branch, Some("trunk".to_string()));
    assert_eq!(
        out.settings_path,
        format!("{PROJECT_DIR}/.claude/settings.json")
    );
    let commands = registered_commands(&written(&fs, &out));
    assert!(commands
        .iter()
        .all(|c| c.contains("--default-branch trunk")));
}

// ---- what lands in settings.json ----

#[test]
fn preserves_literal_claude_project_dir_placeholder() {
    // The shell expands this when the hook fires. If setup expanded it, one
    // session's path would be frozen into every future invocation.
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            gh: gh_returning("main"),
            ..Default::default()
        },
    ));
    let raw = fs.get(&out.settings_path).unwrap();
    assert!(
        raw.contains(r#"--project-dir \"${CLAUDE_PROJECT_DIR:-.}\""#),
        "placeholder must survive verbatim in settings.json, got:\n{raw}"
    );
    for command in registered_commands(&written(&fs, &out)) {
        assert!(command.contains(r#"--project-dir "${CLAUDE_PROJECT_DIR:-.}""#));
    }
}

#[test]
fn registers_both_write_and_commit_guards_in_single_write() {
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            git: git_without_detection(),
            ..Default::default()
        },
    ));
    // One write: two `register` calls would leave a half-registered file if the
    // second failed.
    assert_eq!(fs.write_count(), 1);

    let settings = written(&fs, &out);
    let groups = settings["hooks"]["PreToolUse"].as_array().unwrap();
    let matchers: Vec<&str> = groups
        .iter()
        .map(|g| g["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(matchers, vec!["Write|Edit", "Bash"]);

    let commands = registered_commands(&settings);
    assert!(commands[0].starts_with("atelier git guard write "));
    assert!(commands[1].starts_with("atelier git guard commit "));
}

// ---- warm-up ----

#[test]
fn warm_up_runs_before_detection_against_project_dir() {
    // Order matters: warming origin/HEAD after detection would not help the
    // detection that already ran.
    let calls = Rc::new(Recorder::default());
    let warm_calls = Rc::clone(&calls);
    let gh_calls = Rc::clone(&calls);
    let git_calls = Rc::clone(&calls);

    let warmer = MockWarmer {
        warm_origin_head: Box::new(move || {
            warm_calls.push("warm_origin_head");
            true
        }),
    };
    let gh = MockGitHub {
        default_branch: Box::new(move || {
            gh_calls.push("gh.default_branch");
            None
        }),
        ..Default::default()
    };
    let git = MockGit {
        detect_default_branch: Box::new(move || {
            git_calls.push("git.detect_default_branch");
            Err("no remote".to_string())
        }),
        ..Default::default()
    };

    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            warmer,
            git,
            gh,
            ..Default::default()
        },
    ));
    assert_eq!(
        calls.snapshot(),
        vec![
            "warm_origin_head",
            "gh.default_branch",
            "git.detect_default_branch"
        ]
    );
    // Both services are constructed against the project dir by the CLI edge;
    // what the command layer guarantees is that the settings target is derived
    // from that same project dir under project scope.
    assert_eq!(
        out.settings_path,
        format!("{PROJECT_DIR}/.claude/settings.json")
    );
    assert!(out.origin_head_warmed);
}

#[test]
fn warm_up_failure_does_not_abort_setup() {
    // Offline / no remote: the warm-up is best effort, registration still runs.
    let fs = MockFs::new();
    let warmer = MockWarmer {
        warm_origin_head: Box::new(|| false),
    };
    let out = ok(setup(
        &fs,
        Scenario {
            warmer,
            gh: gh_returning("main"),
            ..Default::default()
        },
    ));
    assert!(!out.origin_head_warmed);
    assert_eq!(out.commands.len(), 2);
    assert_eq!(registered_commands(&written(&fs, &out)).len(), 2);
}

// ---- idempotency and migration ----

#[test]
fn rerun_is_idempotent_no_duplicate_hooks() {
    let fs = MockFs::new();
    for _ in 0..3 {
        ok(setup(
            &fs,
            Scenario {
                gh: gh_returning("main"),
                ..Default::default()
            },
        ));
    }
    let out = ok(setup(
        &fs,
        Scenario {
            gh: gh_returning("main"),
            ..Default::default()
        },
    ));
    let commands = registered_commands(&written(&fs, &out));
    assert_eq!(
        commands.len(),
        2,
        "expected exactly two guards: {commands:?}"
    );
}

#[test]
fn removes_stale_guard_entry_with_previous_pin() {
    // Regression fix for the migration hole: `register` replaces by *exact*
    // command, so an entry carrying an old `--default-branch main` is not
    // command-equal to an unpinned re-registration and both would survive —
    // running the guard twice per tool call.
    let fs = MockFs::new();
    let stale_write = r#"atelier git guard write --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main"#;
    let stale_commit = r#"atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch main"#;
    fs.set(
        &format!("{PROJECT_DIR}/.claude/settings.json"),
        &format!(
            r#"{{"hooks":{{"PreToolUse":[
                {{"matcher":"Write|Edit","hooks":[{{"type":"command","command":"{stale_write}"}}]}},
                {{"matcher":"Bash","hooks":[
                    {{"type":"command","command":"{stale_commit}"}},
                    {{"type":"command","command":"protect-stagnation.sh"}}
                ]}}
            ]}}}}"#
        ),
    );

    let out = ok(setup(
        &fs,
        Scenario {
            git: git_without_detection(),
            scope: HookScope::User,
            ..Default::default()
        },
    ));
    // Reported so setup can show the user what was retired.
    assert_eq!(out.removed.len(), 0, "user scope writes elsewhere");

    // Same scenario under project scope, where the stale file actually lives.
    let out = ok(setup(
        &fs,
        Scenario {
            git: git_without_detection(),
            ..Default::default()
        },
    ));
    assert_eq!(
        out.removed.len(),
        2,
        "both stale pins retired: {:?}",
        out.removed
    );

    let commands = registered_commands(&written(&fs, &out));
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.contains("guard write"))
            .count(),
        1
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.contains("guard commit"))
            .count(),
        1
    );
    assert!(
        commands.iter().all(|c| !c.contains("--default-branch")),
        "the stale pin must not survive: {commands:?}"
    );
    // Unrelated siblings in the same matcher group are untouched.
    assert!(commands.iter().any(|c| c == "protect-stagnation.sh"));
}

// ---- dry run ----

#[test]
fn dry_run_does_not_write_settings() {
    // Without this the LLM would write $HOME's settings.json with no chance to
    // review the result first.
    let fs = MockFs::new();
    let out = ok(setup(
        &fs,
        Scenario {
            gh: gh_returning("main"),
            dry_run: true,
            ..Default::default()
        },
    ));
    assert!(out.dry_run);
    assert_eq!(fs.write_count(), 0);
    assert!(fs.get(&out.settings_path).is_none());
    // The plan is still fully reported.
    assert_eq!(out.commands.len(), 2);
    assert_eq!(out.default_branch, Some("main".to_string()));
}
