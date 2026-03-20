use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Returns the claw-workspace path under the given autodev home.
pub fn claw_workspace_path(home: &Path) -> PathBuf {
    home.join("claw-workspace")
}

/// Returns the per-repo claw override path.
fn repo_claw_path(home: &Path, repo_name: &str) -> PathBuf {
    let sanitized = crate::core::config::sanitize_repo_name(repo_name);
    home.join("workspaces").join(sanitized).join("claw")
}

/// Initialize the global claw-workspace with default structure.
///
/// Creates `<home>/claw-workspace/` with CLAUDE.md, rules, commands, and skills.
/// Idempotent: existing files are not overwritten.
pub fn claw_init(home: &Path) -> Result<()> {
    let ws = claw_workspace_path(home);

    // Create directory structure
    let dirs = [
        ws.join(".claude/rules"),
        ws.join("commands"),
        ws.join("skills/decompose"),
        ws.join("skills/gap-detect"),
        ws.join("skills/prioritize"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory: {}", dir.display()))?;
    }

    // Write default files (only if they don't exist)
    let files: &[(&str, &str)] = &[
        ("CLAUDE.md", TPL_CLAUDE_MD),
        (".claude/rules/scheduling.md", TPL_SCHEDULING_MD),
        (".claude/rules/branch-naming.md", TPL_BRANCH_NAMING_MD),
        (".claude/rules/review-policy.md", TPL_REVIEW_POLICY_MD),
        (
            ".claude/rules/decompose-strategy.md",
            TPL_DECOMPOSE_STRATEGY_MD,
        ),
        (".claude/rules/hitl-policy.md", TPL_HITL_POLICY_MD),
        (
            ".claude/rules/auto-approve-policy.md",
            TPL_AUTO_APPROVE_POLICY_MD,
        ),
        (".claude/rules/operations.md", TPL_OPERATIONS_MD),
        ("commands/status.md", DEFAULT_STATUS_MD),
        ("commands/board.md", DEFAULT_BOARD_MD),
        ("commands/hitl.md", DEFAULT_HITL_MD),
        ("commands/spec.md", DEFAULT_SPEC_MD),
        ("commands/repo.md", DEFAULT_REPO_MD),
        ("commands/decisions.md", DEFAULT_DECISIONS_MD),
        ("commands/cron.md", DEFAULT_CRON_MD),
        ("skills/decompose/SKILL.md", TPL_DECOMPOSE_SKILL_MD),
        ("skills/gap-detect/SKILL.md", TPL_GAP_DETECT_SKILL_MD),
        ("skills/prioritize/SKILL.md", TPL_PRIORITIZE_SKILL_MD),
    ];

    for (rel_path, content) in files {
        let path = ws.join(rel_path);
        if !path.exists() {
            std::fs::write(&path, content)
                .with_context(|| format!("failed to write: {}", path.display()))?;
        }
    }

    println!("Claw workspace initialized: {}", ws.display());

    Ok(())
}

/// Initialize a per-repo claw override directory.
///
/// Creates `<home>/workspaces/<org-repo>/claw/` with empty override structure.
pub fn claw_init_repo(home: &Path, repo_name: &str) -> Result<()> {
    let repo_claw = repo_claw_path(home, repo_name);

    let dirs = [
        repo_claw.join(".claude/rules"),
        repo_claw.join("commands"),
        repo_claw.join("skills"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory: {}", dir.display()))?;
    }

    println!(
        "Per-repo claw override initialized: {}",
        repo_claw.display()
    );

    Ok(())
}

/// List applied rule files from global claw-workspace and optionally per-repo overrides.
///
/// Returns a list of rule file paths (relative display).
pub fn claw_rules(home: &Path, repo: Option<&str>) -> Result<Vec<String>> {
    let ws = claw_workspace_path(home);
    let global_rules_dir = ws.join(".claude/rules");

    if !ws.exists() {
        anyhow::bail!("Claw workspace not initialized. Run 'autodev claw init' first.");
    }

    let mut rules = Vec::new();

    // Collect global rules
    if global_rules_dir.is_dir() {
        collect_rule_files(&global_rules_dir, "[global]", &mut rules)?;
    }

    // Collect per-repo override rules if requested
    if let Some(repo_name) = repo {
        let repo_claw = repo_claw_path(home, repo_name);
        let repo_rules_dir = repo_claw.join(".claude/rules");

        if !repo_claw.exists() {
            anyhow::bail!(
                "Per-repo claw override not initialized for '{repo_name}'. Run 'autodev claw init --repo {repo_name}' first."
            );
        }

        if repo_rules_dir.is_dir() {
            collect_rule_files(&repo_rules_dir, &format!("[{repo_name}]"), &mut rules)?;
        }
    }

    Ok(rules)
}

fn collect_rule_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        out.push(format!("{prefix} {name}"));
    }

    Ok(())
}

/// Open a rule/command/skill file in $EDITOR for editing.
pub fn claw_edit(home: &Path, name: &str, repo: Option<&str>) -> Result<()> {
    let base = if let Some(repo_name) = repo {
        repo_claw_path(home, repo_name)
    } else {
        claw_workspace_path(home)
    };

    if !base.exists() {
        anyhow::bail!("Claw workspace not initialized. Run `autodev claw init` first.");
    }

    // Search for the file in multiple locations
    let candidates = [
        base.join(".claude/rules").join(format!("{name}.md")),
        base.join("commands").join(format!("{name}.md")),
        base.join("skills").join(name).join("SKILL.md"),
    ];

    let target = candidates.iter().find(|p| p.exists());

    let file_path = match target {
        Some(p) => p.clone(),
        None => {
            let paths: Vec<String> = candidates
                .iter()
                .map(|p| format!("  {}", p.display()))
                .collect();
            anyhow::bail!("Rule '{}' not found. Searched:\n{}", name, paths.join("\n"));
        }
    };

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(&file_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Validate edited file
    let content = std::fs::read_to_string(&file_path)
        .with_context(|| format!("failed to read edited file: {}", file_path.display()))?;

    let warnings = validate_rule_content(&content);
    if warnings.is_empty() {
        println!("Updated: {}", file_path.display());
    } else {
        println!("Updated: {} (with warnings)", file_path.display());
        for w in &warnings {
            eprintln!("  warning: {w}");
        }
    }

    Ok(())
}

/// Validate rule/command/skill markdown content after editing.
///
/// Returns a list of warnings (empty if valid).
fn validate_rule_content(content: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    let trimmed = content.trim();
    if trimmed.is_empty() {
        warnings.push("file is empty".to_string());
        return warnings;
    }

    if !trimmed.lines().any(|l| l.starts_with('#')) {
        warnings.push("no markdown heading found (expected at least one '# ...')".to_string());
    }

    warnings
}

// ─── Template-based content (source of truth: templates/claw-workspace/) ───

const TPL_CLAUDE_MD: &str = include_str!("../../../templates/claw-workspace/CLAUDE.md");
const TPL_SCHEDULING_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/scheduling.md");
const TPL_BRANCH_NAMING_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/branch-naming.md");
const TPL_REVIEW_POLICY_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/review-policy.md");
const TPL_DECOMPOSE_STRATEGY_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/decompose-strategy.md");
const TPL_HITL_POLICY_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/hitl-policy.md");
const TPL_AUTO_APPROVE_POLICY_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/auto-approve-policy.md");
const TPL_OPERATIONS_MD: &str =
    include_str!("../../../templates/claw-workspace/.claude/rules/operations.md");
const TPL_DECOMPOSE_SKILL_MD: &str =
    include_str!("../../../templates/claw-workspace/skills/decompose/SKILL.md");
const TPL_GAP_DETECT_SKILL_MD: &str =
    include_str!("../../../templates/claw-workspace/skills/gap-detect/SKILL.md");
const TPL_PRIORITIZE_SKILL_MD: &str =
    include_str!("../../../templates/claw-workspace/skills/prioritize/SKILL.md");

const DEFAULT_CRON_MD: &str = include_str!("../../../commands/cron.md");

// ─── Hardcoded content (no template file) ───

const DEFAULT_STATUS_MD: &str = r#"# /status 커맨드

현재 Claw 세션의 상태를 요약합니다.

## 출력 항목
- 활성 작업 수
- 대기 중인 HITL 이벤트
- 최근 완료된 작업
- 에러/블로커 현황

## 실행
```
autodev queue list --json
autodev hitl list --json
```
"#;

const DEFAULT_BOARD_MD: &str = r#"# /board 커맨드

칸반 보드 형태로 현재 작업 상태를 표시합니다.

## 컬럼
- Backlog: 대기 중인 작업
- In Progress: 진행 중인 작업
- Review: 리뷰 중인 작업
- Done: 완료된 작업

## 데이터 소스
```
autodev queue list --json
autodev spec list --json
```
"#;

const DEFAULT_HITL_MD: &str = r#"# /hitl 커맨드

Human-in-the-Loop 이벤트를 관리합니다.

## 기능
- 대기 중인 HITL 이벤트 목록 표시
- 이벤트 상세 정보 조회
- 이벤트 응답 (선택지 또는 메시지)

## 실행
```
autodev hitl list --json
autodev hitl show <id>
autodev hitl respond <id> --choice <n>
```
"#;

const DEFAULT_SPEC_MD: &str = r#"# /spec 커맨드

스펙을 관리합니다.

## 기능
- 스펙 목록 조회
- 스펙 상세 / 진행 상태 조회
- 스펙 완료 판정

## 실행
```
autodev spec list --json
autodev spec show <id> --json
autodev spec status <id> --json
autodev spec complete <id>
```
"#;

const DEFAULT_REPO_MD: &str = r#"# /repo 커맨드

등록된 레포를 관리합니다.

## 기능
- 레포 목록 조회
- 레포 상세 조회
- 레포 설정 확인

## 실행
```
autodev repo list
autodev repo show <name> --json
autodev repo config <name>
```
"#;

const DEFAULT_DECISIONS_MD: &str = r#"# /decisions 커맨드

Claw의 판단 이력을 조회합니다.

## 기능
- 최근 판단 목록 조회
- 판단 상세 조회

## 실행
```
autodev decisions list --json
autodev decisions show <id> --json
```
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_content() {
        let warnings = validate_rule_content("");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("empty"));
    }

    #[test]
    fn validate_whitespace_only() {
        let warnings = validate_rule_content("   \n  \n  ");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("empty"));
    }

    #[test]
    fn validate_no_heading() {
        let warnings = validate_rule_content("some content without heading\nmore lines");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("heading"));
    }

    #[test]
    fn validate_valid_content() {
        let warnings = validate_rule_content("# My Rule\n\nSome description");
        assert!(warnings.is_empty());
    }

    #[test]
    fn validate_h2_heading_counts() {
        let warnings = validate_rule_content("## Subsection\n\nContent here");
        assert!(warnings.is_empty());
    }
}
