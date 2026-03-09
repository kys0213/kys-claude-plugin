/// Integration tests for prompt role filtering (#202).
///
/// Verifies that the v3 indexing pipeline correctly classifies prompts
/// by role (human/system/meta) and that the prompts perspective filters
/// by role with the default of "human".
mod helpers;

use helpers::{cli_with_home, setup_project};

fn index_and_query_prompts(fixtures: &[&str], extra_params: &[&str]) -> serde_json::Value {
    let (tmp, project) = setup_project(fixtures);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    let mut args = vec![
        "query",
        "--project",
        project.to_str().unwrap(),
        "--perspective",
        "prompts",
        "--param",
        "search=%",
    ];
    for p in extra_params {
        args.push("--param");
        args.push(p);
    }

    let output = cli_with_home(&tmp)
        .args(&args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    serde_json::from_slice(&output).unwrap()
}

#[test]
fn prompts_default_returns_human_only() {
    // Default role=human: should return only genuine human prompts
    let result = index_and_query_prompts(&["system_noise.jsonl"], &[]);
    let rows = result.as_array().unwrap();

    // system_noise.jsonl has 2 human prompts: "Fix the login bug" and "Now deploy to production"
    // "ok" after stripping system-reminder is < 5 chars with had_system_reminders=true → system
    // <system-reminder> only → meta (skipped)
    // <local-command-run> → system
    assert_eq!(rows.len(), 2, "expected 2 human prompts, got: {:?}", rows);

    let snippets: Vec<&str> = rows
        .iter()
        .map(|r| r["snippet"].as_str().unwrap())
        .collect();
    assert!(
        snippets.iter().any(|s| s.contains("Fix the login bug")),
        "should contain 'Fix the login bug'"
    );
    assert!(
        snippets
            .iter()
            .any(|s| s.contains("Now deploy to production")),
        "should contain 'Now deploy to production'"
    );

    // All rows should have role=human
    for row in rows {
        assert_eq!(row["role"].as_str().unwrap(), "human");
    }
}

#[test]
fn prompts_role_all_returns_human_and_system() {
    let result = index_and_query_prompts(&["system_noise.jsonl"], &["role=all"]);
    let rows = result.as_array().unwrap();

    // human: 2, system: 3 (<local-command-run>, "ok" with system remnant, one more system)
    // meta: 0 (skipped entirely — empty after strip)
    assert!(
        rows.len() > 2,
        "role=all should return more than just human prompts, got {}",
        rows.len()
    );

    let roles: Vec<&str> = rows.iter().map(|r| r["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"human"), "should include human prompts");
    assert!(roles.contains(&"system"), "should include system prompts");
    assert!(
        !roles.contains(&"meta"),
        "meta prompts should never be stored"
    );
}

#[test]
fn prompts_role_system_excludes_human() {
    let result = index_and_query_prompts(&["system_noise.jsonl"], &["role=system"]);
    let rows = result.as_array().unwrap();

    assert!(!rows.is_empty(), "should have at least one system prompt");

    for row in rows {
        assert_eq!(
            row["role"].as_str().unwrap(),
            "system",
            "role=system filter should only return system prompts"
        );
    }

    // Verify no human content leaked
    let snippets: Vec<&str> = rows
        .iter()
        .map(|r| r["snippet"].as_str().unwrap())
        .collect();
    assert!(
        !snippets.iter().any(|s| s.contains("Fix the login bug")),
        "should not contain human prompts"
    );
}

#[test]
fn prompts_perspective_includes_role_column() {
    let result = index_and_query_prompts(&["system_noise.jsonl"], &[]);
    let rows = result.as_array().unwrap();

    assert!(!rows.is_empty());
    // Verify the role column is present in the output
    assert!(
        rows[0].get("role").is_some(),
        "prompts perspective should include 'role' column"
    );
}

#[test]
fn existing_fixtures_classify_as_human() {
    // Existing fixtures contain only genuine user prompts — all should be role=human
    let result = index_and_query_prompts(&["minimal.jsonl"], &["role=all"]);
    let rows = result.as_array().unwrap();

    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(
            row["role"].as_str().unwrap(),
            "human",
            "existing fixtures should all classify as human"
        );
    }
}
