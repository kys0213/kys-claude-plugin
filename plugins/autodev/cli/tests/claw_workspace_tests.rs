use std::path::Path;

use anyhow::Result;
use tempfile::TempDir;

use autodev::cli::claw;

// ─── claw_init ───

#[test]
fn claw_init_creates_expected_directory_structure() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    claw::claw_init(home)?;

    let ws = home.join("claw-workspace");

    // Top-level files
    assert!(ws.join("CLAUDE.md").is_file(), "CLAUDE.md should exist");

    // .claude/rules/
    assert!(
        ws.join(".claude/rules/scheduling.md").is_file(),
        "scheduling.md should exist"
    );
    assert!(
        ws.join(".claude/rules/branch-naming.md").is_file(),
        "branch-naming.md should exist"
    );
    assert!(
        ws.join(".claude/rules/review-policy.md").is_file(),
        "review-policy.md should exist"
    );

    // commands/
    assert!(
        ws.join("commands/status.md").is_file(),
        "status.md should exist"
    );
    assert!(
        ws.join("commands/board.md").is_file(),
        "board.md should exist"
    );
    assert!(
        ws.join("commands/hitl.md").is_file(),
        "hitl.md should exist"
    );

    // skills/
    assert!(
        ws.join("skills/decompose/SKILL.md").is_file(),
        "decompose SKILL.md should exist"
    );
    assert!(
        ws.join("skills/prioritize/SKILL.md").is_file(),
        "prioritize SKILL.md should exist"
    );

    Ok(())
}

#[test]
fn claw_init_claude_md_has_expected_content() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    claw::claw_init(home)?;

    let content = std::fs::read_to_string(home.join("claw-workspace/CLAUDE.md"))?;
    assert!(content.contains("Claw 판단 원칙"), "should contain title");
    assert!(
        content.contains("autodev queue list --json"),
        "should contain tool reference"
    );

    Ok(())
}

#[test]
fn claw_init_is_idempotent() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    claw::claw_init(home)?;

    // Write custom content to CLAUDE.md
    let claude_md = home.join("claw-workspace/CLAUDE.md");
    std::fs::write(&claude_md, "custom content")?;

    // Run init again — should NOT overwrite existing files
    claw::claw_init(home)?;

    let content = std::fs::read_to_string(&claude_md)?;
    assert_eq!(
        content, "custom content",
        "existing files should not be overwritten"
    );

    Ok(())
}

// ─── claw_init --repo ───

#[test]
fn claw_init_repo_creates_per_repo_override_directory() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    claw::claw_init_repo(home, "org/my-repo")?;

    let repo_claw = home.join("workspaces/org-my-repo/claw");
    assert!(repo_claw.is_dir(), "per-repo claw dir should exist");

    // Override structure should exist (empty files for override)
    assert!(
        repo_claw.join(".claude/rules").is_dir(),
        ".claude/rules dir should exist"
    );
    assert!(
        repo_claw.join("commands").is_dir(),
        "commands dir should exist"
    );
    assert!(repo_claw.join("skills").is_dir(), "skills dir should exist");

    Ok(())
}

#[test]
fn claw_init_repo_is_idempotent() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    claw::claw_init_repo(home, "org/repo")?;
    // Running again should not error
    claw::claw_init_repo(home, "org/repo")?;

    let repo_claw = home.join("workspaces/org-repo/claw");
    assert!(repo_claw.is_dir());

    Ok(())
}

// ─── claw_rules ───

#[test]
fn claw_rules_lists_global_rules() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    // Initialize first
    claw::claw_init(home)?;

    let rules = claw::claw_rules(home, None)?;

    assert!(!rules.is_empty(), "should have rules");
    assert!(
        rules.iter().any(|r| r.contains("scheduling.md")),
        "should include scheduling.md"
    );
    assert!(
        rules.iter().any(|r| r.contains("branch-naming.md")),
        "should include branch-naming.md"
    );
    assert!(
        rules.iter().any(|r| r.contains("review-policy.md")),
        "should include review-policy.md"
    );

    Ok(())
}

#[test]
fn claw_rules_with_repo_shows_merged_view() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    // Init global + repo
    claw::claw_init(home)?;
    claw::claw_init_repo(home, "org/repo")?;

    // Add a custom rule in per-repo override
    let repo_rules = home.join("workspaces/org-repo/claw/.claude/rules");
    std::fs::write(repo_rules.join("custom.md"), "# Custom rule")?;

    let rules = claw::claw_rules(home, Some("org/repo"))?;

    // Global rules present
    assert!(
        rules.iter().any(|r| r.contains("scheduling.md")),
        "global rules should be listed"
    );
    // Per-repo override present
    assert!(
        rules.iter().any(|r| r.contains("custom.md")),
        "per-repo rules should be listed"
    );

    Ok(())
}

#[test]
fn claw_rules_with_no_workspace_returns_error() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    let result = claw::claw_rules(home, None);
    assert!(
        result.is_err(),
        "should error when workspace not initialized"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not initialized"),
        "error should mention 'not initialized', got: {err}"
    );

    Ok(())
}

#[test]
fn claw_rules_with_nonexistent_repo_returns_error() -> Result<()> {
    let tmp = TempDir::new()?;
    let home = tmp.path();

    // Init global but not the repo
    claw::claw_init(home)?;

    let result = claw::claw_rules(home, Some("org/nonexistent"));
    assert!(
        result.is_err(),
        "should error when repo override not initialized"
    );

    Ok(())
}

// ─── claw_workspace_path ───

#[test]
fn claw_workspace_path_returns_expected_path() {
    let home = Path::new("/home/user/.autodev");
    let path = claw::claw_workspace_path(home);
    assert_eq!(path, home.join("claw-workspace"));
}
