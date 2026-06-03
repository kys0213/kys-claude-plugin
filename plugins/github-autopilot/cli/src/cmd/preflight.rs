use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::fs::FsOps;
use crate::gh::GhOps;
use crate::git::GitOps;

#[derive(Serialize)]
struct CheckResult {
    check: String,
    status: String,
    detail: String,
}

impl CheckResult {
    fn pass(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "PASS".to_string(),
            detail: detail.to_string(),
        }
    }
    fn warn(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "WARN".to_string(),
            detail: detail.to_string(),
        }
    }
    fn fail(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "FAIL".to_string(),
            detail: detail.to_string(),
        }
    }
}

/// Run all preflight checks. Returns exit code 0 (no FAIL) or 1 (any FAIL).
pub fn run(
    gh: &dyn GhOps,
    git: &dyn GitOps,
    fs: &dyn FsOps,
    config_path: &str,
    repo_root: &Path,
) -> Result<i32> {
    let mut results = Vec::new();

    // A-1. CLAUDE.md existence and content
    check_claude_md(fs, repo_root, &mut results);

    // A-2. Rules coverage
    check_rules_coverage(fs, repo_root, &mut results);

    // B-1. gh auth
    check_gh_auth(gh, &mut results);

    // B-2. Guard PR base hook
    check_guard_hook(fs, repo_root, &mut results);

    // B-3. Quality gate command
    check_quality_gate(fs, config_path, &mut results);

    // B-4. Git remote
    check_git_remote(git, &mut results);

    // B-5. branch_strategy enum validation (#758)
    check_branch_strategy(fs, config_path, &mut results);

    // C. Spec files existence
    check_spec_files(fs, config_path, repo_root, &mut results);

    let has_fail = results.iter().any(|r| r.status == "FAIL");
    println!("{}", serde_json::to_string(&results)?);

    if has_fail {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn check_claude_md(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    let path = repo_root.join("CLAUDE.md");
    if !fs.file_exists(&path) {
        results.push(CheckResult::fail("CLAUDE.md", "CLAUDE.md not found"));
        return;
    }

    let content = match fs.read_file(&path) {
        Ok(c) => c,
        Err(_) => {
            results.push(CheckResult::fail("CLAUDE.md", "failed to read CLAUDE.md"));
            return;
        }
    };

    let has_tree = content.contains("├")
        || content.contains("└")
        || content.contains("directory")
        || content.contains("structure");
    let has_build = ["cargo", "npm", "go ", "make", "pytest", "jest", "gradle"]
        .iter()
        .any(|kw| content.contains(kw));
    let has_convention = ["stack", "convention", "principle", "컨벤션", "원칙"]
        .iter()
        .any(|kw| content.contains(kw));

    let mut missing = Vec::new();
    if !has_tree {
        missing.push("file tree");
    }
    if !has_build {
        missing.push("build/test commands");
    }
    if !has_convention {
        missing.push("conventions");
    }

    if missing.is_empty() {
        results.push(CheckResult::pass(
            "CLAUDE.md",
            "file tree, build commands, conventions present",
        ));
    } else if missing.len() < 3 {
        results.push(CheckResult::warn(
            "CLAUDE.md",
            &format!("missing: {}", missing.join(", ")),
        ));
    } else {
        results.push(CheckResult::fail(
            "CLAUDE.md",
            &format!("missing all: {}", missing.join(", ")),
        ));
    }
}

fn check_rules_coverage(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    let rules_dir = repo_root.join(".claude/rules");
    let rule_files = match fs.list_files(&rules_dir, "md") {
        Ok(f) => f,
        Err(_) => {
            results.push(CheckResult::warn(
                "Rules coverage",
                ".claude/rules/ directory not found",
            ));
            return;
        }
    };

    if rule_files.is_empty() {
        results.push(CheckResult::warn("Rules coverage", "no rule files found"));
        return;
    }

    // Count rules with paths frontmatter as a simple coverage proxy
    let mut rules_with_paths = 0;
    for file in &rule_files {
        if let Ok(content) = fs.read_file(file) {
            if content.contains("paths:") {
                rules_with_paths += 1;
            }
        }
    }

    results.push(CheckResult::pass(
        "Rules coverage",
        &format!(
            "{} rules found ({} with paths)",
            rule_files.len(),
            rules_with_paths
        ),
    ));
}

fn check_gh_auth(gh: &dyn GhOps, results: &mut Vec<CheckResult>) {
    match gh.run(&["auth", "status"]) {
        Ok(_) => results.push(CheckResult::pass("gh auth", "authenticated")),
        Err(_) => results.push(CheckResult::fail("gh auth", "gh auth login required")),
    }
}

fn check_guard_hook(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    // /github-autopilot:setup은 user scope(~/.claude/settings.json)에 hook을 설치하지만,
    // 프로젝트 단위로 직접 등록하는 경우도 있으므로 두 scope를 모두 확인한다.
    // (setup이 user scope에 쓰는데 preflight이 project scope만 보면 항상 WARN이 떠서
    //  사용자가 잘못된 상대경로 hook을 수동으로 박는 사고로 이어진다 — #731)
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".claude/settings.json"));
    }
    candidates.push(repo_root.join(".claude/settings.json"));
    candidates.push(repo_root.join(".claude/settings.local.json"));

    let mut any_settings_found = false;
    for settings in &candidates {
        if !fs.file_exists(settings) {
            continue;
        }
        any_settings_found = true;
        if let Ok(content) = fs.read_file(settings) {
            if content.contains("guard-pr-base") {
                results.push(CheckResult::pass("Hooks", "guard-pr-base registered"));
                return;
            }
        }
    }

    if any_settings_found {
        results.push(CheckResult::warn(
            "Hooks",
            "guard-pr-base hook not found — run /github-autopilot:setup (do not edit settings.json manually)",
        ));
    } else {
        results.push(CheckResult::warn("Hooks", "settings.json not found"));
    }
}

fn check_quality_gate(fs: &dyn FsOps, config_path: &str, results: &mut Vec<CheckResult>) {
    let path = PathBuf::from(config_path);
    if !fs.file_exists(&path) {
        results.push(CheckResult::warn(
            "Quality Gate",
            "config file not found, auto-detect",
        ));
        return;
    }

    match fs.read_file(&path) {
        Ok(content) => {
            let qg = content
                .lines()
                .find(|l| l.starts_with("quality_gate_command:"))
                .and_then(|l| {
                    let val = l.trim_start_matches("quality_gate_command:").trim();
                    let val = val.trim_matches('"').trim_matches('\'').trim();
                    if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                });

            match qg {
                None => results.push(CheckResult::pass("Quality Gate", "auto-detect")),
                Some(cmd) => results.push(CheckResult::pass("Quality Gate", &cmd)),
            }
        }
        Err(_) => {
            results.push(CheckResult::warn("Quality Gate", "failed to read config"));
        }
    }
}

/// frontmatter에서 `key: "value"` 형태의 스칼라 값을 추출한다 (따옴표 제거).
/// 값이 없거나 비어 있으면 None.
fn frontmatter_scalar(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    content
        .lines()
        .find(|l| l.trim_start().starts_with(&prefix))
        .and_then(|l| {
            let val = l.trim().trim_start_matches(&prefix).trim();
            let val = val.trim_matches('"').trim_matches('\'').trim();
            // 인라인 주석 제거 (예: `branch_strategy: "draft-main"  # comment`)
            let val = val.split('#').next().unwrap_or(val).trim();
            if val.is_empty() {
                None
            } else {
                Some(val.to_string())
            }
        })
}

/// branch_strategy 값이 알려진 enum인지 검증한다.
/// 알 수 없는 값은 silent fallback to main 대신 명시적으로 WARN을 띄운다 (#758).
fn check_branch_strategy(fs: &dyn FsOps, config_path: &str, results: &mut Vec<CheckResult>) {
    const KNOWN: [&str; 2] = ["draft-main", "draft-develop-main"];

    let path = PathBuf::from(config_path);
    if !fs.file_exists(&path) {
        // config 부재는 다른 check에서 이미 보고됨 — 여기서는 조용히 통과
        return;
    }

    let content = match fs.read_file(&path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // work_branch가 설정되어 있으면 branch_strategy보다 우선하므로 검증 불필요
    if frontmatter_scalar(&content, "work_branch").is_some() {
        results.push(CheckResult::pass("Branch Strategy", "work_branch override"));
        return;
    }

    match frontmatter_scalar(&content, "branch_strategy") {
        // 미설정 → 문서화된 기본값(main)으로 동작
        None => results.push(CheckResult::pass("Branch Strategy", "draft-main (default)")),
        Some(v) if KNOWN.contains(&v.as_str()) => {
            results.push(CheckResult::pass("Branch Strategy", &v));
        }
        Some(v) => results.push(CheckResult::warn(
            "Branch Strategy",
            &format!(
                "unknown value '{v}' falls back to main — expected one of: draft-main, draft-develop-main"
            ),
        )),
    }
}

fn check_git_remote(git: &dyn GitOps, results: &mut Vec<CheckResult>) {
    match git.remote_url("origin") {
        Ok(url) => results.push(CheckResult::pass("Git Remote", &url)),
        Err(_) => results.push(CheckResult::fail("Git Remote", "origin remote not found")),
    }
}

fn check_spec_files(
    fs: &dyn FsOps,
    config_path: &str,
    repo_root: &Path,
    results: &mut Vec<CheckResult>,
) {
    let path = PathBuf::from(config_path);
    if !fs.file_exists(&path) {
        results.push(CheckResult::warn("Spec files", "config not found"));
        return;
    }

    let content = match fs.read_file(&path) {
        Ok(c) => c,
        Err(_) => {
            results.push(CheckResult::warn("Spec files", "failed to read config"));
            return;
        }
    };

    // Parse spec_paths from YAML-like config
    let spec_paths: Vec<String> = parse_spec_paths(&content);

    if spec_paths.is_empty() {
        results.push(CheckResult::warn("Spec files", "spec_paths not configured"));
        return;
    }

    let mut total = 0;
    for sp in &spec_paths {
        let dir = repo_root.join(sp);
        if let Ok(files) = fs.list_files(&dir, "md") {
            total += files.len();
        }
    }

    if total > 0 {
        results.push(CheckResult::pass(
            "Spec files",
            &format!("{total} spec files found"),
        ));
    } else {
        results.push(CheckResult::fail(
            "Spec files",
            &format!("no .md files in spec_paths ({})", spec_paths.join(", ")),
        ));
    }
}

/// Simple parser for spec_paths YAML list.
fn parse_spec_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_spec = false;

    for line in content.lines() {
        if line.starts_with("spec_paths:") {
            in_spec = true;
            continue;
        }
        if in_spec {
            let trimmed = line.trim();
            if trimmed.starts_with('-') {
                let val = trimmed
                    .trim_start_matches('-')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();
                if !val.is_empty() {
                    paths.push(val.to_string());
                }
            } else if !trimmed.is_empty() {
                break;
            }
        }
    }
    paths
}
