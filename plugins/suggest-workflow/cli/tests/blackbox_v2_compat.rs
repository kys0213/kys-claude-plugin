/// G1~G2: v2 backward compatibility tests.
/// Verify that v2 --cache flag still produces expected output structure.
mod helpers;

use helpers::{cli_with_home, setup_project};
use std::path::PathBuf;

// ============================================================
// G2: cache command produces analysis-snapshot.json
// ============================================================

#[test]
fn g2_cache_produces_analysis_snapshot() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "bash_classified.jsonl"]);

    let output = cli_with_home(&tmp)
        .args(["--cache", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();

    // stdout should contain the cache directory path
    let stdout = String::from_utf8_lossy(&output.stdout);
    let cache_dir = PathBuf::from(stdout.trim());
    assert!(
        cache_dir.exists(),
        "cache directory should exist: {}",
        cache_dir.display()
    );

    // analysis-snapshot.json should exist and be valid JSON
    let snapshot_path = cache_dir.join("analysis-snapshot.json");
    assert!(
        snapshot_path.exists(),
        "analysis-snapshot.json should exist"
    );

    let snapshot_content = std::fs::read_to_string(&snapshot_path).unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot_content)
        .expect("analysis-snapshot.json should be valid JSON");

    // Verify expected top-level keys
    let obj = snapshot
        .as_object()
        .expect("snapshot should be a JSON object");
    assert!(obj.contains_key("analyzedAt"), "should have analyzedAt");
    assert!(obj.contains_key("depth"), "should have depth");
    assert!(obj.contains_key("project"), "should have project");
    assert!(obj.contains_key("cacheVersion"), "should have cacheVersion");
    assert!(
        obj.contains_key("toolTransitions"),
        "should have toolTransitions"
    );
    assert!(obj.contains_key("weeklyTrends"), "should have weeklyTrends");
    assert!(obj.contains_key("fileAnalysis"), "should have fileAnalysis");
}

#[test]
fn g2_cache_produces_session_summaries() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    let output = cli_with_home(&tmp)
        .args(["--cache", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let cache_dir = PathBuf::from(stdout.trim());

    // sessions/ directory should contain summary files
    let sessions_dir = cache_dir.join("sessions");
    assert!(sessions_dir.exists(), "sessions/ directory should exist");

    let summaries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    assert_eq!(
        summaries.len(),
        1,
        "should have 1 session summary (multi_tool.jsonl)"
    );

    // Verify summary structure
    let summary_content = std::fs::read_to_string(summaries[0].path()).unwrap();
    let summary: serde_json::Value =
        serde_json::from_str(&summary_content).expect("session summary should be valid JSON");

    let obj = summary
        .as_object()
        .expect("summary should be a JSON object");
    assert!(obj.contains_key("id"), "summary should have id");
    assert!(obj.contains_key("prompts"), "summary should have prompts");
    // camelCase: toolUseCount (serde rename_all = "camelCase")
    assert!(
        obj.contains_key("toolUseCount"),
        "summary should have toolUseCount"
    );
    assert!(obj.contains_key("stats"), "summary should have stats");
}

#[test]
fn g2_cache_produces_index_json() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl", "bash_classified.jsonl"]);

    let output = cli_with_home(&tmp)
        .args(["--cache", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let cache_dir = PathBuf::from(stdout.trim());

    // index.json should exist and be valid
    let index_path = cache_dir.join("index.json");
    assert!(index_path.exists(), "index.json should exist");

    let index_content = std::fs::read_to_string(&index_path).unwrap();
    let index: serde_json::Value =
        serde_json::from_str(&index_content).expect("index.json should be valid JSON");

    let obj = index.as_object().expect("index should be a JSON object");
    // camelCase: totalSessions (serde rename_all = "camelCase")
    assert_eq!(
        obj["totalSessions"].as_i64(),
        Some(2),
        "should have 2 sessions"
    );
    assert!(obj.contains_key("sessions"), "should have sessions array");
    assert_eq!(
        obj["sessions"].as_array().unwrap().len(),
        2,
        "sessions array should have 2 entries"
    );
}

// ============================================================
// G2+: cache also populates v3 index DB
// ============================================================

#[test]
fn g2_cache_also_populates_v3_index_db() {
    let (tmp, project) = setup_project(&["multi_tool.jsonl"]);

    // Run cache
    cli_with_home(&tmp)
        .args(["--cache", "--project", project.to_str().unwrap()])
        .assert()
        .success();

    // After cache, v3 index DB should exist and be queryable
    cli_with_home(&tmp)
        .args([
            "query",
            "--project",
            project.to_str().unwrap(),
            "--perspective",
            "tool-frequency",
            "--param",
            "top=5",
        ])
        .assert()
        .success();
}
