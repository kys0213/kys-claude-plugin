mod kanban_support;

use kanban_support::{
    asking_bash, asking_permission, asking_permission_at, blocked, session_with, wait_of, NOW,
    REPO, SEC,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn bash_wait(command: &str) -> Value {
    wait_of(asking_bash(command))
}

fn assert_refused_with(command: &str, reason: &str) {
    let wait = bash_wait(command);
    let reasons = blocked(&wait);
    assert_eq!(wait["proxyable"], false, "{command}: {reasons:?}");
    assert!(
        reasons.iter().any(|r| r.starts_with(reason)),
        "{command}: expected {reason} in {reasons:?}"
    );
}

fn assert_proxyable(command: &str) {
    let wait = bash_wait(command);
    assert_eq!(wait["blocked"], json!([]), "{command}");
    assert_eq!(wait["proxyable"], true, "{command}");
}

#[test]
fn rg_options_that_run_a_program_are_not_allowlisted() {
    for command in ["rg foo --pre=sh", "rg --pre sh foo", "rg -z foo"] {
        assert_refused_with(command, "not_allowlisted:");
    }
}

#[test]
fn git_options_that_write_files_or_run_programs_are_not_allowlisted() {
    for command in [
        "git diff --output=/tmp/x",
        "git log --output=x",
        "git show --output=x HEAD",
        "git diff --ext-diff",
        "git log --textconv",
        "git -c x=y log",
    ] {
        assert_refused_with(command, "not_allowlisted:");
    }
}

#[test]
fn sort_output_options_are_not_allowlisted() {
    for command in ["sort -o out in", "sort --output=out in"] {
        assert_refused_with(command, "not_allowlisted:");
    }
}

#[test]
fn uniq_second_positional_is_its_output_file() {
    assert_refused_with("uniq in out", "not_allowlisted:out");
}

#[test]
fn tree_output_options_are_not_allowlisted() {
    for command in ["tree -o out", "tree -H x"] {
        assert_refused_with(command, "not_allowlisted:");
    }
}

#[test]
fn find_actions_that_write_run_or_delete_are_not_allowlisted() {
    for command in [
        "find . -fls out",
        "find . -exec rm {} ;",
        "find . -exec rm {} \\;",
        "find . -delete",
        "find . -fprintf f x",
    ] {
        assert_refused_with(command, "not_allowlisted:");
    }
}

#[test]
fn git_branch_with_a_name_is_not_allowlisted() {
    assert_refused_with("git branch new-name", "not_allowlisted:new-name");
}

#[test]
fn chained_redirected_or_substituted_commands_are_refused() {
    for (command, reason) in [
        ("cat a && rm b", "compound"),
        ("ls > x", "compound"),
        ("cat $(echo a)", "expansion"),
        ("X=1 ls", "expansion"),
        ("cat `a`", "expansion"),
    ] {
        assert_refused_with(command, reason);
    }
}

#[test]
fn a_dollar_sign_is_an_expansion_even_inside_single_quotes() {
    assert_refused_with("jq '.a | $x' f.json", "expansion");
}

#[test]
fn jq_filters_that_read_the_environment_or_other_files_are_not_allowlisted() {
    for (command, word) in [
        ("jq -r env f", "env"),
        ("jq 'include \"x\"; .' f", "include"),
        ("jq 'import \"x\" as m; .' f", "import"),
        ("jq -r '\"\\(env)\"' f", "env"),
        ("jq -c 'input' f", "input"),
        ("jq -c '[inputs]' f", "inputs"),
        ("jq -r 'input_filename' f", "input_filename"),
        ("jq -r '.a # keys\n| get_search_list' f", "get_search_list"),
    ] {
        assert_refused_with(command, &format!("not_allowlisted:{word}"));
    }
}

#[test]
fn jq_filters_of_fields_and_pure_builtins_are_proxyable() {
    for command in [
        "jq -r .name package.json",
        "jq '.env' f",
        "jq -r '.a | keys | map(select(. != \"input\")) | length' f",
        "jq -r '\"\\(.a) env\"' f",
        "jq -c 'if .a then 1e3 else null end' f",
        "jq -r '.[] | @sh' f",
    ] {
        assert_proxyable(command);
    }
}

#[test]
fn paths_outside_the_repository_are_refused() {
    for command in ["cat /etc/passwd", "ls ../other"] {
        assert_refused_with(command, "outside_repo");
    }
}

#[test]
fn read_only_commands_within_their_options_are_proxyable() {
    for command in [
        "git status -s",
        "rg -n foo src",
        "find . -name '*.rs' -type f",
        "uniq -c file",
        "git branch --show-current",
    ] {
        assert_proxyable(command);
    }
}

#[test]
fn web_tools_are_not_allowlisted() {
    for (tool, input) in [
        (
            "WebFetch",
            json!({ "url": "https://example.com/a", "prompt": "summarize" }),
        ),
        ("WebSearch", json!({ "query": "rust sha2" })),
    ] {
        let wait = wait_of(asking_permission(tool, input));
        assert_eq!(wait["proxyable"], false);
        assert!(blocked(&wait).contains(&format!("not_allowlisted:{tool}")));
    }
}

#[test]
fn write_tools_are_not_allowlisted() {
    let wait = wait_of(asking_permission(
        "Edit",
        json!({ "file_path": format!("{REPO}/src/a.rs"), "old_string": "a", "new_string": "b" }),
    ));
    assert_eq!(wait["proxyable"], false);
    assert!(blocked(&wait).contains(&"not_allowlisted:Edit".to_string()));
}

#[test]
fn file_reading_tools_are_proxyable_only_inside_the_repository() {
    let inside = wait_of(asking_permission(
        "Read",
        json!({ "file_path": format!("{REPO}/src/main.rs") }),
    ));
    assert_eq!(inside["proxyable"], true);

    let outside = wait_of(asking_permission(
        "Read",
        json!({ "file_path": "/etc/passwd" }),
    ));
    assert!(blocked(&outside).contains(&"outside_repo".to_string()));

    let glob = wait_of(asking_permission(
        "Glob",
        json!({ "pattern": "../**/*.rs" }),
    ));
    assert!(blocked(&glob).contains(&"outside_repo".to_string()));
}

#[test]
fn file_tool_paths_from_the_home_directory_are_outside_the_repository() {
    for (tool, input) in [
        ("Read", json!({ "file_path": "~/.ssh/id_rsa" })),
        ("Grep", json!({ "pattern": "key", "path": "~/.aws" })),
        ("Glob", json!({ "pattern": "*", "path": "~" })),
        ("Glob", json!({ "pattern": "~/.ssh/*" })),
    ] {
        let wait = wait_of(asking_permission(tool, input.clone()));
        assert_eq!(wait["proxyable"], false, "{tool} {input}");
        assert!(
            blocked(&wait).contains(&"outside_repo".to_string()),
            "{tool} {input}: {:?}",
            blocked(&wait)
        );
    }
}

#[test]
fn a_glob_pattern_climbing_out_after_a_wildcard_is_outside_the_repository() {
    let wait = wait_of(asking_permission(
        "Glob",
        json!({ "pattern": "*/../../etc/*" }),
    ));
    assert_eq!(wait["proxyable"], false);
    assert!(blocked(&wait).contains(&"outside_repo".to_string()));

    let inside = wait_of(asking_permission(
        "Glob",
        json!({ "pattern": "src/**/*.rs" }),
    ));
    assert_eq!(inside["proxyable"], true);
}

#[test]
fn a_link_inside_the_repository_to_a_target_outside_is_outside_the_repository() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().join("repo");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret"), "x").unwrap();
    std::os::unix::fs::symlink(&outside, repo.join("out")).unwrap();
    let repo = repo.to_str().unwrap();

    let bash = wait_of(asking_permission_at(
        repo,
        repo,
        "Bash",
        json!({ "command": "cat out/secret" }),
    ));
    assert!(blocked(&bash).contains(&"outside_repo".to_string()));

    let read = wait_of(asking_permission_at(
        repo,
        repo,
        "Read",
        json!({ "file_path": format!("{repo}/out/secret") }),
    ));
    assert!(blocked(&read).contains(&"outside_repo".to_string()));

    let plain = wait_of(asking_permission_at(
        repo,
        repo,
        "Bash",
        json!({ "command": "ls" }),
    ));
    assert_eq!(plain["proxyable"], true);
}

#[test]
fn a_working_directory_outside_the_repository_is_outside_the_repository() {
    let wait = wait_of(asking_permission_at(
        REPO,
        "/elsewhere",
        "Bash",
        json!({ "command": "ls" }),
    ));
    assert!(blocked(&wait).contains(&"outside_repo".to_string()));
}

#[test]
fn unknown_commands_options_and_extra_arguments_are_named() {
    assert_refused_with("rm -rf build", "not_allowlisted:rm");
    assert_refused_with("ls --color=always", "not_allowlisted:--color");
    assert_refused_with("head -n x file", "not_allowlisted:-n");
    assert_refused_with("cat a b c d e f", "not_allowlisted:f");
    assert_refused_with("git push", "not_allowlisted:push");
}

#[test]
fn bundled_short_flags_are_each_checked() {
    assert_proxyable("ls -la");
    assert_proxyable("grep -rn foo src");
    assert_refused_with("ls -lz", "not_allowlisted:-z");
}

#[test]
fn a_recorded_command_that_was_cut_is_truncated() {
    let mut record = asking_bash("git status");
    record["permission"]["input_len"]["command"] = json!(500);
    let wait = wait_of(record);
    assert_eq!(wait["truncated"], true);
    assert!(blocked(&wait).contains(&"truncated".to_string()));
}

#[test]
fn a_record_without_length_facts_counts_as_truncated() {
    let record = session_with(
        "w",
        json!([{ "name": "classic.PermissionRequest", "detail": "Bash", "at": NOW - 10 * SEC }]),
        json!({
            "cwd": REPO,
            "repo_root": REPO,
            "permission": { "id": "p", "tool": "Bash", "input": { "command": "ls" }, "at": NOW - 10 * SEC },
        }),
    );
    let wait = wait_of(record);
    assert!(blocked(&wait).contains(&"truncated".to_string()));
    assert_eq!(wait["proxyable"], false);
}
