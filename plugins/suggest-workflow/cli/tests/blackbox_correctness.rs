/// E2E data correctness: index → query → verify exact values.
/// These tests ensure internal refactoring doesn't break data integrity.
mod helpers;

use helpers::{cli_with_home, setup_project};
use std::collections::HashMap;

fn index_and_query(fixtures: &[&str], perspective: &str, params: &[&str]) -> serde_json::Value {
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
        perspective,
    ];
    for p in params {
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

fn to_freq_map(json: &serde_json::Value) -> HashMap<String, i64> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let tool = row["tool"].as_str().unwrap().to_string();
            let freq = row["frequency"].as_i64().unwrap();
            (tool, freq)
        })
        .collect()
}

fn to_hotfile_map(json: &serde_json::Value) -> HashMap<String, i64> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let path = row["file_path"].as_str().unwrap().to_string();
            let count = row["edit_count"].as_i64().unwrap();
            (path, count)
        })
        .collect()
}

// ============================================================
// Tool frequency exact counts
// ============================================================

/// multi_tool.jsonl tools:
///   assistant1: Read, Grep, Read
///   assistant2: Edit, Edit, Bash(cargo test), Bash(cargo build)
///   assistant3: Bash(git add), Bash(git commit), Read, Glob, Write
/// Expected: Read=3, Grep=1, Edit=2, Bash:test=1, Bash:build=1, Bash:git=2, Glob=1, Write=1
#[test]
fn tool_frequency_exact_counts_multi_tool() {
    let json = index_and_query(&["multi_tool.jsonl"], "tool-frequency", &["top=20"]);
    let freq = to_freq_map(&json);

    assert_eq!(freq.get("Read"), Some(&3), "Read should appear 3 times");
    assert_eq!(freq.get("Grep"), Some(&1));
    assert_eq!(freq.get("Edit"), Some(&2));
    assert_eq!(freq.get("Bash:test"), Some(&1));
    assert_eq!(freq.get("Bash:build"), Some(&1));
    assert_eq!(freq.get("Bash:git"), Some(&2));
    assert_eq!(freq.get("Glob"), Some(&1));
    assert_eq!(freq.get("Write"), Some(&1));

    let total: i64 = freq.values().sum();
    assert_eq!(total, 12, "total tool_uses should be 12");
}

// ============================================================
// Bash classification accuracy
// ============================================================

/// bash_classified.jsonl:
///   git status, git diff --cached → Bash:git
///   npm run build → Bash:build
///   npm test → Bash:test
///   eslint src/ → Bash:lint
///   prettier --check . → Bash:lint
///   ls -la → Bash:other
///   git push origin main → Bash:git
///   gh pr create --title test → Bash:git
/// Expected: Bash:git=4, Bash:test=1, Bash:build=1, Bash:lint=2, Bash:other=1
#[test]
fn bash_classification_accuracy() {
    let json = index_and_query(&["bash_classified.jsonl"], "tool-frequency", &["top=20"]);
    let freq = to_freq_map(&json);

    assert_eq!(
        freq.get("Bash:git"),
        Some(&4),
        "git status/diff/push + gh pr create"
    );
    assert_eq!(freq.get("Bash:test"), Some(&1), "npm test");
    assert_eq!(freq.get("Bash:build"), Some(&1), "npm run build");
    assert_eq!(freq.get("Bash:lint"), Some(&2), "eslint + prettier");
    assert_eq!(freq.get("Bash:other"), Some(&1), "ls -la");

    let total: i64 = freq.values().sum();
    assert_eq!(total, 9, "total bash tools should be 9");
}

// ============================================================
// Hotfiles exact counts
// ============================================================

/// file_edits.jsonl file edits:
///   config.toml: Edit×2 + Edit×1 = 3
///   .env: Write×1 = 1
///   src/main.rs: Edit×1 = 1
///   analysis.ipynb: NotebookEdit×1 = 1
///   README.md: Edit×1 = 1
#[test]
fn hotfiles_exact_edit_counts() {
    let json = index_and_query(&["file_edits.jsonl"], "hotfiles", &["top=20"]);
    let hotfiles = to_hotfile_map(&json);

    assert_eq!(hotfiles.len(), 5, "5 unique files edited");
    assert_eq!(
        hotfiles.get("/home/user/project/config.toml"),
        Some(&3),
        "config.toml edited 3 times"
    );
    assert_eq!(hotfiles.get("/home/user/project/.env"), Some(&1));
    assert_eq!(hotfiles.get("/home/user/project/src/main.rs"), Some(&1));
    assert_eq!(hotfiles.get("/home/user/project/analysis.ipynb"), Some(&1));
    assert_eq!(hotfiles.get("/home/user/project/README.md"), Some(&1));
}

// ============================================================
// Sessions perspective — session count + prompt count
// ============================================================

#[test]
fn sessions_count_matches_fixtures() {
    let json = index_and_query(
        &["minimal.jsonl", "multi_tool.jsonl", "bash_classified.jsonl"],
        "sessions",
        &["top=100"],
    );
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 3, "3 session files → 3 sessions");
}

#[test]
fn sessions_prompt_count_correct() {
    // multi_tool.jsonl has 3 user prompts
    let json = index_and_query(&["multi_tool.jsonl"], "sessions", &["top=10"]);
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["prompt_count"].as_i64(), Some(3));
    assert_eq!(arr[0]["tool_use_count"].as_i64(), Some(12));
}

// ============================================================
// Transitions — verify derived table correctness
// ============================================================

/// multi_tool.jsonl tool sequence:
///   Read(0)→Grep(1)→Read(2)→Edit(3)→Edit(4)→Bash:test(5)→Bash:build(6)→
///   Bash:git(7)→Bash:git(8)→Read(9)→Glob(10)→Write(11)
///
/// Expected transitions from Edit: Edit→Edit(1), Edit→Bash:test(1)
#[test]
fn transitions_correct_for_edit() {
    let json = index_and_query(&["multi_tool.jsonl"], "transitions", &["tool=Edit"]);
    let arr = json.as_array().unwrap();

    let transition_map: HashMap<String, i64> = arr
        .iter()
        .map(|row| {
            (
                row["to_tool"].as_str().unwrap().to_string(),
                row["count"].as_i64().unwrap(),
            )
        })
        .collect();

    assert_eq!(transition_map.get("Edit"), Some(&1), "Edit→Edit once");
    assert_eq!(
        transition_map.get("Bash:test"),
        Some(&1),
        "Edit→Bash:test once"
    );
    assert_eq!(
        transition_map.len(),
        2,
        "Edit only transitions to Edit and Bash:test"
    );
}

#[test]
fn transitions_correct_for_bash_git() {
    // Bash:git(7)→Bash:git(8), Bash:git(8)→Read(9)
    let json = index_and_query(&["multi_tool.jsonl"], "transitions", &["tool=Bash:git"]);
    let arr = json.as_array().unwrap();

    let transition_map: HashMap<String, i64> = arr
        .iter()
        .map(|row| {
            (
                row["to_tool"].as_str().unwrap().to_string(),
                row["count"].as_i64().unwrap(),
            )
        })
        .collect();

    assert_eq!(transition_map.get("Bash:git"), Some(&1));
    assert_eq!(transition_map.get("Read"), Some(&1));
}

// ============================================================
// Sequences — verify from tool_transitions
// ============================================================

#[test]
fn sequences_shows_bigrams_with_counts() {
    let json = index_and_query(&["multi_tool.jsonl"], "sequences", &["min_count=1"]);
    let arr = json.as_array().unwrap();

    assert!(!arr.is_empty(), "should have at least one sequence");

    // Each sequence should have format "X → Y"
    for item in arr {
        let seq = item["sequence"].as_str().unwrap();
        assert!(
            seq.contains(" → "),
            "sequence format should be 'X → Y': {}",
            seq
        );
        assert!(item.get("count").is_some());
        assert!(item.get("probability").is_some());
    }
}

// ============================================================
// Prompts search — keyword matching
// ============================================================

#[test]
fn prompts_search_finds_matching_text() {
    let json = index_and_query(&["multi_tool.jsonl"], "prompts", &["search=refactor"]);
    let arr = json.as_array().unwrap();

    assert!(!arr.is_empty(), "should find prompt containing 'refactor'");
    for item in arr {
        let snippet = item["snippet"].as_str().unwrap().to_lowercase();
        assert!(
            snippet.contains("refactor"),
            "snippet should contain search term"
        );
    }
}

#[test]
fn prompts_search_returns_empty_for_no_match() {
    let json = index_and_query(
        &["multi_tool.jsonl"],
        "prompts",
        &["search=xyznonexistent999"],
    );
    let arr = json.as_array().unwrap();
    assert!(arr.is_empty(), "no prompts should match nonexistent term");
}

// ============================================================
// Repetition perspective — z-score anomaly detection
// ============================================================

#[test]
fn repetition_returns_valid_structure() {
    // With a single session, there's no variance, so anomaly detection may not trigger.
    // Use multiple fixtures to create variance across sessions.
    let json = index_and_query(
        &[
            "multi_tool.jsonl",
            "bash_classified.jsonl",
            "file_edits.jsonl",
        ],
        "repetition",
        &["z_threshold=0.5"],
    );
    let arr = json.as_array().unwrap();

    for item in arr {
        assert!(item.get("session_id").is_some());
        assert!(item.get("tool").is_some());
        assert!(item.get("deviation_score").is_some());
        // deviation_score = sign(z) * z², so |deviation_score| >= threshold²
        let d = item["deviation_score"].as_f64().unwrap();
        assert!(
            d.abs() >= 0.25,
            "deviation_score {} should have |z²| >= 0.5² = 0.25",
            d
        );
    }
}

// ============================================================
// Session-links — file sharing overlap
// ============================================================

#[test]
fn session_links_detects_shared_files() {
    // multi_tool.jsonl edits auth.rs, routes.rs, notes.txt
    // file_edits.jsonl edits config.toml, .env, main.rs, ipynb, README.md
    // No overlap between these two → no links expected
    let json = index_and_query(
        &["multi_tool.jsonl", "file_edits.jsonl"],
        "session-links",
        &["min_overlap=0.0"],
    );
    let arr = json.as_array().unwrap();

    // These fixtures don't share edited files, so no links
    assert!(arr.is_empty(), "no shared files between these fixtures");
}

// ============================================================
// Trends — weekly aggregation
// ============================================================

#[test]
fn trends_weekly_aggregation_correct() {
    let json = index_and_query(&["multi_tool.jsonl"], "trends", &["since=2020-01-01"]);
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "should have weekly buckets");

    // All entries should be from the same week (all timestamps in 2026-02-10)
    let first_week = arr[0]["week_start"].as_str().unwrap();
    for item in arr {
        assert_eq!(item["week_start"].as_str().unwrap(), first_week);
    }

    // Sum of all tool counts should equal total tool_uses (12)
    let total_count: i64 = arr.iter().map(|row| row["count"].as_i64().unwrap()).sum();
    assert_eq!(total_count, 12, "weekly sum should equal total tool_uses");
}

// ============================================================
// Multi-fixture aggregation
// ============================================================

#[test]
fn multi_fixture_tool_counts_aggregate_correctly() {
    // multi_tool.jsonl: 12 tools
    // bash_classified.jsonl: 9 tools
    // Total: 21 tools
    let json = index_and_query(
        &["multi_tool.jsonl", "bash_classified.jsonl"],
        "tool-frequency",
        &["top=50"],
    );
    let freq = to_freq_map(&json);
    let total: i64 = freq.values().sum();
    assert_eq!(total, 21, "combined tool count: 12 + 9 = 21");

    // Bash:git should aggregate: 2 (multi) + 4 (bash_classified) = 6
    assert_eq!(freq.get("Bash:git"), Some(&6));
}

#[test]
fn hotfiles_aggregation_across_fixtures() {
    // multi_tool.jsonl: auth.rs(1), routes.rs(1), notes.txt(1)
    // file_edits.jsonl: config.toml(3), .env(1), main.rs(1), ipynb(1), README.md(1)
    // Total unique files: 8
    let json = index_and_query(
        &["multi_tool.jsonl", "file_edits.jsonl"],
        "hotfiles",
        &["top=50"],
    );
    let hotfiles = to_hotfile_map(&json);
    assert_eq!(hotfiles.len(), 8, "8 unique files across both fixtures");
}
